// File viewer session commands (open, seek, search, close)

import { commands } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

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
  /** Byte offset of the line start. Used for accurate scroll positioning in ByteSeek mode. */
  byteOffset: number
}

/** Result from polling search progress. */
export interface SearchPollResult {
  status: 'running' | 'done' | 'cancelled' | 'idle'
  /** Only matches discovered since the caller's `sinceIndex`. Accumulate locally. */
  newMatches: ViewerSearchMatch[]
  /** Authoritative total match count (including matches the caller already has). */
  totalMatchCount: number
  totalBytes: number
  bytesScanned: number
  /** True when match count was capped (search kept scanning for progress but stopped storing) */
  matchLimitReached: boolean
}

/** Opens a viewer session for a file. Returns session metadata + initial lines. */
export async function viewerOpen(path: string): Promise<ViewerOpenResult> {
  const res = await commands.viewerOpen(path)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Fetches lines from a viewer session. */
export async function viewerGetLines(
  sessionId: string,
  targetType: 'line' | 'byte' | 'fraction',
  targetValue: number,
  count: number,
): Promise<LineChunk> {
  const res = await commands.viewerGetLines(sessionId, targetType, targetValue, count)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Starts a background search in the viewer session. */
export async function viewerSearchStart(sessionId: string, query: string): Promise<void> {
  const res = await commands.viewerSearchStart(sessionId, query)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Polls search progress and new matches since `sinceIndex`. */
export async function viewerSearchPoll(sessionId: string, sinceIndex: number): Promise<SearchPollResult> {
  const res = await commands.viewerSearchPoll(sessionId, sinceIndex)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Cancels an ongoing search. */
export async function viewerSearchCancel(sessionId: string): Promise<void> {
  const res = await commands.viewerSearchCancel(sessionId)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Gets the current status of a viewer session (backend type, indexing state). */
export async function viewerGetStatus(sessionId: string): Promise<ViewerSessionStatus> {
  const res = await commands.viewerGetStatus(sessionId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Closes a viewer session and frees resources. */
export async function viewerClose(sessionId: string): Promise<void> {
  const res = await commands.viewerClose(sessionId)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Sets up a viewer-specific menu on the given window (adds "Word wrap" to View submenu). */
export async function viewerSetupMenu(label: string): Promise<void> {
  const res = await commands.viewerSetupMenu(label)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Syncs the viewer menu "Word wrap" check state (called when toggled via keyboard). */
export async function viewerSetWordWrap(label: string, checked: boolean): Promise<void> {
  const res = await commands.viewerSetWordWrap(label, checked)
  if (res.status === 'error') throwIpcError(res.error)
}
