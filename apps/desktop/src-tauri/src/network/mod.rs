//! Network host discovery and SMB share listing.
//!
//! Discovers SMB-capable hosts on the local network using mDNS/DNS-SD
//! and enumerates shares using the smb-rs crate.
//!
//! Platform-specific modules:
//! - `keychain.rs`: credential storage (delegates to `crate::secrets` for platform-agnostic
//!   backend)
//! - `mount.rs` / `mount_linux.rs`: SMB mounting (macOS NetFS / Linux gio)

pub mod credential_store;
pub mod keychain;

pub mod connect_wiring;
pub mod discovery_cache;
pub mod discovery_gate;
// Every typed event the module emits. Always compiled: `collect_events!` in
// `ipc.rs` can't cfg-gate inline.
pub mod events;
pub mod known_shares;
pub mod live_server_edit;
pub mod manual_servers;
pub mod mdns_discovery;
pub mod one_shot_credentials;
pub mod s3_known_places;
pub mod s3_volume_wiring;
pub mod saved_server_fields;

// The durable trusted-SSH-host-key store, which answers the `HostKeys` seam.
// Not SMB's business, but it lives here for the same reason `credential_store`
// does: this module is where the app keeps what it knows about servers.
pub mod server_list_file;
pub mod sftp_host_keys;
pub mod sftp_known_servers;
pub mod sftp_volume_wiring;
pub mod webdav_known_servers;
pub mod webdav_volume_wiring;

/// Reads both SFTP stores off disk: the host keys the user has approved and the
/// servers they've connected to.
///
/// One call at startup, ❗ before any volume is built: a dial that runs before
/// the trust store is loaded reads every server as first contact and asks about
/// a key the user already approved.
pub fn load_sftp_stores<R: tauri::Runtime>(app: &AppHandle<R>) {
    sftp_host_keys::load_trusted_host_keys(app);
    sftp_known_servers::load_known_sftp_servers(app);
}

/// Reads the WebDAV server list off disk. One call at startup, beside
/// [`load_sftp_stores`].
pub fn load_webdav_stores<R: tauri::Runtime>(app: &AppHandle<R>) {
    webdav_known_servers::load_known_webdav_servers(app);
}

/// Reads the S3 place list off disk. One call at startup, beside
/// [`load_webdav_stores`].
pub fn load_s3_stores<R: tauri::Runtime>(app: &AppHandle<R>) {
    s3_known_places::load_known_s3_places(app);
}

#[cfg(target_os = "macos")]
#[path = "mount.rs"]
pub mod mount;

#[cfg(target_os = "linux")]
#[path = "mount_linux.rs"]
pub mod mount;

// Cross-platform: `same_server` and `credential_key` are used on both macOS and Linux
// (mount dedup, `smb_upgrade`, keychain keying).
pub mod server_identity;
// Asks the server itself why a mount found no share. Both platforms' mounts reach
// it through `mount_share` below.
pub(crate) mod share_access;
pub mod smb_client;
pub mod smb_sign_in_diagnostics;
// Why an smb2 connect didn't get in: who was refused, and at which step. Shared by
// `share_access` and both upgrade paths, so they read one answer one way.
pub(crate) mod smb_connect_failure;

// SMB submodules - these are implementation details of smb_client
#[cfg(target_os = "linux")]
mod linux_distro;
mod smb_cache;
#[cfg(target_os = "linux")]
mod smb_smbclient;
mod smb_smbutil;
// Who a `statfs` server string is: what to dial, what to call it, its saved credentials.
pub(crate) mod smb_server_address;
pub(crate) mod smb_upgrade;
// "Connect directly": the upgrade someone asked for, answered with where it left
// the volume. The auto-upgrade paths stay in `smb_upgrade`.
pub(crate) mod smb_connect_directly;
// The per-share "Use Cmdr's fast direct connection" switch, and the pane-open upgrade.
pub(crate) mod smb_direct_switch;
pub(crate) mod smb_pane_upgrade;
// Saved SMB share places: recorded when Cmdr mounts a share, brought back by `connect_saved_place`.
pub(crate) mod smb_saved_shares;

// The "we're stuck on the kernel mount" notice's once-per-server ledger. Lives
// beside `smb_upgrade` (its only caller) rather than inside it, so the ledger is
// testable without a live volume manager.
pub(crate) mod os_mount_notice;

#[cfg(feature = "smb-e2e")]
pub mod virtual_smb_hosts;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

