//! mDNS/DNS-SD discovery using the `mdns-sd` crate.
//!
//! Discovers SMB services on the local network via multicast DNS.
//! Replaces the deprecated NSNetServiceBrowser approach with a pure-Rust,
//! cross-platform implementation that runs on a background thread.

use crate::ignore_poison::IgnorePoison;
use crate::network::{
    DiscoveryState, NetworkHost, on_discovery_state_changed, on_host_found, on_host_lost, on_host_resolved,
    service_name_to_id,
};
use log::{debug, warn};
use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;

/// SMB service type for mDNS discovery (mdns-sd requires the trailing `.local.` form).
const SMB_SERVICE_TYPE: &str = "_smb._tcp.local.";
/// Default SMB port.
const SMB_DEFAULT_PORT: u16 = 445;
/// Default timeout for service resolution in milliseconds.
const DEFAULT_RESOLVE_TIMEOUT_MS: u64 = 5000;

/// Configured resolve timeout in milliseconds (set by frontend via update_resolve_timeout).
/// With mdns-sd, browse automatically resolves services. This timeout is only relevant
/// for the manual DNS fallback path in `mod.rs::resolve_host_ip()`.
static RESOLVE_TIMEOUT_MS: AtomicU64 = AtomicU64::new(DEFAULT_RESOLVE_TIMEOUT_MS);

/// Updates the mDNS service resolve timeout.
/// This affects future service resolutions; ongoing resolutions keep their original timeout.
pub fn update_resolve_timeout(ms: u64) {
    RESOLVE_TIMEOUT_MS.store(ms, Ordering::Relaxed);
    debug!("mDNS resolve timeout updated to {} ms", ms);
}

/// Global mDNS discovery daemon.
static DISCOVERY_DAEMON: OnceLock<Mutex<Option<ServiceDaemon>>> = OnceLock::new();

/// Global app handle for sending events.
static APP_HANDLE: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();

fn get_daemon_lock() -> &'static Mutex<Option<ServiceDaemon>> {
    DISCOVERY_DAEMON.get_or_init(|| Mutex::new(None))
}

fn get_app_handle() -> Option<AppHandle> {
    APP_HANDLE
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|guard| guard.clone())
}

fn set_app_handle(handle: AppHandle) {
    let storage = APP_HANDLE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = storage.lock() {
        *guard = Some(handle);
    }
}

/// Starts mDNS discovery for SMB hosts.
///
/// Spawns a background thread that listens for service events. No main-thread requirement.
pub fn start_discovery(app_handle: AppHandle) {
    let mut guard = get_daemon_lock().lock_ignore_poison();

    // Don't start if already running
    if guard.is_some() {
        return;
    }

    set_app_handle(app_handle);

    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            warn!("Failed to create mDNS daemon: {}", e);
            return;
        }
    };

    let receiver = match daemon.browse(SMB_SERVICE_TYPE) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to start mDNS browse: {}", e);
            return;
        }
    };

    *guard = Some(daemon);

    // Process events on a dedicated thread
    std::thread::Builder::new()
        .name("mdns-event-loop".into())
        .spawn(move || process_events(receiver))
        .expect("Failed to spawn mDNS event thread");
}

/// Stops mDNS discovery and shuts down the daemon.
pub fn stop_discovery() {
    let mut guard = get_daemon_lock().lock_ignore_poison();

    if let Some(daemon) = guard.take() {
        let _ = daemon.stop_browse(SMB_SERVICE_TYPE);
        let _ = daemon.shutdown();
    }
}

