//! mDNS/DNS-SD discovery using the `mdns-sd` crate.
//!
//! Browses for SMB services on the local network on a daemon thread plus an event
//! thread of our own. Both exist only while a browse runs: `discovery_gate.rs`
//! decides when, and calls [`start_browse`] and [`stop_browse`]. What the browse
//! finds goes into `discovery_cache.rs`, tagged with the browse's number.

use crate::ignore_poison::IgnorePoison;
use crate::network::discovery_cache::{
    self, MdnsResolution, on_mdns_host_found, on_mdns_host_lost, on_mdns_host_resolved, on_mdns_state_changed,
};
use crate::network::{DiscoveryState, HostSource, NetworkHost, service_name_to_id};
use cmdr_fs::log_private::LogPrivate;
use log::{debug, warn};
use mdns_sd::{Error as MdnsError, Receiver, ServiceDaemon, ServiceEvent};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::AppHandle;

/// SMB service type for mDNS discovery (mdns-sd requires the trailing `.local.` form).
const SMB_SERVICE_TYPE: &str = "_smb._tcp.local.";
/// Default SMB port.
const SMB_DEFAULT_PORT: u16 = 445;
/// Default timeout for service resolution in milliseconds.
const DEFAULT_RESOLVE_TIMEOUT_MS: u64 = 5000;
/// How long a browse runs before the cached hosts it hasn't heard from count as gone.
/// Every host on the network answers the first query within a second or two; the
/// margin is for a NAS waking from sleep.
const SETTLE_AFTER: Duration = Duration::from_secs(10);

fn mdns_error_kind(error: &MdnsError) -> &'static str {
    match error {
        MdnsError::Again => "again",
        MdnsError::DaemonShutdown => "daemon_shutdown",
        MdnsError::Msg(_) => "message",
        MdnsError::ParseIpAddr(_) => "invalid_ip",
        _ => "unknown",
    }
}

/// Configured resolve timeout in milliseconds (set by frontend via update_resolve_timeout).
/// With mdns-sd, browse automatically resolves services, and nothing reads this today: the manual
/// DNS fallback (`mod.rs::resolve_host_ip()`) it was meant for doesn't take a timeout.
static RESOLVE_TIMEOUT_MS: AtomicU64 = AtomicU64::new(DEFAULT_RESOLVE_TIMEOUT_MS);

/// Updates the mDNS service resolve timeout.
/// This affects future service resolutions; ongoing resolutions keep their original timeout.
pub fn update_resolve_timeout(ms: u64) {
    RESOLVE_TIMEOUT_MS.store(ms, Ordering::Relaxed);
    debug!("mDNS resolve timeout updated to {} ms", ms);
}

/// The running browse's daemon, `None` while nothing browses.
static DISCOVERY_DAEMON: Mutex<Option<ServiceDaemon>> = Mutex::new(None);

/// The app handle every browse reports through, installed once at setup.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Gives discovery the app handle it emits through. Call once, at setup, before
/// anything can hold a `DiscoveryLease`; a browse asked for before this starts nothing.
pub fn install(app_handle: &AppHandle) {
    let _ = APP_HANDLE.set(app_handle.clone());
}

/// Starts a browse for SMB hosts. Answers whether one is running now.
///
/// Only `discovery_gate` calls this, under its lock.
pub(super) fn start_browse() -> bool {
    let Some(app_handle) = APP_HANDLE.get() else {
        return false;
    };
    let mut guard = DISCOVERY_DAEMON.lock_ignore_poison();
    if guard.is_some() {
        return true;
    }

    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            warn!(
                "mDNS operation stopped: source=mdns, operation=daemon_create, error_kind={}, detail={:?}",
                mdns_error_kind(&e),
                cmdr_fs::log_detail::LogDetail(&e.to_string())
            );
            return false;
        }
    };
    let receiver = match daemon.browse(SMB_SERVICE_TYPE) {
        Ok(r) => r,
        Err(e) => {
            warn!(
                "mDNS operation stopped: source=mdns, operation=browse_start, error_kind={}, detail={:?}",
                mdns_error_kind(&e),
                cmdr_fs::log_detail::LogDetail(&e.to_string())
            );
            let _ = daemon.shutdown();
            return false;
        }
    };

    let browse = discovery_cache::begin_browse();
    let events_handle = app_handle.clone();
    if let Err(e) = std::thread::Builder::new()
        .name("mdns-event-loop".into())
        .spawn(move || process_events(receiver, browse, events_handle))
    {
        warn!(
            "mDNS operation stopped: source=os, operation=thread_spawn, error_kind={:?}, code={:?}, detail={:?}",
            e.kind(),
            e.raw_os_error(),
            cmdr_fs::log_detail::LogDetail(&e.to_string())
        );
        let _ = daemon.shutdown();
        discovery_cache::end_browse(app_handle);
        return false;
    }
    *guard = Some(daemon);
    debug!("mDNS browse {browse} started");

    let settle_handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SETTLE_AFTER).await;
        discovery_cache::settle_browse(browse, &settle_handle);
    });
    true
}

