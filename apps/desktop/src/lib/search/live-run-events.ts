/**
 * The four live-search events, per run.
 *
 * The raw wire, and nothing else: two consumers subscribe to it and they can't
 * import each other. The dialog's transport (`live-search-source.ts`) turns a run
 * into the shared streaming contract, and the walk handoff
 * (`walk-handoff.svelte.ts`) keeps listening to one after the dialog is gone.
 *
 * Every listener drops events naming another run here, so no caller has to remember
 * the rule — a superseded run's batches keep arriving, because superseding stops the
 * events, not the walk (Decision 11).
 */

import {
  onSearchCancelled,
  onSearchComplete,
  onSearchError,
  onSearchProgress,
  type SearchResultEntry,
  type SearchRunCoverage,
  type UnlistenFn,
} from '$lib/tauri-commands'
import type { LiveRunView, QueryStreamPhase } from '$lib/query-ui/query-stream'

/** Where a run has got to, without the batch that carried it. */
export function liveViewOf(event: LiveRunProgress): LiveRunView {
  return {
    phase: event.phase,
    matchCount: event.matchCount,
    dirsFound: event.dirsFound,
    currentPath: event.currentPath,
    capped: event.capped,
    running: true,
    incomplete: false,
    phaseSince: Date.now(),
  }
}

/** One batch of a live run, before any of it is translated into shared vocabulary. */
export interface LiveRunProgress {
  phase: QueryStreamPhase
  entries: SearchResultEntry[]
  matchCount: number
  dirsFound: number
  currentPath: string | null
  capped: boolean
}

/** What a live run says, in Search's own words. */
export interface LiveRunHandlers {
  onProgress: (event: LiveRunProgress) => void
  /** The run reached a terminal state, with the coverage answer it earned. */
  onSettled: (matchCount: number, coverage: SearchRunCoverage) => void
  /** The run couldn't run at all. `message` is the sentence to show. */
  onFailed: (message: string) => void
}

/**
 * Listens to the run named `runId` and resolves with its teardown.
 *
 * Shared by the dialog's own run and by the walk handoff, which keeps listening after
 * the dialog is gone. Events naming any other run are somebody else's and dropped here,
 * so no caller has to remember the rule.
 */
export async function observeSearchRun(runId: string, handlers: LiveRunHandlers): Promise<() => void> {
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
        handlers.onProgress({
          phase: event.phase,
          entries: event.entries,
          matchCount: event.matchCount,
          dirsFound: event.dirsFound,
          currentPath: event.currentPath,
          capped: event.capped,
        })
      }),
    )
    unlisten.push(
      await onSearchComplete((event) => {
        if (mine(event.runId)) handlers.onSettled(event.matchCount, event.coverage)
      }),
    )
    unlisten.push(
      await onSearchCancelled((event) => {
        if (mine(event.runId)) handlers.onSettled(event.matchCount, event.coverage)
      }),
    )
    unlisten.push(
      await onSearchError((event) => {
        // The typed `error` is the branch a future caller acts on (routing
        // `indexUnreadable` somewhere of its own); the sentence is rendered backend-side for
        // the same reason the engine's "Query too broad" is.
        if (mine(event.runId)) handlers.onFailed(event.message)
      }),
    )
  } catch (err) {
    stop()
    throw err
  }
  return stop
}
