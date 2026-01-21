// Typed wrapper functions for Tauri commands

import { invoke } from '@tauri-apps/api/core'
import { openPath, openUrl } from '@tauri-apps/plugin-opener'
import { type Event, listen, type UnlistenFn } from '@tauri-apps/api/event'
import type {
    AuthMode,
    AuthOptions,
    ConflictResolution,
    ConnectionMode,
    DiscoveryState,
    FileEntry,
    KeychainError,
    KnownNetworkShare,
    ListingCancelledEvent,
    ListingCompleteEvent,
    ListingErrorEvent,
    ListingProgressEvent,
    ListingStartResult,
    ListingStats,
    MountError,
    MountResult,
    NetworkHost,
    ResortResult,
    ShareListResult,
    SmbCredentials,
    SortColumn,
    SortOrder,
    StreamingListingStartResult,
    SyncStatus,
    VolumeInfo,
    WriteCancelledEvent,
    WriteCompleteEvent,
    WriteConflictEvent,
    WriteErrorEvent,
    WriteOperationConfig,
    WriteOperationError,
    WriteOperationStartResult,
    WriteProgressEvent,
    ConflictInfo,
    DryRunResult,
    OperationStatus,
    OperationSummary,
    ScanProgressEvent,
} from './file-explorer/types'

export type {
    ListingProgressEvent,
    ListingCompleteEvent,
    ListingErrorEvent,
    ListingCancelledEvent,
    StreamingListingStartResult,
    WriteCancelledEvent,
    WriteCompleteEvent,
    WriteConflictEvent,
    WriteErrorEvent,
    WriteOperationConfig,
    WriteOperationError,
    WriteOperationStartResult,
    WriteProgressEvent,
    ConflictInfo,
    DryRunResult,
    OperationStatus,
    OperationSummary,
    ScanProgressEvent,
}

export type { Event, UnlistenFn }
export { listen }

// ============================================================================
// On-demand virtual scrolling API (listing-based)
// ============================================================================

/**
 * Starts a new directory listing (synchronous version).
 * Reads the directory once, caches on backend, returns listing ID + total count.
 * Frontend then fetches visible ranges on demand via getFileRange.
 * NOTE: This blocks until the directory is fully read. For non-blocking operation,
 * use listDirectoryStartStreaming instead.
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
 * Starts a new streaming directory listing (async version).
 * Returns immediately with listing ID and "loading" status.
 * Progress is reported via events: listing-progress, listing-complete, listing-error, listing-cancelled.
 * @param path - Directory path to list. Supports tilde expansion (~).
 * @param includeHidden - Whether to include hidden files in total count.
 * @param sortBy - Column to sort by.
 * @param sortOrder - Ascending or descending.
 * @param listingId - Unique identifier for the listing (used for cancellation)
 */
