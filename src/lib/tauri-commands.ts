// Typed wrapper functions for Tauri commands

import { invoke } from '@tauri-apps/api/core'
import { openPath } from '@tauri-apps/plugin-opener'
import { listen, type UnlistenFn, type Event } from '@tauri-apps/api/event'
import type {
    DiscoveryState,
    FileEntry,
    ListingStartResult,
    NetworkHost,
    ResortResult,
    SortColumn,
    SortOrder,
    SyncStatus,
    VolumeInfo,
} from './file-explorer/types'

export type { Event, UnlistenFn }
export { listen }

// ============================================================================
// On-demand virtual scrolling API (listing-based)
// ============================================================================

/**
 * Starts a new directory listing.
 * Reads the directory once, caches on backend, returns listing ID + total count.
 * Frontend then fetches visible ranges on demand via getFileRange.
 * @param path - Directory path to list. Supports tilde expansion (~).
 * @param includeHidden - Whether to include hidden files in total count.
 * @param sortBy - Column to sort by.
 * @param sortOrder - Ascending or descending.
 */
export async function listDirectoryStart(
    path: string,
    includeHidden: boolean,
    sortBy: SortColumn,
    sortOrder: SortOrder,
): Promise<ListingStartResult> {
    return invoke<ListingStartResult>('list_directory_start', { path, includeHidden, sortBy, sortOrder })
}

/**
 * Re-sorts an existing cached listing in-place.
 * More efficient than creating a new listing when you just want to change the sort order.
 * @param listingId - The listing ID from listDirectoryStart.
 * @param sortBy - Column to sort by.
 * @param sortOrder - Ascending or descending.
 * @param cursorFilename - Optional filename to track; returns its new index after sorting.
 * @param includeHidden - Whether to include hidden files when calculating cursor index.
 * @public
 */
export async function resortListing(
    listingId: string,
    sortBy: SortColumn,
    sortOrder: SortOrder,
    cursorFilename: string | undefined,
    includeHidden: boolean,
): Promise<ResortResult> {
    return invoke<ResortResult>('resort_listing', { listingId, sortBy, sortOrder, cursorFilename, includeHidden })
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
 * Checks if a path exists.
 * @param path - Path to check.
 * @returns True if the path exists.
 */
export async function pathExists(path: string): Promise<boolean> {
    return invoke<boolean>('path_exists', { path })
}

/**
 * Opens a file with the system's default application.
 * @param path - Path to the file to open.
 */
export async function openFile(path: string): Promise<void> {
    await openPath(path)
}

/**
 * Gets icon data URLs for the requested icon IDs.
 * @param iconIds - Array of icon IDs like "ext:jpg", "dir", "symlink"
 * @returns Map of icon_id → base64 WebP data URL
 */
export async function getIcons(iconIds: string[]): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('get_icons', { iconIds })
}

/**
 * Refreshes icons for a directory listing.
 * Fetches icons in parallel for directories (by path) and extensions.
 * @param directoryPaths - Array of directory paths to fetch icons for
 * @param extensions - Array of file extensions (without dot)
 * @returns Map of icon_id → base64 WebP data URL
 */
export async function refreshDirectoryIcons(
    directoryPaths: string[],
    extensions: string[],
): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('refresh_directory_icons', {
        directoryPaths,
        extensions,
    })
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
 * Shows the main window.
 * Should be called when the frontend is ready to avoid white flash.
 */
export async function showMainWindow(): Promise<void> {
    await invoke('show_main_window')
}

/**
 * Gets sync status for multiple file paths.
 * Returns a map of path → sync status.
 * Only works on macOS with files in cloud-synced folders (Dropbox, iCloud, etc.)
 * @param paths - Array of absolute file paths.
 * @returns Map of path → SyncStatus
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
 * @param widths - Map of code point → width in pixels
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

// ============================================================================
// Volume management (macOS only)
// ============================================================================

/** Default volume ID for the root filesystem */
export const DEFAULT_VOLUME_ID = 'root'

/**
 * Lists all mounted volumes.
 * Only available on macOS.
 * @returns Array of VolumeInfo objects, sorted with root first
 */
export async function listVolumes(): Promise<VolumeInfo[]> {
    try {
        return await invoke<VolumeInfo[]>('list_volumes')
    } catch {
        // Command not available (non-macOS) - return empty array
        return []
    }
}

/**
 * Gets the default volume ID (root filesystem).
 * @returns The default volume ID string
 */
export async function getDefaultVolumeId(): Promise<string> {
    try {
        return await invoke<string>('get_default_volume_id')
    } catch {
        // Fallback for non-macOS
        return DEFAULT_VOLUME_ID
    }
}

/**
 * Finds the actual volume (not a favorite) that contains a given path.
 * This is used to determine which volume to highlight when a favorite is selected.
 * @param path - Path to find the containing volume for
 * @returns The VolumeInfo for the containing volume, or null if not found
 */
export async function findContainingVolume(path: string): Promise<VolumeInfo | null> {
    try {
        return await invoke<VolumeInfo | null>('find_containing_volume', { path })
    } catch {
        // Command not available (non-macOS) - return null
        return null
    }
}

// ============================================================================
// Permission checking (macOS only)
// ============================================================================

/**
 * Checks if the app has full disk access.
 * Only available on macOS.
 * @returns True if the app has FDA, false otherwise
 */
export async function checkFullDiskAccess(): Promise<boolean> {
    try {
        return await invoke<boolean>('check_full_disk_access')
    } catch {
        // Command not available (non-macOS) - assume we have access
        return true
    }
}

/**
 * Opens System Settings > Privacy & Security > Privacy.
 * Only available on macOS.
 */
export async function openPrivacySettings(): Promise<void> {
    try {
        await invoke('open_privacy_settings')
    } catch {
        // Command not available (non-macOS) - silently fail
    }
}

// ============================================================================
// Network discovery (macOS only)
// ============================================================================

/**
 * Gets all currently discovered network hosts.
 * Only available on macOS.
 * @returns Array of NetworkHost objects
 */
export async function listNetworkHosts(): Promise<NetworkHost[]> {
    try {
        return await invoke<NetworkHost[]>('list_network_hosts')
    } catch {
        // Command not available (non-macOS) - return empty array
        return []
    }
}

/**
 * Gets the current network discovery state.
 * Only available on macOS.
 * @returns Current DiscoveryState
 */
export async function getNetworkDiscoveryState(): Promise<DiscoveryState> {
    try {
        return await invoke<DiscoveryState>('get_network_discovery_state')
    } catch {
        // Command not available (non-macOS) - return idle
        return 'idle'
    }
}

/**
 * Resolves a network host's hostname and IP address.
 * This performs lazy resolution - only called on hover or when connecting.
 * Only available on macOS.
 * @param hostId The host ID to resolve
 * @returns Updated NetworkHost with hostname and IP, or null if not found
 */
export async function resolveNetworkHost(hostId: string): Promise<NetworkHost | null> {
    try {
        return await invoke<NetworkHost | null>('resolve_host', { hostId })
    } catch {
        // Command not available (non-macOS) - return null
        return null
    }
}

// noinspection JSUnusedGlobalSymbols -- This is a utility mechanism for debugging
/**
 * Logs a message through the backend for unified timestamp tracking.
 * Used for debugging timing issues between frontend and backend.
 */
export function feLog(message: string): void {
    void invoke('fe_log', { message }).catch(() => {
        // Fallback to console if command not available
        // eslint-disable-next-line no-console -- We do want to log to the console here
        console.log('[FE]', message)
    })
}
