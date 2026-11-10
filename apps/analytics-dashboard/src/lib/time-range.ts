/**
 * Client-safe: the dashboard's time ranges, shared by the layout's picker (browser) and the server's
 * selection resolver (`#lib/server/types.ts` re-exports these).
 */

/** What window the dashboard's aggregate sections cover: a rolling window of whole days ending now. */
export type TimeRange = '7d' | '30d' | '90d'

/** The picker's ranges, in display order. */
export const timeRanges: readonly TimeRange[] = ['7d', '30d', '90d']

/** The range a page opens on when the URL names none (or an unknown one). */
export const defaultRange: TimeRange = '30d'

/** How many days each range spans. */
export const rangeDays: Record<TimeRange, number> = { '7d': 7, '30d': 30, '90d': 90 }
