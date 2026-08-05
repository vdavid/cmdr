/**
 * Search's transport for a query that answers over time: the four live-search events,
 * mapped onto the shared `QueryStreamSource` contract (`$lib/query-ui/query-stream`).
 *
 * Everything Search-flavoured stops here. The runner is handed phases, batches, and one
 * "the run ended, here's whether it covered everything"; `WalkEnding`, coverage lists,
 * and run ids never reach the shared dialog. What the run COULDN'T cover goes to
 * `onCoverage`, because the sentence for it belongs above the results, in Search's own
 * coverage note.
 *
 * Listeners are installed BEFORE the command is invoked: the backend can emit its first
 * batch (or its whole run, on a small folder) before the invoke resolves, and a listener
 * installed after would miss it.
 */

import {
  cancelSearch,
  onSearchCancelled,
  onSearchComplete,
  onSearchError,
  onSearchProgress,
  searchFilesStreaming,
  type SearchQuery,
  type SearchRunCoverage,
  type SearchResultEntry,
  type UnlistenFn,
} from '$lib/tauri-commands'
import type { QueryStreamCallbacks, QueryStreamSource } from '$lib/query-ui/query-stream'

export interface LiveSearchSourceDeps {
  /** Builds the payload for the run about to start (scope parsing included, so async). */
  buildQuery: () => Promise<SearchQuery>
  /**
   * The run's coverage answer: `null` when a run starts (a caveat may never outlive the
   * run that earned it) and the terminal one when it ends.
   */
  onCoverage: (coverage: SearchRunCoverage | null) => void
  /** The order a finished walk's rows are left in. */
  rank: (entries: SearchResultEntry[]) => SearchResultEntry[]
  /** Called once per started run, for the "a search ran" analytics event. */
  onStarted?: () => void
}

/**
 * A walk that ended any way but "covered it" leaves the list a lower bound. Two of the
 * four endings are that; the shared dialog only needs the boolean, and Search's note
 * says which one it was.
 */
function isIncomplete(coverage: SearchRunCoverage): boolean {
  return coverage.walk === 'interrupted' || coverage.walk === 'cancelled'
}

export function createLiveSearchSource(deps: LiveSearchSourceDeps): QueryStreamSource {
  return {
    start: async (runId: string, callbacks: QueryStreamCallbacks): Promise<() => void> => {
      deps.onCoverage(null)
      const unlisten: UnlistenFn[] = []
      const stop = (): void => {
        for (const off of unlisten) off()
        unlisten.length = 0
      }
      /** Everything this run says; anything naming another run is somebody else's. */
      const mine = (eventRunId: string): boolean => eventRunId === runId

      try {
        unlisten.push(
          await onSearchProgress((event) => {
            if (!mine(event.runId)) return
            callbacks.onProgress({
              phase: event.phase,
              entries: event.entries,
              matchCount: event.matchCount,
              dirsFound: event.dirsFound,
              currentPath: event.currentPath,
              capped: event.capped,
            })
          }),
        )
        const settle = (matchCount: number, coverage: SearchRunCoverage): void => {
          deps.onCoverage(coverage)
          callbacks.onEnd({
            matchCount,
            incomplete: isIncomplete(coverage),
            // `nothingToWalk` means the index answered the whole scope, so its rows came
            // ranked and re-ranking them would only throw that ranking away.
            walked: coverage.walk !== 'nothingToWalk',
            capped: coverage.capped,
          })
        }
        unlisten.push(
          await onSearchComplete((event) => {
            if (mine(event.runId)) settle(event.matchCount, event.coverage)
          }),
        )
        unlisten.push(
          await onSearchCancelled((event) => {
            if (mine(event.runId)) settle(event.matchCount, event.coverage)
          }),
        )
        unlisten.push(
          await onSearchError((event) => {
            // The typed `error` is the branch a future caller acts on (M8 routes
            // `indexUnreadable` differently); the sentence is rendered backend-side for
            // the same reason the engine's "Query too broad" is.
            if (mine(event.runId)) callbacks.onFailed(event.message)
          }),
        )

        const query = await deps.buildQuery()
        await searchFilesStreaming(query, runId)
        deps.onStarted?.()
      } catch (err) {
        stop()
        throw err
      }
      return stop
    },

    cancel: (runId: string): void => {
      void cancelSearch(runId).catch(() => {
        // Nothing to do: the run either already ended or never registered, and either
        // way the terminal event is what the UI acts on.
      })
    },

    rankOnCompletion: deps.rank,
  }
}
