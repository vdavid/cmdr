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
 *
 * Two ways in, because a walk can outlive the dialog that started it ("Open in pane",
 * `walk-handoff.svelte.ts`): `start` runs a NEW query, and `resume` re-attaches a
 * reopened dialog to the one still running.
 */

import {
  cancelSearch,
  searchFilesStreaming,
  type SearchQuery,
  type SearchRunCoverage,
  type SearchResultEntry,
} from '$lib/tauri-commands'
import type {
  LiveRunView,
  QueryStreamCallbacks,
  QueryStreamResumption,
  QueryStreamSource,
} from '$lib/query-ui/query-stream'
import { liveViewOf, observeSearchRun } from './live-run-events'
import { resumeHandedOffWalk, supersedeHandedOffWalk } from './walk-handoff.svelte'

export interface LiveSearchSourceDeps {
  /** Builds the payload for the run about to start (scope parsing included, so async). */
  buildQuery: () => Promise<SearchQuery>
  /**
   * The run's coverage answer: `null` when a run starts (a caveat may never outlive the
   * run that earned it) and the terminal one when it ends.
   *
   * Also the run's two clock edges, which is why the `null` matters as much as the
   * answer: a run can end BEFORE `searchFilesStreaming` resolves (a small folder's
   * whole run arrives while the invoke is still in flight), so nothing downstream of
   * that promise can be trusted to see a run start.
   */
  onCoverage: (coverage: SearchRunCoverage | null) => void
  /** The order a finished walk's rows are left in. */
  rank: (entries: SearchResultEntry[]) => SearchResultEntry[]
  /**
   * The run in flight and where it has got to, or `null` once it has ended.
   *
   * The runner owns the run for the DIALOG's purposes and keeps its id private; this
   * is for the one thing Search does with a run the dialog is finished with — hand it
   * to a pane and let it keep walking (`walk-handoff.svelte.ts`). ❌ Don't mirror it
   * into reactive state: it fires per batch, and the dialog already renders the same
   * numbers off the runner.
   */
  onRunState?: (state: { runId: string; view: LiveRunView } | null) => void
}

/**
 * A walk that ended any way but "covered it" leaves the list a lower bound. Two of the
 * four endings are that, and so is a walk that finished having abandoned folders on
 * the way (Accepted difference 9) — the third reason, and the one nothing else on the
 * wire hints at. The shared dialog only needs the boolean; Search's note says which
 * of the three it was.
 */
function isIncomplete(coverage: SearchRunCoverage): boolean {
  return coverage.walk === 'interrupted' || coverage.walk === 'cancelled' || coverage.abandonedGround
}

export function createLiveSearchSource(deps: LiveSearchSourceDeps): QueryStreamSource {
  /** The shared "the run ended" shape, from Search's own terminal answer. */
  const settle = (callbacks: QueryStreamCallbacks) => (matchCount: number, coverage: SearchRunCoverage) => {
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

  return {
    start: async (runId: string, callbacks: QueryStreamCallbacks): Promise<() => void> => {
      deps.onCoverage(null)
      // Starting a run supersedes every other one backend side, so a walk handed off
      // to a pane goes silent from here on. Telling it now is what stops its toast
      // waiting forever on a terminal event that is never coming.
      supersedeHandedOffWalk()
      deps.onRunState?.({
        runId,
        view: {
          phase: 'resolvingCoverage',
          matchCount: 0,
          dirsFound: 0,
          currentPath: null,
          capped: false,
          running: true,
          incomplete: false,
          phaseSince: Date.now(),
        },
      })
      const stop = await observeSearchRun(runId, {
        onProgress: (event) => {
          deps.onRunState?.({ runId, view: liveViewOf(event) })
          callbacks.onProgress(event)
        },
        onSettled: (matchCount, coverage) => {
          deps.onRunState?.(null)
          settle(callbacks)(matchCount, coverage)
        },
        onFailed: (message) => {
          deps.onRunState?.(null)
          callbacks.onFailed(message)
        },
      })

      try {
        const query = await deps.buildQuery()
        await searchFilesStreaming(query, runId)
      } catch (err) {
        stop()
        deps.onRunState?.(null)
        throw err
      }
      return stop
    },

    resume: (callbacks: QueryStreamCallbacks): QueryStreamResumption | null =>
      resumeHandedOffWalk({
        onProgress: callbacks.onProgress,
        onSettled: settle(callbacks),
        onFailed: callbacks.onFailed,
      }),

    cancel: (runId: string): void => {
      void cancelSearch(runId).catch(() => {
        // Nothing to do: the run either already ended or never registered, and either
        // way the terminal event is what the UI acts on.
      })
    },

    rankOnCompletion: deps.rank,
  }
}
