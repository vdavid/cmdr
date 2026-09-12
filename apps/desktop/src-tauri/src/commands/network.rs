//! Tauri commands for network host discovery and SMB share listing.

use crate::file_system::volume::reconnect_error::ReconnectError;
use crate::network::{
    AuthMode, DiscoveryState, NetworkHost, ShareListError, ShareListResult, get_discovered_hosts,
    get_discovery_state_value, get_host_for_resolution, resolve_host_ip, service_name_to_hostname, smb_client,
    update_host_resolution,
};

use crate::network::smb_connect_directly::{self, UpgradeResult};
use crate::network::smb_upgrade::register_smb_volume;

/// Gets all currently discovered network hosts.
#[tauri::command]
#[specta::specta]
pub fn list_network_hosts() -> Vec<NetworkHost> {
    get_discovered_hosts()
}

/// Gets the current discovery state.
#[tauri::command]
#[specta::specta]
pub fn get_network_discovery_state() -> DiscoveryState {
    get_discovery_state_value()
}

/// Resolves a network host by ID, returning the host with hostname and IP address populated.
/// This is an async command that uses spawn_blocking for the DNS lookup to avoid blocking
/// the main thread pool. Multiple hosts can resolve in parallel.
#[tauri::command]
#[specta::specta]
pub async fn resolve_host(host_id: String) -> Option<NetworkHost> {
    // Get host info (brief mutex hold)
    let info = get_host_for_resolution(&host_id)?;

    // If already resolved, return current state quickly
    if info.ip_address.is_some() {
        return Some(NetworkHost {
            id: info.id,
            name: info.name,
            hostname: info.hostname,
            ip_address: info.ip_address,
            port: info.port,
            source: info.source,
        });
    }

    // Generate hostname
    let hostname = info.hostname.unwrap_or_else(|| service_name_to_hostname(&info.name));
    let hostname_clone = hostname.clone();

    // Do DNS resolution in a blocking task (this is the slow part - runs on separate thread)
    let ip_address = tokio::task::spawn_blocking(move || resolve_host_ip(&hostname_clone))
        .await
        .ok()
        .flatten();

    // Update host with results (brief mutex hold)
    update_host_resolution(&host_id, hostname, ip_address)
}

/// Lists shares available on a network host.
///
/// Returns cached results if available, otherwise queries the host.
/// Attempts guest access first; returns an error if authentication is required.
///
/// # Arguments
/// * `host_id` - Unique identifier for the host (used for caching)
/// * `hostname` - Hostname to connect to (for example, "TEST_SERVER.local")
/// * `ip_address` - Optional resolved IP address (preferred over hostname for reliability)
/// * `port` - SMB port (default 445, but Docker containers may use different ports)
/// * `timeout_ms` - Optional timeout in milliseconds (default: 15000)
/// * `cache_ttl_ms` - Optional cache TTL in milliseconds (default: 30000)
#[tauri::command]
#[specta::specta]
pub async fn list_shares_on_host(
    host_id: String,
    hostname: String,
    ip_address: Option<String>,
    port: u16,
    timeout_ms: Option<u64>,
    cache_ttl_ms: Option<u64>,
) -> Result<ShareListResult, ShareListError> {
    smb_client::list_shares(
        &host_id,
        &hostname,
        ip_address.as_deref(),
        port,
        None,
        timeout_ms,
        cache_ttl_ms,
    )
    .await
}

/// Prefetches shares for a host (for example, on hover).
/// Same as list_shares_on_host but designed for prefetching - errors are silently ignored.
/// Returns immediately if shares are already cached.
#[tauri::command]
#[specta::specta]
pub async fn prefetch_shares(
    host_id: String,
    hostname: String,
    ip_address: Option<String>,
    port: u16,
    timeout_ms: Option<u64>,
    cache_ttl_ms: Option<u64>,
) {
    // Fire and forget - we don't care about the result for prefetching
    let _ = smb_client::list_shares(
        &host_id,
        &hostname,
        ip_address.as_deref(),
        port,
        None,
        timeout_ms,
        cache_ttl_ms,
    )
    .await;
}

/// Gets auth mode detected for a host (from cached share list if available).
#[tauri::command]
#[specta::specta]
pub fn get_host_auth_mode(host_id: String) -> AuthMode {
    // Try to get from cache
    if let Some(cached) = smb_client::get_cached_shares_auth_mode(&host_id) {
        return cached;
    }
    AuthMode::Unknown
}

// --- Known Shares Commands ---

