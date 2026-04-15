//! SMB upgrade helpers: establish direct smb2 connections for OS-mounted SMB volumes.
//!
//! Shared across three upgrade paths:
//! 1. **Startup** (`file_system::upgrade_existing_smb_mounts`) — scans existing mounts
//! 2. **Mount-time** (`volumes::watcher::try_upgrade_smb_mount`) — FSEvents detects new mount
//! 3. **Manual** (`commands::network::upgrade_to_smb_volume`) — user clicks "Connect directly"

use crate::network::get_discovered_hosts;

/// Result of an SMB volume upgrade attempt.
#[derive(serde::Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum UpgradeResult {
    /// Upgrade succeeded — volume now uses direct smb2.
    Success,
    /// Credentials needed — frontend should show login form.
    CredentialsNeeded {
        server: String,
        share: String,
        port: u16,
        /// Friendly display name for the server (mDNS hostname or IP).
        display_name: String,
        /// Username hint from stored credentials or the OS mount.
        username_hint: Option<String>,
        /// Optional message explaining why credentials are needed.
        message: Option<String>,
    },
    /// Non-auth error (DNS, network, unreachable).
    NetworkError { message: String },
}

/// Internal error type for upgrade attempts, distinguishing auth from network failures.
pub(crate) enum UpgradeError {
    Auth,
    Network(String),
}

/// Tries to establish a direct smb2 connection and register as `SmbVolume`.
///
/// Best-effort: logs a warning and returns quietly on failure. The FSEvents
/// watcher will register a `LocalPosixVolume` as fallback.
pub(crate) async fn register_smb_volume(
    server: &str,
    share: &str,
    mount_path: &str,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
) {
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::smb::connect_smb_volume;
    use std::sync::Arc;

    // Resolve mDNS service names (like "Naspolya._smb._tcp.local") to an IP
    let resolved_server = resolve_server_address(server);

    log::debug!(
        "Establishing smb2 connection for SmbVolume: {}:{}/{}",
        resolved_server,
        port,
        share
    );

    match connect_smb_volume(share, mount_path, &resolved_server, share, username, password, port).await {
        Ok(volume) => {
            let volume_id = crate::file_system::volume::path_to_id(mount_path);
            // Use register (overwrite) so SmbVolume always wins over any
            // LocalPosixVolume the watcher may have registered in the race window.
            get_volume_manager().register(&volume_id, Arc::new(volume));
            log::info!("Registered SmbVolume for {} (id={})", mount_path, volume_id);
        }
        Err(e) => {
            log::warn!(
                "Failed to establish smb2 connection for {}/{}: {}. \
                 Falling back to LocalPosixVolume via OS mount.",
                server,
                share,
                e
            );
        }
    }
}

/// Attempts the smb2 connection and registers the volume. Returns `Ok(())` on success.
pub(crate) async fn try_smb_upgrade(
    server: &str,
    share: &str,
    mount_path: &str,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
    volume_id: &str,
) -> Result<(), UpgradeError> {
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::smb::connect_smb_volume;
    use crate::network::smb_util::is_auth_error;
    use std::sync::Arc;

    // Resolve mDNS service names to connectable addresses
    let resolved_server = resolve_server_address(server);
    let display = friendly_server_name(server);

    match connect_smb_volume(share, mount_path, &resolved_server, share, username, password, port).await {
        Ok(volume) => {
            get_volume_manager().register(volume_id, Arc::new(volume));
            log::info!("Registered SmbVolume for {} (id={})", mount_path, volume_id);
            Ok(())
        }
        Err(e) => {
            if is_auth_error(&e) {
                Err(UpgradeError::Auth)
            } else {
                log::warn!(
                    "Failed to establish smb2 connection for {}/{}: {}",
                    resolved_server,
                    share,
                    e
                );
                Err(UpgradeError::Network(format!(
                    "Can't connect to {} — check that it's reachable on your network",
                    display
                )))
            }
        }
    }
}

