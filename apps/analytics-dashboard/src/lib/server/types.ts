/**
 * What window the dashboard's aggregate sections cover. The named ranges are relative windows ending
 * now; `day` is a single specific UTC calendar day, carried separately in `DashboardSelection.day`.
 */
export type TimeRange = 'today' | '24h' | '7d' | '30d' | 'day'

/** The ranges shown as buttons in the picker, in display order. `day` is set via a date input, not a button. */
export const rangeButtons = ['today', '24h', '7d', '30d'] as const

/**
 * The resolved time selection for a page load: the range plus, when `range === 'day'`, the specific UTC
 * day (`YYYY-MM-DD`) it refers to. Sources convert this to their own native windows via `toTimeWindow`.
 */
export interface DashboardSelection {
  range: TimeRange
  /** Set only when `range === 'day'`: the specific UTC day the sections cover. */
  day: string | null
}

/** Matches a `YYYY-MM-DD` string (a basic shape check; not a full calendar validation). */
export function isIsoDay(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(value)
}

/**
 * Resolve the URL's `range` and `day` params into a `DashboardSelection`. A valid `day` param wins:
 * it forces `range: 'day'` regardless of the `range` param, so a shared single-day link is stable.
 * Otherwise the `range` param is used if it's a known relative range, defaulting to `7d`.
 */
export function resolveSelection(rangeParam: string | null, dayParam: string | null): DashboardSelection {
  if (dayParam && isIsoDay(dayParam)) return { range: 'day', day: dayParam }
  const relative = new Set<TimeRange>(['today', '24h', '7d', '30d'])
  const range = rangeParam && relative.has(rangeParam as TimeRange) ? (rangeParam as TimeRange) : '7d'
  return { range, day: null }
}

/**
 * Converts a selection to a start/end timestamp pair (milliseconds, UTC). `today` and `day` snap to UTC
 * calendar-day boundaries; `today` runs from UTC midnight to now (a partial day), a specific `day` spans
 * its full 24h. The relative ranges (`24h`/`7d`/`30d`) are rolling windows ending now.
 */
export function toTimeWindow(selection: DashboardSelection): { startAt: number; endAt: number } {
  const now = Date.now()
  const msPerDay = 86_400_000

  if (selection.range === 'today') {
    const d = new Date(now)
    const startAt = Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate())
    return { startAt, endAt: now }
  }

  if (selection.range === 'day' && selection.day) {
    const startAt = Date.parse(`${selection.day}T00:00:00Z`)
    return { startAt, endAt: startAt + msPerDay }
  }

  const durationMs: Record<'24h' | '7d' | '30d', number> = {
    '24h': msPerDay,
    '7d': 7 * msPerDay,
    '30d': 30 * msPerDay,
  }
  // Falls back to 7d for `today`/`day` without the data they need (shouldn't happen given the branches above).
  const duration = durationMs[selection.range as '24h' | '7d' | '30d'] ?? 7 * msPerDay
  return { startAt: now - duration, endAt: now }
}

/** A stable cache key for a selection: the range name, or `day:YYYY-MM-DD` for a specific day. */
export function selectionCacheKey(selection: DashboardSelection): string {
  return selection.range === 'day' && selection.day ? `day:${selection.day}` : selection.range
}

/**
 * Map a selection to the coarse range string the worker admin endpoints and PostHog understand
 * (`24h`/`7d`/`30d`/`90d`). Those sources can't isolate `today` or one past day cheaply, so both snap to
 * `24h` (the nearest single-day-ish window). The funnel table is the real per-day server view; sections
 * built on these coarse sources note that they fall back to ~24h on a single-day selection.
 */
export function selectionToWorkerRange(selection: DashboardSelection): '24h' | '7d' | '30d' {
  if (selection.range === '7d' || selection.range === '30d') return selection.range
  // 'today', '24h', and a specific 'day' all map to the coarse 24h window.
  return '24h'
}

/** Wraps a data source result. Either the data or an error message for the UI. */
export type SourceResult<T> = { ok: true; data: T } | { ok: false; error: string }
