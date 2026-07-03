//! Tauri commands for network host discovery and SMB share listing.

use crate::network::{
    AuthMode, DiscoveryState, NetworkHost, ShareListError, ShareListResult, get_discovered_hosts,
    get_discovery_state_value, get_host_for_resolution, resolve_host_ip, service_name_to_hostname, smb_client,
    update_host_resolution,
};

use crate::network::smb_upgrade::{
    UpgradeError, UpgradeResult, friendly_server_name, get_keychain_password, register_smb_volume,
    resolve_ip_to_hostname_with_wait, try_smb_upgrade,
};
// Only the macOS-gated commands below read the system keychain aliases; an unconditional
// import fails the Linux build via `#![deny(unused)]`.
#[cfg(target_os = "macos")]
use crate::network::smb_upgrade::system_keychain_aliases;

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

/// Gets username hints for servers (last used username per server).
#[tauri::command]
#[specta::specta]
pub fn get_username_hints() -> std::collections::HashMap<String, String> {
    known_shares::get_username_hints()
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
    let result = mount::mount_share(
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

/// Upgrades an existing OS-mounted SMB volume to use a direct smb2 connection.
///
/// Extracts server/share/username from `statfs`, tries stored credentials,
/// and either upgrades to `SmbVolume` or returns `CredentialsNeeded` so
/// the frontend can show a login form.
///
/// Called from the "Connect directly for faster access" UI action.
#[tauri::command]
#[specta::specta]
pub async fn upgrade_to_smb_volume(volume_id: String, app_handle: tauri::AppHandle) -> Result<UpgradeResult, String> {
    // Kick mDNS off so IP → hostname resolution has a shot before we hit the
    // Keychain. Idempotent; no-op if already running or `network.enabled` is off.
    // Kept here (and not in `upgrade_to_smb_volume_inner`) because
    // `ensure_mdns_started` requires a concrete `AppHandle`, while the inner
    // function needs to stay AppHandle-free so the MCP executor (generic over
    // `Runtime`) can call it. MCP relies on mDNS having been started elsewhere
    // (initial launch with `firstTriggerDone == true`, or any prior network
    // action); the MCP tool's description tells agents to take a network
    // action first if their target volume needs hostname-keyed creds.
    crate::network::ensure_mdns_started(app_handle);
    upgrade_to_smb_volume_inner(volume_id).await
}

// Per-drive indexing enable/disable/rescan lives in `commands/indexing.rs` as a
// single drive-type-agnostic surface (`enable_drive_index` / `disable_drive_index`
// / `rescan_drive_index`), so the freshness UX drives any drive (local or SMB)
// through one set of commands. The SMB-specific gate + typed `SmbIndexGateReason`
// it surfaces still live in `indexing::start_indexing_for_smb`.

/// Body of `upgrade_to_smb_volume` minus the mDNS kick (which needs concrete
/// `AppHandle`). Used by the Tauri command above and by the MCP
/// `upgrade_smb_to_direct` executor — both routes share the same Keychain
/// lookup, mDNS-cached hostname resolution, and `try_smb_upgrade` body.
pub async fn upgrade_to_smb_volume_inner(volume_id: String) -> Result<UpgradeResult, String> {
    use crate::file_system::get_volume_manager;
    #[cfg(target_os = "macos")]
    use crate::volumes::get_smb_mount_info;
    #[cfg(target_os = "linux")]
    use crate::volumes_linux::get_smb_mount_info;

    let manager = get_volume_manager();

    // Get the current volume's root path
    let volume = manager.get(&volume_id).ok_or("Volume not found")?;
    let mount_path = volume.root().to_string_lossy().to_string();

    // Check if already an SmbVolume
    if volume.smb_connection_state().is_some() {
        return Ok(UpgradeResult::Success);
    }

    // Extract SMB connection info from statfs
    let info = get_smb_mount_info(&mount_path).ok_or_else(|| {
        format!(
            "Can't determine SMB server info for {}. Is this an SMB mount?",
            mount_path
        )
    })?;

    log::info!(
        "Upgrading volume {} to SmbVolume: server={}, share={}, user={:?}",
        volume_id,
        info.server,
        info.share,
        info.username
    );

    // Try to get credentials from Keychain. The mount source has the IP, but Cmdr
    // stores Keychain credentials keyed by hostname (from mDNS). Try both. Briefly
    // wait for mDNS to warm up so we don't prompt for creds the user already saved.
    let hostname = resolve_ip_to_hostname_with_wait(&info.server, std::time::Duration::from_millis(1500)).await;
    let display_name = friendly_server_name(&info.server);
    let creds = get_keychain_password(&info.server, hostname.as_deref(), &info.share).await;

    match &creds {
        Some((u, _)) => log::info!("Found Keychain credentials for user={}", u),
        None => {
            log::info!("No stored credentials found, requesting credentials from user");
            return Ok(UpgradeResult::CredentialsNeeded {
                server: info.server,
                share: info.share,
                port: info.port,
                display_name,
                username_hint: info.username,
                message: None,
            });
        }
    }

    let (username, password) = match &creds {
        Some((u, p)) => (Some(u.as_str()), Some(p.as_str())),
        None => unreachable!(),
    };

    // Try connecting with stored credentials
    let result = try_smb_upgrade(
        &info.server,
        &info.share,
        &mount_path,
        username,
        password,
        info.port,
        &volume_id,
    )
    .await;

    match result {
        Ok(()) => Ok(UpgradeResult::Success),
        Err(UpgradeError::Auth) => {
            log::info!("Stored credentials didn't work, requesting new credentials");
            Ok(UpgradeResult::CredentialsNeeded {
                server: info.server,
                share: info.share,
                port: info.port,
                display_name,
                username_hint: username.map(|s| s.to_string()),
                message: Some("Stored credentials didn't work".to_string()),
            })
        }
        Err(UpgradeError::Network(msg)) => Ok(UpgradeResult::NetworkError { message: msg }),
    }
}

/// Upgrades an existing OS-mounted SMB volume using explicit credentials.
///
/// Called after the user fills in the login form shown by `upgrade_to_smb_volume`.
#[tauri::command]
#[specta::specta]
pub async fn upgrade_to_smb_volume_with_credentials(
    volume_id: String,
    username: Option<String>,
    password: Option<String>,
    remember_in_keychain: bool,
    app_handle: tauri::AppHandle,
) -> Result<UpgradeResult, String> {
    use crate::file_system::get_volume_manager;
    #[cfg(target_os = "macos")]
    use crate::volumes::get_smb_mount_info;
    #[cfg(target_os = "linux")]
    use crate::volumes_linux::get_smb_mount_info;

    let manager = get_volume_manager();

    let volume = manager.get(&volume_id).ok_or("Volume not found")?;
    let mount_path = volume.root().to_string_lossy().to_string();

    if volume.smb_connection_state().is_some() {
        return Ok(UpgradeResult::Success);
    }

    let info = get_smb_mount_info(&mount_path).ok_or_else(|| {
        format!(
            "Can't determine SMB server info for {}. Is this an SMB mount?",
            mount_path
        )
    })?;

    // Kick mDNS off so we can save credentials keyed by hostname (not raw IP)
    // when the user picks "remember".
    crate::network::ensure_mdns_started(app_handle);

    let hostname = resolve_ip_to_hostname_with_wait(&info.server, std::time::Duration::from_millis(1500)).await;
    let display_name = friendly_server_name(&info.server);

    let result = try_smb_upgrade(
        &info.server,
        &info.share,
        &mount_path,
        username.as_deref(),
        password.as_deref(),
        info.port,
        &volume_id,
    )
    .await;

    match result {
        Ok(()) => {
            // Save credentials on success if requested
            if remember_in_keychain && let (Some(u), Some(p)) = (&username, &password) {
                let server_key = hostname.as_deref().unwrap_or(&info.server);
                if let Err(e) = keychain::save_credentials(server_key, Some(&info.share), u, p) {
                    log::warn!("Couldn't save credentials to Keychain: {}", e);
                }
            }
            Ok(UpgradeResult::Success)
        }
        Err(UpgradeError::Auth) => Ok(UpgradeResult::CredentialsNeeded {
            server: info.server,
            share: info.share,
            port: info.port,
            display_name,
            username_hint: username,
            message: Some("Invalid username or password".to_string()),
        }),
        Err(UpgradeError::Network(msg)) => Ok(UpgradeResult::NetworkError { message: msg }),
    }
}

/// Does the system (login) keychain hold an SMB password another app (Finder) saved for
/// this volume's server? Attributes-only probe — **never triggers the consent dialog** —
/// so the frontend can decide whether to offer the "Use the password macOS saved"
/// affordance. macOS-only; returns `false` everywhere else.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub async fn system_has_saved_smb_password(volume_id: String) -> Result<bool, String> {
    use crate::file_system::get_volume_manager;
    use crate::secrets::system_keychain_smb;
    use crate::volumes::get_smb_mount_info;

    let manager = get_volume_manager();
    let Some(volume) = manager.get(&volume_id) else {
        return Ok(false);
    };
    let mount_path = volume.root().to_string_lossy().to_string();
    let Some(info) = get_smb_mount_info(&mount_path) else {
        return Ok(false);
    };

    // Use whatever the discovery state already knows (don't warm mDNS just to probe).
    let aliases = system_keychain_aliases(&info.server);
    let candidates = system_keychain_smb::server_query_candidates(&info.server, None, &aliases);

    // Attribute read is fast and prompt-free, but still FFI — keep it off the async worker.
    Ok(
        tokio::task::spawn_blocking(move || system_keychain_smb::account_for_any(&candidates).is_some())
            .await
            .unwrap_or(false),
    )
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub async fn system_has_saved_smb_password(_volume_id: String) -> Result<bool, String> {
    Ok(false)
}

/// Upgrades an OS-mounted SMB volume to a direct smb2 connection using the password that
/// another app (Finder/macOS) already saved in the login keychain — so the user doesn't
/// retype it. Reading the password triggers the macOS consent dialog (the frontend primes
/// the user first; we can't customize the system dialog's text). On success, the password
/// is also copied into Cmdr's own store so future reconnects are silent. If nothing is
/// saved or the user denies access, returns `CredentialsNeeded` so the frontend falls back
/// to its login form. **User-initiated only** — never call this at startup.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub async fn upgrade_to_smb_volume_using_saved_password(
    volume_id: String,
    app_handle: tauri::AppHandle,
) -> Result<UpgradeResult, String> {
    use crate::file_system::get_volume_manager;
    use crate::secrets::system_keychain_smb;
    use crate::volumes::get_smb_mount_info;

    let manager = get_volume_manager();
    let volume = manager.get(&volume_id).ok_or("Volume not found")?;
    let mount_path = volume.root().to_string_lossy().to_string();

    if volume.smb_connection_state().is_some() {
        return Ok(UpgradeResult::Success);
    }

    let info = get_smb_mount_info(&mount_path).ok_or_else(|| {
        format!(
            "Can't determine SMB server info for {}. Is this an SMB mount?",
            mount_path
        )
    })?;

    // Warm mDNS so the alias set (Finder keys by the mDNS service name) is populated.
    crate::network::ensure_mdns_started(app_handle);
    let hostname = resolve_ip_to_hostname_with_wait(&info.server, std::time::Duration::from_millis(1500)).await;
    let display_name = friendly_server_name(&info.server);

    let aliases = system_keychain_aliases(&info.server);
    let candidates = system_keychain_smb::server_query_candidates(&info.server, hostname.as_deref(), &aliases);

    // The data read triggers the consent dialog and blocks on the user — keep it off the
    // async worker pool.
    let creds = tokio::task::spawn_blocking(move || system_keychain_smb::read_password(&candidates))
        .await
        .ok()
        .flatten();

    let Some(creds) = creds else {
        // Nothing readable, or the user denied access → fall back to the login form.
        return Ok(UpgradeResult::CredentialsNeeded {
            server: info.server,
            share: info.share,
            port: info.port,
            display_name,
            username_hint: info.username,
            message: None,
        });
    };

    let result = try_smb_upgrade(
        &info.server,
        &info.share,
        &mount_path,
        Some(&creds.username),
        Some(&creds.password),
        info.port,
        &volume_id,
    )
    .await;

    match result {
        Ok(()) => {
            // Copy the borrowed password into Cmdr's own store so the next reconnect is
            // silent (no consent dialog). Keyed by hostname when known, else the server.
            let server_key = hostname.as_deref().unwrap_or(&info.server);
            if let Err(e) = keychain::save_credentials(server_key, Some(&info.share), &creds.username, &creds.password)
            {
                log::warn!("Couldn't copy borrowed credentials into Cmdr's store: {}", e);
            }
            Ok(UpgradeResult::Success)
        }
        Err(UpgradeError::Auth) => Ok(UpgradeResult::CredentialsNeeded {
            server: info.server,
            share: info.share,
            port: info.port,
            display_name,
            username_hint: Some(creds.username),
            message: Some("The saved password didn't work".to_string()),
        }),
        Err(UpgradeError::Network(msg)) => Ok(UpgradeResult::NetworkError { message: msg }),
    }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub async fn upgrade_to_smb_volume_using_saved_password(
    _volume_id: String,
    _app_handle: tauri::AppHandle,
) -> Result<UpgradeResult, String> {
    Err("Reading saved SMB passwords is only supported on macOS".to_string())
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
    use crate::commands::util::blocking_with_timeout;
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

// --- SMB direct-connection reconnect ---

/// Tries to rebuild the smb2 session for a Disconnected `SmbVolume` in place.
///
/// Called by the frontend reconnect manager on each backoff tick (and on
/// "Retry now" / lazy nav-time retry). Backend single-flights concurrent calls,
/// so the FE is free to fire on its own schedule. Returns `Ok(())` on success
/// (state is now `Direct`), or an `IpcError` describing why the rebuild failed.
///
/// Calling this on a non-SMB volume yields `IpcError` with `NotSupported`
/// (the trait default). The FE only ever invokes this for known SMB volumes.
#[tauri::command]
#[specta::specta]
pub async fn reconnect_smb_volume(volume_id: String) -> Result<(), crate::commands::util::IpcError> {
    use crate::commands::util::IpcError;
    use crate::file_system::get_volume_manager;

    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| IpcError::from_err(format!("Volume not found: {}", volume_id)))?;

    volume
        .attempt_reconnect()
        .await
        .map_err(|e| IpcError::from_err(e.to_string()))
}

/// Reconnects an SMB volume with freshly-entered credentials.
///
/// Invoked by the "Sign in" affordance shown when an in-place reconnect gave up on an
/// auth failure (a `needs_auth` `smb-connection-changed` event). The volume persists the
/// new password (so future reconnects are silent) and runs the standard reconnect; on
/// success the backend emits `smb-connection-changed { state: "direct" }`. On a non-SMB
/// volume this yields `NotSupported` (trait default); the FE only invokes it for SMB.
#[tauri::command]
#[specta::specta]
pub async fn reconnect_smb_volume_with_credentials(
    volume_id: String,
    username: String,
    password: String,
) -> Result<(), crate::commands::util::IpcError> {
    use crate::commands::util::IpcError;
    use crate::file_system::get_volume_manager;

    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| IpcError::from_err(format!("Volume not found: {}", volume_id)))?;

    volume
        .reconnect_with_credentials(username, password)
        .await
        .map_err(|e| IpcError::from_err(e.to_string()))
}

/// Disconnects a single SMB volume by tearing down its OS mount.
///
/// Thin delegate to [`crate::file_system::volume::eject::disconnect_smb`], mapping
/// the typed `EjectError` to the wire `IpcError` (preserving the timeout flag).
/// Called by the "Disconnect" button in `SmbReconnectingView` / the gave-up
/// `VolumeUnreachableBanner`.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_smb_volume(volume_id: String) -> Result<(), crate::commands::util::IpcError> {
    use crate::commands::util::IpcError;
    use crate::file_system::volume::eject::{self, EjectError};

    eject::disconnect_smb(&volume_id).await.map_err(|e| match e {
        EjectError::TimedOut => IpcError::timeout(),
        other => IpcError::from_err(other),
    })
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
