/**
 * Shared interface for DualPaneExplorer's exported methods.
 * Used by +page.svelte, command-dispatch.ts, and mcp-listeners.ts.
 */

import type { ViewMode } from '$lib/app-status-store'
import type { McpSelectMode, McpTabAction, ConfirmDialogType } from '$lib/commands'
import type { QuickLookKeyEventPayload } from '$lib/file-explorer/quick-look/quick-look-state.svelte'
import type { FileEntry, FriendlyError, TransferOperationType } from '$lib/file-explorer/types'
import type { NavigateIntent, NavigateResult } from '$lib/file-explorer/pane/navigate'

/**
 * Closed action set for `handleSelectionAction` (the selection sub-dispatcher).
 * `clear` and `deselectAll` both clear; `selectRange` uses the index args.
 */
export type SelectionAction =
  | 'clear'
  | 'deselectAll'
  | 'selectAll'
  | 'toggleAtCursor'
  | 'toggleAtCursorAndMoveDown'
  | 'selectRange'

export interface ExplorerAPI {
  refocus: () => void
  switchPane: () => void
  swapPanes: () => void
  copyPathBetweenPanes: (source: 'left' | 'right', target: 'left' | 'right') => void
  toggleVolumeChooser: (pane: 'left' | 'right') => void
  openVolumeChooser: () => void
  closeVolumeChooser: () => void
  toggleHiddenFiles: () => boolean
  setViewMode: (mode: ViewMode, pane?: 'left' | 'right') => void
  /**
   * Sets a specific pane's view mode in response to a native-menu click
   * (`view-mode-changed`). Same set + persist as `setViewMode`, but WITHOUT
   * `pushViewMenuState`: the menu already toggled its own CheckMenuItem on click,
   * so pushing the state back would double-sync against Rust's
   * `sync_view_mode_check_states`. Focus-preserving (the target pane changes even
   * when the other pane is focused).
   */
  setViewModeFromMenu: (pane: 'left' | 'right', mode: ViewMode) => void
  /**
   * The single coordinator-level navigation entry. Replaces the old
   * `navigate(action)` + `navigateToPath(pane, path)` pair: pass a typed
   * `NavigateIntent` (volume/path change, history walk, or snapshot open) and get
   * back a `NavigateResult` — `{ status: 'started', settled }` or
   * `{ status: 'refused', reason }` whose `reason.message` is the exact refusal
   * string the MCP adapter forwards verbatim (L12).
   */
  navigate: (intent: NavigateIntent) => NavigateResult
  getFileAndPathUnderCursor: () => { path: string; filename: string } | null
  sendKeyToFocusedPane: (key: string) => void
  /**
   * Routes a key event received from the native Quick Look panel back into the
   * focused pane's navigation primitives. Used while the panel is key (the panel
   * delegate forwards keys it didn't want via the `quick-look-key` Tauri event).
   * Implementation keeps this narrow: arrow / page / home / end / type-to-jump
   * letters; everything else is ignored. Shift+Space close is handled by the
   * listener directly, not via this method.
   */
  routePanelKey: (payload: QuickLookKeyEventPayload) => void
  openItemUnderCursor: () => Promise<void>
  setSortColumn: (column: 'name' | 'extension' | 'size' | 'modified' | 'created', pane?: 'left' | 'right') => void
  setSortOrder: (order: 'asc' | 'desc' | 'toggle', pane?: 'left' | 'right') => void
  setSort: (
    column: 'name' | 'extension' | 'size' | 'modified' | 'created',
    order: 'asc' | 'desc',
    pane: 'left' | 'right',
  ) => Promise<void>
  getFocusedPane: () => 'left' | 'right'
  selectVolumeByName: (pane: 'left' | 'right', name: string) => Promise<boolean>
  handleSelectionAction: (action: SelectionAction, startIndex?: number, endIndex?: number) => void
  handleMcpSelect: (pane: 'left' | 'right', start: number, count: number | 'all', mode: McpSelectMode) => Promise<void>
  /**
   * By-name selection for the MCP `select` tool's `names` mode. Throws when the
   * pane is unavailable or any name isn't in the listing (the MCP adapter
   * forwards the message as the round-trip error).
   */
  handleMcpSelectNames: (pane: 'left' | 'right', names: string[], mode: McpSelectMode) => Promise<void>
  /**
   * Per-pane tab action from the MCP `tab` tool. Targets a SPECIFIC pane (and
   * optionally a specific tab), unlike the focused-pane `newTab`/`cycleTab`/etc.
   */
  handleMcpTabAction: (pane: 'left' | 'right', action: McpTabAction, tabId?: string, pinned?: boolean) => void
  startRename: () => void
  openCopyDialog: (autoConfirm?: boolean, onConflict?: string) => Promise<void>
  openMoveDialog: (autoConfirm?: boolean, onConflict?: string) => Promise<void>
  copyToClipboard: () => Promise<void>
  cutToClipboard: () => Promise<void>
  pasteFromClipboard: (forceMove: boolean) => Promise<void>
  openNewFolderDialog: () => Promise<void>
  openNewFileDialog: () => Promise<void>
  openDeleteDialog: (permanent: boolean, autoConfirm?: boolean) => Promise<void>
  closeConfirmationDialog: () => void
  confirmDialog: (dialogType: ConfirmDialogType, onConflict?: string) => void
  isConfirmationDialogOpen: () => boolean
  isRenaming: () => boolean
  openViewerForCursor: () => Promise<void>
  /**
   * Open a search-results snapshot in the target pane (defaults to focused).
   * The snapshot must already exist in `$lib/search/snapshot-store.svelte`; the
   * caller is responsible for `getOrCreate` + `setLastAttemptId` (the
   * SearchDialog's "Open in pane" handler does both). Routes through
   * `navigate({ to: { snapshot } })` so pinned-tab fork, focus, and history push
   * all apply.
   */
  openSearchSnapshotInPane: (snapshotId: string, pane?: 'left' | 'right') => void
  moveCursor: (pane: 'left' | 'right', to: number | string) => Promise<void>
  scrollTo: (pane: 'left' | 'right', index: number) => void
  refreshPane: () => Promise<void>
  refreshNetworkHosts: () => void
  injectError: (pane: 'left' | 'right', friendly: FriendlyError) => void
  resetError: (pane: 'left' | 'right' | 'both') => void
  triggerTransferError: (friendly: FriendlyError) => void
  /** E2E only: drive the native drag-and-drop drop entry programmatically (real
   *  OS drag can't be synthesized in Playwright). Wired only behind the E2E gate
   *  in `+page.svelte`; never reachable in production.
   *
   *  `recordedIdentity` models an IN-APP self-drag: the drop builds its transfer
   *  from the recorded source volume + the paths the volume knows (volume-relative
   *  for MTP/SMB), exactly as a real self-drag does, instead of resolving the
   *  pasteboard paths. Omit it to model a genuine EXTERNAL drop (local absolute
   *  paths through the resolver). */
  triggerFileDrop: (
    paths: string[],
    targetPane: 'left' | 'right',
    targetFolderPath?: string,
    operation?: TransferOperationType,
    recordedIdentity?: { sourceVolumeId: string; sourcePaths: string[] },
  ) => void
  newTab: () => boolean
  closeActiveTabWithConfirmation: () => Promise<'closed' | 'last-tab' | 'cancelled'>
  reopenLastClosedTab: () => 'reopened' | 'empty' | 'cap'
  cycleTab: (direction: 'next' | 'prev') => void
  togglePinActiveTab: () => void
  closeOtherTabs: () => void
  /**
   * Bulk-applies matched indices to the focused pane's selection set. Used by the
   * Selection dialog on commit.
   */
  applyIndicesToFocusedPane: (idxs: number[], mode: 'add' | 'remove') => void
  /**
   * Returns a snapshot of the focused pane's entries + cursor index for the Selection
   * dialog. Captured ONCE at dialog open; the dialog does not refresh on mid-dialog
   * focused-pane change.
   */
  getFocusedPaneEntries: () => Promise<{
    entries: FileEntry[]
    cursorIndex: number
    isSnapshotPane: boolean
  }>
}