/// Main event loop — maps mdns-sd events to the existing network module callbacks.
fn process_events(receiver: Receiver<ServiceEvent>) {
    let mut initial_scan_complete = false;

    while let Ok(event) = receiver.recv() {
        let Some(app_handle) = get_app_handle() else {
            continue;
        };

        match event {
            ServiceEvent::SearchStarted(stype) => {
                // mdns-sd sends SearchStarted on every periodic re-query, not just once.
                // Only transition to Searching before the initial scan is complete —
                // after that, we stay in Active to avoid resetting the UI spinner.
                if !initial_scan_complete {
                    debug!("mDNS SearchStarted: {}", stype);
                    on_discovery_state_changed(DiscoveryState::Searching, &app_handle);
                } else {
                    debug!("mDNS SearchStarted (ignored, already active): {}", stype);
                }
            }
            ServiceEvent::ServiceFound(_, fullname) => {
                let name = extract_instance_name(&fullname);
                let id = service_name_to_id(&name);
                debug!("mDNS ServiceFound: {} (id={})", name, id);

                let host = NetworkHost {
                    id,
                    name,
                    hostname: None,
                    ip_address: None,
                    port: SMB_DEFAULT_PORT,
                };
                on_host_found(host, &app_handle);

                // Transition to Active on the first found host. The old NSNetServiceBrowser
                // code used the `moreComing` flag for this, but mdns-sd doesn't expose that
                // concept. Triggering on the first host is a good approximation — the user
                // sees a host, so the "Searching..." spinner should stop.
                if !initial_scan_complete {
                    initial_scan_complete = true;
                    debug!("mDNS initial scan complete, transitioning to Active");
                    on_discovery_state_changed(DiscoveryState::Active, &app_handle);
                }
            }
            ServiceEvent::ServiceResolved(info) => {
                let name = extract_instance_name(info.get_fullname());
                let id = service_name_to_id(&name);

                let hostname = Some(info.get_hostname().trim_end_matches('.').to_string());
                let ip_address = extract_preferred_ip(info.get_addresses());
                let port = info.get_port();

                debug!(
                    "mDNS ServiceResolved: {} hostname={:?}, ip={:?}, port={}",
                    id, hostname, ip_address, port
                );

                on_host_resolved(&id, hostname, ip_address, port, &app_handle);
            }
            ServiceEvent::ServiceRemoved(_, fullname) => {
                let name = extract_instance_name(&fullname);
                let id = service_name_to_id(&name);
                debug!("mDNS ServiceRemoved: {} (id={})", name, id);
                on_host_lost(&id, &app_handle);
            }
            ServiceEvent::SearchStopped(stype) => {
                debug!("mDNS SearchStopped: {}", stype);
                on_discovery_state_changed(DiscoveryState::Idle, &app_handle);
            }
            other => {
                debug!("mDNS unhandled event: {:?}", other);
            }
        }
    }

    // Channel closed — daemon was shut down
    debug!("mDNS event loop ended");
}

/// Extracts the instance name from a full mDNS service name.
///
/// For example, `"David's MacBook._smb._tcp.local."` → `"David's MacBook"`.
fn extract_instance_name(fullname: &str) -> String {
    // The instance name is everything before the first `._` separator.
    // mdns-sd escapes dots in instance names as `\.`, so splitting on `._`
    // (unescaped dot followed by underscore) is safe.
    match fullname.find("._") {
        Some(pos) => fullname[..pos].replace("\\.", "."),
        None => fullname.to_string(),
    }
}

/// Picks the best IP from a set of addresses, preferring IPv4 over IPv6.
fn extract_preferred_ip(addresses: &std::collections::HashSet<mdns_sd::ScopedIp>) -> Option<String> {
    let mut ipv6_fallback: Option<String> = None;

    for scoped in addresses {
        let ip = scoped.to_ip_addr();
        match ip {
            IpAddr::V4(_) => return Some(ip.to_string()),
            IpAddr::V6(_) if ipv6_fallback.is_none() => {
                ipv6_fallback = Some(ip.to_string());
            }
            _ => {}
        }
    }

    ipv6_fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(SMB_SERVICE_TYPE, "_smb._tcp.local.");
        assert_eq!(SMB_DEFAULT_PORT, 445);
    }

    #[test]
    fn test_extract_instance_name_basic() {
        assert_eq!(
            extract_instance_name("David's MacBook._smb._tcp.local."),
            "David's MacBook"
        );
    }

    #[test]
    fn test_extract_instance_name_with_escaped_dot() {
        // mdns-sd escapes dots in instance names as `\.`
        assert_eq!(extract_instance_name("My\\.Server._smb._tcp.local."), "My.Server");
    }

    #[test]
    fn test_extract_instance_name_no_separator() {
        assert_eq!(extract_instance_name("plain-name"), "plain-name");
    }

    #[test]
    fn test_extract_instance_name_hyphenated() {
        assert_eq!(extract_instance_name("NAS-Server._smb._tcp.local."), "NAS-Server");
    }

    #[test]
    fn test_extract_preferred_ip_prefers_v4() {
        use mdns_sd::ScopedIp;
        use std::collections::HashSet;
        use std::net::{Ipv4Addr, Ipv6Addr};

        let mut addrs = HashSet::new();
        addrs.insert(ScopedIp::from(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        addrs.insert(ScopedIp::from(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42))));

        assert_eq!(extract_preferred_ip(&addrs), Some("192.168.1.42".to_string()));
    }

    #[test]
    fn test_extract_preferred_ip_v6_fallback() {
        use mdns_sd::ScopedIp;
        use std::collections::HashSet;
        use std::net::Ipv6Addr;

        let mut addrs = HashSet::new();
        addrs.insert(ScopedIp::from(IpAddr::V6(Ipv6Addr::LOCALHOST)));

        assert_eq!(extract_preferred_ip(&addrs), Some("::1".to_string()));
    }

    #[test]
    fn test_extract_preferred_ip_empty() {
        use std::collections::HashSet;
        let addrs = HashSet::new();
        assert_eq!(extract_preferred_ip(&addrs), None);
    }
}
