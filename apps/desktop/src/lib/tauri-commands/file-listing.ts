// On-demand virtual scrolling API (listing-based), sync status, font metrics

import { invoke } from '@tauri-apps/api/core'
import type {
    FileEntry,
    ListingStats,
    ResortResult,
    SortColumn,
    SortOrder,
    StreamingListingStartResult,
    SyncStatus,
} from '../file-explorer/types'
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
    return invoke<StreamingListingStartResult>('list_directory_start_streaming', {
        volumeId,
        path,
        includeHidden,
        sortBy,
        sortOrder,
        listingId,
        directorySortMode,
    })
}

/**
 * Cancels an in-progress streaming directory listing.
 * The task will emit a listing-cancelled event when it stops.
 * @param listingId - The listing ID to cancel.
 */
export async function cancelListing(listingId: string): Promise<void> {
    await invoke('cancel_listing', { listingId })
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
    return invoke<ResortResult>('resort_listing', {
        listingId,
        sortBy,
        sortOrder,
        cursorFilename,
        includeHidden,
        selectedIndices,
        allSelected,
        directorySortMode,
    })
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
    return invoke<FileEntry[]>('get_file_range', { listingId, start, count, includeHidden })
}

/**
 * Gets total count of entries in a cached listing.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param includeHidden - Whether to include hidden files in count.
 */
export async function getTotalCount(listingId: string, includeHidden: boolean): Promise<number> {
    return invoke<number>('get_total_count', { listingId, includeHidden })
}

/**
 * Gets the maximum filename width for a cached listing.
 * Recalculates based on current entries, useful after file watcher updates.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param includeHidden - Whether to include hidden files.
 */
export async function getMaxFilenameWidth(listingId: string, includeHidden: boolean): Promise<number | undefined> {
    return invoke<number | undefined>('get_max_filename_width', { listingId, includeHidden })
}

/**
 * Finds the index of a file by name in a cached listing.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param name - File name to find.
 * @param includeHidden - Whether to include hidden files when calculating index.
 */
export async function findFileIndex(listingId: string, name: string, includeHidden: boolean): Promise<number | null> {
    return invoke<number | null>('find_file_index', { listingId, name, includeHidden })
}

/**
 * Gets a single file at the given index.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param index - Index of the file to get.
 * @param includeHidden - Whether to include hidden files when calculating index.
 */
export async function getFileAt(listingId: string, index: number, includeHidden: boolean): Promise<FileEntry | null> {
    return invoke<FileEntry | null>('get_file_at', { listingId, index, includeHidden })
}

/**
 * Ends a directory listing and cleans up the cache.
 * @param listingId - The listing ID to clean up.
 */
export async function listDirectoryEnd(listingId: string): Promise<void> {
    await invoke('list_directory_end', { listingId })
}

/**
 * Gets statistics about a cached listing.
 * Returns total file/dir counts and sizes. If selectedIndices is provided,
 * also returns statistics for the selected items.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param includeHidden - Whether to include hidden files in calculations.
 * @param selectedIndices - Optional indices of selected files to calculate selection stats.
 */
export async function getListingStats(
    listingId: string,
    includeHidden: boolean,
    selectedIndices?: number[],
): Promise<ListingStats> {
    return invoke<ListingStats>('get_listing_stats', { listingId, includeHidden, selectedIndices })
}

/**
 * Starts a native drag operation for selected files from a cached listing.
 * This initiates the drag from Rust directly, avoiding IPC transfer of file paths.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param selectedIndices - Frontend indices of selected files.
 * @param includeHidden - Whether hidden files are shown (affects index mapping).
 * @param hasParent - Whether the ".." entry is shown at index 0.
 * @param mode - Drag mode: "copy" or "move".
 * @param iconPath - Path to the drag preview icon (temp file).
 */
export async function startSelectionDrag(
    listingId: string,
    selectedIndices: number[],
    includeHidden: boolean,
    hasParent: boolean,
    mode: 'copy' | 'move',
    iconPath: string,
): Promise<void> {
    await invoke('start_selection_drag', { listingId, selectedIndices, includeHidden, hasParent, mode, iconPath })
}

/**
 * Marks a self-drag as active and stores the rich image path. The native swizzle will:
 * - Hide the OS drag image over our window (swap to transparent in `draggingEntered:`)
 * - Show the rich image outside the window (swap back in `draggingExited:`)
 * @param richImagePath - Path to the rich canvas-rendered drag image.
 */
export async function prepareSelfDragOverlay(richImagePath: string): Promise<void> {
    await invoke('prepare_self_drag_overlay', { richImagePath })
}

/**
 * Clears self-drag state after drop or cancellation.
 */
export async function clearSelfDragOverlay(): Promise<void> {
    await invoke('clear_self_drag_overlay')
}

/**
 * Checks if a path exists.
 * @param path - Path to check.
 * @param volumeId - Optional volume ID. Defaults to "root" for local filesystem.
 * @returns True if the path exists.
 */
export async function pathExists(path: string, volumeId?: string): Promise<boolean> {
    return invoke<boolean>('path_exists', { volumeId, path })
}

/**
 * Creates a new directory.
 * @param parentPath - The parent directory path.
 * @param name - The folder name to create.
 * @param volumeId - Optional volume ID. Defaults to "root" for local filesystem.
 * @returns The full path of the created directory.
 */
export async function createDirectory(parentPath: string, name: string, volumeId?: string): Promise<string> {
    return invoke<string>('create_directory', { volumeId, parentPath, name })
}

// ============================================================================
// Sync status and font metrics (support file list display)
// ============================================================================

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
 * @param fontId - Font identifier (like "system-400-12")
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
