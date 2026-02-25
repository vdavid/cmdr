// File viewer session commands (open, seek, search, close)

import { invoke } from '@tauri-apps/api/core'

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

/** Sets up a viewer-specific menu on the given window (adds "Word wrap" to View submenu). */
export async function viewerSetupMenu(label: string): Promise<void> {
    await invoke('viewer_setup_menu', { label })
}

/** Syncs the viewer menu "Word wrap" check state (called when toggled via keyboard). */
export async function viewerSetWordWrap(label: string, checked: boolean): Promise<void> {
    await invoke('viewer_set_word_wrap', { label, checked })
}
