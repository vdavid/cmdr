/**
 * The walk that outlives its dialog: "Open in pane" while a live search is still
 * running.
 *
 * Closing the search dialog normally stops the walk behind it, because nobody is
 * waiting for it. "Open in pane" is the one exception: the results
 * are on screen in a pane, so the walk keeps going and the rows it finds keep
 * arriving there. This module is what makes that true — module state, so it survives
 * the dialog's unmount, and the only thing in Search that owns a run nobody is
 * looking at.
 *
 * It does four things:
 *
 *   1. Keeps listening to the run (`observeSearchRun`) after the dialog is gone.
 *   2. Appends each batch to the snapshot, which is what makes the pane grow.
 *   3. Drives the toast: running (with Reopen search and Stop) → an auto-hiding
 *      "finished" one.
 *   4. Hands the run back to a REOPENED dialog (`resumeHandedOffWalk`), rows and all,
 *      so it shows the search rather than its leftovers.
 *
 * ## The four ways it settles
 *
 * Terminal event, superseded by a new search, the pane it fed going away, and a run
 * that couldn't run. Each stops the toast; only the pane-went-away case stops the
 * WALK, because that's the only one where the work has lost its last consumer.
 */

import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'
import type { LiveRunView, QueryStreamResumption } from '$lib/query-ui/query-stream'
import type { SearchResultEntry, SearchRunCoverage } from '$lib/tauri-commands'
import { addToast, dismissToast } from '$lib/ui/toast/toast-store.svelte'
import { observeSearchRun, type LiveRunHandlers, type LiveRunProgress } from './live-run-events'
import { appendSnapshotEntries } from './snapshot-store.svelte'
import {
  WALK_HANDOFF_TOAST_ID,
  getWalkHandoff,
  setSearchReopener,
  setWalkHandoff,
  stopHandedOffWalk,
} from './walk-handoff-state.svelte'
import WalkHandoffToastContent from './WalkHandoffToastContent.svelte'

/** Teardown for the module's own subscription to the run. */
let stopListening: (() => void) | null = null

/**
 * Rows that arrived while no dialog was listening, so a reopened one can catch up.
 *
 * Bounded by the backend's row cap (rows stop at the cap while the count carries on),
 * so this can't grow with the walk.
 */
let missedEntries: SearchResultEntry[] = []

/** The reopened dialog's callbacks, while one is attached. */
let resumedInto: LiveRunHandlers | null = null

/**
 * Starts feeding `snapshotId` from the still-running `runId`, and says so.
 *
 * Called by `SearchDialog` from "Open in pane" when the run is live. `view` is where
 * the dialog had got to, so the toast opens on real numbers rather than zeroes.
 *
 * **Returns the run id it took over**, and the caller must keep it: the dialog's close
 * stops every live run but the one it names, and this is that one. ❌ Don't have the
 * caller ask a module function or the state cell for it at teardown instead. That was
 * the shape this started as, and in the running app the answer came back `null` while
 * every unit test passed — the walk died the moment the pane appeared, with a toast
 * still saying "still searching" over it. A value the caller holds can't be defeated by
 * module resolution or by teardown ordering.
 */
export function handOffWalk(params: { runId: string; snapshotId: string; label: string; view: LiveRunView }): string {
  // One at a time: a second "Open in pane" supersedes the first run backend side
  // anyway, so the earlier handoff has nothing left to hear.
  settle(null)
  setWalkHandoff({
    runId: params.runId,
    snapshotId: params.snapshotId,
    label: params.label,
    view: { ...params.view, running: true },
  })
  missedEntries = []
  showRunningToast()

  const runId = params.runId
  void observeSearchRun(runId, {
    onProgress: (event) => {
      if (getWalkHandoff()?.runId === runId) takeProgress(event)
    },
    onSettled: (matchCount, coverage) => {
      if (getWalkHandoff()?.runId === runId) takeSettled(matchCount, coverage)
    },
    onFailed: (message) => {
      if (getWalkHandoff()?.runId === runId) takeFailed(message)
    },
  })
    .then((stop) => {
      // The handoff can end while the listeners are being installed (a fast walk, or
      // the user closing the pane): tear them down rather than leaving them feeding
      // a snapshot nobody reads.
      if (getWalkHandoff()?.runId === runId) stopListening = stop
      else stop()
    })
    .catch(() => {
      // No listeners means no rows and no terminal event, so the toast would sit
      // there forever. Drop it and leave the pane with what it already has.
      if (getWalkHandoff()?.runId === runId) settle(null)
    })

  return runId
}

