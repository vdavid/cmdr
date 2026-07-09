/**
 * Level-scaling for the compressed-size estimate.
 *
 * The backend samples every file once at deflate reference level 6 and ships
 * the estimate as three per-compressibility-class subtotals of estimated
 * level-6 bytes (`CompressedSizeEstimate`). This helper re-scales that estimate
 * to the user's selected level (1..9) arithmetically, so moving the Compress
 * dialog's level slider updates the shown number with NO re-scan and NO IPC.
 *
 * The per-class multiplier table is baked from the measurement spike
 * (`docs/notes/compress-size-estimate-spike.md`, § "The measured level-scaling
 * curve"). Anchors were measured at levels 1, 3, 6, and 9 for each class;
 * levels 2, 4, 5, 7, and 8 are linear interpolations between the nearest
 * anchors (the curves are smooth and monotonic, and the estimate is already
 * explicitly approximate). Level 6 is the reference (all multipliers 1.0), so at
 * level 6 the scaled total equals the plain sum of the three subtotals.
 *
 * Side finding from the spike: with the app's `flate2`/`miniz_oxide` backend
 * levels 6..9 differ by under 0.5%, so the visible movement is almost entirely
 * on the "Faster" (levels 1..4) half.
 */

import type { CompressedSizeEstimate } from '$lib/ipc/bindings'

/** Multiplier applied to a class's level-6 bytes, indexed by `level - 1`. */
const LEVEL_CURVE: Record<'compressible' | 'medium' | 'incompressible', readonly number[]> = {
  // ratio < 0.35: the "Faster" end inflates output the most.
  compressible: [1.448, 1.245, 1.042, 1.028, 1.014, 1.0, 0.999, 0.998, 0.997],
  // 0.35 <= ratio < 0.8
  medium: [1.104, 1.061, 1.017, 1.011, 1.006, 1.0, 1.0, 0.999, 0.999],
  // ratio >= 0.8: already-compressed content barely moves across levels.
  incompressible: [1.002, 1.001, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
}

/**
 * Scales a level-6 per-class estimate to `level` (clamped to 1..9) and returns
 * the total estimated compressed bytes for that level.
 */
export function scaleCompressedEstimate(estimate: CompressedSizeEstimate, level: number): number {
  const index = Math.min(9, Math.max(1, Math.round(level))) - 1
  return (
    estimate.compressibleBytes * LEVEL_CURVE.compressible[index] +
    estimate.mediumBytes * LEVEL_CURVE.medium[index] +
    estimate.incompressibleBytes * LEVEL_CURVE.incompressible[index]
  )
}
