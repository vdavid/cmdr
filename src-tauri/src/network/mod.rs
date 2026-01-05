//! Network host discovery for macOS using Bonjour (mDNS/DNS-SD).
//!
//! Discovers SMB-capable hosts on the local network by listening for `_smb._tcp.local` service
//! advertisements. Uses Apple's `NSNetServiceBrowser` via objc2 bindings.

mod bonjour;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

pub use bonjour::start_discovery;

/// A discovered network host advertising SMB services.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkHost {
    /// Unique identifier for the host (derived from service name).
    pub id: String,
    /// Display name (the advertised service name).
    pub name: String,
    /// Resolved hostname (e.g., "macbook.local"), or None if not yet resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Resolved IP address, or None if not yet resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    /// SMB port (usually 445).
    pub port: u16,
}

/// State of network discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryState {
    /// Discovery has not started.
    Idle,
    /// Actively searching for hosts.
    Searching,
    /// Discovery is running but initial burst is complete.
    Active,
}

/// Current network discovery state, accessible globally.
struct NetworkDiscoveryState {
    /// Map of host ID to NetworkHost.
    hosts: HashMap<String, NetworkHost>,
    /// Current discovery state.
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
    let state = get_discovery_state().lock().unwrap();
    state.hosts.values().cloned().collect()
}

/// Gets the current discovery state.
pub fn get_discovery_state_value() -> DiscoveryState {
    let state = get_discovery_state().lock().unwrap();
    state.state
}

/// Called by the Bonjour module when a host is discovered.
pub(crate) fn on_host_found(host: NetworkHost, app_handle: &AppHandle) {
    let mut state = get_discovery_state().lock().unwrap();

    // Insert or update the host
    state.hosts.insert(host.id.clone(), host.clone());

    // Emit event to frontend
    let _ = app_handle.emit("network-host-found", &host);
}

/// Called by the Bonjour module when a host disappears.
pub(crate) fn on_host_lost(host_id: &str, app_handle: &AppHandle) {
    let mut state = get_discovery_state().lock().unwrap();

    if state.hosts.remove(host_id).is_some() {
        // Emit event to frontend
        let _ = app_handle.emit("network-host-lost", serde_json::json!({ "id": host_id }));
    }
}

/// Called when discovery state changes.
pub(crate) fn on_discovery_state_changed(new_state: DiscoveryState, app_handle: &AppHandle) {
    let mut state = get_discovery_state().lock().unwrap();
    state.state = new_state;

    // Emit event to frontend
    let _ = app_handle.emit(
        "network-discovery-state-changed",
        serde_json::json!({ "state": new_state }),
    );
}

/// Generates a stable ID from a service name.
pub(crate) fn service_name_to_id(name: &str) -> String {
    // Create a URL-safe ID from the service name
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>()
        .to_lowercase()
}

/// Converts a Bonjour service name to a hostname that can be resolved.
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
}

/// Gets the information needed to resolve a host. Brief mutex hold.
pub fn get_host_for_resolution(host_id: &str) -> Option<HostResolutionInfo> {
    let state = get_discovery_state().lock().unwrap();
    state.hosts.get(host_id).map(|h| HostResolutionInfo {
        id: h.id.clone(),
        name: h.name.clone(),
        hostname: h.hostname.clone(),
        ip_address: h.ip_address.clone(),
        port: h.port,
    })
}

/// Updates a host with resolved hostname and IP. Brief mutex hold.
pub fn update_host_resolution(host_id: &str, hostname: String, ip_address: Option<String>) -> Option<NetworkHost> {
    let mut state = get_discovery_state().lock().unwrap();
    if let Some(host) = state.hosts.get_mut(host_id) {
        host.hostname = Some(hostname);
        host.ip_address = ip_address;
        Some(host.clone())
    } else {
        None
    }
}

/// Resolves a network host by its ID (synchronous version for testing).
/// For async resolution, use the async command in commands/network.rs.
#[allow(dead_code)]
pub fn resolve_network_host_sync(host_id: &str) -> Option<NetworkHost> {
    // Get host info (brief mutex hold)
    let info = get_host_for_resolution(host_id)?;

    // If already resolved, return current state
    if info.ip_address.is_some() {
        let state = get_discovery_state().lock().unwrap();
        return state.hosts.get(host_id).cloned();
    }

    // Generate hostname
    let hostname = info.hostname.unwrap_or_else(|| service_name_to_hostname(&info.name));

    // Do DNS resolution (this is the slow blocking part - but mutex is NOT held!)
    let ip_address = resolve_host_ip(&hostname);

    // Update host (brief mutex hold)
    update_host_resolution(host_id, hostname, ip_address)
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
        };

        let json = serde_json::to_string(&host).unwrap();
        // hostname and ip_address should be omitted when None
        assert!(!json.contains("hostname"));
        assert!(!json.contains("ipAddress"));
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