/** One batch: into the snapshot, into the toast, and on to a reopened dialog. */
function takeProgress(event: LiveRunProgress): void {
  const current = getWalkHandoff()
  if (!current) return
  const stillThere = appendSnapshotEntries(current.snapshotId, event.entries, event.matchCount)
  if (!stillThere) {
    // The pane that held these results is gone, so the walk has lost its last
    // consumer. Stopping it is the resource promise, not a nicety: what it already
    // read stays in the index either way.
    stopHandedOffWalk()
    settle(null)
    return
  }
  setWalkHandoff({
    ...current,
    view: {
      phase: event.phase,
      matchCount: event.matchCount,
      dirsFound: event.dirsFound,
      currentPath: event.currentPath,
      capped: event.capped,
      running: true,
      incomplete: false,
      phaseSince: Date.now(),
    },
  })
  if (resumedInto) resumedInto.onProgress(event)
  else missedEntries = [...missedEntries, ...event.entries]
}

/** The run's last word: a finished toast, and nothing left listening. */
function takeSettled(matchCount: number, coverage: SearchRunCoverage): void {
  const current = getWalkHandoff()
  if (!current) return
  appendSnapshotEntries(current.snapshotId, [], matchCount)
  resumedInto?.onSettled(matchCount, coverage)
  const finishedWhole = coverage.walk === 'completed' || coverage.walk === 'nothingToWalk'
  settle(
    finishedWhole && !coverage.abandonedGround
      ? {
          message: tString('search.walkHandoff.finished', {
            label: current.label,
            count: matchCount,
            countText: formatInteger(matchCount),
          }),
          level: 'success',
        }
      : // Short, and the pane can't say why on its own. The dialog's coverage note
        // has the room for the detail; this says the one thing someone looking at
        // the pane needs to know.
        {
          message: tString('search.walkHandoff.stoppedShort', { label: current.label }),
          level: 'warn',
        },
  )
}

/** The run couldn't run. Say so where the pane is, not where the dialog was. */
function takeFailed(message: string): void {
  resumedInto?.onFailed(message)
  settle({ message, level: 'warn' })
}

/**
 * A new search superseded the handed-off run, so no more of its events are coming.
 *
 * Superseding doesn't cancel the walk (Decision 11) — it carries on filling the index,
 * which is why this isn't a warning about lost work. What it does stop is the stream
 * feeding this pane, and a toast waiting on a terminal event that will never arrive.
 */
export function supersedeHandedOffWalk(): void {
  const current = getWalkHandoff()
  if (!current) return
  settle({
    message: tString('search.walkHandoff.superseded', { label: current.label }),
    level: 'default',
  })
}

/**
 * Hands the running walk to a dialog that just reopened, or `null` when there isn't
 * one.
 *
 * The handoff keeps its own subscription either way — the pane still needs feeding —
 * and simply fans out to the dialog as a second reader. Its running toast goes away
 * while the dialog is up (the dialog's own progress strip says all of this, in more
 * detail) and comes back when the dialog detaches with the walk still going.
 */
export function resumeHandedOffWalk(callbacks: LiveRunHandlers): QueryStreamResumption | null {
  const current = getWalkHandoff()
  if (!current?.view.running) return null
  resumedInto = callbacks
  const entries = missedEntries
  missedEntries = []
  dismissToast(WALK_HANDOFF_TOAST_ID)
  return {
    runId: current.runId,
    view: current.view,
    missedEntries: entries,
    stop: () => {
      if (resumedInto !== callbacks) return
      resumedInto = null
      if (getWalkHandoff()?.view.running) showRunningToast()
    },
  }
}

/**
 * Ends the handoff: stop listening, forget the run, and replace the running toast
 * with `last` (or just drop it).
 *
 * ❌ It does NOT stop the walk. Only the caller knows whether the work still has a
 * reason to exist, and in three of the four cases it does.
 */
function settle(last: { message: string; level: 'success' | 'warn' | 'default' } | null): void {
  stopListening?.()
  stopListening = null
  setWalkHandoff(null)
  missedEntries = []
  resumedInto = null
  dismissToast(WALK_HANDOFF_TOAST_ID)
  if (last) addToast(last.message, { level: last.level, dismissal: 'transient' })
}

/**
 * Shows (or refreshes) the persistent running toast.
 *
 * The content component reads this module directly rather than taking props: a toast
 * replaced in place keeps its original props (`toast-store.svelte.ts`), so counters
 * passed in would freeze at the values they had when the pane opened.
 */
function showRunningToast(): void {
  addToast(WalkHandoffToastContent, {
    id: WALK_HANDOFF_TOAST_ID,
    level: 'info',
    dismissal: 'persistent',
    closeTooltip: tString('search.walkHandoff.hideTooltip'),
  })
}

/** Test-only reset, so one spec's handoff can't leak into the next. */
export function _resetWalkHandoffForTesting(): void {
  stopListening?.()
  stopListening = null
  setWalkHandoff(null)
  missedEntries = []
  resumedInto = null
  setSearchReopener(null)
}
