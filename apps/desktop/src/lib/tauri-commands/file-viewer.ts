// File viewer commands

import { invoke } from '@tauri-apps/api/core'
import { openPath, openUrl } from '@tauri-apps/plugin-opener'
import type { SyncStatus } from '../file-explorer/types'

/** A chunk of lines returned by the viewer backend. */
export interface LineChunk {
    lines: string[]
    firstLineNumber: number
    byteOffset: number
    totalLines: number | null
    totalBytes: number
}

/** Backend capabilities. */
export interface BackendCapabilities {
    supportsLineSeek: boolean
    supportsByteSeek: boolean
    supportsFractionSeek: boolean
    knowsTotalLines: boolean
}

/** Result from opening a viewer session. */
export interface ViewerOpenResult {
    sessionId: string
    fileName: string
    totalBytes: number
    totalLines: number | null
    /** Estimated total lines based on initial sample (for ByteSeek where totalLines is unknown) */
    estimatedTotalLines: number
    backendType: 'fullLoad' | 'byteSeek' | 'lineIndex'
    capabilities: BackendCapabilities
    initialLines: LineChunk
    /** Whether background indexing is in progress */
    isIndexing: boolean
}

/** Current status of a viewer session. */
export interface ViewerSessionStatus {
    backendType: 'fullLoad' | 'byteSeek' | 'lineIndex'
    isIndexing: boolean
    totalLines: number | null
}

/** A search match found in the file. */
export interface ViewerSearchMatch {
    line: number
    column: number
    length: number
}

/** Result from polling search progress. */
export interface SearchPollResult {
    status: 'running' | 'done' | 'cancelled' | 'idle'
    matches: ViewerSearchMatch[]
    totalBytes: number
    bytesScanned: number
}

/** Opens a viewer session for a file. Returns session metadata + initial lines. */
export async function viewerOpen(path: string): Promise<ViewerOpenResult> {
    return invoke<ViewerOpenResult>('viewer_open', { path })
}

/** Fetches lines from a viewer session. */
export async function viewerGetLines(
    sessionId: string,
    targetType: 'line' | 'byte' | 'fraction',
    targetValue: number,
    count: number,
): Promise<LineChunk> {
    return invoke<LineChunk>('viewer_get_lines', { sessionId, targetType, targetValue, count })
}

/** Starts a background search in the viewer session. */
export async function viewerSearchStart(sessionId: string, query: string): Promise<void> {
    await invoke('viewer_search_start', { sessionId, query })
}

/** Polls search progress and matches. */
export async function viewerSearchPoll(sessionId: string): Promise<SearchPollResult> {
    return invoke<SearchPollResult>('viewer_search_poll', { sessionId })
}

/** Cancels an ongoing search. */
export async function viewerSearchCancel(sessionId: string): Promise<void> {
    await invoke('viewer_search_cancel', { sessionId })
}

/** Gets the current status of a viewer session (backend type, indexing state). */
export async function viewerGetStatus(sessionId: string): Promise<ViewerSessionStatus> {
    return invoke<ViewerSessionStatus>('viewer_get_status', { sessionId })
}

/** Closes a viewer session and frees resources. */
export async function viewerClose(sessionId: string): Promise<void> {
    await invoke('viewer_close', { sessionId })
}

/**
 * Opens a file with the system's default application.
 * @param path - Path to the file to open.
 */
export async function openFile(path: string): Promise<void> {
    await openPath(path)
}

/**
 * Opens a URL in the system's default browser.
 * @param url - URL to open (e.g., "https://getcmdr.com/renew")
 */
export async function openExternalUrl(url: string): Promise<void> {
    await openUrl(url)
}

/**
 * Gets icon data URLs for the requested icon IDs.
 * @param iconIds - Array of icon IDs like "ext:jpg", "dir", "symlink"
 * @param useAppIconsForDocuments - Whether to use app icons as fallback for documents
 * @returns Map of icon_id -> base64 WebP data URL
 */
export async function getIcons(iconIds: string[], useAppIconsForDocuments: boolean): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('get_icons', { iconIds, useAppIconsForDocuments })
}