use crate::network::known_shares::{
    self, AuthOptions, ConnectionMode, KnownNetworkShare, get_all_known_shares,
    get_known_share as get_known_share_inner,
};

/// Gets all known network shares (previously connected).
#[tauri::command]
#[specta::specta]
pub fn get_known_shares() -> Vec<KnownNetworkShare> {
    get_all_known_shares()
}

/// Gets a specific known share by server and share name.
#[tauri::command]
#[specta::specta]
pub fn get_known_share_by_name(server_name: String, share_name: String) -> Option<KnownNetworkShare> {
    get_known_share_inner(&server_name, &share_name)
}

/// Updates or adds a known network share after successful connection.
#[tauri::command]
#[specta::specta]
pub fn update_known_share(
    app: tauri::AppHandle,
    server_name: String,
    share_name: String,
    last_connection_mode: ConnectionMode,
    last_known_auth_options: AuthOptions,
    username: Option<String>,
) {
    let share = KnownNetworkShare {
        server_name,
        share_name,
        protocol: "smb".to_string(),
        last_connected_at: chrono::Utc::now().to_rfc3339(),
        last_connection_mode,
        last_known_auth_options,
        username,
    };

    known_shares::update_known_share(&app, share);
}

/// The username to pre-fill for `server_name`, or `None` if it has never been signed in to.
///
/// Takes the server by name and answers for that one server, rather than handing back a
/// map the caller has to key into: the identity rule lives in `known_shares`, not in the
/// IPC contract. See `known_shares::get_username_hint`.
#[tauri::command]
#[specta::specta]
pub fn get_username_hint(server_name: String) -> Option<String> {
    known_shares::get_username_hint(&server_name)
}

// --- Keychain Commands ---

use crate::network::keychain::{self, KeychainError, SmbCredentials};

/// Saves SMB credentials to the Keychain.
/// Credentials are stored under "Cmdr" service name.
#[tauri::command]
#[specta::specta]
pub fn save_smb_credentials(
    server: String,
    share: Option<String>,
    username: String,
    password: String,
) -> Result<(), KeychainError> {
    keychain::save_credentials(&server, share.as_deref(), &username, &password)
}

/// Retrieves SMB credentials from the Keychain.
/// Returns the stored username and password if found.
#[tauri::command]
#[specta::specta]
pub fn get_smb_credentials(server: String, share: Option<String>) -> Result<SmbCredentials, KeychainError> {
    keychain::get_credentials(&server, share.as_deref())
}

/// Checks if credentials exist in the Keychain for a server/share.
#[tauri::command]
#[specta::specta]
pub fn has_smb_credentials(server: String, share: Option<String>) -> bool {
    keychain::has_credentials(&server, share.as_deref())
}

/// Deletes SMB credentials from the Keychain.
#[tauri::command]
#[specta::specta]
pub fn delete_smb_credentials(server: String, share: Option<String>) -> Result<(), KeychainError> {
    keychain::delete_credentials(&server, share.as_deref())
}

/// Returns whether credential storage is using an encrypted file fallback
/// instead of the system keyring. The frontend can use this to show a one-time
/// info toast when the user first saves credentials without a system keyring.
#[tauri::command]
#[specta::specta]
pub fn is_using_credential_file_fallback() -> bool {
    crate::secrets::is_file_backed()
}

/// Lists shares on a host using stored or provided credentials.
/// This is the main command for authenticated share listing.
///
/// # Arguments
/// * `host_id` - Unique identifier for the host (used for caching)
/// * `hostname` - Hostname to connect to
/// * `ip_address` - Optional resolved IP address
/// * `port` - SMB port
/// * `username` - Username for authentication (or None for guest)
/// * `password` - Password for authentication (or None for guest)
/// * `timeout_ms` - Optional timeout in milliseconds (default: 15000)
/// * `cache_ttl_ms` - Optional cache TTL in milliseconds (default: 30000)
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::too_many_arguments,
    reason = "Tauri command requires all parameters to be top-level"
)]
pub async fn list_shares_with_credentials(
    host_id: String,
    hostname: String,
    ip_address: Option<String>,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    timeout_ms: Option<u64>,
    cache_ttl_ms: Option<u64>,
) -> Result<ShareListResult, ShareListError> {
    let credentials = match (username, password) {
        (Some(u), Some(p)) => Some((u, p)),
        _ => None,
    };

    smb_client::list_shares(
        &host_id,
        &hostname,
        ip_address.as_deref(),
        port,
        credentials.as_ref().map(|(u, p)| (u.as_str(), p.as_str())),
        timeout_ms,
        cache_ttl_ms,
    )
    .await
}

