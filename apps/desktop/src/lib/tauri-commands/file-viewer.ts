// File viewer session commands (open, seek, search, close)

import type { UnlistenFn } from '@tauri-apps/api/event'
import type { Window } from '@tauri-apps/api/window'

import {
  commands,
  events,
  type ChunkEnd,
  type FileEncoding,
  type LineChunk,
  type MarkdownLink,
  type MarkdownRender,
  type MediaDimensions,
  type RangeEnd,
  type RowText,
  type SavedAs,
  type SearchMatch as ViewerSearchMatch,
  type SearchMode as ViewerSearchMode,
  type SearchPollResult,
  type SearchStatus as ViewerSearchStatus,
  type SeekTargetKind,
  type TotalRows,
  type ViewerContentKind,
  type ViewerError,
  type ViewerPullProgress,
  type ViewerRow,
} from '#lib/ipc/bindings.js'
import { throwIpcError } from './ipc-types'

export type {
  ChunkEnd,
  FileEncoding,
  LineChunk,
  MediaDimensions,
  RangeEnd,
  RowText,
  SavedAs,
  SearchPollResult,
  TotalRows,
  ViewerContentKind,
  ViewerError,
  ViewerPullProgress,
  ViewerRow,
  ViewerSearchMatch,
  ViewerSearchMode,
  ViewerSearchStatus,
}

/**
 * Listens for `viewer-pull-progress` on ONE viewer window: how far that window's open
 * has pulled its file into a preview temp (a file on a phone or server, a zip entry).
 * Scoped to `window` because the backend emits to the window's label; a global
 * listener would hear every viewer's pull.
 */
export function onViewerPullProgress(
  window: Window,
  handler: (progress: ViewerPullProgress) => void,
): Promise<UnlistenFn> {
  return events.viewerPullProgress(window).listen((event) => {
    handler(event.payload)
  })
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
  /** Physical LINES, when the backend knows them. ❌ Never a row coordinate. */
  totalLines: number | null
  /**
   * The file's ROW count. `initialLines.totalRows` carries the same number plus whether
   * it's counted or sampled, which is what the frontend actually reads.
   */
  estimatedTotalRows: number
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
  /** Rows, and whether that is a count or an estimate. The scroll coordinate. */
  totalRows: TotalRows
  /** Physical lines, when the backend knows them. The status bar's count. */
  totalLines: number | null
}

/**
 * Opens a viewer session for a file. Returns session metadata + initial lines.
 *
 * `volumeId` is the volume the file lives on (`'root'` for the local drive). A file
 * INSIDE a `.zip` on a remote parent (direct SMB / MTP) is previewed by pulling the
 * entry through that volume, so the id must be the pane's, not a hardcoded root.
 *
 * `windowLabel` links the session to the owning viewer window so the backend can
 * free the session when the window is closed via the titlebar X (a path that never
 * fires `viewerClose`). Pass `getCurrentWindow().label`. Defaults to `''` (no
 * mapping) for callers without an owning window.
 *
 * `rows` is how the session shows its rows: `'logDisplay'` for Cmdr's own log (markers
 * hidden, each marked value as itself), `'raw'` for everything else.
 */
export async function viewerOpen(
  path: string,
  volumeId = 'root',
  windowLabel = '',
  rows: RowText = 'raw',
): Promise<ViewerOpenResult> {
  const res = await commands.viewerOpen(path, volumeId, windowLabel, rows)
  if (res.status === 'error') throwViewerError(res.error)
  return res.data
}

/**
 * Throws a viewer failure carrying the typed `ViewerError` on a `viewerError`
 * property, so the caller can branch on the VARIANT (the archive family:
 * `tooLargeToPreview`, `archive`; the deadline: `timedOut`) and render friendly
 * per-variant copy. The `Error.message` is a best-effort diagnostic for logs and
 * generic consumers; ❌ nothing a user reads comes from it.
 */
function throwViewerError(error: ViewerError): never {
  const message = 'message' in error ? error.message : error.kind
  throw Object.assign(new Error(message), { viewerError: error })
}

/** The typed viewer refusal behind a caught value, or `null` when it isn't one. */
export function asViewerError(error: unknown): ViewerError | null {
  if (typeof error !== 'object' || error === null) return null
  const carried = (error as { viewerError?: unknown }).viewerError
  return typeof carried === 'object' && carried !== null && 'kind' in carried ? (carried as ViewerError) : null
}

/**
 * Opens a fresh full text session for a file, bypassing media detection. Backs the
 * viewer's "View as text" override for an image / PDF: the caller swaps to the new
 * session and closes the old media one. Returns a `kind: 'text'` result with the
 * line fields populated, exactly like a text-detected `viewerOpen`.
 */
export async function viewerOpenAsText(path: string, volumeId = 'root', windowLabel = ''): Promise<ViewerOpenResult> {
  const res = await commands.viewerOpenAsText(path, volumeId, windowLabel)
  if (res.status === 'error') throwViewerError(res.error)
  return res.data
}

