/**
 * Shared interface for DualPaneExplorer's exported methods.
 * Used by +page.svelte, command-dispatch.ts, and mcp-listeners.ts.
 */

import type { ViewMode } from '$lib/app-status-store'
import type { FriendlyError } from '$lib/file-explorer/types'

export interface ExplorerAPI {
    refocus: () => void
    switchPane: () => void
    swapPanes: () => void
    toggleVolumeChooser: (pane: 'left' | 'right') => void
    openVolumeChooser: () => void
    closeVolumeChooser: () => void
    toggleHiddenFiles: () => void
    setViewMode: (mode: ViewMode, pane?: 'left' | 'right') => void
    navigate: (action: 'back' | 'forward' | 'parent') => void
    getFileAndPathUnderCursor: () => { path: string; filename: string } | null
    sendKeyToFocusedPane: (key: string) => void
    setSortColumn: (column: 'name' | 'extension' | 'size' | 'modified' | 'created', pane?: 'left' | 'right') => void
    setSortOrder: (order: 'asc' | 'desc' | 'toggle', pane?: 'left' | 'right') => void
    setSort: (
        column: 'name' | 'extension' | 'size' | 'modified' | 'created',
        order: 'asc' | 'desc',
        pane: 'left' | 'right',
    ) => Promise<void>
    getFocusedPane: () => 'left' | 'right'
    getFocusedPanePath: () => string
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
    moveCursor: (pane: 'left' | 'right', to: number | string) => Promise<void>
    scrollTo: (pane: 'left' | 'right', index: number) => void
    refreshPane: () => void
    refreshNetworkHosts: () => void
    injectError: (pane: 'left' | 'right', friendly: FriendlyError) => void
    resetError: (pane: 'left' | 'right' | 'both') => void
    newTab: () => boolean
    closeActiveTab: () => 'closed' | 'last-tab'
    closeActiveTabWithConfirmation: () => Promise<'closed' | 'last-tab' | 'cancelled'>
    cycleTab: (direction: 'next' | 'prev') => void
    togglePinActiveTab: () => void
    closeOtherTabs: () => void
}
