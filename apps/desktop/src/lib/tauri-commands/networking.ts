// Network hosts, SMB shares, keychain, mounting (macOS only)

import { invoke } from '@tauri-apps/api/core'
import type {
    AuthOptions,
    ConnectionMode,
    DiscoveryState,
    KeychainError,
    KnownNetworkShare,
    MountError,
    MountResult,
    NetworkHost,
    ShareListResult,
    SmbCredentials,
} from '../file-explorer/types'

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
 * Returns cached results if available, otherwise queries the host.
 * Attempts guest access first; returns an error if authentication is required.
 * @param hostId Unique identifier for the host (used for caching)
 * @param hostname Hostname to connect to (for example, "TEST_SERVER.local")
 * @param ipAddress Optional resolved IP address (preferred over hostname for reliability)
 * @param port SMB port (default 445, but Docker containers may use different ports)
 * @param timeoutMs Optional timeout in milliseconds (default: 15000)
 * @param cacheTtlMs Optional cache TTL in milliseconds (default: 30000)
 * @returns Result with shares and auth mode, or error
 */
export async function listSharesOnHost(
    hostId: string,
    hostname: string,
    ipAddress: string | undefined,
    port: number,
    timeoutMs?: number,
    cacheTtlMs?: number,
): Promise<ShareListResult> {
    // The Rust command returns Result<ShareListResult, ShareListError>
    // Tauri auto-converts Ok to value and Err to thrown error
    return invoke<ShareListResult>('list_shares_on_host', { hostId, hostname, ipAddress, port, timeoutMs, cacheTtlMs })
}

/**
 * Prefetches shares for a host (for example, on hover).
 * Same as listSharesOnHost but designed for prefetching - errors are silently ignored.
 * Returns immediately if shares are already cached.
 * @param hostId Unique identifier for the host
 * @param hostname Hostname to connect to
 * @param ipAddress Optional resolved IP address
 * @param port SMB port
 * @param timeoutMs Optional timeout in milliseconds (default: 15000)
 * @param cacheTtlMs Optional cache TTL in milliseconds (default: 30000)
 */
export async function prefetchShares(
    hostId: string,
    hostname: string,
    ipAddress: string | undefined,
    port: number,
    timeoutMs?: number,
    cacheTtlMs?: number,
): Promise<void> {
    try {
        await invoke('prefetch_shares', { hostId, hostname, ipAddress, port, timeoutMs, cacheTtlMs })
    } catch {
        // Silently ignore prefetch errors
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
 * @returns Map of server name (lowercase) -> username
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
 * @param timeoutMs Optional timeout in milliseconds (default: 15000)
 * @param cacheTtlMs Optional cache TTL in milliseconds (default: 30000)
 */
export async function listSharesWithCredentials(
    hostId: string,
    hostname: string,
    ipAddress: string | undefined,
    port: number,
    username: string | null,
    password: string | null,
    timeoutMs?: number,
    cacheTtlMs?: number,
): Promise<ShareListResult> {
    return invoke<ShareListResult>('list_shares_with_credentials', {
        hostId,
        hostname,
        ipAddress,
        port,
        username,
        password,
        timeoutMs,
        cacheTtlMs,
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
 * @param timeoutMs Optional timeout in milliseconds (default: 20000)
 * @returns MountResult with mount path on success
 * @throws MountError on failure
 */
export async function mountNetworkShare(
    server: string,
    share: string,
    username: string | null,
    password: string | null,
    timeoutMs?: number,
): Promise<MountResult> {
    return invoke<MountResult>('mount_network_share', {
        server,
        share,
        username,
        password,
        timeoutMs,
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
