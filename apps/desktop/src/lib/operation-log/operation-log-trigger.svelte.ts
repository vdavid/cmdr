/**
 * Reactive state + open/close seam for the alpha "Operation log" dialog.
 *
 * Menu-triggered (View > Operation log, ⌥⌘L) and command-palette-reachable, modeled
 * on the What's-new trigger: `$state` lives here because reactive state needs a
 * `.svelte.ts` file, and `+page.svelte` mounts `OperationLogDialog` against
 * `operationLogState.open`. The dialog reads the newest 50 operations on open and
 * appends 50 more on demand (requirement 6b). The paging offset is `entries.length`
 * (one source of truth), so an append can't desync from what's shown.
 */

import { getOperationLogDetail, getRecentOperationLogEntries, type OperationRow } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('operationLog')

/** One page: the newest 50 on open, then 50 more per "Load more". */
export const OPERATION_LOG_PAGE = 50

interface OperationLogState {
  open: boolean
  entries: OperationRow[]
  /** `true` while the first page is loading (the dialog shows a spinner). */
  loading: boolean
  /** `true` when the first-page read threw (the dialog shows a friendly notice). */
  loadError: boolean
  /** `true` when the last page came back full, so more operations may exist. */
  hasMore: boolean
  /** `true` while a "Load more" append is in flight (disables the button). */
  loadingMore: boolean
}

export const operationLogState = $state<OperationLogState>({
  open: false,
  entries: [],
  loading: false,
  loadError: false,
  hasMore: false,
  loadingMore: false,
})

export function closeOperationLog(): void {
  operationLogState.open = false
}

/**
 * Opens the dialog and loads the newest page. Idempotent: a menu/palette/shortcut
 * double-fire opens it once. Always opens (even on a read failure) so the menu
 * item never feels dead; the failure surfaces as a friendly in-dialog notice.
 */
export async function openOperationLog(): Promise<void> {
  if (operationLogState.open) return

  operationLogState.open = true
  operationLogState.loading = true
  operationLogState.loadError = false
  operationLogState.entries = []
  operationLogState.hasMore = false

  try {
    const page = await getRecentOperationLogEntries(OPERATION_LOG_PAGE, 0)
    operationLogState.entries = page
    operationLogState.hasMore = page.length === OPERATION_LOG_PAGE
  } catch (e) {
    operationLogState.loadError = true
    log.warn("Couldn't load the operation log: {error}", { error: String(e) })
  } finally {
    operationLogState.loading = false
  }
}

/**
 * Appends the next page. Offset is the current entry count, so pages never
 * overlap. A short read means no more operations exist.
 */
export async function loadMoreOperations(): Promise<void> {
  if (operationLogState.loadingMore || !operationLogState.hasMore) return

  operationLogState.loadingMore = true
  try {
    const page = await getRecentOperationLogEntries(OPERATION_LOG_PAGE, operationLogState.entries.length)
    operationLogState.entries = [...operationLogState.entries, ...page]
    operationLogState.hasMore = page.length === OPERATION_LOG_PAGE
  } catch (e) {
    // A failed append leaves what's already shown intact and keeps `hasMore`, so Load
    // more stays up as the retry. Hiding it would make the list read as the whole history.
    log.warn("Couldn't load more operations: {error}", { error: String(e) })
  } finally {
    operationLogState.loadingMore = false
  }
}

/**
 * Flip a row to "rolling back" once its dispatch has landed, and record which
 * operation is doing the rolling.
 *
 * Not a guess: the backend's gate records `rolling_back` in the journal
 * synchronously, before the dispatch returns, so this only repeats what the journal
 * already says. It's also the whole of the user's feedback that the press landed,
 * since the reversal belongs to the operation queue from that moment on: the row's
 * badge changes under their cursor, and the status corner picks the operation up.
 *
 * `inverseOpId` is the same field a fresh read fills (`OperationRow.inverse_op_id`),
 * carrying the same journal fact, so the row's Pause and Cancel work whether the
 * dialog started this reversal or found it already running. ❌ Not a second source
 * of truth: a dispatch and a re-read can't disagree about which operation the
 * backend just opened.
 */
export function markOperationRollingBack(opId: string, inverseOpId: string): void {
  operationLogState.entries = operationLogState.entries.map((entry) =>
    entry.opId === opId ? { ...entry, rollbackState: 'rollingBack', inverseOpId } : entry,
  )
}

/**
 * Re-read ONE row's header from the journal and swap it in place. Called when the
 * live session a row's controls follow says its reversal has ended, so the badge
 * stops saying "Rolling back" without the dialog polling anything.
 *
 * Safe to fire on that signal: the engine writes the final `rollback_state` (over the
 * writer's synchronous reply channel) before the reversal leaves the registry. A row
 * that's no longer listed, or that the journal no longer has, is left alone.
 */
export async function refreshOperation(opId: string): Promise<void> {
  if (!operationLogState.entries.some((entry) => entry.opId === opId)) return
  try {
    // Zero items: only the header drifts; a finished operation's items don't.
    const detail = await getOperationLogDetail(opId, 0, 0)
    if (detail === null) return
    operationLogState.entries = operationLogState.entries.map((entry) =>
      entry.opId === opId ? detail.operation : entry,
    )
  } catch (e) {
    // The row keeps what it showed; reopening the dialog reads it fresh.
    log.warn("Couldn't refresh operation {opId}: {error}", { opId, error: String(e) })
  }
}
