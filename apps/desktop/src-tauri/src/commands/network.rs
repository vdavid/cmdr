//! Tauri commands for network host discovery and SMB share listing.

use crate::network::{
    AuthMode, DiscoveryState, NetworkHost, ShareListError, ShareListResult, get_discovered_hosts,
    get_discovery_state_value, get_host_for_resolution, resolve_host_ip, service_name_to_hostname, smb_client,
    update_host_resolution,
};

use crate::network::smb_upgrade::{
    UpgradeError, UpgradeResult, connect_smb_volume_direct, friendly_server_name, get_keychain_password,
    register_smb_volume, resolve_ip_to_hostname_with_wait, try_smb_upgrade,
};

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

/// Mounts a network SMB share for use inside Cmdr.
///
/// When `network.directSmbConnection` is `true` (the default), opens a direct
/// smb2 session, registers it as an `SmbVolume` in the `VolumeManager`, and
/// returns a logical `MountResult` with `mount_path = /Volumes/<share>`. No OS
/// mount happens, so macOS never shows the kernel `smbfs` credentials dialog.
/// The synthesized path is purely a logical address: `SmbVolume::supports_local_fs_access()`
/// returns `false`, so nothing in Cmdr calls `std::fs::*` against it.
///
/// When the setting is `false`, falls back to the legacy behavior: OS mount via
/// `NetFSMountURLSync`/`gio mount`, then `register_smb_volume` to layer a direct
/// smb2 session on top. Use this opt-out only when an external app (Finder,
/// Terminal) genuinely needs the OS mount.
///
/// The watcher (`volumes::watcher::try_upgrade_smb_mount`) and the manual
/// `upgrade_to_smb_volume` command still handle OS-mounted shares from Finder
/// or `Cmd+K`, so this change doesn't break those paths.
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
/// * `Err(MountError)` - Mount failed with a typed error (e.g. `AuthFailed`,
///   `HostUnreachable`). The frontend re-prompts for credentials inline.
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

    if crate::file_system::is_direct_smb_enabled() {
        // Direct-smb2-only path. Skips the OS mount entirely; no macOS
        // kernel smbfs credentials dialog.
        connect_smb_volume_direct(
            &server,
            &share,
            username.as_deref(),
            password.as_deref(),
            actual_port,
            timeout_ms,
        )
        .await
    } else {
        // Legacy fallback for users who explicitly opt out of direct
        // connections. Goes through the OS mount, then layers smb2 on top.
        let result = mount::mount_share(
            server.clone(),
            share.clone(),
            username.clone(),
            password.clone(),
            actual_port,
            timeout_ms,
        )
        .await?;

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

    // Kick mDNS off so IP → hostname resolution has a shot before we hit the
    // Keychain. Idempotent; no-op if already running or `network.enabled` is off.
    crate::network::ensure_mdns_started(app_handle);

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

// --- Disconnect Command ---

/// Unmounts all SMB shares mounted from a given server.
/// Returns the list of mount paths that were unmounted.
/// Uses a 15s timeout because `statfs` on hung mounts can block indefinitely
/// and `diskutil unmount` may wait for the OS to release the mount.
#[tauri::command]
#[specta::specta]
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

/// Disconnects a single SMB volume by unmounting it at the OS level.
///
/// Called by the "Disconnect" button in `SmbReconnectingView` / the gave-up
/// `VolumeUnreachableBanner`. Looks up the volume by id, then runs `diskutil
/// unmount <mount_path>` (macOS). The OS unmount fires FSEvents, which triggers
/// the existing volume watcher → `Volume::on_unmount` pipeline:
///   - `SmbVolume::on_unmount` flips `unmounted=true`, stops the watcher task,
///     drops the smb2 session.
///   - The volume is removed from `VolumeManager`.
///   - A `volumes-changed` event flows to the frontend.
///
/// On Linux the OS-level unmount isn't wired up yet (mirrors
/// `unmount_smb_shares_from_host`), so we just drop the smb2 session via
/// `on_unmount` directly. The OS mount stays alive but the user can eject it
/// from the file manager.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_smb_volume(volume_id: String) -> Result<(), crate::commands::util::IpcError> {
    use crate::commands::util::IpcError;
    use crate::file_system::get_volume_manager;

    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| IpcError::from_err(format!("Volume not found: {}", volume_id)))?;

    if volume.smb_connection_state().is_none() {
        return Err(IpcError::from_err(format!(
            "Volume {} isn't an SMB volume; can't disconnect",
            volume_id
        )));
    }

    #[cfg(target_os = "macos")]
    let mount_path = volume.root().to_string_lossy().to_string();

    #[cfg(target_os = "macos")]
    {
        use crate::commands::util::blocking_with_timeout;
        use std::time::Duration;

        let path_for_cmd = mount_path.clone();
        let result = blocking_with_timeout(
            Duration::from_secs(15),
            Err("Unmount timed out".to_string()),
            move || {
                let output = std::process::Command::new("diskutil")
                    .args(["unmount", &path_for_cmd])
                    .output()
                    .map_err(|e| format!("Couldn't run diskutil: {}", e))?;
                if output.status.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "diskutil unmount failed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ))
                }
            },
        )
        .await;

        if let Err(e) = result {
            return Err(IpcError::from_err(e));
        }

        log::info!("Disconnected SMB volume {} (unmounted {})", volume_id, mount_path);
        // FSEvents will fire shortly and trigger on_unmount + volume-manager removal.
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Linux: OS-level unmount isn't wired up yet (matches `unmount_smb_shares_from_host`).
        // Drop the smb2 session directly so the volume stops trying to reach the server.
        volume.on_unmount();
        log::info!(
            "Dropped smb2 session for {} (OS unmount not yet implemented on this platform)",
            volume_id
        );
    }

    Ok(())
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
    crate::file_system::upgrade_existing_smb_mounts();

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
mod tests {
    //! Tests for `mount_network_share`.
    //!
    //! The regression guard here is that opening an SMB share from Cmdr does
    //! NOT trigger an OS-level `smbfs` mount (which on macOS pops the kernel
    //! credentials dialog). The Docker-backed integration tests assert that
    //! after the command succeeds, `/Volumes/<share>` is not an `smbfs` mount.
    //!
    //! The integration tests live behind `#[ignore]` so they only run when the
    //! `smb-consumer` Docker stack is up (see
    //! `apps/desktop/test/smb-servers/start.sh`).
    use super::*;
    use crate::file_system::{get_volume_manager, is_direct_smb_enabled, set_direct_smb_enabled, volume::path_to_id};

