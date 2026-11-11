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

import { searchFiles, parseSearchScope, type SearchQuery } from '#lib/tauri-commands/index.js'
import type { LiveRunView, QueryStreamSource } from '#lib/query-ui/query-stream.js'
import type { QueryAnswer, QueryPage } from '#lib/query-ui/result-paging.js'
import { coverageNoteFrom, coverageNoteFromRun } from './coverage-note'
import { createLiveSearchSource } from './live-search-source'
import { fullRowQuery } from './snapshot-fill'
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
  setResultsVolumeId,
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
  runQuery: (page: QueryPage) => Promise<QueryAnswer>
  /**
   * The last run's payload asked of the index again for more rows: QueryDialog's
   * `loadMore`. The payload is the one the run SENT, so a bar or chip edited since without
   * re-running can't splice another question's rows in, and it repeats none of a run's
   * side effects (the coverage note, analytics). `null` before this dialog has started a
   * run of its own (a walk adopted on reopen): the list it shows was asked elsewhere.
   */
  loadMore: (page: QueryPage) => Promise<QueryAnswer | null>
  /** The index's half, then a walk over what it can't answer for. QueryDialog's `streamingSource`. */
  streamingSource: QueryStreamSource
  /**
   * The current search, asked for as many rows as a PANE can hold rather than the 30 a
   * dialog list wants. "Open in pane" fills the snapshot with it, and hands the same
   * query to a handed-off walk so its ending can top the pane up (`snapshot-fill.ts`).
   *
   * Built from live state at call time, exactly like a run, so it asks the question the
   * user is looking at.
   */
  buildFullRowQuery: () => Promise<SearchQuery>
}

export function createSearchRunners(deps: SearchRunnersDeps): SearchRunners {
  const tracker = createSearchRunTracker()
  /** What the latest run asked, for `loadMore`. Set the moment either path builds it. */
  let lastRunPayload: SearchQuery | null = null

  async function runSearch(page: QueryPage): Promise<QueryAnswer> {
    // The note belongs to the run that produced it. Dropping it up front (rather than
    // only on the way out) means a run that throws can't leave the previous run's
    // caveat sitting under a fresh answer.
    setCoverageNote(null)
    const query = { ...(await buildRunQuery(deps.getDefaultScopePath())), limit: page.limit }
    lastRunPayload = query
    const result = await searchFiles(query)
    // Ahead of the rows reaching the dialog: anything that acts on one needs its volume.
    setResultsVolumeId(result.targetVolumeId ?? null)
    // Coverage honesty: an empty answer with a structural reason says so, instead of
    // reading as "nothing matched" (`search/DETAILS.md` § Honesty).
    setCoverageNote(coverageNoteFrom(result))
    tracker.autoAppliedRun(result)
    return { entries: result.entries, totalCount: result.totalCount }
  }

  const streamingSource = createLiveSearchSource({
    buildQuery: async () => {
      const query = await buildRunQuery(deps.getDefaultScopePath())
      lastRunPayload = query
      return query
    },
    onRunState: deps.onRunState,
    onRowsVolume: setResultsVolumeId,
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

  async function loadMore(page: QueryPage): Promise<QueryAnswer | null> {
    if (!lastRunPayload) return null
    const result = await searchFiles({ ...lastRunPayload, limit: page.limit })
    return { entries: result.entries, totalCount: result.totalCount }
  }

  return {
    runQuery: runSearch,
    loadMore,
    streamingSource,
    buildFullRowQuery: async () => fullRowQuery(await buildRunQuery(deps.getDefaultScopePath())),
  }
}
