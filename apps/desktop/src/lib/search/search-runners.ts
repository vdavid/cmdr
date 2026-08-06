/**
 * The two paths a Search run can take, and the one query builder they share.
 *
 * `runQuery` is the one-shot index answer the auto-apply DEBOUNCE takes; `streamingSource`
 * is every run the user asked for, which walks ground the index doesn't cover (Decision 7 —
 * a debounced live walk would start and abandon a walk per keystroke). Both build their
 * payload with `buildRunQuery`, or an auto-applied answer and an Enter-run one could differ
 * for reasons nobody could see.
 *
 * Both also write the coverage note, and the note always belongs to the run on screen:
 * cleared before the ask, filled from the answer. Analytics rides along the same edges
 * (`search-run-tracking.ts`).
 */

import { searchFiles, parseSearchScope, type SearchQuery, type SearchResultEntry } from '$lib/tauri-commands'
import type { LiveRunView, QueryStreamSource } from '$lib/query-ui/query-stream'
import { coverageNoteFrom, coverageNoteFromRun } from './coverage-note'
import { createLiveSearchSource } from './live-search-source'
import { rankLiveResults } from './live-ranking'
import { createSearchRunTracker } from './search-run-tracking'
import {
  buildSearchQuery,
  getCaseSensitive,
  getLastAiPattern,
  getLastAiPatternKind,
  getMode,
  getQuery,
  getScope,
  setCoverageNote,
} from './search-state.svelte'

/**
 * Builds the run's payload: the bar + filters + AI pattern off the Search state via
 * `buildSearchQuery()`, plus the scope, whose parse is async and so can't live in there.
 *
 * `defaultScopePath` is where the run goes when the scope box is empty. An EMPTY box isn't
 * "everywhere": a search covers one volume at most, and the default rung of that ladder is
 * the focused pane's current folder, resolved at run time so it follows the pane.
 */
export async function buildRunQuery(defaultScopePath: string): Promise<SearchQuery> {
  const query = buildSearchQuery()
  // After an AI translation, the bar still shows the user's natural-language
  // prompt. The actual search must run against the AI's produced pattern, not
  // the prompt. Same for any AI-mode search where the user kept a pattern around.
  if (getMode() === 'ai') {
    const aiPattern = getLastAiPattern()
    const aiKind = getLastAiPatternKind()
    query.namePattern = aiPattern && aiPattern.trim() ? aiPattern : null
    query.patternType = aiKind === 'regex' ? 'regex' : 'glob'
  }
  const scopeStr = getScope().trim()
  if (scopeStr) {
    const parsed = await parseSearchScope(scopeStr)
    if (parsed.includePaths.length > 0) query.includePaths = parsed.includePaths
    if (parsed.excludePatterns.length > 0) query.excludeDirNames = parsed.excludePatterns
  } else {
    query.includePaths = [defaultScopePath]
  }
  return query
}

export interface SearchRunnersDeps {
  /**
   * Where a run lands when the user hasn't set a scope. Read per run rather than captured,
   * so it follows the focused pane instead of freezing at dialog-open time.
   */
  getDefaultScopePath: () => string
  /**
   * The live run in flight and where it has got to, for the one thing Search does with a
   * run the dialog is finished with: handing it to a pane (`walk-handoff.svelte.ts`).
   */
  onRunState: (state: { runId: string; view: LiveRunView } | null) => void
}

export interface SearchRunners {
  /** The one-shot index answer, in one promise. QueryDialog's `runQuery`. */
  runQuery: () => Promise<{ entries: SearchResultEntry[]; totalCount: number }>
  /** The index's half, then a walk over what it can't answer for. QueryDialog's `streamingSource`. */
  streamingSource: QueryStreamSource
}

export function createSearchRunners(deps: SearchRunnersDeps): SearchRunners {
  const tracker = createSearchRunTracker()

  async function runSearch(): Promise<{ entries: SearchResultEntry[]; totalCount: number }> {
    // The note belongs to the run that produced it. Dropping it up front (rather than
    // only on the way out) means a run that throws can't leave the previous run's
    // caveat sitting under a fresh answer.
    setCoverageNote(null)
    const query = await buildRunQuery(deps.getDefaultScopePath())
    const result = await searchFiles(query)
    // Coverage honesty: an empty answer with a structural reason says so, instead of
    // reading as "nothing matched" (`search/DETAILS.md` § Honesty).
    setCoverageNote(coverageNoteFrom(result))
    tracker.autoAppliedRun(result)
    return { entries: result.entries, totalCount: result.totalCount }
  }

  const streamingSource = createLiveSearchSource({
    buildQuery: () => buildRunQuery(deps.getDefaultScopePath()),
    onRunState: deps.onRunState,
    onCoverage: (coverage) => {
      setCoverageNote(coverage === null ? null : coverageNoteFromRun(coverage))
      // `null` is a run STARTING, and it's the run's clock edge as well as the note's:
      // a small folder's whole run can be emitted before the invoke returns.
      if (coverage === null) tracker.liveRunStarting()
      else tracker.liveRunEnded(coverage)
    },
    rank: (entries) =>
      rankLiveResults(entries, {
        query: getMode() === 'ai' ? (getLastAiPattern() ?? getQuery()) : getQuery(),
        mode: getMode(),
        caseSensitive: getCaseSensitive(),
      }),
  })

  return { runQuery: runSearch, streamingSource }
}
