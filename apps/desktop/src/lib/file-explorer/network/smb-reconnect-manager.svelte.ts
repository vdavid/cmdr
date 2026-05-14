/**
 * Per-volume SMB reconnect manager.
 *
 * Drives the backoff cycle that re-establishes a Disconnected `SmbVolume`. One
 * cycle per volume; both panes on the same share share a single cycle and see
 * identical UI.
 *
 * Lifecycle:
 * - `init()` is called once at app startup. Sets up the global
 *   `smb-connection-changed` event listener.
 * - `subscribe(volumeId, onSuccess?)` returns an unsubscribe fn. Refcounted;
 *   when the last subscriber leaves, any in-flight cycle is cancelled (the
 *   connection stays Disconnected; lazy reconnect on next nav handles re-entry).
 * - On `disconnected` event for a volume with subscribers, a cycle starts
 *   automatically. On `direct`, the cycle resolves and registered `onSuccess`
 *   callbacks fire.
 * - `startCycle(volumeId)` exposes the same trigger for the lazy nav path
 *   (when the user opens a share that's already Disconnected and we never saw
 *   the event).
 * - `retryNow(volumeId)` fires an attempt immediately and resets backoff.
 * - `cancel(volumeId)` clears the cycle without touching the connection.
 */

import { untrack } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { reconnectSmbVolume } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

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

export type ReconnectStatus = 'waiting' | 'attempting' | 'gave-up'

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
    this.unlisten = await listen<{ volumeId: string; state: 'direct' | 'disconnected' }>(
      'smb-connection-changed',
      (event) => {
        const { volumeId, state } = event.payload
        log.debug('smb-connection-changed: volumeId={volumeId}, state={state}', { volumeId, state })
        if (state === 'disconnected') {
          this.handleDisconnected(volumeId)
        } else {
          this.handleDirect(volumeId)
        }
      },
    )
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

  /** Whether a cycle is currently running for this volume. */
  isActive(volumeId: string): boolean {
    return this.getState(volumeId) !== null
  }

  /**
   * Explicitly kicks off a cycle. Used by the lazy nav-time path when the user
   * opens a share that's already Disconnected (no recent `smb-connection-changed`
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

  private handleDirect(volumeId: string): void {
    untrack(() => {
      const entry = this.map.get(volumeId)
      if (!entry) return
      // Idempotent: if no cycle is in flight (state is the baseline + no timer),
      // there's nothing to clean up and no subscribers to notify. This guards
      // against double-firing when both `runAttempt`'s success branch and the
      // `smb-connection-changed` event fire; whichever runs first wins.
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
   * `smb-connection-changed { state: "direct" }` event will arrive and
   * `handleDirect` cleans up. On failure, schedule the next attempt or give up.
   */
  private async runAttempt(volumeId: string, attemptIndex: number): Promise<void> {
    const entry = this.map.get(volumeId)
    if (!entry) return
    entry.state = { ...entry.state, status: 'attempting', attemptIndex }
    entry.timerId = null
    this.map.set(volumeId, entry) // notify subscribers

    try {
      await reconnectSmbVolume(volumeId)
      // Success: defensive backstop in case the `smb-connection-changed`
      // event somehow doesn't arrive (unexpected, but `handleDirect` is
      // idempotent so calling both paths is safe).
      this.handleDirect(volumeId)
    } catch (e) {
      log.info('Reconnect attempt {attempt} for {volumeId} failed: {error}', {
        attempt: attemptIndex + 1,
        volumeId,
        error: String(e),
      })
      // Re-fetch entry: `cancel` may have run during the attempt.
      const e2 = this.map.get(volumeId)
      if (!e2) return
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
    timerId: null,
    successCallbacks: new Set(),
  }
}

/** Singleton. Call `init()` once at app startup. */
export const smbReconnectManager = new SmbReconnectManager()

// ── Display helpers (pure; tested separately) ─────────────────────

/** "1 → once", "2 → twice", "n → n times". */
export function ordinalCount(n: number): string {
  if (n === 1) return 'once'
  if (n === 2) return 'twice'
  return `${String(n)} times`
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
    return `Retried ${retried}, this is the final attempt — will drop the connection if it fails.`
  }
  return `Retried ${retried}, will try it ${ordinalCount(remaining)} more after this.`
}
