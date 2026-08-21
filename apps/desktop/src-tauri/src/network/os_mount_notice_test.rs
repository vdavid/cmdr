//! Tests for the OS-mount notice ledger: who gets told, and how often.

use super::*;

/// A host as mDNS would report it, so a test can exercise the IP ↔ name
/// equivalence `same_server` reads out of the discovery state.
fn host(name: &str, hostname: &str, ip: &str) -> NetworkHost {
    NetworkHost {
        id: name.to_lowercase(),
        name: name.to_string(),
        hostname: Some(hostname.to_string()),
        ip_address: Some(ip.to_string()),
        port: 445,
        source: crate::network::HostSource::default(),
    }
}

/// The whole point of the ledger. The startup pass and the mount watcher call
/// the fallback once per MOUNTED SHARE, so a NAS with every share mounted at
/// login would otherwise raise one notice per share.
#[test]
fn a_server_is_worth_one_notice_however_many_of_its_shares_fall_back() {
    let mut notices = OsMountNotices::default();
    let shares = ["photos", "archive", "backups", "media", "scratch"];

    let spoken = shares.iter().filter(|_| notices.claim("naspolya", &[])).count();

    assert_eq!(spoken, 1, "50 shares on one NAS must not raise 50 notices");
}

/// The ledger keys on server IDENTITY, not on whatever string `statfs` echoed
/// back. One mount reports the mDNS service name, the next the `.local`
/// hostname; a raw string key would let one server speak three times.
#[test]
fn one_server_under_its_many_name_forms_still_gets_one_notice() {
    let mut notices = OsMountNotices::default();

    assert!(notices.claim("Naspolya._smb._tcp.local", &[]));
    assert!(!notices.claim("naspolya.local", &[]));
    assert!(!notices.claim("NASPOLYA", &[]));
}

/// Cmdr mounts by IP while Finder mounts by name, so the same NAS arrives as
/// both within one session. Only the discovery state can pair them, which is
/// why the ledger consults it rather than comparing bare names.
#[test]
fn a_server_reached_by_ip_and_by_name_is_one_server_when_mdns_knows_the_pairing() {
    let hosts = [host("Naspolya", "naspolya.local", "192.168.1.111")];
    let mut notices = OsMountNotices::default();

    assert!(notices.claim("192.168.1.111", &hosts));
    assert!(!notices.claim("Naspolya._smb._tcp.local", &hosts));
}

#[test]
fn a_second_server_gets_its_own_notice() {
    let mut notices = OsMountNotices::default();

    assert!(notices.claim("first-nas.local", &[]));
    assert!(notices.claim("second-nas.local", &[]));
}

/// A notice describes a situation ("this server is on the slow path"), not an
/// event. Once a direct session lands, the situation is over, so the next
/// genuine regression is worth saying out loud again.
#[test]
fn a_direct_connection_clears_the_notice_so_a_later_regression_can_speak_again() {
    let mut notices = OsMountNotices::default();
    assert!(notices.claim("recovering.local", &[]));
    assert!(!notices.claim("recovering.local", &[]));

    notices.forget("Recovering._smb._tcp.local", &[]);

    assert!(
        notices.claim("recovering.local", &[]),
        "after a direct connect lands, a fresh fallback deserves a fresh notice"
    );
}

/// Clearing one server's notice must not un-tell the user about another.
#[test]
fn clearing_one_server_leaves_the_others_told() {
    let mut notices = OsMountNotices::default();
    assert!(notices.claim("alpha.local", &[]));
    assert!(notices.claim("beta.local", &[]));

    notices.forget("alpha.local", &[]);

    assert!(
        notices.claim("alpha.local", &[]),
        "alpha was forgotten, so it speaks again"
    );
    assert!(!notices.claim("beta.local", &[]), "beta was never forgotten");
}

/// A direct connect can land on a server nobody was ever warned about (the
/// common case: everything worked the first time).
#[test]
fn forgetting_a_server_nobody_was_told_about_is_harmless() {
    let mut notices = OsMountNotices::default();

    notices.forget("never-mentioned.local", &[]);

    assert!(notices.claim("never-mentioned.local", &[]));
}
