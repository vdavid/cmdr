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

import { getRecentOperationLogEntries, type OperationRow } from '$lib/tauri-commands'
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
    // A failed append leaves what's already shown intact; stop offering more.
    operationLogState.hasMore = false
    log.warn("Couldn't load more operations: {error}", { error: String(e) })
  } finally {
    operationLogState.loadingMore = false
  }
}
