/**
 * Client-safe: the dashboard's time ranges and the window math behind the picker's back/forward steps,
 * shared by the layout (browser) and the server's selection resolver (`#lib/server/types.ts` re-exports
 * these). Days are UTC `YYYY-MM-DD` strings throughout.
 */

/** What window the dashboard's aggregate sections cover: a run of whole days. */
export type TimeRange = '7d' | '30d' | '90d'

/** The picker's ranges, in display order. */
export const timeRanges: readonly TimeRange[] = ['7d', '30d', '90d']

/** The range a page opens on when the URL names none (or an unknown one). */
export const defaultRange: TimeRange = '30d'

/** How many days each range spans. */
export const rangeDays: Record<TimeRange, number> = { '7d': 7, '30d': 30, '90d': 90 }

/** The trend charts' rolling line: how many days each point pools, so a window needs that many minus one before it. */
export const rollingDays = 7

/**
 * How many UTC days the funnel usually fetches: 180 complete days plus today (partial), so the 90-day stats
 * column has a full previous 90 days to compare against.
 */
export const funnelFetchDays = 181

/** The deepest funnel fetch, for a window stepped back past `funnelFetchDays`: the worker's `?days=` ceiling. */
export const funnelMaxFetchDays = 400

/** The days in view. `current` is the window ending today: its complete days plus today, partial. */
export interface ViewWindow {
  start: string
  /** The last day drawn, inclusive: today for the current window. */
  end: string
  current: boolean
}

/** The UTC day `days` days after (negative: before) `day`. */
export function shiftDay(day: string, days: number): string {
  return new Date(Date.parse(`${day}T00:00:00Z`) + days * 86_400_000).toISOString().slice(0, 10)
}

/** How many days `to` is after `from`. */
function daysBetween(from: string, to: string): number {
  return Math.round((Date.parse(`${to}T00:00:00Z`) - Date.parse(`${from}T00:00:00Z`)) / 86_400_000)
}

/**
 * The window a range and `end` describe. `end: null` is the current window: the range's complete days
 * plus today (partial), as the charts always drew it. A past window is exactly the range, ending `end`.
 */
export function viewWindow(range: TimeRange, end: string | null, today: string): ViewWindow {
  const days = rangeDays[range]
  if (end === null) return { start: shiftDay(today, -days), end: today, current: true }
  return { start: shiftDay(end, -(days - 1)), end, current: false }
}

/** The window as the header shows it, like "2026-09-11 – 2026-10-10". */
export function formatWindow(window: ViewWindow): string {
  return `${window.start} – ${window.end}`
}

/** The oldest day a window may start on: the deepest funnel fetch must still hold its rolling line. */
export function earliestStart(today: string): string {
  return shiftDay(today, -(funnelMaxFetchDays - 1) + (rollingDays - 1))
}

/**
 * The URL's `end` as a past window's last day, or null for the current window (no `end`, a malformed or
 * impossible day, or yesterday onwards, which the current window already covers). A day too old for the
 * history moves up to the oldest window that fits.
 */
export function normalizeEnd(range: TimeRange, raw: string | null, today: string): string | null {
  if (!raw || !/^\d{4}-\d{2}-\d{2}$/.test(raw) || Number.isNaN(Date.parse(`${raw}T00:00:00Z`))) return null
  if (shiftDay(raw, 0) !== raw) return null
  if (raw >= shiftDay(today, -1)) return null
  const oldestEnd = shiftDay(earliestStart(today), rangeDays[range] - 1)
  return raw < oldestEnd ? oldestEnd : raw
}

/**
 * The `end` one step (the range's length) back or forward, null for the current window, or `undefined`
 * when there's no step that way: forward from the current window, or back past `earliestStart`.
 */
export function stepEnd(
  range: TimeRange,
  end: string | null,
  today: string,
  direction: 'back' | 'forward',
): string | null | undefined {
  const days = rangeDays[range]
  const yesterday = shiftDay(today, -1)
  if (direction === 'forward') {
    if (end === null) return undefined
    const next = shiftDay(end, days)
    return next >= yesterday ? null : next
  }
  const next = shiftDay(end ?? yesterday, -days)
  return viewWindow(range, next, today).start < earliestStart(today) ? undefined : next
}

/** How many days the funnel must fetch (ending today) to draw `window` with its rolling line. */
export function funnelDaysFor(window: ViewWindow, today: string): number {
  const needed = daysBetween(shiftDay(window.start, -(rollingDays - 1)), today) + 1
  return needed <= funnelFetchDays ? funnelFetchDays : funnelMaxFetchDays
}

/** A page's query string for a selection and the Acquisition page's country filter. */
export function selectionQuery(selection: { range: TimeRange; end: string | null }, country: string | null): string {
  const params = [`range=${selection.range}`]
  if (selection.end) params.push(`end=${selection.end}`)
  if (country) params.push(`country=${encodeURIComponent(country)}`)
  return `?${params.join('&')}`
}
