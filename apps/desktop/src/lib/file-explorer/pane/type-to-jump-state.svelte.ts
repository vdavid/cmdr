/**
 * Reactive state for the type-to-jump feature. One factory instance per pane.
 *
 * Holds the buffer the user is currently typing, the indicator's visibility and
 * stale flag, plus the two timers that drive the timeouts. The factory exposes a
 * small surface (`appendChar`, `clear`, plus reactive getters) so `FilePane` doesn't
 * have to care about timer plumbing and so unit tests can drive it directly.
 *
 * ## Timer model
 *
 * - **Buffer reset**: configurable (default 1000 ms via `getResetMs`). After this
 *   delay since the last keystroke, the buffer clears but the indicator stays
 *   visible in a "stale" state so the user can see at a glance that the next
 *   keystroke starts fresh.
 * - **Indicator hide**: hardcoded 5000 ms since the last keystroke. Hides the
 *   indicator entirely. The two timers are independent; the indicator-hide timer
 *   always wins eventually because it runs longer.
 *
 * ## Generation counter (race protection)
 *
 * `appendChar` returns a generation number that callers attach to their async
 * match call. When the response comes back, the caller checks against
 * `getGeneration()` and discards the result if a newer keystroke has fired since.
 * Same pattern as `adjust-selection-indices.ts`'s `diffGeneration`.
 */

/** Hardcoded indicator-hide timeout. Cosmetic; not user-configurable per the plan. */
export const INDICATOR_HIDE_MS = 5_000

export interface TypeToJumpStateOptions {
  /** Returns the current buffer-reset timeout in ms. Read on each keystroke so a live setting change takes effect. */
  getResetMs: () => number
  /** Called when a keystroke is appended and the buffer has updated. Receives the new buffer and a generation tag. */
  onMatch: (buffer: string, generation: number) => void
  /** Called when the indicator-hide timer fires. Lets the parent run side-effects on disappearance if needed. */
  onIndicatorHide?: () => void
  /** Optional logger for diagnostics. Receives a short message. */
  log?: (message: string) => void
}

export interface TypeToJumpState {
  /** Current buffer (what the user has typed since the last reset). */
  readonly buffer: string
  /** Whether the indicator is currently shown. */
  readonly indicatorVisible: boolean
  /** Whether the indicator is in its "stale" state (buffer reset fired but indicator still visible). */
  readonly indicatorStale: boolean
  /** Generation tag of the latest keystroke. Used to discard out-of-order async responses. */
  readonly generation: number
  /** Appends a single character to the buffer and (re)starts the timers. Returns the new generation tag. */
  appendChar: (char: string) => number
  /** Clears the buffer + indicator + timers immediately. Idempotent. */
  clear: () => void
  /**
   * Stops any pending timers so they can't fire after the owning component is gone.
   * Call from the component's destroy hook. Idempotent; safe to call alongside `clear`.
   * After `dispose`, `appendChar` would schedule new timers — don't call it post-dispose.
   */
  dispose: () => void
}

export function createTypeToJumpState(options: TypeToJumpStateOptions): TypeToJumpState {
  const { getResetMs, onMatch, onIndicatorHide, log } = options

  let buffer = $state('')
  let indicatorVisible = $state(false)
  let indicatorStale = $state(false)
  let generation = $state(0)

  // Timers live outside $state — they're handles, not reactive values.
  let bufferResetTimer: ReturnType<typeof setTimeout> | null = null
  let indicatorHideTimer: ReturnType<typeof setTimeout> | null = null

  function clearTimers() {
    if (bufferResetTimer !== null) {
      clearTimeout(bufferResetTimer)
      bufferResetTimer = null
    }
    if (indicatorHideTimer !== null) {
      clearTimeout(indicatorHideTimer)
      indicatorHideTimer = null
    }
  }

  function scheduleTimers() {
    clearTimers()

    // Buffer reset: clears `buffer` but keeps `indicatorVisible` true and flips
    // `indicatorStale` so the visual cue tells the user the next keystroke
    // starts fresh.
    bufferResetTimer = setTimeout(
      () => {
        buffer = ''
        indicatorStale = true
        bufferResetTimer = null
        log?.('type-to-jump: buffer reset (stale)')
      },
      Math.max(0, getResetMs()),
    )

    // Indicator hide: cosmetic, removes the indicator entirely.
    indicatorHideTimer = setTimeout(() => {
      buffer = ''
      indicatorVisible = false
      indicatorStale = false
      indicatorHideTimer = null
      onIndicatorHide?.()
      log?.('type-to-jump: indicator hidden')
    }, INDICATOR_HIDE_MS)
  }

  function appendChar(char: string): number {
    // If the previous buffer was already stale, treat this keystroke as a fresh
    // start — the visible buffer was already conceptually empty.
    if (indicatorStale) {
      buffer = ''
      indicatorStale = false
    }
    buffer = buffer + char.toLowerCase()
    indicatorVisible = true
    generation = generation + 1
    scheduleTimers()
    onMatch(buffer, generation)
    return generation
  }

  function clear() {
    clearTimers()
    buffer = ''
    indicatorVisible = false
    indicatorStale = false
    // Don't bump the generation here — that's reserved for new keystrokes.
    // Out-of-order responses from before the clear will still apply against the
    // (now-empty) buffer, but the caller's downstream check (`buffer !== ''`)
    // gates that.
  }

  function dispose() {
    // Stop pending timers without resetting reactive fields — the owning
    // component is on its way out so its $state slots are about to be GC'd
    // anyway. Mirrors the pattern other Svelte factories use for cleanup.
    clearTimers()
  }

  return {
    get buffer() {
      return buffer
    },
    get indicatorVisible() {
      return indicatorVisible
    },
    get indicatorStale() {
      return indicatorStale
    },
    get generation() {
      return generation
    },
    appendChar,
    clear,
    dispose,
  }
}
