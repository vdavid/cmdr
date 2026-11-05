/**
 * Per-volume SMB reconnect manager.
 *
 * Drives the backoff cycle that re-establishes a Disconnected `SmbVolume`. One
 * cycle per volume; both panes on the same share share a single cycle and see
 * identical UI.
 *
 * Lifecycle:
 * - `init()` is called once at app startup. Sets up the global
 *   `volume-connection-changed` event listener.
 * - `subscribe(volumeId, onSuccess?)` returns an unsubscribe fn. Refcounted;
 *   when the last subscriber leaves, any in-flight cycle is cancelled (the
 *   connection stays Disconnected; lazy reconnect on next nav handles re-entry).
 * - On a `disconnected` event for a volume with subscribers, a cycle starts
 *   automatically. On `connected`, the cycle resolves and registered `onSuccess`
 *   callbacks fire.
 * - `startCycle(volumeId)` exposes the same trigger for the lazy nav path
 *   (when the user opens a share that's already Disconnected and we never saw
 *   the event).
 * - `retryNow(volumeId)` fires an attempt immediately and resets backoff.
 * - `cancel(volumeId)` clears the cycle without touching the connection.
 *
 * On a `needs_credentials` event the cycle stops and the manager asks the backend
 * what a sign-in on that volume would want (`getSignInShape` reads it back).
 * Asked at the flip and never carried over: the credential a remote volume comes
 * back on is decided per dial. `DETAILS.md` § "SMB live-reconnect flow".
 */

import { untrack } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  reconnectVolume,
  getVolumeSignInState,
  onVolumeConnectionChanged,
  onVolumeReconnectRequested,
} from '#lib/tauri-commands/index.js'
import type { SignInShape } from '#lib/tauri-commands/index.js'
import type { VolumeError } from '#lib/ipc/bindings.js'
import { asReconnectError, describeReconnectRefusal } from './reconnect-error'
import { getAppLogger } from '#lib/logging/logger.js'
import { LogOnceGate } from '#lib/logging/log-once.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'

const log = getAppLogger('smbReconnect')

/**
 * Backoff schedule for reconnect attempts. The total wait time is the sum of
 * these delays. Single source of truth; every label and progress bar derives
 * from this array, so changing it propagates to the UI automatically.
 *
 * The first attempt waits for nothing and runs QUIET ({@link QUIET_ATTEMPT_GRACE_MS}):
 * most drops (a NAS hanging up on an idle connection, a socket that died while
 * the Mac slept) come back on the first dial in a fraction of a second, and a
 * progress bar counting down to it only made a healthy share look broken.
 */
export const RECONNECT_DELAYS_MS = [0, 2000, 4000, 8000, 16000, 30000] as const

/**
 * How long the first attempt may run before the pane shows the cycle. A dial that
 * comes back inside it shows nothing; one that fails sooner moves to the next
 * attempt, which shows at once; one still dialing after it shows as attempting.
 */
export const QUIET_ATTEMPT_GRACE_MS = 1000

/** Sum of `RECONNECT_DELAYS_MS`. Cached once because it's constant. */
export const TOTAL_DURATION_MS = RECONNECT_DELAYS_MS.reduce<number>((a, b) => a + b, 0)

/** Number of attempts in a full cycle. */
export const TOTAL_ATTEMPTS = RECONNECT_DELAYS_MS.length

export type ReconnectStatus =
  | 'waiting'
  | 'attempting'
  | 'gave-up'
  | 'needs-auth'
  | 'needs-host-key'
  | 'guest-refused'
  | 'encryption-required'
  | 'negotiation-invalid'
  | 'encryption-unavailable'

/** The statuses only a person (or the server's admin) can move on from: the backoff stops at them. */
const STOPPED_STATUSES: ReadonlySet<ReconnectStatus> = new Set([
  'needs-auth',
  'needs-host-key',
  'guest-refused',
  'encryption-required',
  'negotiation-invalid',
  'encryption-unavailable',
])