// --- Mount Commands ---

use crate::network::mount::{self, MountError, MountResult};

/// Mounts an SMB share to the local filesystem.
///
/// Attempts to mount the specified share on the server. If credentials are
/// provided, they are used for authentication. If the share is already mounted,
/// returns the existing mount path without re-mounting.
///
/// After a successful OS mount, also establishes a direct smb2 connection and
/// registers the share as an `SmbVolume` in the `VolumeManager`. This means
/// Cmdr's own file operations go through smb2 (fast), while Finder/Terminal
/// use the OS mount (compatible). If smb2 connection fails, the volume falls
/// through to a regular `LocalPosixVolume` (registered by the watcher).
///
/// # Arguments
/// * `server` - Server hostname or IP address
/// * `share` - Name of the share to mount
/// * `username` - Optional username for authentication
/// * `password` - Optional password for authentication
/// * `port` - SMB port (default 445)
/// * `timeout_ms` - Optional timeout in milliseconds (default: 20000)
///
/// # Returns
/// * `Ok(MountResult)` - Mount successful, with path to mount point
/// * `Err(MountError)` - Mount failed with specific error type
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::too_many_arguments,
    reason = "Tauri command requires all parameters to be top-level"
)]
pub async fn mount_network_share(
    server: String,
    share: String,
    username: Option<String>,
    password: Option<String>,
    port: Option<u16>,
    timeout_ms: Option<u64>,
) -> Result<MountResult, MountError> {
    let actual_port = port.unwrap_or(445);
    let result = crate::network::mount_share(
        server.clone(),
        share.clone(),
        username.clone(),
        password.clone(),
        actual_port,
        timeout_ms,
    )
    .await?;

    // Try to establish a direct smb2 connection and register as SmbVolume.
    // If this fails, the FSEvents watcher will register a LocalPosixVolume
    // as fallback (slower but still functional).
    register_smb_volume(
        &server,
        &share,
        &result.mount_path,
        username.as_deref(),
        password.as_deref(),
        actual_port,
    )
    .await;

    Ok(result)
}

/// Upgrades an existing OS-mounted SMB volume to use a direct smb2 connection, with
/// the credentials Cmdr stored for its share.
///
/// Called from the "Connect directly for faster access" UI action. Every answer,
/// a volume that's gone or isn't an SMB mount included:
/// `network::smb_connect_directly::UpgradeResult`.
#[tauri::command]
#[specta::specta]
pub async fn upgrade_to_smb_volume(volume_id: String, app_handle: tauri::AppHandle) -> UpgradeResult {
    // Kick mDNS off so IP → hostname resolution has a shot before the Keychain
    // lookup. Idempotent; a no-op if it's already running or `network.enabled` is
    // off. Here rather than in the upgrade itself, which stays `AppHandle`-free so
    // the MCP executor (generic over `Runtime`) can call it.
    crate::network::ensure_mdns_started(app_handle);
    smb_connect_directly::connect_directly(&volume_id).await
}

// Per-drive indexing enable/disable/rescan lives in `commands/indexing.rs` as a
// single drive-type-agnostic surface (`enable_drive_index` / `disable_drive_index`
// / `rescan_drive_index`), so the freshness UX drives any drive (local or SMB)
// through one set of commands. The SMB-specific gate + typed `SmbIndexGateReason`
// it surfaces still live in `indexing::start_indexing_for_smb`.

/// Upgrades an existing OS-mounted SMB volume using explicit credentials.
///
/// Called with what the user typed into the sign-in sheet `upgrade_to_smb_volume`
/// led to.
#[tauri::command]
#[specta::specta]
pub async fn upgrade_to_smb_volume_with_credentials(
    volume_id: String,
    username: Option<String>,
    password: Option<String>,
    remember_in_keychain: bool,
    app_handle: tauri::AppHandle,
) -> UpgradeResult {
    // Kick mDNS off so a remembered password is saved under the hostname, not the raw IP.
    crate::network::ensure_mdns_started(app_handle);
    smb_connect_directly::connect_directly_with_credentials(&volume_id, username, password, remember_in_keychain).await
}

/// Does the system (login) keychain hold an SMB password another app (Finder) saved for
/// this volume's server? Attributes-only probe — **never triggers the consent dialog** —
/// so the frontend can decide whether to offer the "Use the password macOS saved"
/// affordance. macOS-only; returns `false` everywhere else.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub async fn system_has_saved_smb_password(volume_id: String) -> Result<bool, String> {
    Ok(smb_connect_directly::system_has_saved_password(&volume_id).await)
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub async fn system_has_saved_smb_password(_volume_id: String) -> Result<bool, String> {
    Ok(false)
}