export async function listDirectoryStartStreaming(
    path: string,
    includeHidden: boolean,
    sortBy: SortColumn,
    sortOrder: SortOrder,
    listingId: string,
): Promise<StreamingListingStartResult> {
    return invoke<StreamingListingStartResult>('list_directory_start_streaming', {
        path,
        includeHidden,
        sortBy,
        sortOrder,
        listingId,
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
): Promise<ResortResult> {
    return invoke<ResortResult>('resort_listing', {
        listingId,
        sortBy,
        sortOrder,
        cursorFilename,
        includeHidden,
        selectedIndices,
        allSelected,
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
 * Opens a URL in the system's default browser.
 * @param url - URL to open (e.g., "https://getcmdr.com/renew")
 */
export async function openExternalUrl(url: string): Promise<void> {
    await openUrl(url)
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
    files: PaneFileEntry[]
    cursorIndex: number
    viewMode: string
    selectedIndices: number[]
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
 * This is used to determine which volume to set as active when the user navigates to a "favorite folder".
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

/** Space information for a volume. */
export interface VolumeSpaceInfo {
    totalBytes: number
    availableBytes: number
}

/**
 * Gets space information (total and available bytes) for a volume at the given path.
 * @param path - Any path on the volume to get space info for
 * @returns Space info or null if unavailable
 */
export async function getVolumeSpace(path: string): Promise<VolumeSpaceInfo | null> {
    try {
        return await invoke<VolumeSpaceInfo | null>('get_volume_space', { path })
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

// ============================================================================
// SMB share listing (macOS only)
// ============================================================================

/**
 * Lists shares available on a network host.
 * Returns cached results if available (30 second TTL), otherwise queries the host.
 * Attempts guest access first; returns an error if authentication is required.
 * @param hostId Unique identifier for the host (used for caching)
 * @param hostname Hostname to connect to (for example, "TEST_SERVER.local")
 * @param ipAddress Optional resolved IP address (preferred over hostname for reliability)
 * @param port SMB port (default 445, but Docker containers may use different ports)
 * @returns Result with shares and auth mode, or error
 */
export async function listSharesOnHost(
    hostId: string,
    hostname: string,
    ipAddress: string | undefined,
    port: number,
): Promise<ShareListResult> {
    // The Rust command returns Result<ShareListResult, ShareListError>
    // Tauri auto-converts Ok to value and Err to thrown error
    return invoke<ShareListResult>('list_shares_on_host', { hostId, hostname, ipAddress, port })
}

/**
 * Prefetches shares for a host (for example, on hover).
 * Same as listSharesOnHost but designed for prefetching - errors are silently ignored.
 * Returns immediately if shares are already cached.
 * @param hostId Unique identifier for the host
 * @param hostname Hostname to connect to
 * @param ipAddress Optional resolved IP address
 * @param port SMB port
 */
export async function prefetchShares(
    hostId: string,
    hostname: string,
    ipAddress: string | undefined,
    port: number,
): Promise<void> {
    try {
        await invoke('prefetch_shares', { hostId, hostname, ipAddress, port })
    } catch {
        // Silently ignore prefetch errors
    }
}

/**
 * Gets the cached authentication mode for a host.
 * Returns 'unknown' if no cached data is available.
 * @param hostId The host ID to check
 * @returns Cached AuthMode or 'unknown'
 */
export async function getHostAuthMode(hostId: string): Promise<AuthMode> {
    try {
        return await invoke<AuthMode>('get_host_auth_mode', { hostId })
    } catch {
        return 'unknown'
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

// ============================================================================
// Known shares store (macOS only)
// ============================================================================

/**
 * Gets all known network shares (previously connected).
 * Only available on macOS.
 * @returns Array of KnownNetworkShare objects
 */
export async function getKnownShares(): Promise<KnownNetworkShare[]> {
    try {
        return await invoke<KnownNetworkShare[]>('get_known_shares')
    } catch {
        // Command not available (non-macOS) - return empty array
        return []
    }
}

/**
 * Gets a specific known share by server and share name.
 * Only available on macOS.
 * @param serverName Server hostname or IP
 * @param shareName Share name
 * @returns KnownNetworkShare if found, null otherwise
 */
export async function getKnownShareByName(serverName: string, shareName: string): Promise<KnownNetworkShare | null> {
    try {
        return await invoke<KnownNetworkShare | null>('get_known_share_by_name', { serverName, shareName })
    } catch {
        // Command not available (non-macOS) - return null
        return null
    }
}

/**
 * Updates or adds a known network share after successful connection.
 * Only available on macOS.
 * @param serverName Server hostname or IP
 * @param shareName Share name
 * @param lastConnectionMode How we connected (guest or credentials)
 * @param lastKnownAuthOptions Available auth options
 * @param username Username used (null for guest)
 */
export async function updateKnownShare(
    serverName: string,
    shareName: string,
    lastConnectionMode: ConnectionMode,
    lastKnownAuthOptions: AuthOptions,
    username: string | null,
): Promise<void> {
    try {
        await invoke('update_known_share', {
            serverName,
            shareName,
            lastConnectionMode,
            lastKnownAuthOptions,
            username,
        })
    } catch {
        // Command not available (non-macOS) - silently fail
    }
}

/**
 * Gets username hints for servers (last used username per server).
 * Useful for pre-filling login forms.
 * Only available on macOS.
 * @returns Map of server name (lowercase) → username
 */
export async function getUsernameHints(): Promise<Record<string, string>> {
    try {
        return await invoke<Record<string, string>>('get_username_hints')
    } catch {
        // Command not available (non-macOS) - return empty map
        return {}
    }
}

// ============================================================================
// Keychain operations (macOS only)
// ============================================================================

/**
 * Saves SMB credentials to the Keychain.
 * Credentials are stored under "Cmdr" service name in Keychain Access.
 * @param server Server hostname or IP
 * @param share Optional share name (null for server-level credentials)
 * @param username Username for authentication
 * @param password Password for authentication
 */
export async function saveSmbCredentials(
    server: string,
    share: string | null,
    username: string,
    password: string,
): Promise<void> {
    await invoke('save_smb_credentials', { server, share, username, password })
}

/**
 * Retrieves SMB credentials from the Keychain.
 * @param server Server hostname or IP
 * @param share Optional share name (null for server-level credentials)
 * @returns Stored credentials if found
 * @throws KeychainError if credentials not found or access denied
 */
export async function getSmbCredentials(server: string, share: string | null): Promise<SmbCredentials> {
    return invoke<SmbCredentials>('get_smb_credentials', { server, share })
}

/**
 * Checks if credentials exist in the Keychain for a server/share.
 * @param server Server hostname or IP
 * @param share Optional share name
 * @returns True if credentials are stored
 */
export async function hasSmbCredentials(server: string, share: string | null): Promise<boolean> {
    try {
        return await invoke<boolean>('has_smb_credentials', { server, share })
    } catch {
        return false
    }
}

/**
 * Deletes SMB credentials from the Keychain.
 * @param server Server hostname or IP
 * @param share Optional share name
 */
export async function deleteSmbCredentials(server: string, share: string | null): Promise<void> {
    await invoke('delete_smb_credentials', { server, share })
}

/**
 * Lists shares on a host using provided credentials.
 * This is the authenticated version of listSharesOnHost.
 * @param hostId Unique identifier for the host (used for caching)
 * @param hostname Hostname to connect to
 * @param ipAddress Optional resolved IP address
 * @param port SMB port
 * @param username Username for authentication (null for guest)
 * @param password Password for authentication (null for guest)
 */
export async function listSharesWithCredentials(
    hostId: string,
    hostname: string,
    ipAddress: string | undefined,
    port: number,
    username: string | null,
    password: string | null,
): Promise<ShareListResult> {
    return invoke<ShareListResult>('list_shares_with_credentials', {
        hostId,
        hostname,
        ipAddress,
        port,
        username,
        password,
    })
}

/**
 * Helper to check if an error is a KeychainError
 */
export function isKeychainError(error: unknown): error is KeychainError {
    return (
        typeof error === 'object' &&
        error !== null &&
        'type' in error &&
        ['not_found', 'access_denied', 'other'].includes((error as KeychainError).type)
    )
}

// ============================================================================
// SMB mounting (macOS only)
// ============================================================================

/**
 * Mounts an SMB share to the local filesystem.
 * If the share is already mounted, returns the existing mount path without re-mounting.
 *
 * @param server Server hostname or IP address
 * @param share Name of the share to mount
 * @param username Optional username for authentication
 * @param password Optional password for authentication
 * @returns MountResult with mount path on success
 * @throws MountError on failure
 */
export async function mountNetworkShare(
    server: string,
    share: string,
    username: string | null,
    password: string | null,
): Promise<MountResult> {
    return invoke<MountResult>('mount_network_share', {
        server,
        share,
        username,
        password,
    })
}

/**
 * Helper to check if an error is a MountError
 */
export function isMountError(error: unknown): error is MountError {
    return (
        typeof error === 'object' &&
        error !== null &&
        'type' in error &&
        [
            'host_unreachable',
            'share_not_found',
            'auth_required',
            'auth_failed',
            'permission_denied',
            'timeout',
            'cancelled',
            'protocol_error',
            'mount_path_conflict',
        ].includes((error as MountError).type)
    )
}

// ============================================================================
// Licensing
// ============================================================================

/** License types */
export type LicenseType = 'supporter' | 'commercial_subscription' | 'commercial_perpetual'

/** Application license status */
export type LicenseStatus =
    | { type: 'personal'; showCommercialReminder: boolean }
    | { type: 'supporter'; showCommercialReminder: boolean }
    | { type: 'commercial'; licenseType: LicenseType; organizationName: string | null; expiresAt: string | null }
    | { type: 'expired'; organizationName: string | null; expiredAt: string; showModal: boolean }

/** License information from activation */
export interface LicenseInfo {
    email: string
    transactionId: string
    issuedAt: string
    organizationName: string | null
    shortCode: string | null
}

/**
 * Gets the current application license status.
 * @returns Current license status (personal, supporter, commercial, or expired)
 */
export async function getLicenseStatus(): Promise<LicenseStatus> {
    return invoke<LicenseStatus>('get_license_status')
}

/**
 * Gets the window title based on current license status.
 * @returns Window title string (e.g., "Cmdr – Personal use only")
 */
export async function getWindowTitle(): Promise<string> {
    return invoke<string>('get_window_title')
}

/**
 * Activates a license key.
 * @param licenseKey The license key to activate
 * @returns License info on success
 * @throws Error message on failure
 */
export async function activateLicense(licenseKey: string): Promise<LicenseInfo> {
    return invoke<LicenseInfo>('activate_license', { licenseKey })
}

/**
 * Gets information about the current stored license.
 * @returns License info if a valid license is stored, null otherwise
 */
export async function getLicenseInfo(): Promise<LicenseInfo | null> {
    return invoke<LicenseInfo | null>('get_license_info')
}

/**
 * Marks the expiration modal as shown to prevent showing it again.
 */
export async function markExpirationModalShown(): Promise<void> {
    await invoke('mark_expiration_modal_shown')
}

/**
 * Marks the commercial reminder as dismissed (resets the 30-day timer).
 */
export async function markCommercialReminderDismissed(): Promise<void> {
    await invoke('mark_commercial_reminder_dismissed')
}

/**
 * Resets all license data (debug builds only).
 */
export async function resetLicense(): Promise<void> {
    await invoke('reset_license')
}

/**
 * Checks if the license needs re-validation with the server.
 * Should be called on app startup to determine if validateLicenseWithServer should be invoked.
 * @returns True if validation is needed (7+ days since last validation)
 */
export async function needsLicenseValidation(): Promise<boolean> {
    return invoke<boolean>('needs_license_validation')
}

/**
 * Validates the license with the license server.
 * Call this when needsLicenseValidation returns true, or after activating a new license.
 * @returns Updated license status from server
 */
export async function validateLicenseWithServer(): Promise<LicenseStatus> {
    return invoke<LicenseStatus>('validate_license_with_server')
}

// ============================================================================
// Write operations (copy, move, delete)
// ============================================================================

/**
 * Starts a copy operation in the background.
 * Progress events are emitted via write-progress, write-complete, write-error, write-cancelled.
 * @param sources - List of source file/directory paths (absolute)
 * @param destination - Destination directory path (absolute)
 * @param config - Operation configuration (optional)
 */
export async function copyFiles(
    sources: string[],
    destination: string,
    config?: WriteOperationConfig,
): Promise<WriteOperationStartResult> {
    return invoke<WriteOperationStartResult>('copy_files', { sources, destination, config: config ?? {} })
}

/**
 * Starts a move operation in the background.
 * Uses instant rename for same-filesystem moves, copy+delete for cross-filesystem.
 * @param sources - List of source file/directory paths (absolute)
 * @param destination - Destination directory path (absolute)
 * @param config - Operation configuration (optional)
 */
export async function moveFiles(
    sources: string[],
    destination: string,
    config?: WriteOperationConfig,
): Promise<WriteOperationStartResult> {
    return invoke<WriteOperationStartResult>('move_files', { sources, destination, config: config ?? {} })
}

/**
 * Starts a delete operation in the background.
 * Recursively deletes files and directories.
 * @param sources - List of source file/directory paths (absolute)
 * @param config - Operation configuration (optional)
 */
export async function deleteFiles(
    sources: string[],
    config?: WriteOperationConfig,
): Promise<WriteOperationStartResult> {
    return invoke<WriteOperationStartResult>('delete_files', { sources, config: config ?? {} })
}

/**
 * Cancels an in-progress write operation.
 * The operation will emit a write-cancelled event when it stops.
 * @param operationId - The operation ID to cancel
 * @param rollback - If true, delete any partial files created. If false, keep them.
 */
export async function cancelWriteOperation(operationId: string, rollback: boolean): Promise<void> {
    await invoke('cancel_write_operation', { operationId, rollback })
}

/**
 * Resolves a pending conflict for an in-progress write operation.
 * When an operation encounters a conflict in Stop mode, it emits a write-conflict
 * event and waits for this function to be called. The operation will then proceed
 * with the chosen resolution.
 * @param operationId - The operation ID that has a pending conflict
 * @param resolution - How to resolve the conflict (skip, overwrite, or rename)
 * @param applyToAll - If true, apply this resolution to all future conflicts in this operation
 */
export async function resolveWriteConflict(
    operationId: string,
    resolution: ConflictResolution,
    applyToAll: boolean,
): Promise<void> {
    await invoke('resolve_write_conflict', { operationId, resolution, applyToAll })
}

/**
 * Lists all active write operations.
 * Returns a list of operation summaries for all currently running operations.
 * Useful for showing a global progress view or managing multiple concurrent operations.
 * @returns List of operation summaries
 */
export async function listActiveOperations(): Promise<OperationSummary[]> {
    return invoke<OperationSummary[]>('list_active_operations')
}

/**
 * Gets the detailed status of a specific write operation.
 * @param operationId - The operation ID to query
 * @returns Current status, or null if the operation is not found
 */
export async function getOperationStatus(operationId: string): Promise<OperationStatus | null> {
    return invoke<OperationStatus | null>('get_operation_status', { operationId })
}

/**
 * Type guard for WriteOperationError.
 */
export function isWriteOperationError(error: unknown): error is WriteOperationError {
    return (
        typeof error === 'object' &&
        error !== null &&
        'type' in error &&
        typeof (error as { type: unknown }).type === 'string'
    )
}

// ============================================================================
// Write operation event helpers
// ============================================================================

/**
 * Subscribes to write operation progress events.
 * @param callback - Function to call when progress is reported
 * @returns Unsubscribe function
 */
export async function onWriteProgress(callback: (event: WriteProgressEvent) => void): Promise<UnlistenFn> {
    return listen<WriteProgressEvent>('write-progress', (event) => {
        callback(event.payload)
    })
}

/**
 * Subscribes to write operation completion events.
 * @param callback - Function to call when an operation completes successfully
 * @returns Unsubscribe function
 */
export async function onWriteComplete(callback: (event: WriteCompleteEvent) => void): Promise<UnlistenFn> {
    return listen<WriteCompleteEvent>('write-complete', (event) => {
        callback(event.payload)
    })
}

/**
 * Subscribes to write operation error events.
 * @param callback - Function to call when an operation fails
 * @returns Unsubscribe function
 */
export async function onWriteError(callback: (event: WriteErrorEvent) => void): Promise<UnlistenFn> {
    return listen<WriteErrorEvent>('write-error', (event) => {
        callback(event.payload)
    })
}

/**
 * Subscribes to write operation cancelled events.
 * @param callback - Function to call when an operation is cancelled
 * @returns Unsubscribe function
 */
export async function onWriteCancelled(callback: (event: WriteCancelledEvent) => void): Promise<UnlistenFn> {
    return listen<WriteCancelledEvent>('write-cancelled', (event) => {
        callback(event.payload)
    })
}

/**
 * Subscribes to write operation conflict events.
 * Only emitted when using Stop conflict resolution mode.
 * @param callback - Function to call when a conflict is detected
 * @returns Unsubscribe function
 */
export async function onWriteConflict(callback: (event: WriteConflictEvent) => void): Promise<UnlistenFn> {
    return listen<WriteConflictEvent>('write-conflict', (event) => {
        callback(event.payload)
    })
}

/**
 * Subscribes to scan progress events during dry-run operations.
 * @param callback - Function to call when scan progress is reported
 * @returns Unsubscribe function
 */
export async function onScanProgress(callback: (event: ScanProgressEvent) => void): Promise<UnlistenFn> {
    return listen<ScanProgressEvent>('scan-progress', (event) => {
        callback(event.payload)
    })
}

/**
 * Subscribes to scan conflict events during dry-run operations.
 * Conflicts are streamed as they are detected.
 * @param callback - Function to call when a conflict is detected during scan
 * @returns Unsubscribe function
 */
export async function onScanConflict(callback: (event: ConflictInfo) => void): Promise<UnlistenFn> {
    return listen<ConflictInfo>('scan-conflict', (event) => {
        callback(event.payload)
    })
}

/**
 * Subscribes to dry-run completion events.
 * Emitted when a dry-run operation finishes scanning.
 * @param callback - Function to call with the dry-run result
 * @returns Unsubscribe function
 */
export async function onDryRunComplete(callback: (event: DryRunResult) => void): Promise<UnlistenFn> {
    return listen<DryRunResult>('dry-run-complete', (event) => {
        callback(event.payload)
    })
}

// ============================================================================
// Unified write operation event subscription
// ============================================================================

/** Handlers for write operation events. All handlers are optional. */
export interface WriteOperationHandlers {
    onProgress?: (event: WriteProgressEvent) => void
    onComplete?: (event: WriteCompleteEvent) => void
    onError?: (event: WriteErrorEvent) => void
    onCancelled?: (event: WriteCancelledEvent) => void
    onConflict?: (event: WriteConflictEvent) => void
    /** For dry-run mode: progress during scanning */
    onScanProgress?: (event: ScanProgressEvent) => void
    /** For dry-run mode: individual conflicts as they're found */
    onScanConflict?: (event: ConflictInfo) => void
    /** For dry-run mode: final result */
    onDryRunComplete?: (event: DryRunResult) => void
}

/**
 * Subscribes to all events for a specific write operation.
 * Filters events by operationId so handlers only receive events for this operation.
 * Returns a single unlisten function that cleans up all subscriptions.
 *
 * @example
 * ```ts
 * const unlisten = await onOperationEvents(result.operationId, {
 *   onProgress: (e) => updateProgressBar(e.bytesDone / e.bytesTotal),
 *   onComplete: (e) => showSuccess(`Copied ${e.filesProcessed} files`),
 *   onError: (e) => showError(e.error),
 * })
 * // Later: unlisten() to clean up all subscriptions
 * ```
 */
export async function onOperationEvents(operationId: string, handlers: WriteOperationHandlers): Promise<UnlistenFn> {
    const unlisteners: UnlistenFn[] = []

    if (handlers.onProgress) {
        const handler = handlers.onProgress
        unlisteners.push(
            await listen<WriteProgressEvent>('write-progress', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    if (handlers.onComplete) {
        const handler = handlers.onComplete
        unlisteners.push(
            await listen<WriteCompleteEvent>('write-complete', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    if (handlers.onError) {
        const handler = handlers.onError
        unlisteners.push(
            await listen<WriteErrorEvent>('write-error', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    if (handlers.onCancelled) {
        const handler = handlers.onCancelled
        unlisteners.push(
            await listen<WriteCancelledEvent>('write-cancelled', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    if (handlers.onConflict) {
        const handler = handlers.onConflict
        unlisteners.push(
            await listen<WriteConflictEvent>('write-conflict', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    if (handlers.onScanProgress) {
        const handler = handlers.onScanProgress
        unlisteners.push(
            await listen<ScanProgressEvent>('scan-progress', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    // Note: scan-conflict events don't have operationId, they're streamed during the scan
    // The frontend should only subscribe when doing a dry-run for a specific operation
    if (handlers.onScanConflict) {
        const handler = handlers.onScanConflict
        unlisteners.push(
            await listen<ConflictInfo>('scan-conflict', (event) => {
                handler(event.payload)
            }),
        )
    }

    if (handlers.onDryRunComplete) {
        const handler = handlers.onDryRunComplete
        unlisteners.push(
            await listen<DryRunResult>('dry-run-complete', (event) => {
                if (event.payload.operationId === operationId) handler(event.payload)
            }),
        )
    }

    // Return a single function that cleans up all subscriptions
    return () => {
        for (const unlisten of unlisteners) {
            unlisten()
        }
    }
}

/** Statistics derived from write operation progress. */
export interface WriteOperationStats {
    /** Percentage complete (0-100) based on bytes if available, otherwise files */
    percentComplete: number
    /** Bytes per second (0 if not enough data) */
    bytesPerSecond: number
    /** Estimated time remaining in seconds (null if not enough data) */
    estimatedSecondsRemaining: number | null
    /** Elapsed time in seconds */
    elapsedSeconds: number
}

/**
 * Calculates derived statistics from a progress event.
 * Call this from your onProgress handler to get ETA, speed, etc.
 *
 * @param event - The progress event
 * @param startTime - When the operation started (Date.now() when you called copyFiles/etc)
 */
export function calculateOperationStats(event: WriteProgressEvent, startTime: number): WriteOperationStats {
    const now = Date.now()
    const elapsedMs = now - startTime
    const elapsedSeconds = elapsedMs / 1000

    // Calculate percent complete (prefer bytes over files for accuracy)
    let percentComplete = 0
    if (event.bytesTotal > 0) {
        percentComplete = (event.bytesDone / event.bytesTotal) * 100
    } else if (event.filesTotal > 0) {
        percentComplete = (event.filesDone / event.filesTotal) * 100
    }

    // Calculate speed (bytes per second)
    const bytesPerSecond = elapsedSeconds > 0 ? event.bytesDone / elapsedSeconds : 0

    // Calculate ETA
    let estimatedSecondsRemaining: number | null = null
    if (bytesPerSecond > 0 && event.bytesTotal > 0) {
        const bytesRemaining = event.bytesTotal - event.bytesDone
        estimatedSecondsRemaining = bytesRemaining / bytesPerSecond
    } else if (elapsedSeconds > 0 && event.filesTotal > 0 && event.filesDone > 0) {
        // Fallback to file-based ETA
        const filesPerSecond = event.filesDone / elapsedSeconds
        const filesRemaining = event.filesTotal - event.filesDone
        estimatedSecondsRemaining = filesRemaining / filesPerSecond
    }

    return {
        percentComplete: Math.min(100, Math.max(0, percentComplete)),
        bytesPerSecond,
        estimatedSecondsRemaining,
        elapsedSeconds,
    }
}

/**
 * Formats bytes as human-readable string (e.g., "1.5 GB").
 */
export function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${String(bytes)} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
}

/**
 * Formats seconds as human-readable duration (e.g., "2m 30s").
 */
export function formatDuration(seconds: number): string {
    if (seconds < 60) return `${String(Math.round(seconds))}s`
    if (seconds < 3600) {
        const mins = Math.floor(seconds / 60)
        const secs = Math.round(seconds % 60)
        return secs > 0 ? `${String(mins)}m ${String(secs)}s` : `${String(mins)}m`
    }
    const hours = Math.floor(seconds / 3600)
    const mins = Math.round((seconds % 3600) / 60)
    return mins > 0 ? `${String(hours)}h ${String(mins)}m` : `${String(hours)}h`
}
