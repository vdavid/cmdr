//! SMB upgrade helpers: establish direct smb2 connections for OS-mounted SMB volumes.
//!
//! Shared across three upgrade paths:
//! 1. **Startup** (`file_system::upgrade_existing_smb_mounts`): scans existing mounts
//! 2. **Mount-time** (`volumes::watcher::try_upgrade_smb_mount`): FSEvents detects new mount
//! 3. **Manual** (`commands::network::upgrade_to_smb_volume`): user clicks "Connect directly"
//!
//! Plus the FE-initiated share-open path (`commands::network::mount_network_share`)
//! uses `connect_smb_volume_direct` to attach an `SmbVolume` without ever touching
//! the OS mount layer, so macOS doesn't pop a kernel `smbfs` credentials dialog.

use crate::network::get_discovered_hosts;
use crate::network::mount::{MountError, MountResult};

/// Result of an SMB volume upgrade attempt.
#[derive(serde::Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UpgradeResult {
    /// Upgrade succeeded: volume now uses direct smb2.
    Success,
    /// Credentials needed: frontend should show login form.
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

/// Default timeout for the FE-initiated direct-smb2 share-open flow. Mirrors
/// `network::mount::DEFAULT_MOUNT_TIMEOUT_MS` so the user sees the same wait
/// budget whether direct-smb2 is enabled or the legacy OS-mount path runs.
const DEFAULT_DIRECT_MOUNT_TIMEOUT_MS: u64 = 20_000;

/// Establishes a direct smb2 session and registers it as an `SmbVolume` without
/// ever touching the OS mount layer. Used by the FE-initiated share-open flow
/// (`commands::network::mount_network_share`).
///
/// The returned `MountResult.mount_path` is the logical address
/// `/Volumes/<share>` — no real OS mount lives there. `SmbVolume` reports
/// `supports_local_fs_access() = false`, so no code path inside Cmdr ever
/// calls `std::fs::*` against it.
///
/// Errors map onto `MountError` variants so the FE can re-prompt for
/// credentials inline (`AuthFailed`) or surface a reachability problem
/// (`HostUnreachable`) without seeing the old "bug detected" path.
pub(crate) async fn connect_smb_volume_direct(
    server: &str,
    share: &str,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
    timeout_ms: Option<u64>,
) -> Result<MountResult, MountError> {
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::path_to_id;
    use crate::file_system::volume::smb::connect_smb_volume;
    use std::sync::Arc;

    let timeout = std::time::Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_DIRECT_MOUNT_TIMEOUT_MS));
    let resolved_server = resolve_server_address(server);
    let display = friendly_server_name(server);

    // Synthesize the logical mount path. `SmbVolume::supports_local_fs_access()`
    // returns `false`, so nothing in Cmdr ever stats this path; it's an address
    // only. Using `/Volumes/<share>` matches the legacy NetFS naming and keeps
    // the FE breadcrumb / volume-picker copy unchanged.
    let mount_path = format!("/Volumes/{}", share);

    let connect_fut = connect_smb_volume(share, &mount_path, &resolved_server, share, username, password, port);

    let connect_result = match tokio::time::timeout(timeout, connect_fut).await {
        Ok(r) => r,
        Err(_) => {
            return Err(MountError::Timeout {
                message: format!(
                    "Connection to \"{}\" timed out after {} seconds",
                    display,
                    timeout.as_secs()
                ),
            });
        }
    };

    match connect_result {
        Ok(volume) => {
            let volume_id = path_to_id(&mount_path);
            get_volume_manager().register(&volume_id, Arc::new(volume));
            log::info!(
                "Registered direct SmbVolume for {}/{} (id={})",
                resolved_server,
                share,
                volume_id
            );
            Ok(MountResult {
                mount_path,
                already_mounted: false,
            })
        }
        Err(e) => Err(mount_error_from_smb_error(&e, &display, share)),
    }
}