/// Upgrades an OS-mounted SMB volume to a direct smb2 connection using the password
/// another app (Finder) already saved in the login keychain, so the user doesn't retype
/// it. Reading it raises the macOS consent dialog, so this is **user-initiated only**:
/// never call it at startup. Where there's no system keychain, it asks for the password.
#[tauri::command]
#[specta::specta]
pub async fn upgrade_to_smb_volume_using_saved_password(
    volume_id: String,
    app_handle: tauri::AppHandle,
) -> UpgradeResult {
    // Warm mDNS so the alias set (Finder keys by the mDNS service name) is populated.
    crate::network::ensure_mdns_started(app_handle);
    smb_connect_directly::connect_directly_with_system_saved_password(&volume_id).await
}

// --- Disconnect Command ---

/// Unmounts all SMB shares mounted from a given server.
/// Returns the list of mount paths that were unmounted.
/// Uses a 15s timeout because `statfs` on hung mounts can block indefinitely
/// and `diskutil unmount` may wait for the OS to release the mount.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_network_host(
    host_id: String,
    host_name: String,
    ip_address: Option<String>,
) -> Result<Vec<String>, String> {
    use crate::deadline::blocking_with_timeout;
    use std::time::Duration;

    // Drop the cached share list so a later browse re-fetches fresh shares and
    // auth mode rather than serving a stale (up to 30 s TTL) entry for a host
    // the user just disconnected from.
    smb_client::invalidate_cache(&host_id);

    let result = blocking_with_timeout(Duration::from_secs(15), vec![], move || {
        mount::unmount_smb_shares_from_host(&host_name, ip_address.as_deref())
    })
    .await;

    Ok(result)
}

// --- Volume reconnect and sign-in ---
//
// Backend-neutral despite two of the three names: each one asks whatever volume
// is registered under the id, so an SFTP volume goes through them unchanged. The
// frontend's reconnect manager calls all three and never branches on backend.

/// What FORM a "Sign in" affordance on this volume takes, right now: which fields
/// the sheet renders, and whether the username among them is editable.
///
/// ❗ **Asked when the affordance renders, ❌ never carried on a connect result.**
/// A backend that authenticates per connection can prove itself with a different
/// credential each time it dials, so an answer captured when the volume was
/// opened describes a session that may already be gone. This reads the live
/// volume, which is the only moment the answer is true.
///
/// A volume with no sign-in story of its own, and an id nothing is registered
/// under, both answer `Password` (`Volume::sign_in_prompt`'s default): the one
/// safe way to be wrong here is a needless password box, because a wrong
/// `Nothing` is a volume the user can't sign in to at all.
///
/// ❗ The answer is a tagged union, so the frontend switches on `kind` and
/// ❌ never derives the form from the protocol, the mode the sheet is in, or the
/// rung a connect result mentioned.
#[tauri::command]
#[specta::specta]
// No timeout wrapper: this reads a lock and a map, and reaches no device.
pub async fn get_volume_sign_in_state(volume_id: String) -> cmdr_fs::volume::SignInShape {
    use crate::file_system::volume::manager::get_volume_manager;

    get_volume_manager()
        .get(&volume_id)
        .map_or(cmdr_fs::volume::SignInShape::Password, |volume| volume.sign_in_prompt())
}

/// Tries to rebuild a Disconnected volume's session in place.
///
/// ❗ Backend-neutral: every remote backend implements `attempt_reconnect`, and
/// the frontend's reconnect manager drives SMB, SFTP, and WebDAV through this
/// one command. Called on each backoff tick (and on "Retry now" / lazy nav-time
/// retry). The backend single-flights concurrent calls, so the frontend is free
/// to fire on its own schedule. Returns `Ok(())` on success (the volume now
/// reports `Direct`), or a typed [`ReconnectError`] saying why the rebuild
/// didn't happen.
///
/// A volume whose backend can't redial yields `ReconnectError::Volume` carrying
/// `VolumeError::NotSupported` (the trait default).
#[tauri::command]
#[specta::specta]
pub async fn reconnect_volume(volume_id: String) -> Result<(), ReconnectError> {
    use crate::file_system::volume::manager::get_volume_manager;

    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| ReconnectError::VolumeNotFound {
            volume_id: volume_id.clone(),
        })?;

    volume.attempt_reconnect().await.map_err(ReconnectError::from)
}

