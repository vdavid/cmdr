/**
 * Pure ETA helpers for the drive-indexing status indicator.
 *
 * These functions carry no reactive state: the component owns the sliding-window
 * snapshots and feeds them through here. Keeping the math pure lets us unit-test the
 * thresholds and blending without mounting a component.
 */

import { tString } from '$lib/intl/messages.svelte'

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
  // Non-finite guard: every planned caller null-gates before reaching here, but the scan
  // branch is a new caller and a future edit dropping that gate would surface "Infinitym left".
  if (!Number.isFinite(seconds) || seconds < 2) return tString('indexing.eta.almostDone')
  if (seconds < 60) return tString('indexing.eta.secondsLeft', { secondsText: String(Math.round(seconds)) })
  return tString('indexing.eta.minutesLeft', { minutesText: String(Math.round(seconds / 60)) })
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

/** The tier-1 (calibrated) progress ceiling. The prior scan's total is approximate for
 * THIS disk state, so we never report a full 100% mid-scan: 99% paired with "Almost done"
 * is honest, 100% with work left is a lie. */
export const SCAN_PROGRESS_CALIBRATED_MAX = 0.99

/** The tier-2 (rough, first-scan) progress ceiling. Lower than tier 1 because its error band
 * is wider: APFS clones make the per-file physical-byte sum overshoot the statfs used-bytes
 * denominator by up to ~20%, so a clone-heavy disk would hit 100% with minutes still left. */
export const SCAN_PROGRESS_ROUGH_MAX = 0.95

/** A two-tier scan progress fraction with a flag for the rough (first-scan) tier. */
export interface ScanProgress {
  fraction: number
  rough: boolean
}

/**
 * Compute drive-scan progress as a clamped fraction, choosing the tier from the available
 * denominators.
 *
 * - **Tier 1 (calibrated)**: when the previous completed scan's entry total is known,
 *   `entriesScanned / priorTotalEntries`, clamped to `SCAN_PROGRESS_CALIBRATED_MAX`. Both sides
 *   come from the same instrument (the scan's own entry counter), so this is apples-to-apples.
 * - **Tier 2 (rough)**: the first scan has no prior total, so `bytesScanned / volumeUsedBytes`,
 *   clamped to the lower `SCAN_PROGRESS_ROUGH_MAX` and flagged `rough`. Some honest signal beats
 *   none during onboarding.
 * - Neither denominator available (or both zero) → `null`: the caller falls back to a
 *   counter-only tooltip.
 */
export function computeScanProgress(
  entriesScanned: number,
  bytesScanned: number,
  priorTotalEntries: number | null,
  volumeUsedBytes: number | null,
): ScanProgress | null {
  if (priorTotalEntries != null && priorTotalEntries > 0) {
    const fraction = Math.min(SCAN_PROGRESS_CALIBRATED_MAX, entriesScanned / priorTotalEntries)
    return { fraction, rough: false }
  }
  if (volumeUsedBytes != null && volumeUsedBytes > 0) {
    const fraction = Math.min(SCAN_PROGRESS_ROUGH_MAX, bytesScanned / volumeUsedBytes)
    return { fraction, rough: true }
  }
  return null
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