    /// Default guest-share port (matches `smb-consumer-guest`).
    fn guest_port() -> u16 {
        std::env::var("SMB_CONSUMER_GUEST_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10480)
    }

    /// Default auth-share port (matches `smb-consumer-auth`).
    fn auth_port() -> u16 {
        std::env::var("SMB_CONSUMER_AUTH_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10481)
    }

    /// True iff `mount_path` corresponds to a real `smbfs` (or `cifs`) OS mount.
    /// Lives behind `statfs`, so a non-existent path returns `false` cleanly.
    #[cfg(target_os = "macos")]
    fn is_os_smb_mount(mount_path: &str) -> bool {
        crate::volumes::get_smb_mount_info(mount_path).is_some()
    }

    #[cfg(target_os = "linux")]
    fn is_os_smb_mount(mount_path: &str) -> bool {
        crate::volumes_linux::get_smb_mount_info(mount_path).is_some()
    }

    /// Cleans up the registered direct SmbVolume after a test so later tests
    /// (and the watcher) don't see a stale registration. Idempotent.
    fn cleanup_registered(share: &str) {
        let mount_path = format!("/Volumes/{}", share);
        let volume_id = path_to_id(&mount_path);
        get_volume_manager().unregister(&volume_id);
    }

    /// Direct path against the guest container: connects, registers, and never
    /// creates an `smbfs` entry at `/Volumes/<share>`. This is the regression
    /// guard for the kernel credentials dialog.
    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_mount_network_share_skips_os_mount_guest() {
        let prev = is_direct_smb_enabled();
        set_direct_smb_enabled(true);

        let share = "public";
        cleanup_registered(share);

        let result = mount_network_share(
            "127.0.0.1".to_string(),
            share.to_string(),
            None,
            None,
            Some(guest_port()),
            Some(15_000),
        )
        .await
        .expect("direct smb2 mount should succeed against the guest container");

        // The synthesized path is `/Volumes/<share>` (a logical address).
        assert_eq!(result.mount_path, format!("/Volumes/{}", share));

        // Regression guard: no real OS-level smbfs mount was created.
        // The FE-initiated share-open must never produce a kernel mount.
        assert!(
            !is_os_smb_mount(&result.mount_path),
            "expected no smbfs entry at {}, but statfs reports one",
            result.mount_path
        );

        // SmbVolume is registered, ready for FE to navigate into.
        let volume_id = path_to_id(&result.mount_path);
        let volume = get_volume_manager()
            .get(&volume_id)
            .expect("SmbVolume should be registered after mount_network_share");
        assert!(volume.smb_connection_state().is_some());

        cleanup_registered(share);
        set_direct_smb_enabled(prev);
    }

    /// Same flow against the auth container with credentials.
    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_mount_network_share_skips_os_mount_auth() {
        let prev = is_direct_smb_enabled();
        set_direct_smb_enabled(true);

        let share = "private";
        cleanup_registered(share);

        let result = mount_network_share(
            "127.0.0.1".to_string(),
            share.to_string(),
            Some("testuser".to_string()),
            Some("testpass".to_string()),
            Some(auth_port()),
            Some(15_000),
        )
        .await
        .expect("direct smb2 mount should succeed against the auth container");

        assert_eq!(result.mount_path, format!("/Volumes/{}", share));
        assert!(
            !is_os_smb_mount(&result.mount_path),
            "expected no smbfs entry at {}, but statfs reports one",
            result.mount_path
        );

        cleanup_registered(share);
        set_direct_smb_enabled(prev);
    }

    /// Wrong password against the auth container surfaces a typed `AuthFailed`
    /// error from the smb2 path, NOT a silent fallback to OS mount.
    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_mount_network_share_bad_password_is_typed_auth_failure() {
        let prev = is_direct_smb_enabled();
        set_direct_smb_enabled(true);

        let share = "private";
        cleanup_registered(share);

        let result = mount_network_share(
            "127.0.0.1".to_string(),
            share.to_string(),
            Some("testuser".to_string()),
            Some("wrong-password".to_string()),
            Some(auth_port()),
            Some(15_000),
        )
        .await;

        match result {
            Err(MountError::AuthFailed { .. }) | Err(MountError::AuthRequired { .. }) => {}
            Err(other) => panic!("expected AuthFailed/AuthRequired, got {:?}", other),
            Ok(_) => panic!("expected an auth error with the wrong password"),
        }

        // No OS mount sneaks in on the error path either.
        assert!(
            !is_os_smb_mount(&format!("/Volumes/{}", share)),
            "expected no smbfs entry after a failed auth attempt"
        );

        cleanup_registered(share);
        set_direct_smb_enabled(prev);
    }

    /// Unreachable host: typed `HostUnreachable`/`Timeout`/`ProtocolError`,
    /// no OS mount attempted, and no panic. Uses an unrouted private IP so
    /// the connection fails fast on most networks.
    #[tokio::test]
    async fn mount_network_share_unreachable_host_returns_typed_error() {
        let prev = is_direct_smb_enabled();
        set_direct_smb_enabled(true);

        let share = "no-such-share";
        cleanup_registered(share);

        // Aggressive timeout to keep the test snappy; on a reachable network
        // the unrouted IP still triggers connection failure within seconds.
        let result = mount_network_share(
            "192.0.2.1".to_string(), // RFC 5737 TEST-NET-1
            share.to_string(),
            None,
            None,
            Some(445),
            Some(2_000),
        )
        .await;

        // We don't care which specific variant fires; the contract is
        // "typed MountError, not a panic, not a hang past our timeout".
        assert!(
            matches!(
                result,
                Err(MountError::HostUnreachable { .. })
                    | Err(MountError::Timeout { .. })
                    | Err(MountError::ProtocolError { .. })
            ),
            "expected a typed network error, got {:?}",
            result
        );

        // Never registered a volume on the failure path.
        let volume_id = path_to_id(&format!("/Volumes/{}", share));
        assert!(get_volume_manager().get(&volume_id).is_none());

        set_direct_smb_enabled(prev);
    }
}