/**
 * Fetches lines from a viewer session.
 *
 * A refusal throws the backend's typed `ViewerError`; `asViewerError` gets it
 * back, so the deadline is a variant (`timedOut`) rather than a flag beside a
 * sentence.
 */
export async function viewerGetLines(
  sessionId: string,
  targetType: SeekTargetKind,
  targetValue: number,
  count: number,
): Promise<LineChunk> {
  const res = await commands.viewerGetLines(sessionId, targetType, targetValue, count)
  if (res.status === 'error') throwViewerError(res.error)
  return res.data
}

/** Bounded raw bytes for the binary and hex viewer modes. */
export async function viewerGetBytes(sessionId: string, offset: number, count: number): Promise<number[]> {
  const res = await commands.viewerGetBytes(sessionId, offset, count)
  if (res.status === 'error') throwViewerError(res.error)
  return res.data
}

/**
 * A Markdown session's document as sanitized HTML. Its images carry `data-md-image`
 * markers (`local:<token>`, `remote:<token>`, `blocked`) instead of a `src`.
 */
export async function viewerRenderMarkdown(sessionId: string): Promise<MarkdownRender> {
  const res = await commands.viewerRenderMarkdown(sessionId)
  if (res.status === 'error') throwViewerError(res.error)
  return res.data
}

/** The user's "Load remote images" for one Markdown session. */
export async function viewerAllowRemoteImages(sessionId: string): Promise<void> {
  const res = await commands.viewerAllowRemoteImages(sessionId)
  if (res.status === 'error') throwViewerError(res.error)
}

/**
 * Follows a link clicked in a Markdown session's rendered view. A web or mail link has
 * already opened in the user's browser or mail app when this returns; the rest says what
 * the viewer does: scroll to an anchor, open a file in this window, or nothing.
 */
export async function viewerFollowMarkdownLink(sessionId: string, href: string): Promise<MarkdownLink> {
  const res = await commands.viewerFollowMarkdownLink(sessionId, href)
  if (res.status === 'error') throwViewerError(res.error)
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
 * Pops the viewer's native right-click menu (Copy, Select all) at the pointer. Copy is greyed
 * unless `hasSelection`; the pick comes back through `onViewerContextMenuAction`.
 */
export async function showViewerContextMenu(hasSelection: boolean): Promise<void> {
  const res = await commands.showViewerContextMenu(hasSelection)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Tells the viewer menu bar whether this viewer's search box holds keyboard focus, which is what
 * decides whether its Edit > Cut and Edit > Paste look live: that box is the only editable field
 * in the window. Push it on the input's focus and blur, when the search bar closes, and on the
 * window's focus-gain, so a switch between two viewers can't leave the shared bar stale.
 */
export async function viewerSetSearchInputFocused(label: string, focused: boolean): Promise<void> {
  const res = await commands.viewerSetSearchInputFocused(label, focused)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Tells the viewer menu bar whether this viewer can step back or forward through the
 * documents its links led to, which greys Go > Back and Go > Forward. Push it when the
 * history changes while this window has focus, and on the window's focus-gain.
 */
export async function viewerSetHistoryState(canGoBack: boolean, canGoForward: boolean): Promise<void> {
  const res = await commands.viewerSetHistoryState(canGoBack, canGoForward)
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
 * `Cancelled` / `TimedOut` / other variants surface as for `viewerReadRange`. On success,
 * `savedAs` says whether the file is UTF-8 text or the source's own encoding (when the
 * selection held characters the decode lost).
 */
export async function viewerWriteRangeToFile(
  sessionId: string,
  readId: number,
  anchor: RangeEnd,
  focus: RangeEnd,
  destPath: string,
): Promise<{ ok: true; savedAs: SavedAs } | { ok: false; error: ViewerError }> {
  const res = await commands.viewerWriteRangeToFile(sessionId, readId, anchor, focus, destPath)
  if (res.status === 'ok') return { ok: true, savedAs: res.data }
  return { ok: false, error: res.error }
}

// Callers branch on the typed `Result`/error discriminant, so these are
// passthroughs — they don't unwrap the generated `Result` shape.

/** Switches the active encoding for a session. */
export function viewerSetEncoding(sessionId: string, encoding: FileEncoding) {
  return commands.viewerSetEncoding(sessionId, encoding)
}

/** Toggles tail mode for a viewer session (auto-follow new bytes as the file grows). */
export function viewerSetTailMode(sessionId: string, enabled: boolean) {
  return commands.viewerSetTailMode(sessionId, enabled)
}

/** Reopens the viewer's backend against the file on disk under the session's current encoding. */
export function viewerReload(sessionId: string) {
  return commands.viewerReload(sessionId)
}

/** Fetches the encoding-picker options (all, detected, current) for a text session. */
export function viewerGetEncodingOptions(sessionId: string) {
  return commands.viewerGetEncodingOptions(sessionId)
}
