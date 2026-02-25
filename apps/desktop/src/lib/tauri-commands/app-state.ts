// App-level state: MCP pane state, dialog tracking, menu context, window lifecycle

import { invoke } from '@tauri-apps/api/core'

// ============================================================================
// MCP pane state
// ============================================================================

/** File entry for pane state updates. */
export interface PaneFileEntry {
    name: string
    path: string
    isDirectory: boolean
    size?: number
    modified?: string
}

/** State of a single pane. */
export interface PaneState {
    path: string
    volumeId?: string
    volumeName?: string
    files: PaneFileEntry[]
    cursorIndex: number
    viewMode: string
    selectedIndices: number[]
    sortField?: string
    sortOrder?: string
    totalFiles?: number
    loadedStart?: number
    loadedEnd?: number
    showHidden?: boolean
}

/**
 * Update left pane state for MCP context tools.
 */
export async function updateLeftPaneState(state: PaneState): Promise<void> {
    await invoke('update_left_pane_state', { state })
}

/**
 * Update right pane state for MCP context tools.
 */
export async function updateRightPaneState(state: PaneState): Promise<void> {
    await invoke('update_right_pane_state', { state })
}

/**
 * Update focused pane for MCP context tools.
 */
export async function updateFocusedPane(pane: 'left' | 'right'): Promise<void> {
    await invoke('update_focused_pane', { pane })
}

// ============================================================================
// Dialog tracking
// ============================================================================

/** Notify backend that a soft (overlay) dialog opened. */
export async function notifyDialogOpened(dialogType: string): Promise<void> {
    await invoke('notify_dialog_opened', { dialogType })
}

/** Notify backend that a soft (overlay) dialog closed. */
export async function notifyDialogClosed(dialogType: string): Promise<void> {
    await invoke('notify_dialog_closed', { dialogType })
}

/** Register all known soft dialog types with the backend for the MCP "available dialogs" resource. */
export async function registerKnownDialogs(dialogs: readonly { id: string; description?: string }[]): Promise<void> {
    await invoke('register_known_dialogs', { dialogs })
}

// ============================================================================
// Menu context and view settings
// ============================================================================

/**
 * Updates the global menu context (used by app-level File menu).
 * @param path - Absolute path to the file.
 * @param filename - Name of the file.
 */
export async function updateMenuContext(path: string, filename: string): Promise<void> {
    await invoke('update_menu_context', { path, filename })
}

/**
 * Toggle hidden files visibility and sync menu checkbox state.
 * @returns The new state of showHiddenFiles.
 */
export async function toggleHiddenFiles(): Promise<boolean> {
    return invoke<boolean>('toggle_hidden_files')
}

/**
 * Set view mode and sync menu radio button state.
 * @param mode - 'full' or 'brief'
 */
export async function setViewMode(mode: 'full' | 'brief'): Promise<void> {
    await invoke('set_view_mode', { mode })
}

// ============================================================================
// Window lifecycle
// ============================================================================

/**
 * Shows the main window.
 * Should be called when the frontend is ready to avoid white flash.
 */
export async function showMainWindow(): Promise<void> {
    await invoke('show_main_window')
}
