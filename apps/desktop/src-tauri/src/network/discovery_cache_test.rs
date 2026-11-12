use super::*;
use crate::network::server_identity::SmbServer;

fn nas(ip: Option<&str>) -> NetworkHost {
    NetworkHost {
        id: "nas".to_string(),
        name: "NAS".to_string(),
        hostname: ip.map(|_| "testnas.local".to_string()),
        ip_address: ip.map(str::to_string),
        port: 445,
        source: HostSource::Discovered,
    }
}

/// A browse that found and resolved the NAS at `ip`.
fn browse_resolving_nas(cache: &mut DiscoveryCache, ip: &str) -> u64 {
    let browse = cache.begin_browse();
    cache.mdns_found(nas(None), browse);
    cache.mdns_resolved(
        "nas",
        "NAS",
        Some("testnas.local".to_string()),
        Some(ip.to_string()),
        445,
        browse,
    );
    browse
}

fn ids(hosts: &[NetworkHost]) -> Vec<&str> {
    hosts.iter().map(|h| h.id.as_str()).collect()
}

#[test]
fn a_host_resolved_by_the_running_browse_is_fresh() {
    let mut cache = DiscoveryCache::default();
    browse_resolving_nas(&mut cache, "192.168.1.10");
    assert_eq!(ids(&cache.fresh_hosts()), ["nas"]);
}

#[test]
fn a_stopped_browse_leaves_its_hosts_on_display_but_not_as_evidence() {
    let mut cache = DiscoveryCache::default();
    browse_resolving_nas(&mut cache, "192.168.1.10");
    cache.end_browse();

    assert_eq!(ids(&cache.cached_hosts()), ["nas"], "the Servers list still shows it");
    assert!(cache.fresh_hosts().is_empty(), "nothing vouches for its address now");
}

#[test]
fn dedupe_never_matches_two_servers_on_a_stale_entry() {
    // The mDNS name and the IP are one server only while a live browse says so.
    let mut cache = DiscoveryCache::default();
    let by_name = SmbServer::new("NAS._smb._tcp.local", 445);
    let by_ip = SmbServer::new("192.168.1.10", 445);

    browse_resolving_nas(&mut cache, "192.168.1.10");
    assert!(by_name.is(&by_ip, &cache.fresh_hosts()), "fresh evidence pairs them");

    cache.end_browse();
    assert!(
        !by_name.is(&by_ip, &cache.fresh_hosts()),
        "a cached pairing must not merge two servers once its browse is gone"
    );
}

#[test]
fn a_new_browse_trusts_a_cached_host_only_once_it_resolves_again() {
    let mut cache = DiscoveryCache::default();
    browse_resolving_nas(&mut cache, "192.168.1.10");
    cache.end_browse();

    let browse = cache.begin_browse();
    let shown = cache.mdns_found(nas(None), browse);
    assert_eq!(
        shown.and_then(|h| h.ip_address).as_deref(),
        Some("192.168.1.10"),
        "re-found, it keeps its cached address for display"
    );
    assert!(
        cache.fresh_hosts().is_empty(),
        "found isn't resolved: the address is still old"
    );

    cache.mdns_resolved(
        "nas",
        "NAS",
        Some("testnas.local".to_string()),
        Some("192.168.1.20".to_string()),
        445,
        browse,
    );
    let fresh = cache.fresh_hosts();
    assert_eq!(
        fresh.first().and_then(|h| h.ip_address.as_deref()),
        Some("192.168.1.20")
    );
}

#[test]
fn events_from_a_stopped_browse_are_ignored() {
    let mut cache = DiscoveryCache::default();
    let old = cache.begin_browse();
    cache.end_browse();
    let _current = cache.begin_browse();

    assert!(cache.mdns_found(nas(None), old).is_none());
    assert!(
        cache
            .mdns_resolved("nas", "NAS", None, Some("192.168.1.10".to_string()), 445, old)
            .is_none()
    );
    assert!(
        cache.cached_hosts().is_empty(),
        "a late event can't write into the new browse"
    );
}

#[test]
fn a_host_lost_by_an_old_browse_stays_cached() {
    let mut cache = DiscoveryCache::default();
    let old = browse_resolving_nas(&mut cache, "192.168.1.10");
    cache.end_browse();

    assert!(!cache.mdns_lost("nas", old));
    assert_eq!(ids(&cache.cached_hosts()), ["nas"]);
}

#[test]
fn settling_drops_the_cached_hosts_the_new_browse_never_saw() {
    let mut cache = DiscoveryCache::default();
    let first = cache.begin_browse();
    cache.mdns_found(nas(None), first);
    let mut printer = nas(None);
    printer.id = "printer".to_string();
    printer.name = "Printer".to_string();
    cache.mdns_found(printer, first);
    cache.end_browse();

    let second = cache.begin_browse();
    cache.mdns_found(nas(None), second);
    assert_eq!(cache.drop_unseen(second), ["printer"]);
    assert_eq!(ids(&cache.cached_hosts()), ["nas"]);
}

#[test]
fn settling_a_browse_that_already_ended_drops_nothing() {
    let mut cache = DiscoveryCache::default();
    let browse = cache.begin_browse();
    cache.end_browse();
    let _ = cache.begin_browse();
    cache.mdns_found(nas(None), cache.live_browse().expect("a browse runs"));
    assert!(cache.drop_unseen(browse).is_empty());
}

#[test]
fn pinned_hosts_are_evidence_with_or_without_a_browse() {
    // Manual servers and the E2E virtual hosts come from config, not the network.
    let mut cache = DiscoveryCache::default();
    let mut manual = nas(Some("10.0.0.5"));
    manual.id = "manual-10-0-0-5-445".to_string();
    manual.source = HostSource::Manual;
    cache.pinned_found(manual);

    assert_eq!(ids(&cache.fresh_hosts()), ["manual-10-0-0-5-445"]);
    let browse = cache.begin_browse();
    assert!(
        cache.drop_unseen(browse).is_empty(),
        "settling never drops a pinned host"
    );
}

#[test]
fn a_fresh_resolution_never_inherits_an_address_from_an_earlier_browse() {
    let mut cache = DiscoveryCache::default();
    browse_resolving_nas(&mut cache, "192.168.1.10");
    cache.end_browse();

    let browse = cache.begin_browse();
    cache.mdns_resolved("nas", "NAS", Some("testnas.local".to_string()), None, 445, browse);
    let fresh = cache.fresh_hosts();
    assert_eq!(
        fresh.first().map(|h| h.ip_address.as_deref()),
        Some(None),
        "this browse gave no address, so the old one must not ride along as evidence"
    );
}

#[test]
fn a_second_resolution_in_one_browse_keeps_what_the_first_one_gave() {
    let mut cache = DiscoveryCache::default();
    let browse = browse_resolving_nas(&mut cache, "192.168.1.10");
    cache.mdns_resolved("nas", "NAS", Some("testnas.local".to_string()), None, 445, browse);
    let fresh = cache.fresh_hosts();
    assert_eq!(
        fresh.first().and_then(|h| h.ip_address.as_deref()),
        Some("192.168.1.10")
    );
}