/// Looks up the mDNS hostname for an IP address from discovered hosts.
///
/// Returns the hostname (like "naspolya") without `.local` suffix.
pub(crate) fn resolve_ip_to_hostname(ip: &str) -> Option<String> {
    let hosts = get_discovered_hosts();
    for host in &hosts {
        if host.ip_address.as_deref() == Some(ip) {
            // Return the service name (lowercased), which is what Keychain keys use
            return Some(host.name.to_lowercase());
        }
    }
    None
}

/// Resolves a server address from `statfs` to a connectable address.
///
/// `statfs` can return different formats depending on how the mount was created:
/// - An IP address like `192.168.1.111` — usable as-is
/// - A DNS hostname like `fileserver.corp.example.com` — usable as-is
/// - An mDNS service name like `Naspolya._smb._tcp.local` — NOT resolvable by DNS,
///   must be resolved to an IP via the mDNS discovery state
///
/// Returns the resolved IP if possible, otherwise the original string.
pub(crate) fn resolve_server_address(server: &str) -> String {
    // Detect mDNS service names (contain "._tcp" or "._udp")
    if !server.contains("._tcp") && !server.contains("._udp") {
        return server.to_string();
    }

    // Extract the service/display name (everything before the first "._")
    let service_name = server.split("._").next().unwrap_or(server);

    // Look up the discovered host by name (case-insensitive)
    let hosts = get_discovered_hosts();
    for host in &hosts {
        if host.name.eq_ignore_ascii_case(service_name) {
            if let Some(ref ip) = host.ip_address {
                log::debug!("Resolved mDNS service name {} to IP {}", server, ip);
                return ip.clone();
            }
            // Host found but no IP yet — try the hostname
            if let Some(ref hostname) = host.hostname {
                log::debug!("Resolved mDNS service name {} to hostname {}", server, hostname);
                return hostname.clone();
            }
        }
    }

    log::warn!(
        "Could not resolve mDNS service name {} — no matching discovered host",
        server
    );
    server.to_string()
}

/// Extracts the friendly display name from a server address.
///
/// For mDNS service names like `Naspolya._smb._tcp.local`, returns `Naspolya`.
/// For IPs or hostnames, tries `resolve_ip_to_hostname`, falls back to the raw string.
pub(crate) fn friendly_server_name(server: &str) -> String {
    // mDNS service name: extract the part before "._"
    if server.contains("._tcp") || server.contains("._udp") {
        return server.split("._").next().unwrap_or(server).to_string();
    }
    // IP address: try to resolve to mDNS hostname
    resolve_ip_to_hostname(server).unwrap_or_else(|| server.to_string())
}

/// Tries to retrieve SMB credentials from the Keychain.
///
/// Tries multiple keys: by IP (from statfs), by hostname (from mDNS discovery),
/// at both share-level and server-level.
pub(crate) async fn get_keychain_password(
    server_ip: &str,
    hostname: Option<&str>,
    share: &str,
) -> Option<(String, String)> {
    let server_ip = server_ip.to_string();
    let hostname = hostname.map(|s| s.to_string());
    let share = share.to_string();

    tokio::task::spawn_blocking(move || {
        use crate::network::keychain;

        // Build a list of server names to try (hostname first, then IP)
        let mut servers_to_try: Vec<&str> = Vec::new();
        if let Some(ref h) = hostname {
            servers_to_try.push(h);
        }
        servers_to_try.push(&server_ip);

        for server in &servers_to_try {
            // Try share-level credentials first (more specific)
            if let Ok(creds) = keychain::get_credentials(server, Some(&share)) {
                log::debug!("Found Keychain credentials via {}/{}", server, share);
                return Some((creds.username, creds.password));
            }
            // Try server-level credentials
            if let Ok(creds) = keychain::get_credentials(server, None) {
                log::debug!("Found Keychain credentials via {} (server-level)", server);
                return Some((creds.username, creds.password));
            }
        }

        log::debug!(
            "No Keychain credentials for {:?} / {} / {}",
            hostname,
            server_ip,
            share
        );
        None
    })
    .await
    .ok()
    .flatten()
}
