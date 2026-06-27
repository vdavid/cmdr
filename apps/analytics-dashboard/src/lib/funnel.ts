/**
 * Client-safe pure helpers for the "Daily funnel" section. Lives outside `$lib/server` so both the
 * page (client bundle) and the server source/report code can import them; the server-only fetching
 * lives in `$lib/server/sources/funnel.ts`, which re-exports these.
 */

/** One entry in a ranked breakdown: a key (ref or referer host) and its total download count. */
export interface ChannelCount {
  ref: string
  count: number
}

/** Sum a list of per-day count maps into one ranked list, biggest first. `null`/`undefined` maps skip. */
function rankCounts(maps: (Record<string, number> | null | undefined)[]): ChannelCount[] {
  const totals = new Map<string, number>()
  for (const map of maps) {
    if (!map) continue
    for (const [ref, count] of Object.entries(map)) {
      totals.set(ref, (totals.get(ref) ?? 0) + count)
    }
  }
  return [...totals.entries()].map(([ref, count]) => ({ ref, count })).sort((a, b) => b.count - a.count)
}

/**
 * Roll the per-day `downloadsByRef` maps up into one ranked list over the whole funnel window. Days whose
 * worker data was unavailable (`null`) or had no downloads (`{}`) contribute nothing. The `"(none)"`
 * bucket (no-ref downloads: Homebrew, direct links, return visits in a later session, pre-0009 rows) is
 * just another entry, kept in the ranking so the size of "no channel" is visible next to the named ones.
 */
export function aggregateChannels(rows: { downloadsByRef: Record<string, number> | null }[]): ChannelCount[] {
  return rankCounts(rows.map((r) => r.downloadsByRef))
}

/**
 * Roll the per-day `downloadsByReferer` maps up into one ranked list over the window. This is the raw
 * `Referer` host of each `/download` hit, the signal that illuminates the `(none)` first-touch bucket:
 * direct links (AlternativeTo, directories, GitHub, Reddit, forums) carry no `ref` but do carry a
 * `Referer`. The `"(none)"` bucket here is hits with no usable referer (typed URL, privacy browser,
 * referrer-policy strip, Homebrew/curl, pre-0010 rows).
 */
export function aggregateReferers(rows: { downloadsByReferer: Record<string, number> | null }[]): ChannelCount[] {
  return rankCounts(rows.map((r) => r.downloadsByReferer))
}
