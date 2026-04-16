//! Virtual SMB host injection for E2E testing.
//!
//! Registers synthetic NetworkHost entries pointing to smb2's consumer Docker
//! containers so that Playwright E2E tests can exercise the full network UI
//! flow without real hardware or mDNS discovery.
//!
//! Gated behind `--features smb-e2e`. Never enable in production builds.
//!
//! Ports are resolved in order: `SMB_E2E_{SERVICE}_PORT` (test-suite contract,
//! same var the frontend fixture reads) → `smb2::testing::*_port()` (which
//! reads `SMB_CONSUMER_*_PORT`; default 10480+). Host is configurable via
//! `SMB_E2E_{SERVICE}_HOST` or `SMB_E2E_HOST` (default: `localhost`).

use log::info;
use tauri::AppHandle;

use super::{HostSource, NetworkHost};

/// Host for all virtual SMB servers. Override for Docker networking
/// (for example, `SMB_E2E_HOST=smb-consumer-guest` when containers share a network).
fn smb_host() -> String {
    std::env::var("SMB_E2E_HOST").unwrap_or_else(|_| "localhost".to_string())
}

/// Per-server host override. Falls back to the shared `SMB_E2E_HOST`.
fn host_for(env_key: &str) -> String {
    std::env::var(env_key).unwrap_or_else(|_| smb_host())
}

/// Per-server port override (`SMB_E2E_{SERVICE}_PORT`). Matches what the
/// frontend test fixture uses — inside Docker this is the container's internal
/// port (445), not the host port mapping. Falls back to the default (which
/// reads `SMB_CONSUMER_*_PORT` for local macOS dev via docker-compose).
fn port_for(env_key: &str, default_port: u16) -> u16 {
    std::env::var(env_key)
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(default_port)
}

/// Injects all 14 virtual SMB hosts into the discovery state.
///
/// Must be called **after** `start_discovery()` so the hosts appear alongside
/// any real mDNS-discovered hosts.
pub fn setup_virtual_smb_hosts(app_handle: &AppHandle) {
    let hosts = [
        (
            "virtual-smb-guest",
            "SMB Test (Guest)",
            "SMB_E2E_GUEST_HOST",
            "SMB_E2E_GUEST_PORT",
            smb2::testing::guest_port(),
        ),
        (
            "virtual-smb-auth",
            "SMB Test (Auth)",
            "SMB_E2E_AUTH_HOST",
            "SMB_E2E_AUTH_PORT",
            smb2::testing::auth_port(),
        ),
        (
            "virtual-smb-both",
            "SMB Test (Both)",
            "SMB_E2E_BOTH_HOST",
            "SMB_E2E_BOTH_PORT",
            smb2::testing::both_port(),
        ),
        (
            "virtual-smb-readonly",
            "SMB Test (Read-only)",
            "SMB_E2E_READONLY_HOST",
            "SMB_E2E_READONLY_PORT",
            smb2::testing::readonly_port(),
        ),
        (
            "virtual-smb-50shares",
            "SMB Test (50 Shares)",
            "SMB_E2E_50SHARES_HOST",
            "SMB_E2E_50SHARES_PORT",
            smb2::testing::many_shares_port(),
        ),
        (
            "virtual-smb-unicode",
            "SMB Test (Unicode)",
            "SMB_E2E_UNICODE_HOST",
            "SMB_E2E_UNICODE_PORT",
            smb2::testing::unicode_port(),
        ),
        (
            "virtual-smb-longnames",
            "SMB Test (Long Names)",
            "SMB_E2E_LONGNAMES_HOST",
            "SMB_E2E_LONGNAMES_PORT",
            smb2::testing::longnames_port(),
        ),
        (
            "virtual-smb-deepnest",
            "SMB Test (Deep Nesting)",
            "SMB_E2E_DEEPNEST_HOST",
            "SMB_E2E_DEEPNEST_PORT",
            smb2::testing::deepnest_port(),
        ),
        (
            "virtual-smb-manyfiles",
            "SMB Test (10k Files)",
            "SMB_E2E_MANYFILES_HOST",
            "SMB_E2E_MANYFILES_PORT",
            smb2::testing::manyfiles_port(),
        ),
        (
            "virtual-smb-windows",
            "SMB Test (Windows)",
            "SMB_E2E_WINDOWS_HOST",
            "SMB_E2E_WINDOWS_PORT",
            smb2::testing::windows_port(),
        ),
        (
            "virtual-smb-synology",
            "SMB Test (Synology)",
            "SMB_E2E_SYNOLOGY_HOST",
            "SMB_E2E_SYNOLOGY_PORT",
            smb2::testing::synology_port(),
        ),
        (
            "virtual-smb-linux",
            "SMB Test (Linux)",
            "SMB_E2E_LINUX_HOST",
            "SMB_E2E_LINUX_PORT",
            smb2::testing::linux_port(),
        ),
        (
            "virtual-smb-flaky",
            "SMB Test (Flaky)",
            "SMB_E2E_FLAKY_HOST",
            "SMB_E2E_FLAKY_PORT",
            smb2::testing::flaky_port(),
        ),
        (
            "virtual-smb-slow",
            "SMB Test (Slow)",
            "SMB_E2E_SLOW_HOST",
            "SMB_E2E_SLOW_PORT",
            smb2::testing::slow_port(),
        ),
    ];

    let mut registered = Vec::new();
    for (id, name, host_env, port_env, default_port) in hosts {
        let hostname = host_for(host_env);
        let port = port_for(port_env, default_port);
        super::on_host_found(
            NetworkHost {
                id: id.to_string(),
                name: name.to_string(),
                hostname: Some(hostname.clone()),
                ip_address: None,
                port,
                source: HostSource::Discovered,
            },
            app_handle,
        );
        registered.push(format!("{name} ({hostname}:{port})"));
    }

    info!(
        "Registered {} virtual SMB hosts for E2E testing: {}",
        registered.len(),
        registered.join(", ")
    );
}
