//! Tauri commands for network host discovery and SMB share listing.

use crate::network::{
    AuthMode, DiscoveryState, NetworkHost, ShareListError, ShareListResult, get_discovered_hosts,
    get_discovery_state_value, get_host_for_resolution, resolve_host_ip, service_name_to_hostname, smb_client,
    update_host_resolution,
};

use crate::network::smb_upgrade::{
    UpgradeError, UpgradeResult, friendly_server_name, get_keychain_password, register_smb_volume,
    resolve_ip_to_hostname, try_smb_upgrade,
};

/// Gets all currently discovered network hosts.
#[tauri::command]
pub fn list_network_hosts() -> Vec<NetworkHost> {
    get_discovered_hosts()
}

/// Gets the current discovery state.
#[tauri::command]
pub fn get_network_discovery_state() -> DiscoveryState {
    get_discovery_state_value()
}

/// Resolves a network host by ID, returning the host with hostname and IP address populated.
/// This is an async command that uses spawn_blocking for the DNS lookup to avoid blocking
/// the main thread pool. Multiple hosts can resolve in parallel.
#[tauri::command]
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
pub fn get_known_shares() -> Vec<KnownNetworkShare> {
    get_all_known_shares()
}

/// Gets a specific known share by server and share name.
#[tauri::command]
pub fn get_known_share_by_name(server_name: String, share_name: String) -> Option<KnownNetworkShare> {
    get_known_share_inner(&server_name, &share_name)
}

/// Updates or adds a known network share after successful connection.
#[tauri::command]
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
pub fn get_username_hints() -> std::collections::HashMap<String, String> {
    known_shares::get_username_hints()
}

// --- Keychain Commands ---

use crate::network::keychain::{self, KeychainError, SmbCredentials};

/// Saves SMB credentials to the Keychain.
/// Credentials are stored under "Cmdr" service name.
#[tauri::command]
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
pub fn get_smb_credentials(server: String, share: Option<String>) -> Result<SmbCredentials, KeychainError> {
    keychain::get_credentials(&server, share.as_deref())
}

/// Checks if credentials exist in the Keychain for a server/share.
#[tauri::command]
pub fn has_smb_credentials(server: String, share: Option<String>) -> bool {
    keychain::has_credentials(&server, share.as_deref())
}

/// Deletes SMB credentials from the Keychain.
#[tauri::command]
pub fn delete_smb_credentials(server: String, share: Option<String>) -> Result<(), KeychainError> {
    keychain::delete_credentials(&server, share.as_deref())
}

/// Returns whether credential storage is using an encrypted file fallback
/// instead of the system keyring. The frontend can use this to show a one-time
/// info toast when the user first saves credentials without a system keyring.
#[tauri::command]
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
pub async fn upgrade_to_smb_volume(volume_id: String) -> Result<UpgradeResult, String> {
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
    // stores Keychain credentials keyed by hostname (from mDNS). Try both.
    let hostname = resolve_ip_to_hostname(&info.server);
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
pub async fn upgrade_to_smb_volume_with_credentials(
    volume_id: String,
    username: Option<String>,
    password: Option<String>,
    remember_in_keychain: bool,
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

    let hostname = resolve_ip_to_hostname(&info.server);
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

// --- Disconnect Command ---

/// Unmounts all SMB shares mounted from a given server.
/// Returns the list of mount paths that were unmounted.
/// Uses a 15s timeout because `statfs` on hung mounts can block indefinitely
/// and `diskutil unmount` may wait for the OS to release the mount.
#[tauri::command]
pub async fn disconnect_network_host(
    _host_id: String,
    host_name: String,
    ip_address: Option<String>,
) -> Result<Vec<String>, String> {
    use crate::commands::util::blocking_with_timeout;
    use std::time::Duration;

    let result = blocking_with_timeout(Duration::from_secs(15), vec![], move || {
        mount::unmount_smb_shares_from_host(&host_name, ip_address.as_deref())
    })
    .await;

    Ok(result)
}

// --- Manual Server Commands ---

use crate::network::manual_servers::{self, ManualConnectResult};

/// Connects to a manually-specified server: parses, checks reachability, persists, and injects.
#[tauri::command]
pub async fn connect_to_server(address: String, app_handle: tauri::AppHandle) -> Result<ManualConnectResult, String> {
    manual_servers::add_manual_server(&address, &app_handle).await
}

/// Removes a manually-added server by ID.
#[tauri::command]
pub fn remove_manual_server(server_id: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    manual_servers::remove_manual_server(&server_id, &app_handle)
}