/** A session the backend refused whatever the credentials, which the same server answers the same way. */
type SessionRefusalStatus = 'encryption-required' | 'negotiation-invalid' | 'encryption-unavailable'

/**
 * The command's typed refusals that stop the cycle on their own, so it stops even if the event
 * lands after the failed attempt returns.
 */
const SESSION_REFUSAL_FOR_VOLUME_ERROR: Readonly<Partial<Record<VolumeError['type'], SessionRefusalStatus>>> = {
  encryptionRequired: 'encryption-required',
  negotiationInvalid: 'negotiation-invalid',
  encryptionUnavailable: 'encryption-unavailable',
}

export interface ReconnectState {
  status: ReconnectStatus
  /** 0-based index into `RECONNECT_DELAYS_MS`. */
  attemptIndex: number
  /** Delay for the current `waiting` phase, in ms. Mirrors `RECONNECT_DELAYS_MS[attemptIndex]`. */
  currentDelayMs: number
  /** `performance.now()` when the current `waiting` phase started. Used for the progress bar. */
  waitStartedAt: number
}

/**
 * One volume's cycle.
 *
 * ❗ `state` and `signIn` are `$state.raw` FIELDS, and every transition assigns a
 * new object to them. That assignment is what a pane's `$derived` hears. Re-setting
 * the same entry in the map is not: `SvelteMap.set` stays silent when the value is
 * identical, so a manager that mutated the entry and called `map.set` froze every
 * view on its first read (`ERR-VZ4CV`: "Retry now" disabled over a cycle that had
 * long given up, Cancel and Disconnect leaving the view on screen).
 */
class VolumeEntry {
  state = $state.raw<ReconnectState>(freshState())
  /**
   * What a sign-in on this volume would ask for, as of the last `needs-auth`
   * flip. `null` until one happens.
   *
   * The pane's signed-out banner reads it (`getSignInShape`) to decide whether to
   * offer a Sign in button at all: a `nothing` shape means no secret a person
   * could type would help, and a button that can't work is worse than none. The
   * SHEET asks again when it renders, because it describes THIS session.
   * Recorded here rather than kept from the connect result because the credential
   * a remote volume comes back on is decided per dial.
   *
   * ❗ A tagged union: switch on `kind` and ❌ never derive the form from the
   * protocol or from the mode the sheet is in. Whether the username is editable
   * is the VARIANT's answer (`crates/cmdr-fs/src/volume/connection.rs`).
   */
  signIn = $state.raw<SignInShape | null>(null)
  refcount = 0
  /** Active `setTimeout` handle for the next attempt, if `status === 'waiting'`. */
  timerId: ReturnType<typeof setTimeout> | null = null
  /**
   * Whether the first attempt has outlasted {@link QUIET_ATTEMPT_GRACE_MS} (or a
   * person asked for it with "Retry now"), so the pane shows it. Later attempts
   * always show.
   */
  firstAttemptShown = $state(false)
  /** The grace timer that flips `firstAttemptShown`. */
  graceTimerId: ReturnType<typeof setTimeout> | null = null
  /**
   * Bumped by every Cancel. An attempt that was in flight when it landed reads a
   * different number on its way back and drops its answer, so a failure can't
   * schedule the next attempt of a cycle the person called off.
   */
  cycle = 0
  /** Subscribers' success callbacks. Fired when state transitions back to Direct. */
  successCallbacks = new Set<() => void>()
}

class SmbReconnectManager {
  /** Reactive map keyed by volumeId. Component reads via `getState(volumeId)`. */
  private map = new SvelteMap<string, VolumeEntry>()
  /** Keyed by volume ID: which volumes already logged an unreadable sign-in state. */
  private readonly signInReadFailures = new LogOnceGate()
  private unlisten: UnlistenFn | null = null