/// Maps an `smb2::Error` to a `MountError` variant by kind, never by message
/// substring. Mirrors `smb_util::classify_error` but lands on `MountError`
/// (the FE contract for this command) instead of `ShareListError`.
fn mount_error_from_smb_error(err: &smb2::Error, server_display: &str, share: &str) -> MountError {
    use smb2::ErrorKind;

    let raw = err.to_string();
    match err.kind() {
        ErrorKind::AuthRequired | ErrorKind::SigningRequired => MountError::AuthRequired {
            message: format!("\"{}\" needs a username and password", server_display),
        },
        ErrorKind::AccessDenied => MountError::AuthFailed {
            message: "Invalid username or password".to_string(),
        },
        ErrorKind::NotFound => MountError::ShareNotFound {
            message: format!("Share \"{}\" not found on \"{}\"", share, server_display),
        },
        ErrorKind::TimedOut => MountError::Timeout {
            message: format!("Connection to \"{}\" timed out", server_display),
        },
        ErrorKind::ConnectionLost => MountError::HostUnreachable {
            message: format!("Can't connect to \"{}\"", server_display),
        },
        _ => MountError::ProtocolError {
            message: format!("Couldn't connect to \"{}\": {}", server_display, raw),
        },
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
                    "Can't connect to {}. Check that it's reachable on your network.",
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

/// Returns true if `ip` is a literal IPv4 address in a private range (RFC 1918 or
/// link-local 169.254/16). mDNS can only help for those: public/VPN/Tailscale IPs
/// won't show up in the local mDNS cache, so there's no point waiting on them.
///
/// Returns `false` for non-IP strings (hostnames), since `resolve_ip_to_hostname`
/// only matches discovered hosts by exact IP.
pub(crate) fn is_private_ipv4(ip: &str) -> bool {
    use std::net::Ipv4Addr;
    let Ok(addr) = ip.parse::<Ipv4Addr>() else {
        return false;
    };
    addr.is_private() || addr.is_link_local()
}

/// Like `resolve_ip_to_hostname`, but waits briefly for mDNS to populate the
/// discovered-host cache when the lookup misses on the first try. Solves the
/// startup race where macOS auto-remounts an SMB share, FSEvents fires before
/// mDNS has had time to find the host, and `statfs`-derived IP-only Keychain
/// lookups miss the credentials we have keyed by hostname.
///
/// Only waits for private-range IPv4 addresses (where mDNS is plausible) and only
/// if `is_network_enabled()`. Otherwise returns whatever the immediate sync
/// lookup gave us. Polls every 100ms up to `timeout`. The caller is responsible
/// for kicking off discovery via `network::ensure_mdns_started` before calling
/// this; the wait alone won't start the daemon.
pub(crate) async fn resolve_ip_to_hostname_with_wait(ip: &str, timeout: std::time::Duration) -> Option<String> {
    // Fast path: already in the cache.
    if let Some(hostname) = resolve_ip_to_hostname(ip) {
        return Some(hostname);
    }
    // No point waiting for non-private IPs (Tailscale, public DNS, etc.) or when
    // networking is disabled by the user.
    if !is_private_ipv4(ip) || !crate::network::is_network_enabled() {
        return None;
    }

    let poll_interval = std::time::Duration::from_millis(100);
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        tokio::time::sleep(poll_interval).await;
        if let Some(hostname) = resolve_ip_to_hostname(ip) {
            log::debug!(
                "Resolved IP {} to hostname {} after waiting {:?}",
                ip,
                hostname,
                start.elapsed()
            );
            return Some(hostname);
        }
    }
    log::debug!(
        "Couldn't resolve IP {} to a hostname via mDNS within {:?}; proceeding without",
        ip,
        timeout
    );
    None
}

/// Resolves a server address from `statfs` to a connectable address.
///
/// `statfs` can return different formats depending on how the mount was created:
/// - An IP address like `192.168.1.111`: usable as-is
/// - A DNS hostname like `fileserver.corp.example.com`: usable as-is
/// - An mDNS service name like `Naspolya._smb._tcp.local`: NOT resolvable by DNS,
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
            // Host found but no IP yet; try the hostname
            if let Some(ref hostname) = host.hostname {
                log::debug!("Resolved mDNS service name {} to hostname {}", server, hostname);
                return hostname.clone();
            }
        }
    }

    log::warn!(
        "Could not resolve mDNS service name {} (no matching discovered host)",
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

        log::debug!("No Keychain credentials for {:?} / {} / {}", hostname, server_ip, share);
        None
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn is_private_ipv4_recognizes_rfc1918_and_link_local() {
        assert!(is_private_ipv4("10.0.0.1"));
        assert!(is_private_ipv4("192.168.1.111"));
        assert!(is_private_ipv4("172.16.5.7"));
        assert!(is_private_ipv4("169.254.1.2"), "link-local should count");
    }

    #[test]
    fn is_private_ipv4_rejects_public_and_special() {
        assert!(!is_private_ipv4("8.8.8.8"));
        assert!(!is_private_ipv4("100.64.0.1"), "Tailscale/CGNAT not private");
        assert!(!is_private_ipv4("127.0.0.1"), "loopback not private");
        assert!(!is_private_ipv4("naspolya"), "hostnames return false");
        assert!(!is_private_ipv4(""));
        assert!(!is_private_ipv4("::1"), "IPv6 currently returns false");
    }

    /// `resolve_ip_to_hostname_with_wait` must return immediately (no polling)
    /// when the IP isn't a private-range IPv4 — Tailscale/public DNS won't show
    /// up in mDNS so there's nothing to wait for.
    #[tokio::test]
    async fn wait_helper_returns_immediately_for_non_private_ip() {
        let start = std::time::Instant::now();
        let result = resolve_ip_to_hostname_with_wait("8.8.8.8", Duration::from_millis(500)).await;
        let elapsed = start.elapsed();
        assert_eq!(result, None);
        assert!(
            elapsed < Duration::from_millis(50),
            "expected fast path (< 50ms), took {:?}",
            elapsed
        );
    }

    /// `resolve_ip_to_hostname_with_wait` must short-circuit when the runtime
    /// `network.enabled` flag is off, even for a private IP — mDNS isn't running
    /// so polling would just burn the timeout.
    #[tokio::test]
    async fn wait_helper_short_circuits_when_network_disabled() {
        let prev = crate::network::is_network_enabled();
        crate::network::set_network_enabled_flag(false);

        let start = std::time::Instant::now();
        let result = resolve_ip_to_hostname_with_wait("192.168.1.111", Duration::from_millis(500)).await;
        let elapsed = start.elapsed();

        // Restore before assertions so other tests aren't poisoned by panics.
        crate::network::set_network_enabled_flag(prev);

        assert_eq!(result, None);
        assert!(
            elapsed < Duration::from_millis(50),
            "expected fast path (< 50ms), took {:?}",
            elapsed
        );
    }

    /// Times out gracefully when no host ever shows up in the cache (and falls
    /// back to `None` so the caller can use IP-only Keychain lookup).
    #[tokio::test]
    async fn wait_helper_times_out_gracefully() {
        // Ensure network is "enabled" so we exercise the polling path.
        let prev = crate::network::is_network_enabled();
        crate::network::set_network_enabled_flag(true);

        // Use a unique private IP that no test has ever populated, so the cache
        // miss is deterministic.
        let timeout = Duration::from_millis(300);
        let start = std::time::Instant::now();
        let result = resolve_ip_to_hostname_with_wait("10.255.255.254", timeout).await;
        let elapsed = start.elapsed();

        crate::network::set_network_enabled_flag(prev);

        assert_eq!(result, None);
        assert!(
            elapsed >= timeout,
            "should have polled until timeout; elapsed {:?}",
            elapsed
        );
        // Generous upper bound — single poll interval slack.
        assert!(
            elapsed < timeout + Duration::from_millis(250),
            "shouldn't blow past timeout by much; elapsed {:?}",
            elapsed
        );
    }
}
