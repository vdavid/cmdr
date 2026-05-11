/**
 * Pure helpers for mapping a file's modified date to one of five age tiers
 * (fresh / recent / aging / old / ancient), used by the `appearance.dateColors`
 * setting to color modified-date displays.
 *
 * `modifiedAt` is a Unix timestamp **in seconds** (matching the FileEntry
 * convention). The `nowMs` parameter is in milliseconds (matching `Date.now`).
 */

const DAY_MS = 24 * 60 * 60 * 1000
const YEAR_MS = 365 * DAY_MS

/** Upper-bound (exclusive) age in milliseconds for each non-ancient tier. */
export const AGE_THRESHOLDS_MS = {
  fresh: 30 * DAY_MS,
  recent: YEAR_MS,
  aging: 2 * YEAR_MS,
  old: 3 * YEAR_MS,
}

export type AgeTierClass = 'age-fresh' | 'age-recent' | 'age-aging' | 'age-old' | 'age-ancient'

/**
 * Returns the age tier class for a file's modified date, or `null` when the
 * timestamp is missing. Future-dated files clamp to `age-fresh` so clock skew
 * and timezone quirks never produce broken tiers.
 */
export function tierClassForAge(
  modifiedAtSeconds: number | null | undefined,
  nowMs: number = Date.now(),
): AgeTierClass | null {
  if (modifiedAtSeconds == null || !Number.isFinite(modifiedAtSeconds)) return null
  const ageMs = nowMs - modifiedAtSeconds * 1000
  if (ageMs < AGE_THRESHOLDS_MS.fresh) return 'age-fresh'
  if (ageMs < AGE_THRESHOLDS_MS.recent) return 'age-recent'
  if (ageMs < AGE_THRESHOLDS_MS.aging) return 'age-aging'
  if (ageMs < AGE_THRESHOLDS_MS.old) return 'age-old'
  return 'age-ancient'
}
