/**
 * The order a finished live search leaves its rows in.
 *
 * A live run appends as it finds: the index's own ranked half first, then whatever the
 * walk turns up, in the order the filesystem gave it. That's right while the list is
 * growing (rows must not jump under the reader) and wrong once it stops, so the run's
 * last act is one re-rank over the whole set.
 *
 * The signal is **match quality, then recency** — the two things a walked row carries.
 * Importance weights come from the index and a walked row has none, which is exactly
 * what the plan's accepted-differences register says a live answer gives up
 * (`docs/specs/unindexed-search-plan.md`, difference 4). Applying one rule to both
 * halves is deliberate: an index-ranked block followed by an arrival-ordered block
 * would read as two lists stapled together.
 *
 * ❌ This is ORDERING, never membership. It mirrors the backend's `ranking::stem_for` /
 * `classify_match` bands so the two don't disagree about which row leads, but no row is
 * added or dropped here, so it can't make an unindexed drive answer differently the way
 * a forked MATCHING rule would (`src-tauri/src/search/CLAUDE.md`).
 */

import type { SearchResultEntry } from '$lib/tauri-commands'
import type { SearchMode } from '$lib/query-ui/query-filter-state.svelte'

/** How well a name answers the query. Bands are compared before anything else. */
const enum MatchQuality {
  Exact = 0,
  Prefix = 1,
  Other = 2,
}

/**
 * The wildcard-free stem a match-quality gradient can be measured against, or `''` when
 * there isn't one.
 *
 * Only a plain glob substring (`report`, which the backend wraps to `*report*`) has an
 * exact-vs-prefix-vs-the-rest gradient. A wildcard glob (`report*`, `*.pdf`) or a regex
 * has none, so every row sits in one band and recency decides. Mirrors
 * `ranking::stem_for`, NFD included: macOS filenames are NFD and so is the index's copy.
 */
export function rankingStem(query: string, mode: SearchMode): string {
  if (mode === 'regex') return ''
  const trimmed = query.trim()
  if (!trimmed || trimmed.includes('*') || trimmed.includes('?')) return ''
  return trimmed.normalize('NFD')
}

/** Mirrors `ranking::classify_match`: exact beats prefix beats everything else. */
function classify(name: string, stem: string, caseInsensitive: boolean): MatchQuality {
  if (stem === '') return MatchQuality.Other
  const a = caseInsensitive ? name.toLowerCase() : name
  const b = caseInsensitive ? stem.toLowerCase() : stem
  if (a === b) return MatchQuality.Exact
  if (a.startsWith(b)) return MatchQuality.Prefix
  return MatchQuality.Other
}

/**
 * Orders a completed live run's rows: best match quality first, newest first within a
 * band, and path as the last tiebreak so the same result set always renders the same
 * way (two files modified in the same second must not swap places between runs).
 *
 * Returns a new array; the input is left alone.
 */
export function rankLiveResults(
  entries: SearchResultEntry[],
  options: { query: string; mode: SearchMode; caseSensitive: boolean },
): SearchResultEntry[] {
  const stem = rankingStem(options.query, options.mode)
  const caseInsensitive = !options.caseSensitive
  const bands = new Map<string, MatchQuality>()
  for (const entry of entries) bands.set(entry.path, classify(entry.name, stem, caseInsensitive))

  return [...entries].sort((a, b) => {
    const band = (bands.get(a.path) ?? MatchQuality.Other) - (bands.get(b.path) ?? MatchQuality.Other)
    if (band !== 0) return band
    // An unknown modification time sorts oldest rather than newest: a row Cmdr can't
    // date shouldn't lead the list on the strength of not knowing.
    const recency = (b.modifiedAt ?? 0) - (a.modifiedAt ?? 0)
    if (recency !== 0) return recency
    return a.path < b.path ? -1 : a.path > b.path ? 1 : 0
  })
}
