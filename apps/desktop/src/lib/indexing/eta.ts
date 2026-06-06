/**
 * Pure ETA helpers for the drive-indexing status indicator.
 *
 * These functions carry no reactive state: the component owns the sliding-window
 * snapshots and feeds them through here. Keeping the math pure lets us unit-test the
 * thresholds and blending without mounting a component.
 */

/** A point sample of replay progress, used by the sliding-window rate estimate. */
export interface EtaSnapshot {
  timestamp: number
  eventsProcessed: number
}

/**
 * Format an ETA in seconds to a short human-readable string.
 *
 * Thresholds: under two seconds reads "Almost done" (a precise countdown there is noise),
 * under a minute counts down in seconds, otherwise rounds to whole minutes.
 */
export function formatEta(seconds: number): string {
  if (seconds < 2) return 'Almost done'
  if (seconds < 60) return `${String(Math.round(seconds))}s left`
  return `${String(Math.round(seconds / 60))}m left`
}

/**
 * Estimate seconds remaining by extrapolating from elapsed time and the work-done ratio.
 *
 * Returns `null` when there's nothing to extrapolate from yet (no elapsed time or no
 * progress), so callers can fall back to another estimate.
 */
export function computeElapsedEta(elapsedSeconds: number, done: number, remaining: number): number | null {
  if (elapsedSeconds <= 0 || done <= 0) return null
  return elapsedSeconds * (remaining / done)
}

/**
 * Estimate seconds remaining from a sliding window of recent progress snapshots.
 *
 * Needs at least two samples to derive a rate. Returns `null` when the rate can't be
 * computed (too few samples, zero-width window, or a non-positive rate), so early
 * extrapolation alone — which is wildly wrong at the start — doesn't dominate.
 */
export function computeWindowEta(snapshots: EtaSnapshot[], remaining: number): number | null {
  if (snapshots.length < 2) return null
  const oldest = snapshots[0]
  const newest = snapshots[snapshots.length - 1]
  const windowElapsed = (newest.timestamp - oldest.timestamp) / 1000
  if (windowElapsed <= 0) return null
  const windowRate = (newest.eventsProcessed - oldest.eventsProcessed) / windowElapsed
  return windowRate > 0 ? remaining / windowRate : null
}

/** Blend two ETA estimates 50-50, falling back to whichever one is available. */
export function blendEtas(a: number | null, b: number | null): number | null {
  if (a != null && b != null) return (a + b) / 2
  return a ?? b
}

/**
 * Prune snapshots older than `windowMs` before the most recent one, keeping the window
 * bounded. Returns the same array when nothing needs pruning so callers can skip a write.
 */
export function pruneSnapshots(snapshots: EtaSnapshot[], windowMs: number): EtaSnapshot[] {
  if (snapshots.length === 0) return snapshots
  const cutoff = snapshots[snapshots.length - 1].timestamp - windowMs
  const firstValidIndex = snapshots.findIndex((s) => s.timestamp >= cutoff)
  return firstValidIndex > 0 ? snapshots.slice(firstValidIndex) : snapshots
}