  /** Idempotent. Call once at app startup before any FilePane mounts. */
  async init(): Promise<void> {
    if (this.unlisten) return
    this.unlisten = await onVolumeConnectionChanged((payload) => {
      const { volumeId, state } = payload
      log.debug('volume-connection-changed: volumeId={volumeId}, state={state}', { volumeId, state })
      switch (state) {
        case 'disconnected':
          this.handleDisconnected(volumeId)
          break
        case 'needs_credentials':
          void this.handleNeedsAuth(volumeId)
          break
        case 'connected':
          this.handleConnected(volumeId)
          break
        case 'needs_host_key_approval':
          // ❗ Its OWN status, ❌ never the sign-in path: a password box in front
          // of a possible man-in-the-middle is how a password gets typed into
          // one. The pane renders `host_key_changed` instead, which says what
          // happened and offers Disconnect.
          this.handleNeedsHostKey(volumeId)
          break
        case 'guest_session_refused':
          // ❗ Its own status rather than `needs-auth`: the pane names what Cmdr refused (a
          // guest session offered for a named login), which a bare sign-in prompt doesn't.
          this.handleGuestRefused(volumeId)
          break
        case 'encryption_required':
          // "Always require SMB encryption" is on and the server can't encrypt: the same server
          // answers the same until it or the setting changes, so the backoff stops (#966).
          this.handleSessionRefused(volumeId, 'encryption-required')
          break
        case 'negotiation_invalid':
          // The server didn't confirm the security settings it agreed on, and asking again asks
          // the same server (#931).
          this.handleSessionRefused(volumeId, 'negotiation-invalid')
          break
        case 'encryption_unavailable':
          // The share only accepts encryption the session agreed on no way to do (#931).
          this.handleSessionRefused(volumeId, 'encryption-unavailable')
          break
      }
    })
    // Ask Cmdr's `connect_volume` on a dropped volume: the same lazy start a pane landing
    // on it makes (`servers/connect-flow.ts` arm 1), so the backend never runs a loop of its own.
    // Held for the app's life, like the listener above.
    await onVolumeReconnectRequested(({ volumeId }) => {
      log.debug('volume-reconnect-requested: volumeId={volumeId}', { volumeId })
      this.startCycle(volumeId)
    })
  }

  /**
   * Subscribes a viewer (typically a FilePane) to this volume's reconnect
   * cycle. The optional `onSuccess` callback fires when the cycle completes.
   * Returns an unsubscribe function; call it on volume change / unmount.
   *
   * Gotcha/Why: every method that both reads and writes the SvelteMap is
   * wrapped in `untrack`. Without it, calling `subscribe` from a Svelte
   * `$effect` would track the `map.get(volumeId)` read, then the subsequent
   * `map.set` would invalidate that dep, the effect would re-run, and we'd
   * be in a tight subscribe→unsubscribe loop that pegs the main thread (verified
   * (both panes stuck on Loading…). `untrack` decouples our internal map
   * accesses from the caller's reactive context. Reactive readers like the
   * `getState`-backed `$derived` still work because `untrack` only suppresses
   * read tracking; writes still fire invalidations to anyone with a tracked dep.
   */
  subscribe(volumeId: string, onSuccess?: () => void): () => void {
    return untrack(() => {
      let entry = this.map.get(volumeId)
      if (!entry) {
        entry = new VolumeEntry()
        this.map.set(volumeId, entry)
      }
      entry.refcount++
      if (onSuccess) entry.successCallbacks.add(onSuccess)
      log.debug('subscribe({volumeId}): refcount={refcount}', { volumeId, refcount: entry.refcount })

      return () => {
        untrack(() => {
          const e = this.map.get(volumeId)
          if (!e) return
          e.refcount--
          if (onSuccess) e.successCallbacks.delete(onSuccess)
          log.debug('unsubscribe({volumeId}): refcount={refcount}', { volumeId, refcount: e.refcount })
          if (e.refcount <= 0) {
            if (e.timerId) clearTimeout(e.timerId)
            clearGrace(e)
            this.map.delete(volumeId)
          }
        })
      }
    })
  }

