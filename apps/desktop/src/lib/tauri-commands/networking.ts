// Network hosts, SMB shares, keychain, and mounting

import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '$lib/ipc/bindings'
import { throwEjectError } from '$lib/file-explorer/navigation/eject-error'
import { throwMountError } from '$lib/file-explorer/network/mount-error'
import { throwShareListError } from '$lib/file-explorer/network/share-list-error'
import { throwReconnectError } from '$lib/file-explorer/network/reconnect-error'
import type {
  CredentialSource,
  DirectConnectionSwitch,
  MountResult,
  NetworkHostContextAction,
  SignInShape,
  SmbCredentials,
  SmbFellBackToOsMount,
  SmbOsMountNoticeWithdrawn,
  UpgradeResult,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'
import type { MenuAnchor } from './file-actions'
import { throwAddServerError } from '$lib/servers/add-server-error'
import type {
  AuthOptions,
  ConnectionMode,
  DiscoveryState,
  KnownNetworkShare,
  NetworkHost,
  ShareListResult,
} from '../file-explorer/types'

export type { SignInShape }

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
// Network discovery event listeners
// ============================================================================

/** Fires when mDNS discovers a host. Payload is the bare `NetworkHost`. */
export function onNetworkHostFound(handler: (host: NetworkHost) => void): Promise<UnlistenFn> {
  return events.networkHostFound.listen((event) => {
    handler(event.payload as NetworkHost)
  })
}

/** Fires when a host disappears. */
export function onNetworkHostLost(handler: (id: string) => void): Promise<UnlistenFn> {
  return events.networkHostLost.listen((event) => {
    handler(event.payload.id)
  })
}

/** Fires when a host's hostname / IP is resolved. Payload is the updated `NetworkHost`. */
export function onNetworkHostResolved(handler: (host: NetworkHost) => void): Promise<UnlistenFn> {
  return events.networkHostResolved.listen((event) => {
    handler(event.payload as NetworkHost)
  })
}

/** Fires when the discovery state changes (idle / searching / active). */
export function onNetworkDiscoveryStateChanged(handler: (state: DiscoveryState) => void): Promise<UnlistenFn> {
  return events.networkDiscoveryStateChanged.listen((event) => {
    handler(event.payload.state)
  })
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
 * @returns The shares and the auth mode
 * @throws ShareListFailure carrying the typed `ShareListError`; `shareListErrorOf` gets it back
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
  if (res.status === 'error') throwShareListError(res.error)
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
 * The username to pre-fill for `serverName`, or null if it has never been signed in to.
 *
 * Pass whatever name you have (an mDNS instance name, a `.local` hostname, an IP): the
 * backend matches on the server's stable identity, so any name form finds a hint saved
 * under any other. Only available on macOS.
 */
export async function getUsernameHint(serverName: string): Promise<string | null> {
  try {
    return await commands.getUsernameHint(serverName)
  } catch {
    // Command not available (non-macOS) - no hint to offer
    return null
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
 * Whether a server-level password was already read this session (a listing or a
 * mount found it). ❗ Cache only: never touches the Keychain, so it costs no prompt.
 */
export async function hasCachedSmbCredentials(server: string): Promise<boolean> {
  return commands.hasCachedSmbCredentials(server)
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
 * @param credentialSource `typed` for the sign-in sheet, `saved` for a Keychain entry (logged on a refusal)
 * @param timeoutMs Optional timeout in milliseconds (default: 15000)
 * @param cacheTtlMs Optional cache TTL in milliseconds (default: 30000)
 * @throws ShareListFailure carrying the typed `ShareListError`; `shareListErrorOf` gets it back
 */
export async function listSharesWithCredentials(
  hostId: string,
  hostname: string,
  ipAddress: string | undefined,
  port: number,
  username: string | null,
  password: string | null,
  credentialSource: CredentialSource,
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
    credentialSource,
    timeoutMs ?? null,
    cacheTtlMs ?? null,
  )
  if (res.status === 'error') throwShareListError(res.error)
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
 * @param hostName The name the person knows the server by; a mount that goes through is
 *   saved as a share place under it (`docs/specs/saved-smb-shares.md`)
 * @returns MountResult with mount path on success
 * @throws MountFailure carrying the typed `MountError`; `asMountError` gets it back, and
 *   `renderMountError` words it
 */
export async function mountNetworkShare(
  server: string,
  share: string,
  username: string | null,
  password: string | null,
  port?: number,
  timeoutMs?: number,
  hostName?: string,
): Promise<MountResult> {
  const res = await commands.mountNetworkShare(
    server,
    share,
    username,
    password,
    port ?? null,
    timeoutMs ?? null,
    hostName ?? null,
  )
  if (res.status === 'error') throwMountError(res.error)
  return res.data
}

/** Where a "Connect directly" left the volume. Every outcome is an answer, never a throw. */
export type { UpgradeResult }

/**
 * Upgrades an existing OS-mounted SMB volume to use a direct smb2 connection.
 *
 * Tries stored credentials first. Answers `credentialsNeeded` when the frontend
 * should ask for a password, `networkError` for a server that didn't cooperate,
 * and `volumeGone` / `notSmbMount` when there's no OS-mounted share to upgrade.
 */
export async function upgradeToSmbVolume(volumeId: string): Promise<UpgradeResult> {
  return commands.upgradeToSmbVolume(volumeId)
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
  return commands.upgradeToSmbVolumeUsingSavedPassword(volumeId)
}

/** What switching a share's direct connection did. */
export type { DirectConnectionSwitch }

/**
 * Whether the SMB share behind `volumeId` may use Cmdr's fast direct connection (the
 * per-share switch), or `null` when there's no SMB share behind it to ask about.
 */
export async function getSmbDirectConnectionEnabled(volumeId: string): Promise<boolean | null> {
  return commands.getSmbDirectConnectionEnabled(volumeId)
}

/**
 * Switches the SMB share behind `volumeId` onto or off Cmdr's fast direct connection.
 * Off on a direct share hands it back to the macOS mount right away
 * (`returnedToOsMount`); on only saves, and the caller runs "Connect directly".
 */
export async function setSmbDirectConnectionEnabled(
  volumeId: string,
  enabled: boolean,
): Promise<DirectConnectionSwitch> {
  return commands.setSmbDirectConnectionEnabled(volumeId, enabled)
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
  return commands.upgradeToSmbVolumeWithCredentials(volumeId, username, password, rememberInKeychain)
}

/**
 * Tries to rebuild a Disconnected volume's session in place.
 *
 * Backend-neutral: every remote backend implements this, and the per-volume
 * reconnect manager drives SMB, SFTP, and WebDAV through it on each backoff tick
 * (and on "Retry now" / lazy nav-time retry). The backend single-flights
 * concurrent calls. Resolves on success; a refusal throws a `ReconnectFailure`
 * carrying the typed `ReconnectError`, which `asReconnectError` gets back.
 */
export async function reconnectVolume(volumeId: string): Promise<void> {
  const res = await commands.reconnectVolume(volumeId)
  if (res.status === 'error') throwReconnectError(res.error)
}

/**
 * Reconnects a volume with freshly-entered credentials. Used by the "Sign in"
 * affordance shown when an in-place reconnect gave up on an auth failure (the
 * saved password went stale). The backend refreshes what it has stored and
 * reconnects; on success a `volume-connection-changed { state: "connected" }`
 * event follows.
 *
 * Whether the USERNAME may change is the backend's call, not this wrapper's:
 * `getVolumeSignInState` is what tells the sheet which shape to render.
 *
 * Resolves on success; a refusal throws a `ReconnectFailure` carrying the typed
 * `ReconnectError`, which `asReconnectError` gets back.
 */
export async function reconnectVolumeWithCredentials(
  volumeId: string,
  username: string,
  password: string,
): Promise<void> {
  const res = await commands.reconnectVolumeWithCredentials(volumeId, username, password)
  if (res.status === 'error') throwReconnectError(res.error)
}

/**
 * What a "Sign in" affordance on this volume may ask the user for, right now.
 *
 * Ask when the affordance renders, and never hold on to the answer: a backend
 * that authenticates per connection can prove itself with a different credential
 * each time it dials, so a value kept from earlier describes a session that may
 * already be gone.
 *
 * Backend-neutral. A volume with no sign-in story of its own, and an id nothing
 * is registered under, both answer `'password'`, which is the safe way to be
 * wrong: a needless password box is recoverable, a wrong `'nothing'` is a volume
 * the user can't sign in to at all.
 */
export async function getVolumeSignInState(volumeId: string): Promise<SignInShape> {
  return await commands.getVolumeSignInState(volumeId)
}

/**
 * Disconnects an SMB volume by unmounting it at the OS level (macOS) or by
 * dropping the smb2 session (Linux, until GVFS unmount is wired up). The
 * `volumes-changed` event removes the volume from the picker shortly after.
 *
 * Resolves on success; a refusal throws an `EjectFailure` carrying the typed
 * `EjectError` (`unmountRefused` when a Finder window still has the share
 * open). `asEjectError` gets it back, and `renderEjectError` words it.
 */
export async function disconnectSmbVolume(volumeId: string): Promise<void> {
  const res = await commands.disconnectSmbVolume(volumeId)
  if (res.status === 'error') throwEjectError(res.error)
}

// ============================================================================
// Manual server management (macOS only)
// ============================================================================

/**
 * Connects to a manually-specified server: parses address, checks TCP reachability,
 * persists the entry, and injects a synthetic host into the discovery state.
 * @param address Hostname, IP, IP:port, or smb:// URL
 * @param name What the Add form's Name field held; empty leaves the server unnamed
 * @param username The account the person means to sign in as, or `null` for none
 * @param checkReachability `false` for the sheet's "Add anyway": saves without probing
 * @returns The injected host and optional share path
 * @throws AddServerFailure carrying the typed `AddServerError` (`asAddServerError`)
 */
export async function connectToServer(
  address: string,
  name = '',
  username: string | null = null,
  checkReachability = true,
): Promise<ManualConnectResult> {
  const res = await commands.connectToServer(address, name === '' ? null : name, username, checkReachability)
  if (res.status === 'error') throwAddServerError(res.error)
  return res.data as ManualConnectResult
}

/**
 * Removes a manually-added server by ID.
 * Deletes from persistent storage and removes from discovery state.
 * @param serverId The manual server's host ID (like "manual-192-168-1-100-445")
 */
/**
 * Sets the account the SMB server `serverName` (its discovery name) is used with, the
 * same preference a typed username is: "Sign in as…" in its share list, or `null` for
 * guest. Answers whether the store could be written.
 */
export async function setSmbAccountPreference(serverName: string, username: string | null): Promise<boolean> {
  return await commands.setSmbAccountPreference(serverName, username)
}

/**
 * Tells the backend the user took a network action: opening the Servers view,
 * "Connect to server…", or upgrading a mounted share to direct smb2. It brings back
 * the manual servers and runs the existing-SMB-mount upgrade pass. It starts no browse
 * of its own: see `setServersViewShown`.
 */
export async function noteNetworkAction(): Promise<void> {
  try {
    await commands.noteNetworkAction()
  } catch {
    // Stub on unsupported platforms. Silently swallow.
  }
}

/**
 * Whether any Servers view is on screen. The backend browses mDNS only while one is
 * (or while something else needs discovery), so hosts arrive and leave live there and
 * nothing browses at idle. The first browse is what raises macOS's "Cmdr wants to
 * find devices on local networks" prompt; the backend won't browse while
 * `network.enabled` is off.
 */
export async function setServersViewShown(shown: boolean): Promise<void> {
  try {
    await commands.setServersViewShown(shown)
  } catch {
    // Stub on unsupported platforms. Silently swallow.
  }
}

/**
 * Pushes the `network.enabled` toggle live to the backend. When `false`, stops mDNS and
 * clears the discovered host list (the frontend store empties via `network-host-lost`).
 * When `true`, the browse resumes if a Servers view is holding it.
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
 * Shows a native context menu for a servers hub row's SMB host (fire-and-forget).
 * It offers only what does something: "Edit server…" for a saved host, "Disconnect"
 * while a share from it is mounted, "Forget saved password" when one is stored, and
 * "Forget server" for a typed-in host. The host goes whole: its mounts are found by
 * where it dials.
 *
 * ❗ `rowId` is the hub row it was raised on, and the answer carries it back
 * (`NetworkHostContextAction.rowId`): the row, not the host, is what the answer
 * acts on. `anchor` places a keyboard-opened menu; `null` uses the pointer.
 */
export async function showNetworkHostContextMenu(
  rowId: string,
  host: NetworkHost,
  isManual: boolean,
  isSaved: boolean,
  hasCredentials: boolean,
  anchor: MenuAnchor | null = null,
): Promise<void> {
  const res = await commands.showNetworkHostContextMenu(
    rowId,
    {
      id: host.id,
      name: host.name,
      hostname: host.hostname ?? null,
      ipAddress: host.ipAddress ?? null,
      port: host.port,
      source: host.source,
    },
    isManual,
    isSaved,
    hasCredentials,
    anchor,
  )
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Listens for the network host context menu action event emitted by `on_menu_event` in Rust.
 * The event fires asynchronously after `popup()` returns.
 */
export function onNetworkHostContextAction(handler: (payload: NetworkHostContextAction) => void): Promise<UnlistenFn> {
  return events.networkHostContextAction.listen((event) => {
    handler(event.payload)
  })
}

/**
 * Subscribes to the notice that a share Cmdr tried to take over is staying on the
 * macOS kernel mount (slower, and outside Cmdr's control).
 *
 * Fires at most once per server per app run: the backend's notice ledger collapses
 * the one-call-per-mounted-share fallback into one message. Call the returned
 * `UnlistenFn` on destroy.
 */
export function onSmbFellBackToOsMount(handler: (payload: SmbFellBackToOsMount) => void): Promise<UnlistenFn> {
  return events.smbFellBackToOsMount.listen((event) => {
    handler(event.payload)
  })
}

/**
 * Subscribes to the backend taking back a slow-connection notice it raised with
 * `smb-fell-back-to-os-mount`: the share's direct connection was switched off, so
 * the notice's retry has nothing left to offer. Call the returned `UnlistenFn` on
 * destroy.
 */
export function onSmbOsMountNoticeWithdrawn(
  handler: (payload: SmbOsMountNoticeWithdrawn) => void,
): Promise<UnlistenFn> {
  return events.smbOsMountNoticeWithdrawn.listen((event) => {
    handler(event.payload)
  })
}

/**
 * Unmounts all SMB shares mounted from `host`, answering the mount paths that went.
 * ❗ The whole host: its mounts are found by where it dials (hostname, IP, each on
 * its port), never by a display name.
 */
export async function disconnectNetworkHost(host: NetworkHost): Promise<string[]> {
  const res = await commands.disconnectNetworkHost({
    id: host.id,
    name: host.name,
    hostname: host.hostname ?? null,
    ipAddress: host.ipAddress ?? null,
    port: host.port,
    source: host.source,
  })
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}
