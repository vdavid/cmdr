//! What a `known_hosts` file yields, line shape by line shape.
//!
//! The trust decisions built on this live in `trust_test.rs`; these cells pin the
//! reading itself, including the two marker forms whose whole point is that
//! misreading them is invisible.

use super::*;

const BLOB: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
const OTHER_BLOB: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X";

#[test]
fn an_absent_file_says_nothing_rather_than_failing() {
    let file = KnownHostsFile::read_path(Path::new("/nonexistent/known_hosts"));
    assert_eq!(
        file.lookup("testnas", 22, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Unknown
    );
}

#[test]
fn comments_and_blank_lines_are_skipped() {
    let file = KnownHostsFile::parse(&format!("# header\n\n   \ntestnas ssh-ed25519 {BLOB}\n"));
    assert_eq!(
        file.lookup("testnas", 22, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Matches
    );
}

#[test]
fn a_truncated_line_is_skipped_rather_than_panicking() {
    let file = KnownHostsFile::parse("testnas ssh-ed25519\ntestnas\n@revoked\n");
    assert_eq!(
        file.lookup("testnas", 22, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Unknown
    );
}

#[test]
fn an_unrecognized_marker_is_skipped_rather_than_read_as_a_hostname() {
    // A future OpenSSH marker would otherwise land in the hostname field and the
    // key type in the pattern list, which reads as a host key for a host called
    // `@something`. Skipping costs one approval; guessing costs correctness.
    let file = KnownHostsFile::parse(&format!("@future-marker testnas ssh-ed25519 {BLOB}\n"));
    assert_eq!(
        file.lookup("testnas", 22, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Unknown
    );
}

#[test]
fn a_second_entry_for_the_same_host_can_still_match() {
    // OpenSSH allows several keys per host and treats any match as a match. A
    // reader that stopped at the first non-matching line would cry MITM on a
    // server that had simply rotated in a new key alongside the old one.
    let file = KnownHostsFile::parse(&format!(
        "testnas ssh-ed25519 {OTHER_BLOB}\ntestnas ssh-ed25519 {BLOB}\n"
    ));
    assert_eq!(
        file.lookup("testnas", 22, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Matches
    );
}

#[test]
fn a_glob_pattern_is_never_expanded() {
    // OpenSSH allows `*` and `?` here. Expanding them subtly wrong would either
    // trust a host we shouldn't or alarm on one we should, and an exact match is
    // always safe: the worst it costs is one extra approval.
    let file = KnownHostsFile::parse(&format!("*.example.com ssh-ed25519 {BLOB}\n"));
    assert_eq!(
        file.lookup("host.example.com", 22, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Unknown
    );
}

#[test]
fn a_non_default_port_is_only_matched_in_the_bracket_form() {
    let file = KnownHostsFile::parse(&format!("testnas ssh-ed25519 {BLOB}\n"));
    assert_eq!(
        file.lookup("testnas", 2222, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Unknown,
        "a bare hostname line is a port-22 entry; reading it as any port would trust the wrong server"
    );

    let bracketed = KnownHostsFile::parse(&format!("[testnas]:2222 ssh-ed25519 {BLOB}\n"));
    assert_eq!(
        bracketed.lookup("testnas", 2222, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Matches
    );
}

#[test]
fn algorithms_for_lists_plain_entries_only() {
    // The pin must not be widened by a CA entry: the blob on a
    // `@cert-authority` line isn't a host key, so its type isn't one we trust.
    let file = KnownHostsFile::parse(&format!(
        "testnas ssh-ed25519 {BLOB}\ntestnas ssh-rsa {OTHER_BLOB}\n@cert-authority testnas ssh-dss {BLOB}\n"
    ));
    assert_eq!(
        file.algorithms_for("testnas", 22),
        vec!["ssh-ed25519".to_string(), "ssh-rsa".to_string()]
    );
}

#[test]
fn a_revoked_entry_for_a_different_key_leaves_this_one_alone() {
    // Revocation names one key, not a host. Reading it as "this host is
    // revoked" would lock a user out of a server that rotated correctly.
    let file = KnownHostsFile::parse(&format!(
        "@revoked testnas ssh-ed25519 {OTHER_BLOB}\ntestnas ssh-ed25519 {BLOB}\n"
    ));
    assert_eq!(
        file.lookup("testnas", 22, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Matches
    );
}

#[test]
fn a_wildcard_revocation_names_the_key_on_every_host() {
    // `@revoked * ssh-ed25519 AAAA…` is the `sshd(8)` way to revoke a
    // compromised key everywhere. A revocation names a key, not a host, so its
    // patterns can't hide it, even though plain entries never expand a glob.
    let file = KnownHostsFile::parse(&format!("@revoked * ssh-ed25519 {BLOB}\n"));
    assert_eq!(
        file.lookup("testnas", 22, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Revoked
    );
    assert_eq!(
        file.lookup("testnas", 2222, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Revoked
    );
}

#[test]
fn a_revocation_written_for_another_host_still_revokes_the_key() {
    // A compromised key is compromised wherever it turns up: failing closed costs
    // one refused connection, failing open hands a man in the middle the session.
    let file = KnownHostsFile::parse(&format!(
        "@revoked other.example.com ssh-ed25519 {BLOB}\ntestnas ssh-ed25519 {BLOB}\n"
    ));
    assert_eq!(
        file.lookup("testnas", 22, "ssh-ed25519", BLOB),
        KnownHostsVerdict::Revoked
    );
}