  /** Reactive read of the current cycle state, or `null` if no cycle is running. */
  getState(volumeId: string): ReconnectState | null {
    const entry = this.map.get(volumeId)
    if (!entry) return null
    // Only surface the state if we're actively in a cycle (timer set) or just
    // gave up. A bare entry with refcount > 0 but no cycle isn't user-visible.
    if (entry.state.status === 'waiting' && entry.timerId === null && entry.state.attemptIndex === 0) {
      return null
    }
    // The quiet first attempt, inside its grace: nothing to show yet.
    const { status, attemptIndex } = entry.state
    const dialing = status === 'waiting' || status === 'attempting'
    if (dialing && attemptIndex === 0 && !entry.firstAttemptShown) return null
    return entry.state
  }

  /**
   * What a sign-in on this volume would ask for, as of the last `needs-auth`
   * flip, or `null` if there hasn't been one (or the answer is still in flight).
   */
  getSignInShape(volumeId: string): SignInShape | null {
    return this.map.get(volumeId)?.signIn ?? null
  }

  /** Whether a cycle is currently running for this volume. */
  isActive(volumeId: string): boolean {
    return this.getState(volumeId) !== null
  }

  /**
   * Explicitly kicks off a cycle. Used by the lazy nav-time path when the user
   * opens a share that's already Disconnected (no recent `volume-connection-changed`
   * event would arrive in that case).
   *
   * No-op if a cycle is already running for this volume.
   */
  startCycle(volumeId: string): void {
    untrack(() => {
      let entry = this.map.get(volumeId)
      if (!entry) {
        entry = new VolumeEntry()
        this.map.set(volumeId, entry)
      }
      if (entry.timerId !== null || entry.state.status === 'attempting') return
      this.beginAttempt(volumeId, 0)
    })
  }

  /**
   * "Retry now" button: fires an attempt immediately and, on failure, resumes
   * the backoff at the FIRST delay (per the design: full reset, not resume
   * from where we were).
   *
   * Disabled during `attempting` (the button itself is disabled in the view).
   */
  retryNow(volumeId: string): void {
    untrack(() => {
      const entry = this.map.get(volumeId)
      if (!entry) return
      if (entry.state.status === 'attempting') return
      if (entry.timerId) {
        clearTimeout(entry.timerId)
        entry.timerId = null
      }
      // A person asked: the attempt shows from the start, ❌ never a quiet one
      // that blanks the view they just clicked in.
      clearGrace(entry)
      entry.firstAttemptShown = true
      void this.runAttempt(volumeId, 0)
    })
  }

  /**
   * "Cancel" button: stops the cycle and clears state. The connection stays
   * Disconnected; the user can navigate back to the share later and the lazy
   * reconnect path will pick up.
   */
  cancel(volumeId: string): void {
    untrack(() => {
      const entry = this.map.get(volumeId)
      if (!entry) return
      if (entry.timerId) clearTimeout(entry.timerId)
      entry.timerId = null
      clearGrace(entry)
      entry.cycle++
      entry.state = freshState()
    })
  }

  // ── Internal ──────────────────────────────────────────────────────
  // All map-mutating internals run inside `untrack` so a Svelte reactive
  // caller never ends up tracking our internal `map.get` reads.

  private handleDisconnected(volumeId: string): void {
    untrack(() => {
      const entry = this.map.get(volumeId)
      if (!entry) return // No subscribers; lazy reconnect handles it on next nav.
      if (entry.timerId !== null || entry.state.status === 'attempting') return
      this.beginAttempt(volumeId, 0)
    })
  }

