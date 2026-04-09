//! Virtual SMB host injection for E2E testing.
//!
//! Registers synthetic NetworkHost entries pointing to Docker SMB containers
//! so that Playwright E2E tests can exercise the full network UI flow without
//! real hardware or mDNS discovery.
//!
//! Gated behind `--features smb-e2e`. Never enable in production builds.
//!
//! Host/port are configurable via environment variables for Docker networking:
//! - `SMB_E2E_GUEST_HOST` / `SMB_E2E_GUEST_PORT` (default: localhost:9445)
//! - `SMB_E2E_AUTH_HOST` / `SMB_E2E_AUTH_PORT` (default: localhost:9446)

use log::info;
use tauri::AppHandle;

use super::{HostSource, NetworkHost};

/// Reads an env var or returns a default value.
fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Injects two virtual SMB hosts (guest + auth) into the discovery state.
///
/// Must be called **after** `start_discovery()` so the hosts appear alongside
/// any real mDNS-discovered hosts.
pub fn setup_virtual_smb_hosts(app_handle: &AppHandle) {
    let guest_host = env_or("SMB_E2E_GUEST_HOST", "localhost");
    let guest_port: u16 = env_or("SMB_E2E_GUEST_PORT", "9445")
        .parse()
        .expect("SMB_E2E_GUEST_PORT must be a valid port number");
    let auth_host = env_or("SMB_E2E_AUTH_HOST", "localhost");
    let auth_port: u16 = env_or("SMB_E2E_AUTH_PORT", "9446")
        .parse()
        .expect("SMB_E2E_AUTH_PORT must be a valid port number");

    let hosts = [
        NetworkHost {
            id: "virtual-smb-guest".to_string(),
            name: "SMB Test (Guest)".to_string(),
            hostname: Some(guest_host.clone()),
            ip_address: None,
            port: guest_port,
            source: HostSource::Discovered,
        },
        NetworkHost {
            id: "virtual-smb-auth".to_string(),
            name: "SMB Test (Auth)".to_string(),
            hostname: Some(auth_host.clone()),
            ip_address: None,
            port: auth_port,
            source: HostSource::Discovered,
        },
    ];

    for host in hosts {
        super::on_host_found(host, app_handle);
    }

    info!(
        "Registered virtual SMB hosts for E2E testing: SMB Test (Guest) ({guest_host}:{guest_port}), SMB Test (Auth) ({auth_host}:{auth_port})"
    );
}
