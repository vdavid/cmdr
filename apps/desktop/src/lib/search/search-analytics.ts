/**
 * What one search reports to analytics, as pure data.
 *
 * PII-free by construction: every value here is a categorical enum or a bucket
 * name, and nothing in this file can see a query, a pattern, or a path
 * (`src-tauri/src/analytics/CLAUDE.md`). The dialog owns the timing and the IPC;
 * this owns the vocabulary.
 *
 * The question the props exist to answer is "how often does a search still have
 * to walk, how long does that take, and do people stay for it?" — so a run
 * reports once, when it ENDS, and says how it ended.
 */

import type { SearchRunCoverage } from '$lib/tauri-commands'
import type { SearchMode } from '$lib/query-ui/query-filter-state.svelte'

/** What started the run. The two have different costs and different endings. */
export type SearchTrigger =
  /** The auto-apply debounce, which answers from the index only (Decision 7). */
  | 'autoApply'
  /** Enter, or the run button: the path that walks uncovered ground. */
  | 'run'

/**
 * How a run stopped being the one the dialog is showing. `superseded` is the
 * only one the backend never reports: a run the user typed past keeps walking,
 * and it's the arrival of its successor that ends it as far as the person is
 * concerned.
 */
export type SearchEnding = 'completed' | 'interrupted' | 'cancelled' | 'superseded'

/** Which ground the answer came from, or `unknown` for a run that ended before saying. */
export type SearchCoverage = SearchRunCoverage['kind'] | 'unknown'

/** The offers a coverage note can make, `none` included so the denominator is countable. */
export type SearchCta = 'indexDrive' | 'fullDiskAccess' | 'none'

export interface SearchRunFacts {
  /**
   * The dialog's own mode. Typed as the `SearchMode` union rather than `string`, so the
   * documented vocabulary and the emitted one can't drift: a new mode is a compile error
   * in every reader of this prop, and the docs get updated with it.
   */
  mode: SearchMode
  trigger: SearchTrigger
  ending: SearchEnding
  coverage: SearchCoverage
  /** Wall time from the run starting to it ending, or `null` when it wasn't timed. */
  durationMs: number | null
  /** Whether the walk gave up on folders along the way (short without saying so). */
  abandonedGround: boolean
  /** Whether the result cap stopped the rows. */
  capped: boolean
}

/**
 * A run's duration as a bucket name. Bucketed rather than reported, because a
 * millisecond count is a fingerprint of one machine's disk and one folder's size,
 * and the question is only ever "how long do people wait?".
 *
 * The steps are where the experience changes: instant, a pause, a wait worth
 * watching a progress strip for, and long enough that leaving is the sane move.
 */
export function durationBucket(ms: number): string {
  if (ms < 1000) return '<1s'
  if (ms < 5000) return '1-5s'
  if (ms < 30_000) return '5-30s'
  if (ms < 120_000) return '30s-2m'
  return '2m+'
}

/** How a settled run's coverage answer maps onto the ending vocabulary. */
export function endingOf(coverage: SearchRunCoverage): SearchEnding {
  if (coverage.walk === 'cancelled') return 'cancelled'
  if (coverage.walk === 'interrupted') return 'interrupted'
  return 'completed'
}

/**
 * The props for one `search_used` event. Snake_case names, matching the existing
 * events (`volume_kind`, `item_count`).
 *
 * A run with no duration reports none rather than a zero: the index-only path
 * answers inside one promise and timing it would measure the IPC round trip, not
 * a wait anybody felt.
 */
export function searchUsedProps(facts: SearchRunFacts): Record<string, string | boolean> {
  return {
    mode: facts.mode,
    trigger: facts.trigger,
    ending: facts.ending,
    coverage: facts.coverage,
    abandoned_ground: facts.abandonedGround,
    capped: facts.capped,
    ...(facts.durationMs === null ? {} : { duration_bucket: durationBucket(facts.durationMs) }),
  }
}
