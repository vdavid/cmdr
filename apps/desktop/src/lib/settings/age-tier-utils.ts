/**
 * Pure helpers for mapping a file's modified date to per-component age tiers
 * (year, month, day, time), used by the `appearance.dateColors` setting to
 * color the matching segment of a formatted date.
 *
 * Each component decides independently whether to color and which tier to use,
 * based on how close the file's component is to "now" in that same unit:
 *
 * - `tierForYear` colors any year. Current year → fresh, last → recent, 2 ago
 *   → aging, 3+ ago → old.
 * - `tierForMonth` only colors when the file's year equals the current year.
 *   Same scale as the year (0 / 1 / 2 / 3+).
 * - `tierForDay` only colors when the file's month and year both equal the
 *   current ones. Today / yesterday / 2 days / 3+ days ago.
 * - `tierForTime` only colors when the file's date equals today. Within last 1
 *   hour / 2 hours / 3 hours / 3+ hours.
 *
 * Returning `null` means "render without an age span" — the segment inherits
 * the parent's text color.
 *
 * `modifiedAt` is a Unix timestamp **in seconds** (matching the FileEntry
 * convention). `nowMs` is in milliseconds (matching `Date.now`). Future-dated
 * files clamp to the freshest tier so clock skew never produces broken tiers.
 */

const MS_PER_HOUR = 60 * 60 * 1000

export type AgeTierClass = 'age-fresh' | 'age-recent' | 'age-aging' | 'age-old'

/** Maps a 0-based "units ago" distance to a tier class. 3+ → `age-old`. */
function tierForDistance(distance: number): AgeTierClass {
  if (distance <= 0) return 'age-fresh'
  if (distance === 1) return 'age-recent'
  if (distance === 2) return 'age-aging'
  return 'age-old'
}

function toDate(modifiedAtSeconds: number | null | undefined): Date | null {
  if (modifiedAtSeconds == null || !Number.isFinite(modifiedAtSeconds)) return null
  return new Date(modifiedAtSeconds * 1000)
}

/**
 * Year tier — colors every year. Current → fresh, last → recent, 2 ago →
 * aging, 3+ → old. Future years clamp to fresh.
 */
export function tierForYear(
  modifiedAtSeconds: number | null | undefined,
  nowMs: number = Date.now(),
): AgeTierClass | null {
  const d = toDate(modifiedAtSeconds)
  if (!d) return null
  const now = new Date(nowMs)
  const distance = now.getFullYear() - d.getFullYear()
  return tierForDistance(distance)
}

/**
 * Month tier — only colors when the file's year equals the current year.
 * Distance is the difference in calendar months. Future months in the same
 * year clamp to fresh.
 */
export function tierForMonth(
  modifiedAtSeconds: number | null | undefined,
  nowMs: number = Date.now(),
): AgeTierClass | null {
  const d = toDate(modifiedAtSeconds)
  if (!d) return null
  const now = new Date(nowMs)
  if (d.getFullYear() !== now.getFullYear()) return null
  const distance = now.getMonth() - d.getMonth()
  return tierForDistance(distance)
}

/**
 * Day tier — only colors when the file's year and month equal the current
 * ones. Distance is in calendar days within that month. Future days clamp to
 * fresh.
 */
export function tierForDay(
  modifiedAtSeconds: number | null | undefined,
  nowMs: number = Date.now(),
): AgeTierClass | null {
  const d = toDate(modifiedAtSeconds)
  if (!d) return null
  const now = new Date(nowMs)
  if (d.getFullYear() !== now.getFullYear() || d.getMonth() !== now.getMonth()) return null
  const distance = now.getDate() - d.getDate()
  return tierForDistance(distance)
}

/**
 * Time tier — only colors when the file's date (year/month/day) equals today.
 * Distance is the number of full hours between the file and now. Future
 * timestamps within today clamp to fresh.
 */
export function tierForTime(
  modifiedAtSeconds: number | null | undefined,
  nowMs: number = Date.now(),
): AgeTierClass | null {
  const d = toDate(modifiedAtSeconds)
  if (!d) return null
  const now = new Date(nowMs)
  if (d.getFullYear() !== now.getFullYear() || d.getMonth() !== now.getMonth() || d.getDate() !== now.getDate()) {
    return null
  }
  const distance = Math.floor((nowMs - d.getTime()) / MS_PER_HOUR)
  return tierForDistance(distance)
}
