/**
 * The dialog's side of `search-analytics.ts`: the clock, and the one `trackEvent` call
 * per run.
 *
 * A run reports ONCE, when it ENDS, because the numbers worth having (did it need to
 * walk, how long did that take, did the person stay for it) aren't known before then.
 * PII-free by construction: every value is a categorical enum or a bucket name minted in
 * `search-analytics.ts`, which can't see a query, a pattern, or a path.
 *
 * Two things about the wiring are load-bearing:
 *
 *   - **The clock starts on the coverage callback's `null`** (`liveRunStarting`), not on
 *     `searchFilesStreaming` resolving: a small folder's whole run can be emitted before
 *     that promise settles, so a start hook downstream of it fires after the run ended.
 *   - **A run whose successor arrives while it's still going is `superseded`, and only the
 *     frontend can say so.** Its walk keeps running (Decision 11) and no terminal event
 *     for it is coming, so the next run starting is the one moment it can be counted.
 */

import { trackEvent, type SearchResult, type SearchRunCoverage } from '$lib/tauri-commands'
import { endingOf, searchUsedProps, type SearchCta, type SearchRunFacts } from './search-analytics'
import { getMode } from './search-state.svelte'

export interface SearchRunTracker {
  /** The index-only answer the debounce produced. Nothing walked, so nothing to time. */
  autoAppliedRun: (result: SearchResult) => void
  /** A live run is starting. Ends the previous one as `superseded` if it was still going. */
  liveRunStarting: () => void
  /** A live run reached one of the three endings the backend reports. */
  liveRunEnded: (coverage: SearchRunCoverage) => void
}

/**
 * CTA conversion is two events rather than one prop: an offer can only be counted when
 * it's on screen, and it's pressed (or not) later. The ratio is `search_cta_used` over
 * `search_cta_offered`, per `cta`.
 */
export function trackCtaOffered(cta: SearchCta): void {
  void trackEvent('search_cta_offered', { cta })
}

export function trackCtaUsed(cta: SearchCta): void {
  void trackEvent('search_cta_used', { cta })
}

export function createSearchRunTracker(): SearchRunTracker {
  /** The live run being timed, or `null` between runs. */
  let timedRun: { startedAt: number } | null = null

  function trackSearchRun(facts: Omit<SearchRunFacts, 'mode'>): void {
    void trackEvent('search_used', searchUsedProps({ ...facts, mode: getMode() }))
  }

  /**
   * A live run ended. Called for the three endings the backend reports and, with `null`,
   * for the fourth: a run whose successor arrived while it was still going. That one is
   * superseded rather than cancelled — its walk keeps running (Decision 11) — and nobody
   * will ever tell us how it turned out, so its coverage is honestly unknown.
   */
  function endLiveRun(coverage: SearchRunCoverage | null): void {
    const startedAt = timedRun?.startedAt ?? null
    timedRun = null
    trackSearchRun({
      trigger: 'run',
      ending: coverage === null ? 'superseded' : endingOf(coverage),
      coverage: coverage?.kind ?? 'unknown',
      durationMs: startedAt === null ? null : Math.round(performance.now() - startedAt),
      abandonedGround: coverage?.abandonedGround ?? false,
      capped: coverage?.capped ?? false,
    })
  }

  return {
    autoAppliedRun: (result) => {
      trackSearchRun({
        trigger: 'autoApply',
        ending: 'completed',
        // This path can't walk, so what it reports IS what the index covered —
        // and an uncovered scope is a gap it names rather than fills.
        coverage: (result.uncoveredScopes?.length ?? 0) > 0 ? 'unknown' : 'covered',
        durationMs: null,
        abandonedGround: false,
        capped: false,
      })
    },

    liveRunStarting: () => {
      // A run still being timed at this moment is one the user typed past: superseded,
      // with no terminal event coming.
      if (timedRun !== null) endLiveRun(null)
      timedRun = { startedAt: performance.now() }
    },

    liveRunEnded: (coverage) => {
      endLiveRun(coverage)
    },
  }
}
