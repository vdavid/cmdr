/**
 * Timestamp helpers for fixtures that display dates.
 *
 * Dates are relative to "now", not hardcoded: Cmdr colors a date by its AGE
 * (`appearance.dateColors`, see `DateLabel.svelte`), so a pinned calendar date
 * would drift into the oldest tier and the gallery would stop showing the
 * palette the dialog is designed around.
 */

/** Unix seconds, `days` days before now. */
export function daysAgo(days: number): number {
  return Math.floor((Date.now() - days * 24 * 60 * 60 * 1000) / 1000)
}

/** Unix seconds, `hours` hours before now. */
export function hoursAgo(hours: number): number {
  return Math.floor((Date.now() - hours * 60 * 60 * 1000) / 1000)
}