pub use discovery_cache::{
    cached_discovered_hosts, clear_discovered_hosts, fresh_discovered_hosts, get_discovery_state_value,
    get_host_for_resolution, update_host_resolution,
};
pub(crate) use discovery_cache::{on_host_found, on_host_lost};
pub use events::{
    NetworkDiscoveryStateChanged, NetworkHostContextAction, NetworkHostContextActionKind, NetworkHostFound,
    NetworkHostLost, NetworkHostResolved, SmbFellBackToOsMount, SmbOsMountNoticeWithdrawn, VolumeConnection,
    VolumeConnectionChanged,
};
pub use smb_client::{ShareListError, ShareListResult};

/// Runtime mirror of the `network.enabled` setting. Default `true` matches the
/// settings default. `lib.rs::setup` updates this from the persisted settings;
/// `commands::network::set_network_enabled` keeps it in sync with the live toggle.
static NETWORK_ENABLED: AtomicBool = AtomicBool::new(true);

/// Updates the runtime `network.enabled` flag. Call from app setup (after loading
/// settings) and from the live-toggle command.
pub fn set_network_enabled_flag(enabled: bool) {
    NETWORK_ENABLED.store(enabled, Ordering::Relaxed);
    discovery_gate::set_enabled(enabled);
}

/// Returns whether networking is enabled. Used by BE-side upgrade paths to decide
/// whether they're allowed to kick off mDNS or wait for hostname resolution.
pub fn is_network_enabled() -> bool {
    NETWORK_ENABLED.load(Ordering::Relaxed)
}

/// How long a mount attempt may run before we give up on it, in milliseconds.
/// Shared by both platform backends, so a share on a slow server gets the same
/// patience on macOS and on Linux.
pub(crate) const DEFAULT_MOUNT_TIMEOUT_MS: u64 = 20_000;

/// Mounts an SMB share through the platform backend's blocking
/// `mount::mount_share_sync`, on the blocking pool under the mount timeout.
///
/// A panicked task and an expired budget come back as the typed `MountError` the
/// frontend words, and a mount that could only say "not found" spends what's left
/// of the budget asking the server which it was
/// (`share_access::clarify_share_not_found`). A guest's `AuthFailed` reads as the
/// sign-in it is (`share_access::refusal_for_identity`). One function for both
/// platforms, so the timeout and these readings can't drift apart between macOS
/// and Linux.
pub async fn mount_share(
    server: String,
    share: String,
    username: Option<String>,
    password: Option<String>,
    port: u16,
    timeout_ms: Option<u64>,
) -> Result<mount::MountResult, mount::MountError> {
    let attempt = share_access::ShareAttempt::new(&server, &share, port, username.as_deref(), password.as_deref());
    let started = std::time::Instant::now();
    let timeout_duration = std::time::Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_MOUNT_TIMEOUT_MS));
    let mount_future = tokio::task::spawn_blocking(move || {
        mount::mount_share_sync(&server, &share, username.as_deref(), password.as_deref(), port)
    });

    let result = match tokio::time::timeout(timeout_duration, mount_future).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_error)) => Err(mount::MountError::Unexpected {
            server: attempt.server().to_string(),
            share: attempt.share().to_string(),
            detail: format!("the mount task didn't finish: {join_error}"),
        }),
        Err(_timeout) => {
            log::warn!(
                "Mounting share={:?} on server={:?} didn't finish within {} s",
                attempt.share(),
                attempt.server(),
                timeout_duration.as_secs()
            );
            Err(mount::MountError::Timeout {
                server: attempt.server().to_string(),
            })
        }
    };

    match result {
        Err(not_found @ mount::MountError::ShareNotFound { .. }) => {
            let budget_left = timeout_duration.saturating_sub(started.elapsed());
            Err(share_access::clarify_share_not_found(not_found, &attempt, budget_left).await)
        }
        Err(refusal) => Err(share_access::refusal_for_identity(refusal, &attempt)),
        Ok(mounted) => Ok(mounted),
    }
}

/// Whether a host was discovered via mDNS or added manually by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum HostSource {
    #[default]
    Discovered,
    Manual,
}

