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

/** Per-day download split by User-Agent family, as the worker classifies it. See the api-server `classifyUaFamily`. */
export interface UaFamilyCounts {
  human: number
  bot: number
  unknown: number
}

/** Window totals of the UA-family download split, plus the derived human-installs headline. */
export interface UaFamilyTotals extends UaFamilyCounts {
  /** All classified downloads in the window (`human + bot + unknown`). */
  total: number
  /**
   * Downloads minus the provably-impossible `bot` (non-macOS UA) hits, i.e. `human + unknown`. The
   * honest install signal: it drops only the clearly-fake downloads (a Windows/Linux/Android client
   * can't run a macOS `.dmg`) and keeps every ambiguous one, so it never overclaims.
   */
  humanInstalls: number
}

/**
 * Sum the per-day `downloadsByUaFamily` splits into window totals and derive `humanInstalls`. Days whose
 * worker data was unavailable (`null`) contribute nothing. Cmdr is macOS-only, so `bot` (a non-macOS UA)
 * is the one high-confidence exclusion; `unknown` (no/unrecognized UA) stays counted because we can't
 * tell. The scraper spoofs Mac browser UAs, so `human` is "could be a real install", not proof of one.
 */
export function aggregateUaFamilies(rows: { downloadsByUaFamily: UaFamilyCounts | null }[]): UaFamilyTotals {
  let human = 0
  let bot = 0
  let unknown = 0
  for (const row of rows) {
    const f = row.downloadsByUaFamily
    if (!f) continue
    human += f.human
    bot += f.bot
    unknown += f.unknown
  }
  return { human, bot, unknown, total: human + bot + unknown, humanInstalls: human + unknown }
}