/**
 * Refreshes icons for a directory listing.
 * Fetches icons in parallel for directories (by path) and extensions.
 * @param directoryPaths - Array of directory paths to fetch icons for
 * @param extensions - Array of file extensions (without dot)
 * @param useAppIconsForDocuments - Whether to use app icons as fallback for documents
 * @returns Map of icon_id -> base64 WebP data URL
 */
export async function refreshDirectoryIcons(
    directoryPaths: string[],
    extensions: string[],
    useAppIconsForDocuments: boolean,
): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('refresh_directory_icons', {
        directoryPaths,
        extensions,
        useAppIconsForDocuments,
    })
}

/**
 * Clears cached extension icons.
 * Called when the "use app icons for documents" setting changes.
 */
export async function clearExtensionIconCache(): Promise<void> {
    await invoke('clear_extension_icon_cache')
}

/**
 * Shows a native context menu for a file.
 * @param path - Absolute path to the file.
 * @param filename - Name of the file.
 * @param isDirectory - Whether the entry is a directory.
 */
export async function showFileContextMenu(path: string, filename: string, isDirectory: boolean): Promise<void> {
    await invoke('show_file_context_menu', { path, filename, isDirectory })
}

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
// MCP pane state commands
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

/** Notify backend that a soft (overlay) dialog opened. */
export async function notifyDialogOpened(dialogType: string): Promise<void> {
    await invoke('notify_dialog_opened', { dialogType })
}

/** Notify backend that a soft (overlay) dialog closed. */
export async function notifyDialogClosed(dialogType: string): Promise<void> {
    await invoke('notify_dialog_closed', { dialogType })
}

// ============================================================================
// File action commands (for command palette)
// ============================================================================

/**
 * Show a file in Finder (reveal in parent folder).
 * @param path - Absolute path to the file.
 */
export async function showInFinder(path: string): Promise<void> {
    await invoke('show_in_finder', { path })
}

/**
 * Copy text to clipboard.
 * @param text - Text to copy.
 */
export async function copyToClipboard(text: string): Promise<void> {
    await invoke('copy_to_clipboard', { text })
}

/**
 * Quick Look preview (macOS only).
 * @param path - Absolute path to the file.
 */
export async function quickLook(path: string): Promise<void> {
    await invoke('quick_look', { path })
}

/**
 * Open Get Info window in Finder (macOS only).
 * @param path - Absolute path to the file.
 */
export async function getInfo(path: string): Promise<void> {
    await invoke('get_info', { path })
}

/**
 * Open file in the system's default text editor (macOS only).
 * Uses `open -t` which opens the file in the default text editor.
 * @param path - Absolute path to the file.
 */
export async function openInEditor(path: string): Promise<void> {
    await invoke('open_in_editor', { path })
}

/**
 * Shows the main window.
 * Should be called when the frontend is ready to avoid white flash.
 */
export async function showMainWindow(): Promise<void> {
    await invoke('show_main_window')
}

/**
 * Gets sync status for multiple file paths.
 * Returns a map of path -> sync status.
 * Only works on macOS with files in cloud-synced folders (Dropbox, iCloud, etc.)
 * @param paths - Array of absolute file paths.
 * @returns Map of path -> SyncStatus
 */
export async function getSyncStatus(paths: string[]): Promise<Record<string, SyncStatus>> {
    try {
        return await invoke<Record<string, SyncStatus>>('get_sync_status', { paths })
    } catch {
        // Command not available (non-macOS) - return empty map
        return {}
    }
}

/**
 * Stores font metrics for a font configuration.
 * @param fontId - Font identifier (e.g., "system-400-12")
 * @param widths - Map of code point -> width in pixels
 */
export async function storeFontMetrics(fontId: string, widths: Record<number, number>): Promise<void> {
    await invoke('store_font_metrics', { fontId, widths })
}

/**
 * Checks if font metrics are available for a font ID.
 * @param fontId - Font identifier to check
 * @returns True if metrics are cached
 */
export async function hasFontMetrics(fontId: string): Promise<boolean> {
    return invoke<boolean>('has_font_metrics', { fontId })
}