/// Stops the running browse, if any: the daemon thread and the event thread both
/// end, and the cache keeps its hosts, marked stale.
///
/// Only `discovery_gate` calls this, under its lock.
pub(super) fn stop_browse() {
    let Some(daemon) = DISCOVERY_DAEMON.lock_ignore_poison().take() else {
        return;
    };
    let _ = daemon.stop_browse(SMB_SERVICE_TYPE);
    let _ = daemon.shutdown();
    if let Some(app_handle) = APP_HANDLE.get() {
        discovery_cache::end_browse(app_handle);
    }
    debug!("mDNS browse stopped");
}

/// Main event loop: maps mdns-sd events to the discovery cache, tagged with `browse`
/// so a late event from this browse can't write into the next one.
fn process_events(receiver: Receiver<ServiceEvent>, browse: u64, app_handle: AppHandle) {
    let mut initial_scan_complete = false;

    while let Ok(event) = receiver.recv() {
        match event {
            ServiceEvent::SearchStarted(stype) => {
                // mdns-sd sends SearchStarted on every periodic re-query, not just once.
                // Only transition to Searching before the initial scan is complete.
                // After that, we stay in Active to avoid resetting the UI spinner.
                if !initial_scan_complete {
                    debug!("mDNS SearchStarted: service=\"{}\"", stype);
                    on_mdns_state_changed(DiscoveryState::Searching, browse, &app_handle);
                } else {
                    debug!("mDNS SearchStarted (ignored, already active): service=\"{}\"", stype);
                }
            }
            ServiceEvent::ServiceFound(_, fullname) => {
                let name = extract_instance_name(&fullname);
                let id = service_name_to_id(&name);
                debug!(
                    "mDNS ServiceFound: server={:?} serverId={id:?}",
                    LogPrivate::host(&name)
                );

                let host = NetworkHost {
                    id,
                    name,
                    hostname: None,
                    ip_address: None,
                    port: SMB_DEFAULT_PORT,
                    source: HostSource::Discovered,
                };
                on_mdns_host_found(host, browse, &app_handle);

                // Transition to Active on the first found host. The old NSNetServiceBrowser
                // code used the `moreComing` flag for this, but mdns-sd doesn't expose that
                // concept. Triggering on the first host is a good approximation: the user
                // sees a host, so the "Searching…" spinner should stop.
                if !initial_scan_complete {
                    initial_scan_complete = true;
                    debug!("mDNS initial scan complete, transitioning to Active");
                    on_mdns_state_changed(DiscoveryState::Active, browse, &app_handle);
                }
            }
            ServiceEvent::ServiceResolved(info) => {
                let name = extract_instance_name(info.get_fullname());
                let id = service_name_to_id(&name);

                let hostname = Some(info.get_hostname().trim_end_matches('.').to_string());
                let ip_address = extract_preferred_ip(info.get_addresses());
                let port = info.get_port();

                debug!(
                    "mDNS ServiceResolved: serverId={:?} host={:?}, ip={:?}, port={}",
                    id,
                    hostname.as_deref().map(LogPrivate::host),
                    ip_address.as_deref().map(LogPrivate::host),
                    port
                );

                let resolution = MdnsResolution {
                    host_id: id,
                    name,
                    hostname,
                    ip_address,
                    port,
                };
                on_mdns_host_resolved(resolution, browse, &app_handle);
            }
            ServiceEvent::ServiceRemoved(_, fullname) => {
                let name = extract_instance_name(&fullname);
                let id = service_name_to_id(&name);
                debug!(
                    "mDNS ServiceRemoved: server={:?} serverId={id:?}",
                    LogPrivate::host(&name)
                );
                on_mdns_host_lost(&id, browse, &app_handle);
            }
            ServiceEvent::SearchStopped(stype) => {
                debug!("mDNS SearchStopped: service=\"{}\"", stype);
                on_mdns_state_changed(DiscoveryState::Idle, browse, &app_handle);
            }
            other => {
                debug!(
                    "mDNS event unhandled: source=mdns, detail={:?}",
                    cmdr_fs::log_detail::LogDetail(&format!("{other:?}"))
                );
            }
        }
    }

    // Channel closed: daemon was shut down
    debug!("mDNS event loop for browse {browse} ended");
}

/// Extracts the instance name from a full mDNS service name.
///
/// For example, `"Tester's MacBook._smb._tcp.local."` → `"Tester's MacBook"`.
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
            extract_instance_name("Tester's MacBook._smb._tcp.local."),
            "Tester's MacBook"
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
