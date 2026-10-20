//! What the pin store owes a backend: one answer per server, replaced rather
//! than stacked, and gone once forgotten.
//!
//! In-memory only: these run before `load_trusted_certificates` names a file,
//! so `save()` is a no-op and the cells assert on the lookup.

use super::*;

/// The store is process-global, so each cell uses a host of its own.
fn host_for(cell: &str) -> String {
    format!("{cell}.webdav-certificates.test")
}

#[test]
fn an_unpinned_server_has_no_pin() {
    assert_eq!(AppTrustedCertificates.pinned(&host_for("unseen"), 5006), None);
}

#[test]
fn a_pin_comes_back_for_its_server_and_port_only() {
    let host = host_for("pinned");
    pin(&host, 5006, "AB:CD");
    assert_eq!(AppTrustedCertificates.pinned(&host, 5006).as_deref(), Some("AB:CD"));
    assert_eq!(
        AppTrustedCertificates.pinned(&host, 443),
        None,
        "another port is another server"
    );
}

#[test]
fn pinning_again_replaces_the_old_certificate() {
    let host = host_for("replaced");
    pin(&host, 5006, "AB:CD");
    pin(&host, 5006, "EF:01");
    assert_eq!(AppTrustedCertificates.pinned(&host, 5006).as_deref(), Some("EF:01"));
    assert_eq!(list().iter().filter(|entry| entry.host == host).count(), 1);
}

#[test]
fn forgetting_a_pin_makes_the_next_dial_first_contact() {
    let host = host_for("forgotten");
    pin(&host, 5006, "AB:CD");
    assert!(forget(&host, 5006));
    assert_eq!(AppTrustedCertificates.pinned(&host, 5006), None);
    assert!(!forget(&host, 5006), "nothing left to forget");
}
