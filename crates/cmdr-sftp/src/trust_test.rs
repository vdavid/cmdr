//! The host-key decision table.
//!
//! Every cell here is a way of getting trust wrong that has a name in the SSH
//! world, and each one is cheap to hold because the decision is a pure function
//! over a store, a `known_hosts` text, and the key a server presented.

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::host_keys::InMemoryHostKeys;

use super::*;
use crate::known_hosts::KnownHostsFile;

/// The ed25519 key our fixture host presents.
const ED25519_BLOB: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
/// A DIFFERENT ed25519 key, for the man-in-the-middle cells.
const OTHER_ED25519_BLOB: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X";
/// The same host's SECOND key type, which a healthy server may present instead.
const RSA_BLOB: &str = "AAAAB3NzaC1yc2EAAAADAQABAAABgQDexampleexampleexample";

const HOST: &str = "naspolya";
const PORT: u16 = 12480;

fn ed25519() -> PresentedHostKey {
    PresentedHostKey::new(
        "ssh-ed25519",
        ED25519_BLOB,
        "SHA256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
}

fn other_ed25519() -> PresentedHostKey {
    PresentedHostKey::new(
        "ssh-ed25519",
        OTHER_ED25519_BLOB,
        "SHA256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    )
}

fn rsa() -> PresentedHostKey {
    PresentedHostKey::new(
        "ssh-rsa",
        RSA_BLOB,
        "SHA256:ccccccccccccccccccccccccccccccccccccccccccc",
    )
}

fn no_known_hosts() -> KnownHostsFile {
    KnownHostsFile::parse("")
}

// ── The store's own three answers ────────────────────────────────────

#[test]
fn a_host_nobody_has_ever_seen_is_unknown() {
    let store = InMemoryHostKeys::new();
    assert_eq!(
        decide(&store, &no_known_hosts(), HOST, PORT, &ed25519()),
        HostKeyDecision::Unknown
    );
}

#[test]
fn the_key_we_stored_for_this_host_is_trusted() {
    let store = InMemoryHostKeys::new().with_entry(HOST, PORT, "ssh-ed25519", &ed25519().fingerprint);
    assert_eq!(
        decide(&store, &no_known_hosts(), HOST, PORT, &ed25519()),
        HostKeyDecision::Trusted
    );
}

#[test]
fn a_different_key_under_a_stored_algorithm_is_a_changed_key() {
    // The alarm that matters: same host, same key type, different key.
    let store = InMemoryHostKeys::new().with_entry(HOST, PORT, "ssh-ed25519", &ed25519().fingerprint);
    assert_eq!(
        decide(&store, &no_known_hosts(), HOST, PORT, &other_ed25519()),
        HostKeyDecision::Changed
    );
}

#[test]
fn recording_an_approval_turns_unknown_into_trusted() {
    // The approve-then-reconnect loop: without a store that remembers, the
    // fixture harness would spin on "unknown → approve → still unknown".
    let store = InMemoryHostKeys::new();
    let key = ed25519();
    assert_eq!(
        decide(&store, &no_known_hosts(), HOST, PORT, &key),
        HostKeyDecision::Unknown
    );

    record_approval(&store, HOST, PORT, &key);

    assert_eq!(
        decide(&store, &no_known_hosts(), HOST, PORT, &key),
        HostKeyDecision::Trusted
    );
}

// ── The second-algorithm cell, which the pin is what makes safe ──────

#[test]
fn a_second_key_algorithm_is_not_a_changed_key() {
    // A healthy server holding both an ed25519 and an rsa host key may present
    // either. Crying man-in-the-middle here is how people learn to click through
    // the one warning that matters, so an algorithm we hold no entry for reads as
    // FIRST CONTACT for that algorithm.
    let store = InMemoryHostKeys::new().with_entry(HOST, PORT, "ssh-ed25519", &ed25519().fingerprint);
    assert_eq!(
        decide(&store, &no_known_hosts(), HOST, PORT, &rsa()),
        HostKeyDecision::Unknown
    );
}

#[test]
fn the_algorithms_offered_are_pinned_to_the_ones_already_trusted() {
    // The other half of the fix, and without it the cell above is a hole: an
    // attacker offering rsa where we hold an ed25519 entry would land on the
    // unknown path and collect a one-click approval. Pinning the negotiation to
    // what we already trust means a healthy server presents the key we stored,
    // and anything else is a real change.
    let store = InMemoryHostKeys::new().with_entry(HOST, PORT, "ssh-ed25519", &ed25519().fingerprint);
    assert_eq!(
        algorithms_to_pin(&store, &no_known_hosts(), HOST, PORT),
        vec!["ssh-ed25519".to_string()]
    );
}

#[test]
fn first_contact_pins_nothing() {
    // Nothing is trusted yet, so there's nothing to pin to and every algorithm
    // the transport would normally offer stays on the table.
    let store = InMemoryHostKeys::new();
    assert!(algorithms_to_pin(&store, &no_known_hosts(), HOST, PORT).is_empty());
}

#[test]
fn the_pin_covers_algorithms_known_only_from_known_hosts() {
    // A host trusted through `~/.ssh/known_hosts` alone still has to be pinned,
    // or the fallback re-opens the hole the store's pin closes.
    let store = InMemoryHostKeys::new();
    let known = KnownHostsFile::parse(&format!("[{HOST}]:{PORT} ssh-ed25519 {ED25519_BLOB}\n"));
    assert_eq!(
        algorithms_to_pin(&store, &known, HOST, PORT),
        vec!["ssh-ed25519".to_string()]
    );
}

// ── `~/.ssh/known_hosts`, read as a fallback and never written ───────

#[test]
fn a_key_the_users_known_hosts_already_holds_is_trusted() {
    let store = InMemoryHostKeys::new();
    let known = KnownHostsFile::parse(&format!("[{HOST}]:{PORT} ssh-ed25519 {ED25519_BLOB}\n"));
    assert_eq!(decide(&store, &known, HOST, PORT, &ed25519()), HostKeyDecision::Trusted);
}

#[test]
fn a_known_hosts_entry_holding_a_different_key_is_the_strongest_mitm_signal_there_is() {
    let store = InMemoryHostKeys::new();
    let known = KnownHostsFile::parse(&format!("[{HOST}]:{PORT} ssh-ed25519 {OTHER_ED25519_BLOB}\n"));
    assert_eq!(decide(&store, &known, HOST, PORT, &ed25519()), HostKeyDecision::Changed);
}

#[test]
fn a_revoked_key_is_never_a_match_however_it_is_reached() {
    // `@revoked` says "this key is known to be compromised". It must never read
    // as trusted, and it must never read as merely unknown either, which would
    // put a one-click approval in front of a key the user was warned about.
    let store = InMemoryHostKeys::new().with_entry(HOST, PORT, "ssh-ed25519", &ed25519().fingerprint);
    let known = KnownHostsFile::parse(&format!("@revoked [{HOST}]:{PORT} ssh-ed25519 {ED25519_BLOB}\n"));
    assert_eq!(decide(&store, &known, HOST, PORT, &ed25519()), HostKeyDecision::Revoked);
}

#[test]
fn a_cert_authority_line_is_recognized_rather_than_read_as_a_host_key() {
    // `@cert-authority` marks a CA that SIGNS host keys; the blob on the line is
    // not the host's own key. Misreading it as one turns every first connection
    // to a certificate-using host into a "the key changed" alarm.
    let store = InMemoryHostKeys::new();
    let known = KnownHostsFile::parse(&format!("@cert-authority *.{HOST} ssh-ed25519 {OTHER_ED25519_BLOB}\n"));
    assert_eq!(decide(&store, &known, HOST, PORT, &ed25519()), HostKeyDecision::Unknown);
}

#[test]
fn a_marker_line_does_not_take_the_rest_of_the_file_down_with_it() {
    // russh's own reader errors the whole lookup on a line it can't parse as
    // `host keytype blob`, so one `@cert-authority` line would make every host
    // in the file unreadable. Ours skips what it can't use and keeps going.
    let store = InMemoryHostKeys::new();
    let known = KnownHostsFile::parse(&format!(
        "@cert-authority *.example.com ssh-ed25519 {OTHER_ED25519_BLOB}\n\
         # a comment\n\
         \n\
         [{HOST}]:{PORT} ssh-ed25519 {ED25519_BLOB}\n"
    ));
    assert_eq!(decide(&store, &known, HOST, PORT, &ed25519()), HostKeyDecision::Trusted);
}

#[test]
fn a_port_22_host_is_matched_without_the_bracket_form() {
    // OpenSSH writes `host` for port 22 and `[host]:port` otherwise. Getting
    // this backwards means every default-port server reads as first contact.
    let store = InMemoryHostKeys::new();
    let known = KnownHostsFile::parse(&format!("{HOST} ssh-ed25519 {ED25519_BLOB}\n"));
    assert_eq!(decide(&store, &known, HOST, 22, &ed25519()), HostKeyDecision::Trusted);
}

#[test]
fn a_hashed_known_hosts_entry_still_matches() {
    // Debian and Ubuntu ship `HashKnownHosts yes`, and a file copied from such a
    // machine is entirely hashed. Not reading it means every host reads as first
    // contact, and a hashed `@revoked` line would go unseen.
    let store = InMemoryHostKeys::new();
    let known = KnownHostsFile::parse(
        "|1|O33ESRMWPVkMYIwJ1Uw+n877jTo=|nuuC5vEqXlEZ/8BXQR7m619W6Ak= ssh-ed25519 \
         AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF\n",
    );
    let key = PresentedHostKey::new(
        "ssh-ed25519",
        "AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF",
        "SHA256:dddddddddddddddddddddddddddddddddddddddddd",
    );
    assert_eq!(
        decide(&store, &known, "example.com", 22, &key),
        HostKeyDecision::Trusted
    );
}

#[test]
fn a_comma_separated_host_list_matches_each_of_its_hosts() {
    let store = InMemoryHostKeys::new();
    let known = KnownHostsFile::parse(&format!("other.example,{HOST} ssh-ed25519 {ED25519_BLOB}\n"));
    assert_eq!(decide(&store, &known, HOST, 22, &ed25519()), HostKeyDecision::Trusted);
}

// ── The detached host ────────────────────────────────────────────────

#[test]
fn a_detached_host_trusts_nothing() {
    // A bench, a tool, and half the suites run under `VolumeHost::detached()`. A
    // double that accepted any key is how a man-in-the-middle regression ships
    // green, so detached means trust-NOTHING, never trust-everything.
    let host = VolumeHost::detached();
    assert_eq!(
        decide(host.host_keys(), &no_known_hosts(), HOST, PORT, &ed25519()),
        HostKeyDecision::Unknown
    );

    // And recording against it changes nothing, which is exactly why a fixture
    // that has to complete an approval uses `InMemoryHostKeys` instead.
    record_approval(host.host_keys(), HOST, PORT, &ed25519());
    assert_eq!(
        decide(host.host_keys(), &no_known_hosts(), HOST, PORT, &ed25519()),
        HostKeyDecision::Unknown
    );
}
