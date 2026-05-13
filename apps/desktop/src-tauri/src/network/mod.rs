//! Network host discovery and SMB share listing.
//!
//! Discovers SMB-capable hosts on the local network using mDNS/DNS-SD
//! and enumerates shares using the smb-rs crate.
//!
//! Platform-specific modules:
//! - `keychain.rs` — credential storage (delegates to `crate::secrets` for platform-agnostic backend)
//! - `mount.rs` / `mount_linux.rs` — SMB mounting (macOS NetFS / Linux gio)

pub mod keychain;

pub mod known_shares;
pub mod manual_servers;
pub mod mdns_discovery;

#[cfg(target_os = "macos")]
#[path = "mount.rs"]
pub mod mount;

#[cfg(target_os = "linux")]
#[path = "mount_linux.rs"]
pub mod mount;

pub mod smb_client;

// SMB submodules - these are implementation details of smb_client
#[cfg(target_os = "linux")]
mod linux_distro;
mod smb_cache;
pub(crate) mod smb_connection;
#[cfg(target_os = "linux")]
mod smb_smbclient;
mod smb_smbutil;
mod smb_types;
pub(crate) mod smb_upgrade;
pub(crate) mod smb_util;

#[cfg(feature = "smb-e2e")]
pub mod virtual_smb_hosts;

use crate::ignore_poison::IgnorePoison;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

pub use mdns_discovery::start_discovery;
pub use smb_client::{AuthMode, ShareListError, ShareListResult};

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
/// Only serialized (Rust → frontend); no `Deserialize` needed (return type only).
/// Fields serialized as explicit `null` when absent so specta's `validate_exported_command`
/// accepts the type in Unified mode.
#[derive(Debug, Clone, Serialize, specta::Type)]
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

/// Current network discovery state, accessible globally.
struct NetworkDiscoveryState {
    hosts: HashMap<String, NetworkHost>,
    state: DiscoveryState,
}

impl Default for NetworkDiscoveryState {
    fn default() -> Self {
        Self {
            hosts: HashMap::new(),
            state: DiscoveryState::Idle,
        }
    }
}

/// Global discovery state, protected by a mutex.
static DISCOVERY_STATE: OnceLock<Mutex<NetworkDiscoveryState>> = OnceLock::new();

fn get_discovery_state() -> &'static Mutex<NetworkDiscoveryState> {
    DISCOVERY_STATE.get_or_init(|| Mutex::new(NetworkDiscoveryState::default()))
}

/// Gets all currently discovered network hosts.
pub fn get_discovered_hosts() -> Vec<NetworkHost> {
    let state = get_discovery_state().lock_ignore_poison();
    state.hosts.values().cloned().collect()
}

/// Gets the current discovery state.
pub fn get_discovery_state_value() -> DiscoveryState {
    let state = get_discovery_state().lock_ignore_poison();
    state.state
}

/// Drains the cached host map and resets discovery state to `Idle`. Pure
/// mutation — returns the IDs of hosts that were removed so the caller can
/// emit `network-host-lost` for each. Testable without a Tauri runtime.
pub(crate) fn drain_discovered_hosts() -> Vec<String> {
    let mut state = get_discovery_state().lock_ignore_poison();
    let ids: Vec<String> = state.hosts.keys().cloned().collect();
    state.hosts.clear();
    state.state = DiscoveryState::Idle;
    ids
}

/// Clears all discovered hosts and resets discovery state to `Idle`.
/// Called when networking is disabled via the user toggle so the frontend store empties
/// without waiting for `network-host-lost` events from a stopped daemon.
pub fn clear_discovered_hosts<R: tauri::Runtime>(app_handle: &impl Emitter<R>) {
    let removed_ids = drain_discovered_hosts();
    for id in removed_ids {
        let _ = app_handle.emit("network-host-lost", serde_json::json!({ "id": id }));
    }
    let _ = app_handle.emit(
        "network-discovery-state-changed",
        serde_json::json!({ "state": DiscoveryState::Idle }),
    );
}

