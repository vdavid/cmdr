//! The mode fold, on its own: what a landed file wears given what the source
//! reported and what the destination filesystem just created.
//!
//! A `#[path]` CHILD of `landed_mode.rs`, so `super::` is that module.

use super::*;
use cmdr_fs::testing::TestDir;
use std::os::unix::fs::PermissionsExt;

/// The reason the whole module exists, at the smallest scale: an executable
/// source lands executable under the umask everybody has.
#[test]
fn an_executable_source_lands_executable() {
    assert_eq!(landed_mode(0o755, 0o644), Some(0o755));
}

/// A plain source under the same umask asks for nothing: the file it landed on
/// already wears exactly those bits, so there is no `chmod` to spend.
#[test]
fn a_plain_source_matching_what_was_created_asks_for_no_change() {
    assert_eq!(landed_mode(0o644, 0o644), None);
}

/// A source with no mode to report — the `FileEntry::permissions` sentinel every
/// backend without a permission concept answers (SMB, SFTP, MTP, WebDAV, and any
/// archive entry that recorded none) — leaves the destination alone. ❌ Never a
/// `chmod` to `0`, which would land a file the user can't open.
#[test]
fn a_source_with_no_permission_concept_leaves_the_destination_alone() {
    assert_eq!(landed_mode(0, 0o644), None);
}

/// The umask is honored because it is already baked into what the destination
/// created. Under `077` a new file arrives `0o600`, and an executable source
/// lands `0o700` — exactly what `git checkout` of the same blob would produce,
/// ❌ never a group- or world-readable `0o755`.
#[test]
fn a_strict_umask_narrows_what_lands_the_way_a_checkout_would() {
    assert_eq!(landed_mode(0o755, 0o600), Some(0o700));
    assert_eq!(
        landed_mode(0o644, 0o600),
        None,
        "the read bits it can't have are dropped"
    );
}

/// A group-writable umask (`002`) doesn't stop a `0o755` source from landing
/// exactly `0o755`: the source said what it wanted, and the umask only ever
/// takes bits away.
#[test]
fn a_permissive_umask_still_lands_the_mode_the_source_named() {
    assert_eq!(landed_mode(0o755, 0o664), Some(0o755));
    assert_eq!(landed_mode(0o664, 0o664), None);
}

/// The Windows-zip case. rc-zip turns a DOS creator's attributes into a `0o666`
/// stand-in that is a read-only flag wearing a mode's clothes; folding it
/// through what was created lands the plain `0o644` a new file would have had
/// anyway, so a zip made on Windows never widens anything.
#[test]
fn the_stand_in_mode_a_windows_zip_reports_lands_where_a_plain_new_file_would() {
    assert_eq!(landed_mode(0o666, 0o644), None);
    assert_eq!(landed_mode(0o666, 0o600), None);
}

/// A private source stays private: the fold narrows as readily as it widens,
/// which is what makes it a copy of the mode rather than an executable-bit
/// patch.
#[test]
fn a_private_source_lands_private() {
    assert_eq!(landed_mode(0o600, 0o644), Some(0o600));
}

/// setuid, setgid, and sticky are dropped before the fold. An archive or a repo
/// object is untrusted input, and neither is authority enough to hand a landed
/// file one of those bits.
#[test]
fn setuid_setgid_and_sticky_never_travel() {
    assert_eq!(landed_mode(0o4755, 0o644), Some(0o755));
    assert_eq!(landed_mode(0o2755, 0o644), Some(0o755));
    assert_eq!(landed_mode(0o1777, 0o644), Some(0o755));
}

/// A local source's `FileEntry::permissions` is the FULL `st_mode`, file-type
/// bits and all (`0o100755` for a regular executable). Only the permission bits
/// may reach a `chmod`.
#[test]
fn the_file_type_bits_a_local_stat_carries_are_masked_off() {
    assert_eq!(landed_mode(0o100_755, 0o644), Some(0o755));
}

/// End to end against a real file: the bit actually lands, and the fold is what
/// decided it.
#[test]
fn applying_a_mode_to_a_real_file_sets_the_bits_the_fold_chose() {
    let dir = TestDir::new("landed-mode");
    let path = dir.join("run.sh");
    std::fs::write(&path, b"#!/bin/sh\n").expect("write the file");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod 644");

    apply_mode_to_local_file(&path, 0o755);

    let landed = std::fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
    assert_eq!(landed, 0o755, "the executable bit landed");
}

/// A path that isn't there is a debug line, ❌ never a panic: this runs after
/// the bytes are safe, and nothing it does may cost a landed copy.
#[test]
fn a_missing_path_is_survived_quietly() {
    let dir = TestDir::new("landed-mode-missing");
    apply_mode_to_local_file(&dir.join("gone.sh"), 0o755);
}
