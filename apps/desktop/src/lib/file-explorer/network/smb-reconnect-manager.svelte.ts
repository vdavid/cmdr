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
import { reconnectVolume, getVolumeSignInState, onVolumeConnectionChanged } from '$lib/tauri-commands'
import type { SignInShape } from '$lib/tauri-commands'
import { asReconnectError, describeReconnectRefusal } from './reconnect-error'
import { getAppLogger } from '$lib/logging/logger'
import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'

const log = getAppLogger('smbReconnect')

/**
 * Backoff schedule for reconnect attempts. The total wait time is the sum of
 * these delays. Single source of truth; every label and progress bar derives
 * from this array, so changing it propagates to the UI automatically.
 */
export const RECONNECT_DELAYS_MS = [2000, 4000, 8000, 16000, 30000] as const

/** Sum of `RECONNECT_DELAYS_MS`. Cached once because it's constant. */
export const TOTAL_DURATION_MS = RECONNECT_DELAYS_MS.reduce((a, b) => a + b, 0)

/** Number of attempts in a full cycle. */
export const TOTAL_ATTEMPTS = RECONNECT_DELAYS_MS.length

export type ReconnectStatus = 'waiting' | 'attempting' | 'gave-up' | 'needs-auth' | 'needs-host-key'

export interface ReconnectState {
  status: ReconnectStatus
  /** 0-based index into `RECONNECT_DELAYS_MS`. */
  attemptIndex: number
  /** Delay for the current `waiting` phase, in ms. Mirrors `RECONNECT_DELAYS_MS[attemptIndex]`. */
  currentDelayMs: number
  /** `performance.now()` when the current `waiting` phase started. Used for the progress bar. */
  waitStartedAt: number
}

interface VolumeEntry {
  state: ReconnectState
  refcount: number
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
  signIn: SignInShape | null
  /** Active `setTimeout` handle for the next attempt, if `status === 'waiting'`. */
  timerId: ReturnType<typeof setTimeout> | null
  /** Subscribers' success callbacks. Fired when state transitions back to Direct. */
  successCallbacks: Set<() => void>
}

class SmbReconnectManager {
  /** Reactive map keyed by volumeId. Component reads via `getState(volumeId)`. */
  private map = new SvelteMap<string, VolumeEntry>()
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
      }
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
        entry = freshEntry()
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
        entry = freshEntry()
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
      entry.state = freshState()
      // Force reactivity by re-setting the entry with a new state object.
      this.map.set(volumeId, entry)
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
      this.map.set(volumeId, entry) // notify subscribers
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
      log.warn('Reading the sign-in state for {volumeId} failed: {error}', { volumeId, error: String(e) })
      return
    }
    untrack(() => {
      const entry = this.map.get(volumeId)
      if (!entry) return
      entry.signIn = prompt
      this.map.set(volumeId, entry) // notify subscribers
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
      this.map.set(volumeId, entry) // notify subscribers
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
      entry.state = freshState()
      this.map.set(volumeId, entry) // notify subscribers
      for (const cb of entry.successCallbacks) {
        try {
          cb()
        } catch (e) {
          log.warn('Reconnect success callback threw: {error}', { error: String(e) })
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
    entry.state = {
      status: 'waiting',
      attemptIndex,
      currentDelayMs: delay,
      waitStartedAt: performance.now(),
    }
    entry.timerId = setTimeout(() => {
      void this.runAttempt(volumeId, attemptIndex)
    }, delay)
    this.map.set(volumeId, entry) // notify subscribers
  }

  /**
   * Fires one reconnect attempt against the backend. On success, the
   * `volume-connection-changed { state: "connected" }` event will arrive and
   * `handleConnected` cleans up. On failure, schedule the next attempt or give up.
   */
  private async runAttempt(volumeId: string, attemptIndex: number): Promise<void> {
    const entry = this.map.get(volumeId)
    if (!entry) return
    entry.state = { ...entry.state, status: 'attempting', attemptIndex }
    entry.timerId = null
    this.map.set(volumeId, entry) // notify subscribers

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
        reason: refusal === null ? String(e) : describeReconnectRefusal(refusal),
      })
      // Re-fetch entry: `cancel` may have run during the attempt.
      const e2 = this.map.get(volumeId)
      if (!e2) return
      // The backend may have emitted `needs_credentials` during this attempt (stale password).
      // `handleNeedsAuth` already stopped the cycle; don't schedule another doomed retry.
      if (e2.state.status === 'needs-auth' || e2.state.status === 'needs-host-key') return
      const next = attemptIndex + 1
      if (next >= TOTAL_ATTEMPTS) {
        e2.state = { ...e2.state, status: 'gave-up' }
        this.map.set(volumeId, e2) // notify subscribers
      } else {
        this.beginAttempt(volumeId, next)
      }
    }
  }
}

function freshState(): ReconnectState {
  return {
    status: 'waiting',
    attemptIndex: 0,
    currentDelayMs: RECONNECT_DELAYS_MS[0],
    waitStartedAt: performance.now(),
  }
}

function freshEntry(): VolumeEntry {
  return {
    state: freshState(),
    refcount: 0,
    signIn: null,
    timerId: null,
    successCallbacks: new Set(),
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

/** The whole cycle's length as a human sentence ("60 seconds", "2 minutes"). */
function totalDurationLabel(): string {
  const seconds = Math.round(TOTAL_DURATION_MS / 1000)
  if (seconds < 90) return tString('servers.paneState.retryTotalSeconds', { seconds })
  return tString('servers.paneState.retryTotalMinutes', { minutes: Math.round(seconds / 60) })
}