/// Called by the mDNS discovery module when a host is discovered.
pub(crate) fn on_host_found<R: tauri::Runtime>(host: NetworkHost, app_handle: &impl Emitter<R>) {
    let mut state = get_discovery_state().lock_ignore_poison();

    let is_new = !state.hosts.contains_key(&host.id);
    debug!(
        "Host {}: id={}, name={}, ip={:?}, hostname={:?}",
        if is_new { "ADDED" } else { "UPDATED" },
        host.id,
        host.name,
        host.ip_address,
        host.hostname
    );

    // Insert or update the host
    state.hosts.insert(host.id.clone(), host.clone());

    // Emit event to frontend
    let _ = app_handle.emit("network-host-found", &host);
}

/// Called by the mDNS discovery module when a host disappears.
pub(crate) fn on_host_lost<R: tauri::Runtime>(host_id: &str, app_handle: &impl Emitter<R>) {
    let mut state = get_discovery_state().lock_ignore_poison();

    if let Some(removed) = state.hosts.remove(host_id) {
        debug!(
            "Host REMOVED: id={}, name={}, ip={:?}",
            removed.id, removed.name, removed.ip_address
        );
        // Emit event to frontend
        let _ = app_handle.emit("network-host-lost", serde_json::json!({ "id": host_id }));
    }
}

/// Updates the cached discovery state without emitting. Pure mutation —
/// testable in isolation from a Tauri runtime. The public
/// `on_discovery_state_changed` calls this and then emits the FE event.
pub(crate) fn set_discovery_state(new_state: DiscoveryState) {
    let mut state = get_discovery_state().lock_ignore_poison();
    state.state = new_state;
}

/// Called when discovery state changes.
pub(crate) fn on_discovery_state_changed(new_state: DiscoveryState, app_handle: &AppHandle) {
    set_discovery_state(new_state);

    // Emit event to frontend
    let _ = app_handle.emit(
        "network-discovery-state-changed",
        serde_json::json!({ "state": new_state }),
    );
}

