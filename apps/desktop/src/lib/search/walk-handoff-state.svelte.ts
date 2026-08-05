/**
 * What a handed-off walk IS, so the toast can read it without importing the module
 * that drives it.
 *
 * Split from `walk-handoff.svelte.ts` for one reason: that module owns the toast
 * COMPONENT, and the component has to read the live counters. Both in one file is a
 * cycle. So the cell and the two actions a person can take on it live here, and the
 * orchestration — subscribing, feeding the snapshot, swapping toasts — lives there.
 */

import { cancelSearch } from '$lib/tauri-commands'
import type { LiveRunView } from '$lib/query-ui/query-stream'

/** The one running toast, replaced in place as the counters move. */
export const WALK_HANDOFF_TOAST_ID = 'search-walk-handoff'

/** A walk still filling a pane after the dialog that started it closed. */
export interface WalkHandoff {
  runId: string
  snapshotId: string
  /** The snapshot's label, for a toast that has to name what it's talking about. */
  label: string
  /** Where the run has got to. Kept after it ends so the last word can be shown. */
  view: LiveRunView
}

let handoff = $state<WalkHandoff | null>(null)

/** How the host reopens the search dialog, supplied by whoever mounts it. */
let reopenSearch: (() => void) | null = null

/** The walk still feeding a pane, or `null`. Read by the toast and the dialog. */
export function getWalkHandoff(): WalkHandoff | null {
  return handoff
}

/** Replaces the tracked handoff. `walk-handoff.svelte.ts` is the only writer. */
export function setWalkHandoff(next: WalkHandoff | null): void {
  handoff = next
}

/**
 * Registers how to reopen the search dialog, for the toast's Reopen button.
 *
 * The dialog can't reopen itself: by the time the button matters it has unmounted,
 * and its host owns the flag. Set once from the host.
 */
export function setSearchReopener(reopen: (() => void) | null): void {
  reopenSearch = reopen
}

/** Reopens the search dialog, if the host registered a way to. */
export function reopenHandedOffSearch(): void {
  reopenSearch?.()
}

/**
 * Stops the handed-off walk. The terminal event does the rest, which is why this
 * flips nothing itself: what the walk found stays in the pane and in the index.
 */
export function stopHandedOffWalk(): void {
  const runId = handoff?.runId
  if (runId === undefined) return
  void cancelSearch(runId).catch(() => {
    // Already over, or never registered. Either way there's nothing to stop.
  })
}
