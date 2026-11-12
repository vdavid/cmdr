//! The path translation, on a volume rooted at a sub-collection.
//!
//! The rules themselves are `cmdr_fs::volume::remote_paths`' cells; these prove
//! this volume is wired to them, with its own prefix and its own root.

use std::path::Path;

use cmdr_fs::volume::VolumeError;

use super::super::test_support::make_test_volume;
use super::root_remote_path;

const ROOT: &str = "/srv/data";

/// The prefix this crate's test volume mints, spelled out so a cell reads as the
/// app would see it. `webdav://` whatever the transport is: `http` and `https` to
/// one host and port are the same server.
const PREFIX: &str = "webdav://test_user_1@127.0.0.1:1";

#[test]
fn every_spelling_of_the_root_is_the_root() {
    let volume = make_test_volume(ROOT);
    for spelling in ["/", "", "."] {
        assert_eq!(volume.to_remote_path(Path::new(spelling)).expect("the root"), ROOT);
    }
    assert_eq!(
        volume
            .to_remote_path(Path::new(&format!("{PREFIX}{ROOT}")))
            .expect("the root"),
        ROOT
    );
}

/// The two spellings the app uses land on one remote path, and the app-facing
/// one comes back out.
#[test]
fn a_prefixed_and_a_relative_path_land_on_the_same_remote_path_and_round_trip() {
    let volume = make_test_volume(ROOT);
    let relative = volume
        .to_remote_path(Path::new("photos/a b.jpg"))
        .expect("under the root");
    let prefixed = volume
        .to_remote_path(Path::new(&format!("{PREFIX}/srv/data/photos/a b.jpg")))
        .expect("under the root");
    assert_eq!(relative, "/srv/data/photos/a b.jpg");
    assert_eq!(prefixed, relative);
    assert_eq!(
        volume.display_path_for(Path::new("photos/a b.jpg")),
        Some(Path::new(&format!("{PREFIX}/srv/data/photos/a b.jpg")).to_path_buf()),
        "what the listing-cache patcher spells is what a pane holds"
    );
}

/// ❗ A bare server-absolute path is refused rather than anchored: with the
/// prefix in place the app never spells one, and accepting it is the hole
/// `root_anchored` would drive a doubled path through.
#[test]
fn a_bare_remote_path_is_refused() {
    let volume = make_test_volume(ROOT);
    for bare in ["/srv/data/photos", "/etc/passwd"] {
        assert!(
            matches!(volume.to_remote_path(Path::new(bare)), Err(VolumeError::NotFound(_))),
            "a path with no prefix names nothing on this volume: {bare}"
        );
    }
}

#[test]
fn a_sibling_that_shares_a_prefix_is_refused() {
    let volume = make_test_volume(ROOT);
    assert!(matches!(
        volume.to_remote_path(Path::new(&format!("{PREFIX}/srv/data-1/photos"))),
        Err(VolumeError::NotFound(_))
    ));
}

/// Another account on the same server is another volume, and its paths are not
/// this one's.
#[test]
fn another_accounts_prefix_is_refused() {
    let volume = make_test_volume(ROOT);
    assert!(matches!(
        volume.to_remote_path(Path::new("webdav://test_user_2@127.0.0.1:1/srv/data/photos")),
        Err(VolumeError::NotFound(_))
    ));
}

#[test]
fn a_dot_dot_escape_is_refused_however_it_is_spelled() {
    let volume = make_test_volume(ROOT);
    for escape in [
        "photos/../../etc".to_string(),
        format!("{PREFIX}/srv/data/../data-1"),
        "../data-1".to_string(),
    ] {
        assert!(
            matches!(volume.to_remote_path(Path::new(&escape)), Err(VolumeError::NotFound(_))),
            "{escape} must not resolve"
        );
    }
    assert_eq!(
        volume.to_remote_path(Path::new("photos/../docs")).expect("inside"),
        "/srv/data/docs"
    );
}

#[test]
fn the_root_of_a_volume_normalizes_to_one_spelling() {
    assert_eq!(root_remote_path(Path::new("/")), "/");
    assert_eq!(root_remote_path(Path::new("")), "/");
    assert_eq!(root_remote_path(Path::new(".")), "/");
    assert_eq!(root_remote_path(Path::new("Photos/")), "/Photos");
    assert_eq!(root_remote_path(Path::new("/Photos")), "/Photos");
}

/// ❗ **A name Cmdr creates goes out composed (NFC).** macOS hands out many local
/// names decomposed, and a WebDAV server on Linux stores the bytes the URL
/// spelled, where web servers, PHP, and scripts match bytes: a decomposed
/// `café.jpg` would look right and break every link to it. The engine respells a
/// new name through `Volume::spell_new_name`; this pins the answer it reads.
#[test]
fn a_new_name_goes_out_composed() {
    use cmdr_fs::volume::Volume;
    let volume = make_test_volume(ROOT);
    assert!(volume.composes_new_names());
    assert_eq!(volume.spell_new_name("cafe\u{301}.jpg"), "caf\u{e9}.jpg");
    assert_eq!(volume.spell_new_name("caf\u{e9}.jpg"), "caf\u{e9}.jpg");
}

/// ❗ **A path to an EXISTING entry keeps its exact bytes.** A read, a delete,
/// or an overwrite of a decomposed name the server holds must address THAT
/// name: a composed path would miss it, or hit a composed twin beside it.
#[test]
fn a_path_to_an_existing_entry_keeps_its_exact_bytes() {
    let volume = make_test_volume(ROOT);
    assert_eq!(
        volume
            .to_remote_path(Path::new("fo\u{301}to\u{301}k/cafe\u{301}.jpg"))
            .expect("under the root"),
        "/srv/data/fo\u{301}to\u{301}k/cafe\u{301}.jpg"
    );
}