/// Called by the mDNS discovery module when a host's address is resolved.
pub(crate) fn on_host_resolved(
    host_id: &str,
    name: &str,
    hostname: Option<String>,
    ip_address: Option<String>,
    port: u16,
    app_handle: &AppHandle,
) {
    let mut state = get_discovery_state().lock_ignore_poison();

    // If host wasn't seen via ServiceFound (race or library quirk), create it now
    if !state.hosts.contains_key(host_id) {
        debug!(
            "Host RESOLVED before FOUND, creating entry: id={}, name={}, hostname={:?}, ip={:?}",
            host_id, name, hostname, ip_address
        );
        let host = NetworkHost {
            id: host_id.to_string(),
            name: name.to_string(),
            hostname: None,
            ip_address: None,
            port,
            source: HostSource::Discovered,
        };
        state.hosts.insert(host_id.to_string(), host.clone());
        let _ = app_handle.emit("network-host-found", &host);
    }

    let host = state.hosts.get_mut(host_id).expect("just inserted or already present");
    host.hostname = hostname.clone().or(host.hostname.clone());
    host.ip_address = ip_address.clone().or(host.ip_address.clone());
    host.port = port;

    debug!(
        "Host RESOLVED: id={}, hostname={:?}, ip={:?}, port={}",
        host_id, host.hostname, host.ip_address, port
    );

    // Emit event to frontend with updated host info
    let _ = app_handle.emit("network-host-resolved", host.clone());
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

/// Information needed to resolve a host, extracted without holding mutex long.
pub struct HostResolutionInfo {
    pub id: String,
    pub name: String,
    pub hostname: Option<String>,
    pub ip_address: Option<String>,
    pub port: u16,
    pub source: HostSource,
}

/// Gets the information needed to resolve a host. Brief mutex hold.
pub fn get_host_for_resolution(host_id: &str) -> Option<HostResolutionInfo> {
    let state = get_discovery_state().lock_ignore_poison();
    state.hosts.get(host_id).map(|h| HostResolutionInfo {
        id: h.id.clone(),
        name: h.name.clone(),
        hostname: h.hostname.clone(),
        ip_address: h.ip_address.clone(),
        port: h.port,
        source: h.source,
    })
}

/// Updates a host with resolved hostname and IP. Brief mutex hold.
pub fn update_host_resolution(host_id: &str, hostname: String, ip_address: Option<String>) -> Option<NetworkHost> {
    let mut state = get_discovery_state().lock_ignore_poison();
    if let Some(host) = state.hosts.get_mut(host_id) {
        host.hostname = Some(hostname);
        host.ip_address = ip_address;
        Some(host.clone())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── DiscoveryState transitions ─────────────────────────────────────
    //
    // The global `DISCOVERY_STATE` cell is shared across tests and across
    // the rest of the process. To keep these tests deterministic when run
    // in parallel with each other, they share a serializing mutex so only
    // one DiscoveryState test runs at a time.

    static DISCOVERY_TEST_GUARD: Mutex<()> = Mutex::new(());

    #[test]
    fn discovery_state_idle_to_searching_to_active_to_idle_transitions() {
        // Drives the full mDNS-lifecycle path through the public setter:
        // - `Idle → Searching` (daemon start)
        // - `Searching → Active` (initial burst complete)
        // - `Active → Idle` (daemon stop)
        // The setter is the same one `on_discovery_state_changed` calls
        // before emitting the FE event.
        let _guard = DISCOVERY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());

        set_discovery_state(DiscoveryState::Idle);
        assert_eq!(get_discovery_state_value(), DiscoveryState::Idle);

        set_discovery_state(DiscoveryState::Searching);
        assert_eq!(get_discovery_state_value(), DiscoveryState::Searching);

        set_discovery_state(DiscoveryState::Active);
        assert_eq!(get_discovery_state_value(), DiscoveryState::Active);

        set_discovery_state(DiscoveryState::Idle);
        assert_eq!(get_discovery_state_value(), DiscoveryState::Idle);
    }

    #[test]
    fn discovery_state_searching_to_idle_via_drain() {
        // The "clear all discovered hosts" path that the user-facing
        // network-toggle invokes. Drain must reset state to Idle even
        // from Searching (mid-discovery toggle off), not only from Active.
        let _guard = DISCOVERY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());

        set_discovery_state(DiscoveryState::Searching);
        let _removed = drain_discovered_hosts();
        assert_eq!(get_discovery_state_value(), DiscoveryState::Idle);
    }

    #[test]
    fn drain_discovered_hosts_clears_state_and_returns_removed_ids() {
        // Pre-populate the global host map. We bypass `on_host_found` (which
        // takes an Emitter) and write the map directly — the point of this
        // test is the drain side effect on the cache, not the event emit.
        let _guard = DISCOVERY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());

        {
            let mut state = get_discovery_state().lock_ignore_poison();
            state.hosts.clear();
            state.state = DiscoveryState::Active;
            let host = NetworkHost {
                id: "host-drain-test-1".to_string(),
                name: "Drain test host".to_string(),
                hostname: None,
                ip_address: None,
                port: 445,
                source: HostSource::Discovered,
            };
            state.hosts.insert(host.id.clone(), host);
        }

        let removed = drain_discovered_hosts();

        assert!(
            removed.contains(&"host-drain-test-1".to_string()),
            "drain must return removed host IDs so the caller can emit per-host events; got {removed:?}"
        );
        assert_eq!(
            get_discovery_state_value(),
            DiscoveryState::Idle,
            "drain must reset state to Idle so the FE store can clear without daemon teardown events"
        );
        assert!(
            get_discovered_hosts().is_empty(),
            "drain must empty the host cache"
        );
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