/// A discovered network host advertising SMB services.
///
/// `Deserialize` is needed because this type is the flattened payload of the
/// `network-host-found` / `network-host-resolved` typed events (`tauri_specta::Event`
/// derives require `Deserialize` for the FE-side listener).
/// Fields serialized as explicit `null` when absent so specta's `validate_exported_command`
/// accepts the type in Unified mode.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NetworkHost {
    /// Derived from service name.
    pub id: String,
    /// The advertised service name.
    pub name: String,
    /// For example, "macbook.local". None if not yet resolved.
    pub hostname: Option<String>,
    /// None if not yet resolved.
    pub ip_address: Option<String>,
    /// Usually 445.
    pub port: u16,
    /// How this host was added to the list.
    #[serde(default)]
    pub source: HostSource,
}

/// State of network discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryState {
    Idle,
    Searching,
    /// Initial burst is complete, still listening.
    Active,
}

/// Generates a stable ID from a service name.
pub(crate) fn service_name_to_id(name: &str) -> String {
    // Create a URL-safe ID from the service name
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>()
        .to_lowercase()
}

/// Converts a service name to a hostname that can be resolved.
/// Service names like "David's MacBook" become "davids-macbook.local".
pub fn service_name_to_hostname(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c == ' ' || c == '\'' || c == '-' {
                '-'
            } else {
                // Skip other special characters
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect();

    // Remove consecutive dashes and trim dashes from ends
    let mut result = String::new();
    let mut last_was_dash = true; // Start true to trim leading dashes
    for c in cleaned.chars() {
        if c == '-' {
            if !last_was_dash {
                result.push(c);
                last_was_dash = true;
            }
        } else {
            result.push(c);
            last_was_dash = false;
        }
    }

    // Trim trailing dash
    if result.ends_with('-') {
        result.pop();
    }

    format!("{}.local", result)
}

/// Resolves a host by hostname, returning the first IPv4 address found.
pub fn resolve_host_ip(hostname: &str) -> Option<String> {
    use std::net::ToSocketAddrs;

    // Try to resolve the hostname
    let addr_string = format!("{}:445", hostname);
    match addr_string.to_socket_addrs() {
        Ok(addrs) => {
            // Prefer IPv4 addresses
            for addr in addrs {
                if addr.is_ipv4() {
                    return Some(addr.ip().to_string());
                }
            }
            None
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_constant() {
        // Verify default timeout is reasonable (10-60 seconds)
        const { assert!(DEFAULT_MOUNT_TIMEOUT_MS >= 10_000) };
        const { assert!(DEFAULT_MOUNT_TIMEOUT_MS <= 60_000) };
    }

    #[test]
    fn test_service_name_to_id() {
        assert_eq!(service_name_to_id("David's MacBook"), "davidsmacbook");
        assert_eq!(service_name_to_id("NAS-Server"), "nas-server");
        assert_eq!(service_name_to_id("my_server_1"), "my_server_1");
    }

    #[test]
    fn test_network_host_serialization() {
        let host = NetworkHost {
            id: "test-host".to_string(),
            name: "Test Host".to_string(),
            hostname: Some("test.local".to_string()),
            ip_address: Some("192.168.1.100".to_string()),
            port: 445,
            source: HostSource::default(),
        };

        let json = serde_json::to_string(&host).unwrap();
        assert!(json.contains("\"id\":\"test-host\""));
        assert!(json.contains("\"name\":\"Test Host\""));
        assert!(json.contains("\"hostname\":\"test.local\""));
    }

    #[test]
    fn test_host_without_resolution() {
        let host = NetworkHost {
            id: "unresolved".to_string(),
            name: "Unresolved Host".to_string(),
            hostname: None,
            ip_address: None,
            port: 445,
            source: HostSource::default(),
        };

        let json = serde_json::to_string(&host).unwrap();
        // hostname and ip_address serialize as explicit null (no longer omitted)
        assert!(json.contains("\"hostname\":null"));
        assert!(json.contains("\"ipAddress\":null"));
    }

    #[test]
    fn test_service_name_to_hostname() {
        // Basic conversion
        assert_eq!(service_name_to_hostname("MacBook"), "macbook.local");

        // With spaces and apostrophe
        assert_eq!(service_name_to_hostname("David's MacBook"), "david-s-macbook.local");

        // Already hyphenated
        assert_eq!(service_name_to_hostname("NAS-Server"), "nas-server.local");

        // With numbers
        assert_eq!(service_name_to_hostname("My Server 123"), "my-server-123.local");

        // Edge case: consecutive spaces
        assert_eq!(service_name_to_hostname("Server  Name  Here"), "server-name-here.local");

        // Edge case: leading/trailing spaces
        assert_eq!(service_name_to_hostname(" MacBook "), "macbook.local");
    }
}