/// Reconnects a volume with freshly-entered credentials.
///
/// ❗ Backend-neutral, like [`reconnect_volume`]. Invoked by the "Sign in"
/// affordance shown when an in-place reconnect gave up on an auth failure (a
/// `needs_credentials` `volume-connection-changed` event). The volume refreshes
/// what it has stored (so future reconnects are silent) and runs the standard
/// reconnect; on success the backend emits
/// `volume-connection-changed { state: "connected" }`.
///
/// ❗ Whether the USERNAME may change is the backend's call, not this command's:
/// SMB accepts a new one and rewrites its params, SFTP and WebDAV refuse because
/// the volume id IS the account. `SignInShape` is what tells the sheet which it
/// is. A backend that can't redial yields `ReconnectError::Volume` carrying
/// `VolumeError::NotSupported`.
#[tauri::command]
#[specta::specta]
pub async fn reconnect_volume_with_credentials(
    volume_id: String,
    username: String,
    password: String,
) -> Result<(), ReconnectError> {
    use crate::file_system::volume::manager::get_volume_manager;

    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| ReconnectError::VolumeNotFound {
            volume_id: volume_id.clone(),
        })?;

    volume
        .reconnect_with_credentials(username, password)
        .await
        .map_err(ReconnectError::from)
}

/// Disconnects a single SMB volume by tearing down its OS mount.
///
/// Thin delegate to [`crate::file_system::volume::eject::disconnect_smb`]; the
/// typed `EjectError` IS the wire type, so it crosses unchanged. Called by the
/// "Disconnect" button in the pane's reconnect view and the gave-up
/// `VolumeUnreachableBanner`.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_smb_volume(volume_id: String) -> Result<(), crate::file_system::volume::eject::EjectError> {
    crate::file_system::volume::eject::disconnect_smb(&volume_id).await
}

// --- Manual Server Commands ---

use crate::network::manual_servers::{self, ManualConnectResult};

/// Connects to a manually-specified server: parses, checks reachability, persists, and injects.
#[tauri::command]
#[specta::specta]
pub async fn connect_to_server(address: String, app_handle: tauri::AppHandle) -> Result<ManualConnectResult, String> {
    manual_servers::add_manual_server(&address, &app_handle).await
}

/// Removes a manually-added server by ID.
#[tauri::command]
#[specta::specta]
pub fn remove_manual_server(server_id: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    manual_servers::remove_manual_server(&server_id, &app_handle)
}

/// Idempotently starts mDNS discovery if it isn't running. Triggered by the frontend the first
/// time the user takes a network action (clicks "Network", opens "Connect to server…", or
/// upgrades a mounted share to direct smb2). The first call here is what triggers macOS's
/// "Cmdr wants to find devices on local networks" prompt; we defer to the latest reasonable
/// moment so fresh installs don't see the prompt at launch.
///
/// Also kicks off the existing-SMB-mount upgrade pass: if macOS auto-remounted SMB shares
/// at login, this is the first moment we can open direct smb2 connections to them (TCP to a
/// private IP also gates on the Local Network permission).
///
/// Reloads manually-added servers in case discovery was previously stopped (toggle-off path)
/// and `DISCOVERY_STATE` got cleared.
#[tauri::command]
#[specta::specta]
pub fn ensure_network_discovery_started(app_handle: tauri::AppHandle) {
    crate::network::start_discovery(app_handle.clone());
    manual_servers::load_manual_servers(&app_handle);
    crate::file_system::upgrade_existing_smb_mounts(app_handle.clone());

    #[cfg(feature = "smb-e2e")]
    crate::network::virtual_smb_hosts::setup_virtual_smb_hosts(&app_handle);
}

/// Live-apply the `network.enabled` toggle. When `false`, stops mDNS and clears the discovered
/// host list (frontend store empties via emitted `network-host-lost` events). When `true`, this
/// is a no-op; the frontend triggers `ensure_network_discovery_started` separately when the
/// user takes a network action.
#[tauri::command]
#[specta::specta]
pub fn set_network_enabled(enabled: bool, app_handle: tauri::AppHandle) {
    crate::network::set_network_enabled_flag(enabled);
    if !enabled {
        crate::network::mdns_discovery::stop_discovery();
        crate::network::clear_discovered_hosts(&app_handle);
    }
}

#[cfg(test)]
#[path = "network_test.rs"]
mod network_test;
