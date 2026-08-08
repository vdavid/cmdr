/**
 * The shared display rules for what a running operation's progress READS like.
 *
 * There is exactly one definition of each number a user sees, and both surfaces
 * that show an operation render it:
 *
 * - **Speed** is the backend's `write-progress.bytesPerSecond` (`EtaEstimator`
 *   in `src-tauri/src/file_system/write_operations/eta.rs`), rendered through
 *   the `fileOperations.shared.byteRate` catalog phrasing over a `<Size>`
 *   (the per-second marker is user-facing copy, so it lives in the catalog;
 *   `$lib/units` owns the number). The frontend does NOT compute a second,
 *   instantaneous rate for the active phase. The one frontend-computed rate is
 *   `ScanThroughput`, which covers the SCAN phase only, where the backend emits
 *   no rate at all; its readout is labelled as counting progress, never as
 *   transfer speed.
 * - **ETA** is the backend's `write-progress.etaSeconds`, put through
 *   {@link createEtaSmoother} for display, then `formatDuration`.
 *
 * The smoother is stateful, which is exactly why it lives here rather than
 * being re-implemented per window: the copy dialog smoothed the ETA while the
 * operation queue window rendered it raw, so one operation showed "8m 12s remaining"
 * in one window and "5m 46s" in the other at the same moment.
 *
 * Consumers: `transfer/transfer-progress-state.svelte.ts` (the copy dialog) and
 * `queue/operations-store.svelte.ts` (the operation queue window).
 */

import { bytes, bytesPerSecond, seconds, type ByteCount, type BytesPerSecond, type Seconds } from '$lib/units'

/**
 * The numbers a `write-progress` event carries, branded.
 *
 * The event's fields all arrive from IPC as bare `number`s sitting next to each
 * other — `bytesDone`, `filesDone`, `bytesPerSecond`, `etaSeconds`. Branding
 * them here, once, at the boundary is what stops an ETA reaching a size
 * formatter or a rate reaching `<Size>`: the mistakes that render a
 * plausible-looking wrong number instead of failing loudly.
 */
export interface TransferReadout {
  bytesDone: ByteCount
  bytesTotal: ByteCount
  /** Backend `EtaEstimator` rate; `null` during its warm-up window. */
  bytesPerSecond: BytesPerSecond | null
  /** Raw backend ETA. Put it through {@link createEtaSmoother} before display. */
  etaSeconds: Seconds | null
}

/** Brand one `write-progress` payload's numbers. Call this at the event edge. */
export function transferReadout(event: {
  bytesDone: number
  bytesTotal: number
  bytesPerSecond?: number | null
  etaSeconds?: number | null
}): TransferReadout {
  return {
    bytesDone: bytes(event.bytesDone),
    bytesTotal: bytes(event.bytesTotal),
    bytesPerSecond: event.bytesPerSecond == null ? null : bytesPerSecond(event.bytesPerSecond),
    etaSeconds: event.etaSeconds == null ? null : seconds(event.etaSeconds),
  }
}

/**
 * Share of the gap to the latest backend value that the display ETA closes per
 * tick. Real changes still propagate fast (progress ticks land every ~200 ms,
 * so four ticks is under a second), while single-tick jitter is damped.
 */
export const ETA_SMOOTHING_FACTOR = 0.25

export interface EtaSmoother {
  /** Feed the latest raw backend ETA; returns the value to display. `null` clears. */
  push: (rawSeconds: Seconds | null) => Seconds | null
  /** The current display value, without feeding a new sample. */
  readonly value: Seconds | null
  /** Drop the history, so the next sample is adopted as-is. */
  reset: () => void
}

/**
 * Smooth a backend ETA for display. Display-only: the backend estimator stays
 * unsmoothed and reacts to real changes immediately.
 *
 * A `null` sample clears the state, which is what a phase transition wants —
 * the backend estimator resets there, so the displayed number should re-warm
 * with it instead of dragging the previous phase's value along.
 */
export function createEtaSmoother(): EtaSmoother {
  let current: Seconds | null = null

  return {
    push(rawSeconds: Seconds | null): Seconds | null {
      if (rawSeconds === null) {
        current = null
      } else if (current === null) {
        current = rawSeconds
      } else {
        current = seconds(current + ETA_SMOOTHING_FACTOR * (rawSeconds - current))
      }
      return current
    },
    get value(): Seconds | null {
      return current
    },
    reset(): void {
      current = null
    },
  }
}
