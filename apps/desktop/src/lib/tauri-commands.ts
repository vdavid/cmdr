// Typed wrapper functions for Tauri commands

import { invoke } from '@tauri-apps/api/core'
import { openPath, openUrl } from '@tauri-apps/plugin-opener'
import { type Event, listen, type UnlistenFn } from '@tauri-apps/api/event'
import type {
    AuthMode,
    AuthOptions,
    ConnectionMode,
    DiscoveryState,
    FileEntry,
    KeychainError,
    KnownNetworkShare,
    ListingStartResult,
    MountError,
    MountResult,
    NetworkHost,
    ResortResult,
    ShareListResult,
    SmbCredentials,
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
    | { type: 'personal' }
    | { type: 'supporter' }
    | { type: 'commercial'; licenseType: LicenseType; organizationName: string | null; expiresAt: string | null }
    | { type: 'expired'; organizationName: string | null; expiredAt: string; showModal: boolean }

/** License information from activation */
export interface LicenseInfo {
    email: string
    transactionId: string
    issuedAt: string
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
