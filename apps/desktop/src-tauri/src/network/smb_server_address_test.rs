//! Tests for `smb_server_address.rs`: the name forms a server goes by, the mDNS
//! wait, and what a `statfs` server string can be dialed as.

use super::*;
use std::time::Duration;

#[test]
fn system_keychain_aliases_include_the_mdns_service_form_for_an_ip() {
    use crate::network::{HostSource, NetworkHost};
    let hosts = [NetworkHost {
        id: "naspolya".into(),
        name: "Naspolya".into(),
        hostname: Some("Naspolya.local".into()),
        ip_address: Some("192.168.1.111".into()),
        port: 445,
        source: HostSource::Discovered,
    }];
    // We mount by IP; Finder keyed its password by the mDNS service name. The alias
    // set must include that form so the keychain lookup can find it.
    let aliases = system_keychain_aliases_from("192.168.1.111", &hosts);
    assert!(
        aliases.contains(&"Naspolya._smb._tcp.local".to_string()),
        "got {aliases:?}"
    );
    assert!(aliases.contains(&"Naspolya.local".to_string()));
    assert!(aliases.contains(&"Naspolya".to_string()));
}

#[test]
fn system_keychain_aliases_empty_for_an_unknown_server() {
    assert!(system_keychain_aliases_from("10.9.9.9", &[]).is_empty());
}

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

/// `network.enabled` is a process-global, so the two tests that flip it must
/// not run concurrently: one setting it `false` while the other polls made
/// the other short-circuit and fail (whichever lost the race).
/// Async-aware: both tests `await` while holding it.
static NETWORK_FLAG_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// `resolve_ip_to_hostname_with_wait` must short-circuit when the runtime
/// `network.enabled` flag is off, even for a private IP — mDNS isn't running
/// so polling would just burn the timeout.
#[tokio::test]
async fn wait_helper_short_circuits_when_network_disabled() {
    let _serialized = NETWORK_FLAG_LOCK.lock().await;
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
    let _serialized = NETWORK_FLAG_LOCK.lock().await;
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

// ── What a server string can be dialed as ─────────────────────────────────────

/// An IP or a DNS hostname is dialable as it stands; nothing about it needs mDNS.
#[test]
fn a_plain_address_is_connectable_as_it_stands() {
    assert_eq!(
        resolve_server_address("192.168.1.111"),
        ServerAddress::Connectable("192.168.1.111".to_string())
    );
    assert_eq!(
        resolve_server_address("fileserver.corp.example.com"),
        ServerAddress::Connectable("fileserver.corp.example.com".to_string())
    );
}

/// An mDNS SERVICE instance name that discovery hasn't seen has no address at
/// all, and saying so is the whole point.
///
/// `getaddrinfo` can't resolve one (it names a service, not a host), so handing
/// it to the dialer spends the resolver's timeout (8.2 s on the report that
/// prompted this) and then reports a connection failure for a share that
/// connects fine on the next pass, once discovery goes active. Reported as
/// ERR-48RZX.
#[test]
fn an_undiscovered_mdns_service_name_has_nothing_to_dial() {
    assert_eq!(
        resolve_server_address("DS220j._smb._tcp.local."),
        ServerAddress::UndiscoveredService
    );
}
