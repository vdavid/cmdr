// Network hosts, SMB shares, keychain, and mounting

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { commands } from '$lib/ipc/bindings'
import type { MountResult, SmbCredentials, UpgradeResult } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'
import type {
  AuthOptions,
  ConnectionMode,
  DiscoveryState,
  KnownNetworkShare,
  NetworkHost,
  ShareListResult,
} from '../file-explorer/types'

/** Result of connecting to a manually-specified server. */
export interface ManualConnectResult {
  /** The injected network host */
  host: NetworkHost
  /** Optional share path (when user typed smb://host/share) */
  sharePath: string | null
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
    return (await commands.listNetworkHosts()) as NetworkHost[]
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
    return await commands.getNetworkDiscoveryState()
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
    return (await commands.resolveHost(hostId)) as NetworkHost | null
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
  const res = await commands.listSharesOnHost(
    hostId,
    hostname,
    ipAddress ?? null,
    port,
    timeoutMs ?? null,
    cacheTtlMs ?? null,
  )
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
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
    await commands.prefetchShares(hostId, hostname, ipAddress ?? null, port, timeoutMs ?? null, cacheTtlMs ?? null)
  } catch {
    // Silently ignore prefetch errors
  }
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
    return await commands.getKnownShareByName(serverName, shareName)
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
    await commands.updateKnownShare(serverName, shareName, lastConnectionMode, lastKnownAuthOptions, username)
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
    return await commands.getUsernameHints()
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
  const res = await commands.saveSmbCredentials(server, share, username, password)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Returns whether credential storage is using an encrypted file fallback instead of the system keyring. */
export async function isUsingCredentialFileFallback(): Promise<boolean> {
  return commands.isUsingCredentialFileFallback()
}

/**
 * Retrieves SMB credentials from the Keychain.
 * @param server Server hostname or IP
 * @param share Optional share name (null for server-level credentials)
 * @returns Stored credentials if found
 * @throws KeychainError if credentials not found or access denied
 */
