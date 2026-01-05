//! Tauri commands for network host discovery.

use crate::network::{
    DiscoveryState, NetworkHost, get_discovered_hosts, get_discovery_state_value, get_host_for_resolution,
    resolve_host_ip, service_name_to_hostname, update_host_resolution,
};

/// Logs a message from the frontend (for debugging).
#[tauri::command]
pub fn fe_log(message: String) {
    println!("[FE] {}", message);
}

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
