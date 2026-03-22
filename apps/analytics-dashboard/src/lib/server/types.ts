/** Time range for dashboard queries. All sources convert this to their native format. */
export type TimeRange = '24h' | '7d' | '30d'

/** Converts a TimeRange to a start/end timestamp pair (milliseconds). */
export function toTimeWindow(range: TimeRange): { startAt: number; endAt: number } {
    const endAt = Date.now()
    const msPerDay = 86_400_000
    const durationMs: Record<TimeRange, number> = {
        '24h': msPerDay,
        '7d': 7 * msPerDay,
        '30d': 30 * msPerDay,
    }
    return { startAt: endAt - durationMs[range], endAt }
}

/**
 * Converts a TimeRange to an Analytics Engine SQL interval string.
 * For example, '7d' becomes `'7' DAY`.
 */
export function toSqlInterval(range: TimeRange): string {
    const intervalMap: Record<TimeRange, string> = {
        '24h': "'1' DAY",
        '7d': "'7' DAY",
        '30d': "'30' DAY",
    }
    return intervalMap[range]
}

/** Wraps a data source result. Either the data or an error message for the UI. */
export type SourceResult<T> =
    | { ok: true; data: T }
    | { ok: false; error: string }