  /**
   * The backend gave up reconnecting because the saved password no longer works
   * (the server's password changed). Stop the futile backoff — retrying the same
   * stale credentials can't succeed — and flip to `needs-auth` so FilePane shows a
   * "Sign in" prompt instead of the generic "unreachable" banner. The user signs in
   * via `reconnectVolumeWithCredentials`; success arrives as a `connected` event.
   */
  private async handleNeedsAuth(volumeId: string): Promise<void> {
    // Everything up to the first `await` runs synchronously with the event, so
    // `runAttempt`'s in-flight check still sees `needs-auth` the moment it flips.
    const flipped = untrack(() => {
      const entry = this.map.get(volumeId)
      if (!entry) return false // No subscribers; the next nav re-enters the flow.
      if (entry.timerId) clearTimeout(entry.timerId)
      entry.timerId = null
      entry.state = { ...entry.state, status: 'needs-auth' }
      return true
    })
    if (!flipped) return

    // Asked here, and asked again on every later flip: the credential a remote
    // volume comes back on is decided per dial, so a value kept from the connect
    // that opened it describes a session that has since ended.
    let prompt: SignInShape
    try {
      prompt = await getVolumeSignInState(volumeId)
    } catch (e) {
      // A flaky network flips a volume in and out of needs-auth, and every flip
      // re-asks: logged once per volume until a read works.
      if (this.signInReadFailures.shouldLog(volumeId)) {
        log.warn('Reading the sign-in state for {volumeId} failed: {error}', { volumeId, error: e })
      }
      return
    }
    this.signInReadFailures.clear(volumeId)
    untrack(() => {
      const entry = this.map.get(volumeId)
      if (!entry) return
      entry.signIn = prompt
    })
  }

  /**
   * The server presents a host key this Mac doesn't trust, so the backend
   * stopped. Ends the backoff — retrying can't help, and every attempt is
   * another handshake with a server whose identity is in question — and flips to
   * `needs-host-key` so the pane renders the banner rather than a spinner over a
   * dead session.
   *
   * ❗ No `getVolumeSignInState` here, unlike `needs-auth`: nothing about this is
   * a credential question, and asking one would be the first step toward putting
   * a password box in front of it.
   */
  private handleNeedsHostKey(volumeId: string): void {
    untrack(() => {
      const entry = this.map.get(volumeId)
      if (!entry) return // No subscribers; the next nav re-enters the flow.
      if (entry.timerId) clearTimeout(entry.timerId)
      entry.timerId = null
      entry.state = { ...entry.state, status: 'needs-host-key' }
    })
  }

  /**
   * The server answered a named login with a guest session, so the backend refused it. Ends the
   * backoff (the same login gets the same guest offer) and flips to `guest-refused`, which the
   * pane renders with "Try another password" and Disconnect.
   */
  private handleGuestRefused(volumeId: string): void {
    untrack(() => {
      const entry = this.map.get(volumeId)
      if (!entry) return // No subscribers; the next nav re-enters the flow.
      if (entry.timerId) clearTimeout(entry.timerId)
      entry.timerId = null
      entry.state = { ...entry.state, status: 'guest-refused' }
    })
  }

  /**
   * The backend refused the session whatever the credentials: "Always require SMB encryption" and
   * a server that can't encrypt, an unconfirmed negotiation, or a share whose encryption the
   * session couldn't agree on. Ends the backoff (the server answers the same until it or the
   * setting changes) and flips to `status`, which the pane renders with the reason.
   */
  private handleSessionRefused(volumeId: string, status: SessionRefusalStatus): void {
    untrack(() => {
      const entry = this.map.get(volumeId)
      if (!entry) return // No subscribers; the next nav re-enters the flow.
      if (entry.timerId) clearTimeout(entry.timerId)
      entry.timerId = null
      entry.state = { ...entry.state, status }
    })
  }

