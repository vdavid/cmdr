// File viewer session commands (open, seek, search, close)

import {
  commands,
  type RangeEnd,
  type SearchMode as ViewerSearchMode,
  type SearchStatus as ViewerSearchStatus,
  type ViewerError,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type { RangeEnd, ViewerError, ViewerSearchMode, ViewerSearchStatus }

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
  /**
   * Tagged-union status. `invalidQuery` carries the user-facing reason as plain text;
   * the FE renders the message verbatim (no string inspection — see the
   * no-error-string-match rule).
   */
  status: ViewerSearchStatus
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
export async function viewerSearchStart(sessionId: string, query: string, mode: ViewerSearchMode): Promise<void> {
  const res = await commands.viewerSearchStart(sessionId, query, mode)
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

/**
 * Reads a logical `(line, offset)` range of the file as a single UTF-8 string.
 *
 * Returns a typed result so the caller can match on the `ViewerError` variant tag
 * (per the no-string-classification rule). Specifically, `kind: 'cancelled'` is the
 * expected outcome after `viewerCancelRead` lands, and `kind: 'timedOut'` reports an
 * IPC-level timeout (read continues if the per-read cancel flag isn't set).
 */
export async function viewerReadRange(
  sessionId: string,
  readId: number,
  anchor: RangeEnd,
  focus: RangeEnd,
): Promise<{ ok: true; text: string } | { ok: false; error: ViewerError }> {
  const res = await commands.viewerReadRange(sessionId, readId, anchor, focus)
  if (res.status === 'ok') return { ok: true, text: res.data }
  return { ok: false, error: res.error }
}

/**
 * Flips the cancel flag for an in-flight range read. Returns the typed result; an
 * unknown `readId` is treated as a no-op on the backend, so the caller can fire-and-
 * forget without checking.
 */
export async function viewerCancelRead(
  sessionId: string,
  readId: number,
): Promise<{ ok: true } | { ok: false; error: ViewerError }> {
  const res = await commands.viewerCancelRead(sessionId, readId)
  if (res.status === 'ok') return { ok: true }
  return { ok: false, error: res.error }
}

/**
 * Reads a range and writes it to `destPath` atomically. Used by the "Save as file..."
 * action in the copy-confirm and copy-refuse dialogs. Returns a typed result; the same
 * `Cancelled` / `TimedOut` / other variants surface as for `viewerReadRange`.
 */
export async function viewerWriteRangeToFile(
  sessionId: string,
  readId: number,
  anchor: RangeEnd,
  focus: RangeEnd,
  destPath: string,
): Promise<{ ok: true } | { ok: false; error: ViewerError }> {
  const res = await commands.viewerWriteRangeToFile(sessionId, readId, anchor, focus, destPath)
  if (res.status === 'ok') return { ok: true }
  return { ok: false, error: res.error }
}
