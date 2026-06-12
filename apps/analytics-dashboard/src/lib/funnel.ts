/**
 * Client-safe pure helpers for the "Daily funnel" section. Lives outside `$lib/server` so both the
 * page (client bundle) and the server source/report code can import them; the server-only fetching
 * lives in `$lib/server/sources/funnel.ts`, which re-exports these.
 */

/** One channel in the ranked breakdown: a ref value and its total download count over the window. */
export interface ChannelCount {
  ref: string
  count: number
}

/**
 * Roll the per-day `downloadsByRef` maps up into one ranked list over the whole funnel window, summing
 * each ref's counts across days and sorting biggest first. Days whose worker data was unavailable
 * (`null`) or had no downloads (`{}`) contribute nothing. The `"(none)"` bucket (no-ref downloads:
 * Homebrew, direct links, return visits in a later session, pre-0009 rows) is just another entry, kept
 * in the ranking so the size of "no channel" is visible next to the named channels.
 */
export function aggregateChannels(rows: { downloadsByRef: Record<string, number> | null }[]): ChannelCount[] {
  const totals = new Map<string, number>()
  for (const row of rows) {
    if (!row.downloadsByRef) continue
    for (const [ref, count] of Object.entries(row.downloadsByRef)) {
      totals.set(ref, (totals.get(ref) ?? 0) + count)
    }
  }
  return [...totals.entries()].map(([ref, count]) => ({ ref, count })).sort((a, b) => b.count - a.count)
}