  private handleConnected(volumeId: string): void {
    untrack(() => {
      const entry = this.map.get(volumeId)
      if (!entry) return
      // Idempotent: if no cycle is in flight (state is the baseline + no timer),
      // there's nothing to clean up and no subscribers to notify. This guards
      // against double-firing when both `runAttempt`'s success branch and the
      // `volume-connection-changed` event fire; whichever runs first wins.
      const noActiveCycle = entry.state.status === 'waiting' && entry.timerId === null && entry.state.attemptIndex === 0
      if (noActiveCycle) return
      if (entry.timerId) clearTimeout(entry.timerId)
      entry.timerId = null
      clearGrace(entry)
      entry.state = freshState()
      for (const cb of entry.successCallbacks) {
        try {
          cb()
        } catch (e) {
          // Our own subscriber's bug, so it earns an error report.
          log.error('Reconnect success callback threw: {error}', { error: e })
        }
      }
    })
  }

  /**
   * Schedules attempt `attemptIndex` after the corresponding backoff delay.
   * Sets `status='waiting'` and the progress-bar timing fields. Caller is
   * responsible for the surrounding `untrack` (the public methods all are).
   */
  private beginAttempt(volumeId: string, attemptIndex: number): void {
    const entry = this.map.get(volumeId)
    if (!entry) return
    const delay = RECONNECT_DELAYS_MS[attemptIndex]
    if (attemptIndex === 0) {
      clearGrace(entry)
      entry.firstAttemptShown = false
      entry.graceTimerId = setTimeout(() => {
        entry.graceTimerId = null
        entry.firstAttemptShown = true
      }, QUIET_ATTEMPT_GRACE_MS)
    }
    entry.state = {
      status: 'waiting',
      attemptIndex,
      currentDelayMs: delay,
      waitStartedAt: performance.now(),
    }
    entry.timerId = setTimeout(() => {
      void this.runAttempt(volumeId, attemptIndex)
    }, delay)
  }

  /**
   * Fires one reconnect attempt against the backend. On success, the
   * `volume-connection-changed { state: "connected" }` event will arrive and
   * `handleConnected` cleans up. On failure, schedule the next attempt or give up.
   */
  private async runAttempt(volumeId: string, attemptIndex: number): Promise<void> {
    const entry = this.map.get(volumeId)
    if (!entry) return
    const cycle = entry.cycle
    entry.timerId = null
    entry.state = { ...entry.state, status: 'attempting', attemptIndex }

    try {
      await reconnectVolume(volumeId)
      // Success: defensive backstop in case the `volume-connection-changed`
      // event somehow doesn't arrive (unexpected, but `handleConnected` is
      // idempotent so calling both paths is safe).
      this.handleConnected(volumeId)
    } catch (e) {
      // The backend's refusal is typed, so the log records the REASON rather
      // than a sentence a copy edit could change under it.
      const refusal = asReconnectError(e)
      log.info("Reconnect attempt {attempt} for {volumeId} didn't take: {reason}", {
        attempt: attemptIndex + 1,
        volumeId,
        reason: refusal === null ? e : describeReconnectRefusal(refusal),
      })
      // Re-fetch entry: the last subscriber may have left during the attempt,
      // and a Cancel may have called this cycle off.
      const e2 = this.map.get(volumeId)
      if (!e2 || e2.cycle !== cycle) return
      // The backend may have emitted `needs_credentials` during this attempt (stale password).
      // `handleNeedsAuth` already stopped the cycle; don't schedule another doomed retry.
      if (STOPPED_STATUSES.has(e2.state.status)) return
      // The refusal's own type says it too, so the cycle stops even if the event hasn't landed yet.
      const refusedAs = refusal?.type === 'volume' ? SESSION_REFUSAL_FOR_VOLUME_ERROR[refusal.error.type] : undefined
      if (refusedAs !== undefined) {
        e2.state = { ...e2.state, status: refusedAs }
        return
      }
      const next = attemptIndex + 1
      if (next >= TOTAL_ATTEMPTS) {
        e2.state = { ...e2.state, status: 'gave-up' }
      } else {
        this.beginAttempt(volumeId, next)
      }
    }
  }
}

