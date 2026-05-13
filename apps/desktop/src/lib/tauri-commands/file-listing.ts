// On-demand virtual scrolling API (listing-based), sync status, font metrics

import { commands } from '$lib/ipc/bindings'
import type {
  FileEntry,
  ListingStats,
  ResortResult,
  SortColumn,
  SortOrder,
  StreamingListingStartResult,
  SyncStatus,
} from '../file-explorer/types'
import type { TimedOut } from './ipc-types'
import { throwIpcError } from './ipc-types'
import type { DirectorySortMode } from '$lib/settings'

/**
 * Starts a new streaming directory listing.
 * Returns immediately with listing ID and "loading" status.
 * Progress is reported via events: listing-progress, listing-complete, listing-error, listing-cancelled.
 * @param volumeId - Volume ID (like "root", "mtp-336592896:65537").
 * @param path - Directory path to list. Supports tilde expansion (~) for local volumes.
 * @param includeHidden - Whether to include hidden files in total count.
 * @param sortBy - Column to sort by.
 * @param sortOrder - Ascending or descending.
 * @param listingId - Unique identifier for the listing (used for cancellation)
 * @param directorySortMode - How to sort directories: like files or always by name.
 */
export async function listDirectoryStart(
  volumeId: string,
  path: string,
  includeHidden: boolean,
  sortBy: SortColumn,
  sortOrder: SortOrder,
  listingId: string,
  directorySortMode?: DirectorySortMode,
): Promise<StreamingListingStartResult> {
  const res = await commands.listDirectoryStartStreaming(
    volumeId,
    path,
    includeHidden,
    sortBy,
    sortOrder,
    directorySortMode ?? null,
    listingId,
  )
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Cancels an in-progress streaming directory listing.
 * The task will emit a listing-cancelled event when it stops.
 * @param listingId - The listing ID to cancel.
 */
export async function cancelListing(listingId: string): Promise<void> {
  await commands.cancelListing(listingId)
}

/**
 * Re-sorts an existing cached listing in-place.
 * More efficient than creating a new listing when you just want to change the sort order.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param sortBy - Column to sort by.
 * @param sortOrder - Ascending or descending.
 * @param cursorFilename - Optional filename to track; returns its new index after sorting.
 * @param includeHidden - Whether to include hidden files when calculating cursor index.
 * @param selectedIndices - Optional indices of selected files to track through re-sort.
 * @param allSelected - If true, all files are selected (optimization).
 * @param directorySortMode - How to sort directories: like files or always by name.
 * @public
 */
export async function resortListing(
  listingId: string,
  sortBy: SortColumn,
  sortOrder: SortOrder,
  cursorFilename: string | undefined,
  includeHidden: boolean,
  selectedIndices?: number[],
  allSelected?: boolean,
  directorySortMode?: DirectorySortMode,
): Promise<ResortResult> {
  const res = await commands.resortListing(
    listingId,
    sortBy,
    sortOrder,
    directorySortMode ?? null,
    cursorFilename ?? null,
    includeHidden,
    selectedIndices ?? null,
    allSelected ?? null,
  )
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Gets a range of entries from a cached listing.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param start - Start index (0-based).
 * @param count - Number of entries to return.
 * @param includeHidden - Whether to include hidden files.
 */
export async function getFileRange(
  listingId: string,
  start: number,
  count: number,
  includeHidden: boolean,
): Promise<FileEntry[]> {
  const res = await commands.getFileRange(listingId, start, count, includeHidden)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data as FileEntry[]
}

/**
 * Gets total count of entries in a cached listing.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param includeHidden - Whether to include hidden files in count.
 */
export async function getTotalCount(listingId: string, includeHidden: boolean): Promise<number> {
  const res = await commands.getTotalCount(listingId, includeHidden)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Finds the index of a file by name in a cached listing.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param name - File name to find.
 * @param includeHidden - Whether to include hidden files when calculating index.
 */
export async function findFileIndex(listingId: string, name: string, includeHidden: boolean): Promise<number | null> {
  const res = await commands.findFileIndex(listingId, name, includeHidden)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Finds the indices of multiple files by name in a cached listing (batch version of `findFileIndex`).
 * Returns only found names as keys; removed files are absent from the map.
 */
export async function findFileIndices(
  listingId: string,
  names: string[],
  includeHidden: boolean,
): Promise<Record<string, number>> {
  const res = await commands.findFileIndices(listingId, names, includeHidden)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Returns the backend index of the highest-scoring fuzzy match for `query`,
 * or `null` if nothing matches or the listing is empty. Powers the type-to-jump
 * feature. The result is a BACKEND index — the caller must add 1 when the
 * listing has a synthetic ".." parent.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param query - The buffer the user has typed. Lowercased before scoring.
 * @param includeHidden - Whether dotfiles count as candidates.
 */
export async function findFirstFuzzyMatch(
  listingId: string,
  query: string,
  includeHidden: boolean,
): Promise<number | null> {
  const res = await commands.findFirstFuzzyMatch(listingId, query, includeHidden)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Gets a single file at the given index.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param index - Index of the file to get.
 * @param includeHidden - Whether to include hidden files when calculating index.
 */
export async function getFileAt(listingId: string, index: number, includeHidden: boolean): Promise<FileEntry | null> {
  const res = await commands.getFileAt(listingId, index, includeHidden)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data as FileEntry | null
}

/**
 * Gets file paths at specific frontend indices from a cached listing (batch).
 * Handles the parent ".." offset internally — pass frontend indices directly.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param selectedIndices - Frontend indices of selected files.
 * @param includeHidden - Whether hidden files are shown (affects index mapping).
 * @param hasParent - Whether the ".." entry is shown at index 0.
 */
export async function getPathsAtIndices(
  listingId: string,
  selectedIndices: number[],
  includeHidden: boolean,
  hasParent: boolean,
): Promise<string[]> {
  const res = await commands.getPathsAtIndices(listingId, selectedIndices, includeHidden, hasParent)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Gets full FileEntry objects at specific backend indices from a cached listing (batch).
 * Callers are responsible for any parent offset adjustment before passing indices.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param selectedIndices - Backend indices of selected files.
 * @param includeHidden - Whether hidden files are shown (affects index mapping).
 */
export async function getFilesAtIndices(
  listingId: string,
  selectedIndices: number[],
  includeHidden: boolean,
): Promise<FileEntry[]> {
  const res = await commands.getFilesAtIndices(listingId, selectedIndices, includeHidden)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data as FileEntry[]
}

/**
 * Ends a directory listing and cleans up the cache.
 * @param listingId - The listing ID to clean up.
 */
export async function listDirectoryEnd(listingId: string): Promise<void> {
  await commands.listDirectoryEnd(listingId)
}

/** Force a re-read of a watched listing, emitting any diff. */
export async function refreshListing(listingId: string): Promise<TimedOut<null>> {
  return commands.refreshListing(listingId)
}

/**
 * Gets statistics about a cached listing.
 * Returns total file/dir counts and sizes. If selectedIndices is provided,
 * also returns statistics for the selected items.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param includeHidden - Whether to include hidden files in calculations.
 * @param selectedIndices - Optional indices of selected files to calculate selection stats.
 */
/** Re-enriches cached listing entries with fresh drive index data (recursive_size). */
export async function refreshListingIndexSizes(listingId: string): Promise<void> {
  const res = await commands.refreshListingIndexSizes(listingId)
  if (res.status === 'error') throwIpcError(res.error)
}

export async function getListingStats(
  listingId: string,
  includeHidden: boolean,
  selectedIndices?: number[],
): Promise<ListingStats> {
  const res = await commands.getListingStats(listingId, includeHidden, selectedIndices ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Starts a native drag operation for selected files from a cached listing.
 * This initiates the drag from Rust directly, avoiding IPC transfer of file paths.
 * The backend publishes a permissive operation mask (Copy | Move | Generic | Link);
 * macOS arbitrates the actual operation via modifier keys and destination preference.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param selectedIndices - Frontend indices of selected files.
 * @param includeHidden - Whether hidden files are shown (affects index mapping).
 * @param hasParent - Whether the ".." entry is shown at index 0.
 * @param iconPath - Path to the drag preview icon (temp file).
 */
export async function startSelectionDrag(
  listingId: string,
  selectedIndices: number[],
  includeHidden: boolean,
  hasParent: boolean,
  iconPath: string,
): Promise<void> {
  const res = await commands.startSelectionDrag(listingId, selectedIndices, includeHidden, hasParent, iconPath)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Begins a native drag with explicit file paths. Used for single-file drags
 * where the frontend has the path directly. Advertises both `public.file-url`
 * and `public.utf8-plain-text` so terminal apps (Warp, etc.) can paste paths
 * as text. Operation mask is permissive — macOS picks the actual operation.
 */
export async function startDragPaths(paths: string[], iconPath: string): Promise<void> {
  const res = await commands.startDragPaths(paths, iconPath)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Marks a self-drag as active and stores the rich image path. The native swizzle will:
 * - Hide the OS drag image over our window (swap to transparent in `draggingEntered:`)
 * - Show the rich image outside the window (swap back in `draggingExited:`)
 * @param richImagePath - Path to the rich canvas-rendered drag image.
 */
export async function prepareSelfDragOverlay(richImagePath: string): Promise<void> {
  await commands.prepareSelfDragOverlay(richImagePath)
}

/**
 * Clears self-drag state after drop or cancellation.
 */
export async function clearSelfDragOverlay(): Promise<void> {
  await commands.clearSelfDragOverlay()
}

/**
 * Pushes the frontend's resolved drop operation down to the native swizzle so the
 * OS-rendered "+" copy badge tracks reality (Copy → +, Move → no badge). Without this,
 * wry's `draggingEntered:`/`draggingUpdated:` returns `Copy` unconditionally and the
 * badge always shows "+" even on Move. No-op on non-macOS.
 */
export async function setSelfDragResolvedOperation(operation: 'move' | 'copy'): Promise<void> {
  await commands.setSelfDragResolvedOp(operation)
}

export interface PathLimits {
  maxNameBytes: number
  maxPathBytes: number
}

/** Returns platform-specific filesystem path limits from the backend. */
export async function getPathLimits(): Promise<PathLimits> {
  return commands.getPathLimits()
}

/**
 * Checks if a path exists.
 * @param path - Path to check.
 * @param volumeId - Optional volume ID. Defaults to "root" for local filesystem.
 * @returns True if the path exists.
 */
export async function pathExists(path: string, volumeId?: string): Promise<boolean> {
  const result = await commands.pathExists(volumeId ?? null, path)
  return result.data
}

/**
 * Like `pathExists`, but returns the full `TimedOut<boolean>` so the caller can tell
 * "doesn't exist" from "couldn't tell" (timeout, or SMB volume in `Disconnected` state).
 * Use this where treating a transient connection blip as "deleted" would be wrong —
 * for example, the directory-eviction poll in `FilePane.svelte`.
 */
export async function pathExistsChecked(path: string, volumeId?: string): Promise<TimedOut<boolean>> {
  return commands.pathExists(volumeId ?? null, path)
}

/**
 * Creates a new directory.
 * @param parentPath - The parent directory path.
 * @param name - The folder name to create.
 * @param volumeId - Optional volume ID. Defaults to "root" for local filesystem.
 * @returns The full path of the created directory.
 */
export async function createDirectory(parentPath: string, name: string, volumeId?: string): Promise<string> {
  const res = await commands.createDirectory(volumeId ?? null, parentPath, name)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Creates a new empty file.
 * @param parentPath - The parent directory path.
 * @param name - The file name to create.
 * @param volumeId - Optional volume ID. Defaults to "root" for local filesystem.
 * @returns The full path of the created file.
 */
export async function createFile(parentPath: string, name: string, volumeId?: string): Promise<string> {
  const res = await commands.createFile(volumeId ?? null, parentPath, name)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

// ============================================================================
// Sync status and font metrics (support file list display)
// ============================================================================

/**
 * Gets sync status for multiple file paths.
 * Returns a map of path -> sync status, with timeout flag.
 * Only works on macOS with files in cloud-synced folders (Dropbox, iCloud, etc.)
 * @param paths - Array of absolute file paths.
 * @returns Map of path -> SyncStatus, with timeout flag
 */
export async function getSyncStatus(paths: string[]): Promise<TimedOut<Record<string, SyncStatus>>> {
  try {
    return await commands.getSyncStatus(paths)
  } catch {
    // Command not available (non-macOS) - return empty map
    return { data: {}, timedOut: false }
  }
}

/**
 * Stores font metrics for a font configuration.
 * @param fontId - Font identifier (like "system-400-12")
 * @param widths - Map of code point -> width in pixels
 */
export async function storeFontMetrics(fontId: string, widths: Record<number, number>): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core')
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic (<R: tauri::Runtime>); excluded from typed bindings
  await invoke('store_font_metrics', { fontId, widths })
}

/**
 * Checks if font metrics are available for a font ID.
 * @param fontId - Font identifier to check
 * @returns True if metrics are cached
 */
export async function hasFontMetrics(fontId: string): Promise<boolean> {
  return commands.hasFontMetrics(fontId)
}