export async function getSmbCredentials(server: string, share: string | null): Promise<SmbCredentials> {
  const res = await commands.getSmbCredentials(server, share)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Deletes SMB credentials from the Keychain.
 * @param server Server hostname or IP
 * @param share Optional share name
 */
export async function deleteSmbCredentials(server: string, share: string | null): Promise<void> {
  const res = await commands.deleteSmbCredentials(server, share)
  if (res.status === 'error') throwIpcError(res.error)
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
  const res = await commands.listSharesWithCredentials(
    hostId,
    hostname,
    ipAddress ?? null,
    port,
    username,
    password,
    timeoutMs ?? null,
    cacheTtlMs ?? null,
  )
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
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
  port?: number,
  timeoutMs?: number,
): Promise<MountResult> {
  const res = await commands.mountNetworkShare(server, share, username, password, port ?? null, timeoutMs ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Result of an SMB volume upgrade attempt. */
export type { UpgradeResult }

/**
 * Upgrades an existing OS-mounted SMB volume to use a direct smb2 connection.
 *
 * Tries stored credentials first. Returns `credentialsNeeded` if the frontend
 * should show a login form, or `networkError` for non-auth failures.
 */
export async function upgradeToSmbVolume(volumeId: string): Promise<UpgradeResult> {
  const res = await commands.upgradeToSmbVolume(volumeId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Whether the system (login) keychain holds a password another app (Finder) saved for
 * this volume's SMB server. Attribute-only probe — never triggers the macOS consent
 * dialog — so the UI can decide whether to offer "use the saved password". Best-effort:
 * any error resolves to `false` (the offer simply doesn't appear). macOS only; `false`
 * elsewhere.
 */
export async function systemHasSavedSmbPassword(volumeId: string): Promise<boolean> {
  const res = await commands.systemHasSavedSmbPassword(volumeId)
  return res.status === 'ok' ? res.data : false
}

/**
 * Upgrades an OS-mounted SMB volume to direct smb2 using the password Finder already
 * saved in the login keychain. Reading it triggers the macOS consent dialog (prime the
 * user first). On success the password is copied into Cmdr's own store so future
 * reconnects are silent; if nothing is saved or the user denies, the result is
 * `credentialsNeeded` so the caller falls back to the login form.
 */
export async function upgradeToSmbVolumeUsingSavedPassword(volumeId: string): Promise<UpgradeResult> {
  const res = await commands.upgradeToSmbVolumeUsingSavedPassword(volumeId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Upgrades an SMB volume using explicit credentials from the login form.
 */
export async function upgradeToSmbVolumeWithCredentials(
  volumeId: string,
  username: string | null,
  password: string | null,
  rememberInKeychain: boolean,
): Promise<UpgradeResult> {
  const res = await commands.upgradeToSmbVolumeWithCredentials(volumeId, username, password, rememberInKeychain)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Tries to rebuild the smb2 session for a Disconnected `SmbVolume` in place.
 *
 * Called by the per-volume reconnect manager on each backoff tick (and on
 * "Retry now" / lazy nav-time retry). Backend single-flights concurrent calls.
 * Resolves on success; throws on failure with an `IpcError`-shaped exception.
 */
export async function reconnectSmbVolume(volumeId: string): Promise<void> {
  const res = await commands.reconnectSmbVolume(volumeId)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Reconnects an SMB volume with freshly-entered credentials. Used by the "Sign in"
 * affordance shown when an in-place reconnect gave up on an auth failure (the saved
 * password went stale). The backend persists the new password and reconnects; on
 * success a `smb-connection-changed { state: "direct" }` event follows.
 *
 * Resolves on success; throws on failure with an `IpcError`-shaped exception.
 */
export async function reconnectSmbVolumeWithCredentials(
  volumeId: string,
  username: string,
  password: string,
): Promise<void> {
  const res = await commands.reconnectSmbVolumeWithCredentials(volumeId, username, password)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Disconnects an SMB volume by unmounting it at the OS level (macOS) or by
 * dropping the smb2 session (Linux, until GVFS unmount is wired up). The
 * `volumes-changed` event removes the volume from the picker shortly after.
 *
 * Resolves on success; throws on failure with an `IpcError`-shaped exception
 * (for example, "diskutil unmount failed: Resource busy" if a Finder window
 * still has the volume open).
 */
export async function disconnectSmbVolume(volumeId: string): Promise<void> {
  const res = await commands.disconnectSmbVolume(volumeId)
  if (res.status === 'error') throwIpcError(res.error)
}

// ============================================================================
// Manual server management (macOS only)
// ============================================================================

/**
 * Connects to a manually-specified server: parses address, checks TCP reachability,
 * persists the entry, and injects a synthetic host into the discovery state.
 * @param address Hostname, IP, IP:port, or smb:// URL
 * @returns The injected host and optional share path
 * @throws Plain string error on parse failure or unreachable host
 */
export async function connectToServer(address: string): Promise<ManualConnectResult> {
  const res = await commands.connectToServer(address)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data as ManualConnectResult
}

/**
 * Removes a manually-added server by ID.
 * Deletes from persistent storage and removes from discovery state.
 * @param serverId The manual server's host ID (like "manual-192-168-1-100-445")
 */
export async function removeManualServer(serverId: string): Promise<void> {
  const res = await commands.removeManualServer(serverId)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Idempotently kicks off mDNS discovery if it isn't running yet. Call this when the user
 * takes their first network action: clicking "Network" in the volume picker, opening
 * "Connect to server…", or upgrading a mounted share to direct smb2.
 *
 * The first call here is what triggers macOS's "Cmdr wants to find devices on local
 * networks" prompt. Returns immediately on subsequent calls. No-op when networking is
 * disabled (the caller is expected to gate on `network.enabled` before calling).
 */
export async function ensureNetworkDiscoveryStarted(): Promise<void> {
  try {
    await commands.ensureNetworkDiscoveryStarted()
  } catch {
    // Stub on unsupported platforms. Silently swallow.
  }
}

/**
 * Pushes the `network.enabled` toggle live to the backend. When `false`, stops mDNS and
 * clears the discovered host list (the frontend store empties via `network-host-lost`).
 * When `true`, the backend stays passive: discovery starts only when the user takes a
 * network action and the frontend calls `ensureNetworkDiscoveryStarted`.
 */
export async function setNetworkEnabled(enabled: boolean): Promise<void> {
  try {
    await commands.setNetworkEnabled(enabled)
  } catch {
    // Stub on unsupported platforms. Silently swallow.
  }
}

// ============================================================================
// Network host context menu
// ============================================================================

/**
 * Shows a native context menu for a network host (fire-and-forget).
 * The menu always includes "Disconnect", plus "Forget server" for manual hosts
 * and "Forget saved password" for hosts with stored credentials.
 */
export async function showNetworkHostContextMenu(
  hostId: string,
  hostName: string,
  isManual: boolean,
  hasCredentials: boolean,
): Promise<void> {
  const res = await commands.showNetworkHostContextMenu(hostId, hostName, isManual, hasCredentials)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Listens for the network host context menu action event emitted by `on_menu_event` in Rust.
 * The event fires asynchronously after `popup()` returns.
 */
export function onNetworkHostContextAction(
  handler: (payload: { action: string; hostId: string; hostName: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ action: string; hostId: string; hostName: string }>('network-host-context-action', (event) => {
    handler(event.payload)
  })
}

/**
 * Unmounts all SMB shares mounted from a given server.
 * Returns the list of mount paths that were unmounted.
 */
export async function disconnectNetworkHost(
  hostId: string,
  hostName: string,
  ipAddress: string | undefined,
): Promise<string[]> {
  const res = await commands.disconnectNetworkHost(hostId, hostName, ipAddress ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}
