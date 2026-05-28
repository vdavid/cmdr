/**
 * Shared interface for DualPaneExplorer's exported methods.
 * Used by +page.svelte, command-dispatch.ts, and mcp-listeners.ts.
 */

import type { ViewMode } from '$lib/app-status-store'
import type { QuickLookKeyEventPayload } from '$lib/file-explorer/quick-look/quick-look-state.svelte'
import type { FileEntry, FriendlyError } from '$lib/file-explorer/types'

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
  navigate: (action: 'back' | 'forward' | 'parent') => void
  getFileAndPathUnderCursor: () => { path: string; filename: string } | null
  /**
   * Volume id of the focused pane's current tab. Used by Quick Look's open/setPath
   * IPC, which gates non-local-fs volumes (MTP) on the backend.
   */
  getFocusedPaneVolumeId: () => string
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
  getFocusedPanePath: () => string
  /**
   * "Smart current folder" for the Search-in popover. When the focused pane is a
   * `search-results://` snapshot the host walks back through history to find the most
   * recent real folder; when none is reachable it surfaces a disabled state with
   * `disabledReason` as the tooltip. See `lib/search/searchable-folder.ts`.
   */
  getFocusedPaneSearchableFolder: () => {
    path: string | null
    disabled: boolean
    disabledReason: string
  }
  getVolumes: () => { id: string; name: string; path: string }[]
  selectVolumeByIndex: (pane: 'left' | 'right', index: number) => Promise<boolean>
  selectVolumeByName: (pane: 'left' | 'right', name: string) => Promise<boolean>
  handleSelectionAction: (action: string, startIndex?: number, endIndex?: number) => void
  handleMcpSelect: (pane: 'left' | 'right', start: number, count: number | 'all', mode: string) => void
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
  confirmDialog: (dialogType: string, onConflict?: string) => void
  isConfirmationDialogOpen: () => boolean
  isRenaming: () => boolean
  openViewerForCursor: () => Promise<void>
  navigateToPath: (pane: 'left' | 'right', path: string) => string | Promise<void>
  /**
   * Open a search-results snapshot in the target pane (defaults to focused).
   * The snapshot must already exist in `$lib/search/snapshot-store.svelte`; the
   * caller is responsible for `getOrCreate` + `setLastAttemptId` (the
   * SearchDialog's "Open in pane" handler does both). Routes through
   * `handleVolumeChange` so pinned-tab fork, focus, and history push all apply.
   */
  openSearchSnapshotInPane: (snapshotId: string, pane?: 'left' | 'right') => void
  moveCursor: (pane: 'left' | 'right', to: number | string) => Promise<void>
  scrollTo: (pane: 'left' | 'right', index: number) => void
  refreshPane: () => void
  refreshNetworkHosts: () => void
  injectError: (pane: 'left' | 'right', friendly: FriendlyError) => void
  resetError: (pane: 'left' | 'right' | 'both') => void
  triggerTransferError: (friendly: FriendlyError) => void
  newTab: () => boolean
  closeActiveTab: () => 'closed' | 'last-tab'
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
