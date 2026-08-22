//! The three answers the store owes a backend, and the one that decides whether
//! a man-in-the-middle is visible.
//!
//! In-memory only: these run before `load_trusted_host_keys` names a file, so
//! `save()` is a no-op and the cells assert on the lookup rather than on the
//! write. The durability of the write itself is `config::durable_write_json`'s
//! own contract.

use super::*;

/// The store is process-global, so two cells writing the same triple would see
/// each other. Each uses a host of its own instead of taking turns on a mutex.
fn host_for(cell: &str) -> String {
    format!("{cell}.sftp-host-keys.test")
}

#[test]
fn an_unseen_server_is_unknown() {
    assert_eq!(
        AppHostKeys.verdict(&host_for("unseen"), 22, "ssh-ed25519", "SHA256:whatever"),
        HostKeyVerdict::Unknown
    );
}

#[test]
fn a_recorded_key_comes_back_as_a_match() {
    let host = host_for("recorded");
    AppHostKeys.record(&host, 22, "ssh-ed25519", "SHA256:aaa");
    assert_eq!(
        AppHostKeys.verdict(&host, 22, "ssh-ed25519", "SHA256:aaa"),
        HostKeyVerdict::Matches
    );
}

#[test]
fn a_different_key_under_a_stored_algorithm_is_a_changed_key() {
    let host = host_for("changed");
    AppHostKeys.record(&host, 22, "ssh-ed25519", "SHA256:aaa");
    assert_eq!(
        AppHostKeys.verdict(&host, 22, "ssh-ed25519", "SHA256:bbb"),
        HostKeyVerdict::Changed
    );
}

#[test]
fn a_second_algorithm_on_one_server_is_first_contact_not_a_change() {
    // A healthy server may hold several host keys and present whichever the
    // negotiation lands on. Crying man-in-the-middle here is how people learn to
    // click through the one alarm that matters.
    let host = host_for("two-types");
    AppHostKeys.record(&host, 22, "ssh-ed25519", "SHA256:aaa");
    assert_eq!(
        AppHostKeys.verdict(&host, 22, "ssh-rsa", "SHA256:ccc"),
        HostKeyVerdict::Unknown
    );
}

#[test]
fn the_port_is_part_of_the_identity() {
    // A container and a jump box on one machine are different servers.
    let host = host_for("ports");
    AppHostKeys.record(&host, 22, "ssh-ed25519", "SHA256:aaa");
    assert_eq!(
        AppHostKeys.verdict(&host, 2222, "ssh-ed25519", "SHA256:aaa"),
        HostKeyVerdict::Unknown
    );
}

#[test]
fn re_recording_replaces_rather_than_stacking() {
    // Two entries for one triple would make the verdict depend on iteration
    // order, and a stale one could answer `Matches` for a key the user replaced.
    let host = host_for("replace");
    AppHostKeys.record(&host, 22, "ssh-ed25519", "SHA256:aaa");
    AppHostKeys.record(&host, 22, "ssh-ed25519", "SHA256:bbb");

    let entries: Vec<_> = trusted()
        .lock_ignore_poison()
        .trusted_host_keys
        .iter()
        .filter(|e| e.host == host && e.algorithm == "ssh-ed25519")
        .cloned()
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].fingerprint, "SHA256:bbb");
    assert_eq!(
        AppHostKeys.verdict(&host, 22, "ssh-ed25519", "SHA256:aaa"),
        HostKeyVerdict::Changed,
        "the key the user replaced must not still read as trusted"
    );
}

#[test]
fn the_pin_lists_every_algorithm_trusted_for_a_server() {
    // ❗ What the backend narrows its key-exchange offer to. Without it, an
    // attacker offering a type we hold no entry for lands on the first-contact
    // path and collects a one-click approval.
    let host = host_for("pin");
    AppHostKeys.record(&host, 22, "ssh-ed25519", "SHA256:aaa");
    AppHostKeys.record(&host, 22, "rsa-sha2-512", "SHA256:bbb");
    assert_eq!(
        AppHostKeys.trusted_algorithms(&host, 22),
        vec!["rsa-sha2-512".to_string(), "ssh-ed25519".to_string()]
    );
    assert!(
        AppHostKeys.trusted_algorithms(&host_for("pin-unseen"), 22).is_empty(),
        "first contact pins nothing, so every algorithm stays on the table"
    );
}
