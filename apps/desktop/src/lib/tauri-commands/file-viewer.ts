// File viewer session commands (open, seek, search, close)

import {
  commands,
  type FileEncoding,
  type MediaDimensions,
  type RangeEnd,
  type SearchMode as ViewerSearchMode,
  type SearchStatus as ViewerSearchStatus,
  type ViewerContentKind,
  type ViewerError,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type {
  FileEncoding,
  MediaDimensions,
  RangeEnd,
  ViewerContentKind,
  ViewerError,
  ViewerSearchMode,
  ViewerSearchStatus,
}

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
  /** Auto-detected encoding (also the initial picker selection). */
  encoding: FileEncoding
  /**
   * Detected content kind. `text` flows through the line pipeline (the fields above
   * are populated); `image` / `pdf` render inline from `mediaToken` and leave the
   * text fields empty.
   */
  kind: ViewerContentKind
  /**
   * Present only for media kinds (`image` / `pdf`): the unguessable token the FE puts
   * in the `cmdr-media://localhost/<token>` URL. `null` for text.
   */
  mediaToken: string | null
  /**
   * Image pixel dimensions, header-only and best-effort. Set only for some raster
   * images; `null` for HEIC, SVG, PDF, text, or on any read error.
   */
  mediaDimensions: MediaDimensions | null
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

/**
 * Opens a viewer session for a file. Returns session metadata + initial lines.
 *
 * `windowLabel` links the session to the owning viewer window so the backend can
 * free the session when the window is closed via the titlebar X (a path that never
 * fires `viewerClose`). Pass `getCurrentWindow().label`. Defaults to `''` (no
 * mapping) for callers without an owning window.
 */
export async function viewerOpen(path: string, windowLabel = ''): Promise<ViewerOpenResult> {
  const res = await commands.viewerOpen(path, windowLabel)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Opens a fresh full text session for a file, bypassing media detection. Backs the
 * viewer's "View as text" override for an image / PDF: the caller swaps to the new
 * session and closes the old media one. Returns a `kind: 'text'` result with the
 * line fields populated, exactly like a text-detected `viewerOpen`.
 */
export async function viewerOpenAsText(path: string, windowLabel = ''): Promise<ViewerOpenResult> {
  const res = await commands.viewerOpenAsText(path, windowLabel)
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
