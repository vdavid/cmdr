/**
 * Picks the right singular or plural form based on count.
 *
 * Defaults to appending `'s'` for the plural. Pass an explicit `plural` only
 * for irregular forms (`entry`/`entries`, `directory`/`directories`).
 *
 * Use this everywhere a log line, error message, or user-facing string
 * interpolates a count followed by a noun. Hand-rolled `${n} files` reads as
 * `1 files` when `n === 1`. The `pluralize-noun` check catches new
 * occurrences in CI.
 *
 * @example
 *   `${count} ${pluralize(count, 'file')}`         // "1 file" or "3 files"
 *   `${count} ${pluralize(count, 'entry', 'entries')}` // "1 entry" or "3 entries"
 */
export function pluralize(count: number, singular: string, plural?: string): string {
  if (count === 1) return singular
  return plural ?? `${singular}s`
}