/** Stops a quiet first attempt's grace timer, if one is running. */
function clearGrace(entry: VolumeEntry): void {
  if (entry.graceTimerId) clearTimeout(entry.graceTimerId)
  entry.graceTimerId = null
}

function freshState(): ReconnectState {
  return {
    status: 'waiting',
    attemptIndex: 0,
    currentDelayMs: RECONNECT_DELAYS_MS[0],
    waitStartedAt: performance.now(),
  }
}

/** Singleton. Call `init()` once at app startup. */
export const smbReconnectManager = new SmbReconnectManager()

// ── Display helpers (pure; tested separately) ─────────────────────

/** "1 → once", "2 → twice", "n → n times". */
export function ordinalCount(n: number): string {
  if (n === 1) return tString('fileExplorer.network.reconnect.once')
  if (n === 2) return tString('fileExplorer.network.reconnect.twice')
  return tString('fileExplorer.network.reconnect.times', { count: n, countText: formatInteger(n) })
}

/**
 * Builds the body-line-2 sentence shown during a `waiting` phase, starting
 * from attempt 2 (i.e., when `attemptIndex >= 1`). Returns `null` for the
 * very first attempt (no body 2 needed; body 1's "total" copy carries it).
 *
 * Examples (with the default 5-attempt array):
 * - attemptIndex=1 → "Retried once, will try it 3 times more after this."
 * - attemptIndex=2 → "Retried twice, will try it twice more after this."
 * - attemptIndex=3 → "Retried 3 times, will try it once more after this."
 * - attemptIndex=4 → "Retried 4 times, this is the final attempt. Connection drops if it fails."
 */
export function reconnectProgressMessage(attemptIndex: number): string | null {
  if (attemptIndex < 1) return null
  const retried = ordinalCount(attemptIndex)
  // `attemptIndex` is the upcoming attempt's index (the one we're currently waiting on).
  // Attempts AFTER it = TOTAL_ATTEMPTS - 1 - attemptIndex.
  const remaining = TOTAL_ATTEMPTS - 1 - attemptIndex
  if (remaining <= 0) {
    return tString('fileExplorer.network.reconnect.finalAttempt', { retried })
  }
  return tString('fileExplorer.network.reconnect.willTryMore', { retried, remaining: ordinalCount(remaining) })
}

/**
 * The two sentences the pane says while a cycle runs, in order: how long the
 * whole loop keeps going, and which attempt it is on.
 *
 * ❗ The copy lives HERE, beside the delay table it describes, so a change to
 * `RECONNECT_DELAYS_MS` and a change to the words a person reads about it land
 * together. The pane renders whatever strings it is handed.
 */
export function reconnectCycleLines(attemptIndex: number): string[] {
  const total = tString('servers.paneState.retryKeepsTrying', { duration: totalDurationLabel() })
  const progress = reconnectProgressMessage(attemptIndex)
  return progress ? [total, progress] : [total]
}

/**
 * The whole cycle's length as a human sentence ("60 seconds", "2 minutes").
 *
 * ❗ Both keys carry an ICU plural, and both take the raw number for choosing the
 * form PLUS the formatted one for reading. `RECONNECT_DELAYS_MS` can't produce a
 * one today, so this is latent rather than live — which is exactly why it has to
 * be right in the catalog: nothing on screen would say "1 minutes" until somebody
 * shortens the table, and then it would say it in eleven languages.
 */
function totalDurationLabel(): string {
  const seconds = Math.round(TOTAL_DURATION_MS / 1000)
  if (seconds < 90)
    return tString('servers.paneState.retryTotalSeconds', { seconds, secondsText: formatInteger(seconds) })
  const minutes = Math.round(seconds / 60)
  return tString('servers.paneState.retryTotalMinutes', { minutes, minutesText: formatInteger(minutes) })
}
