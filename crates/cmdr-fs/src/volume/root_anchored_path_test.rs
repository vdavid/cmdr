//! Tests for [`super::root_anchored`], the one rule that turns a caller's path
//! into the form every backend accepts.

use super::root_anchored;
use std::path::{Path, PathBuf};

fn anchor(root: &str, path: &str) -> PathBuf {
    root_anchored(Path::new(root), Path::new(path))
}

#[test]
fn a_volume_relative_path_gets_the_root_in_front_of_it() {
    // The transfer dialog's destination box is volume-relative with a leading
    // slash. Handing `/_todo_pics` to a share mounted at `/Volumes/naspi`
    // without anchoring it is what made a 360 GB move die instantly:
    // `SmbVolume::to_smb_path` reads it as absolute, finds it outside the
    // mount, and answers `NotFound` before any I/O.
    assert_eq!(
        anchor("/Volumes/naspi", "/_todo_pics"),
        Path::new("/Volumes/naspi/_todo_pics")
    );
    assert_eq!(
        anchor("/Volumes/naspi", "/_todo_pics/Fiumei footage"),
        Path::new("/Volumes/naspi/_todo_pics/Fiumei footage")
    );
}

#[test]
fn a_path_with_no_leading_slash_gets_the_root_too() {
    assert_eq!(anchor("/Volumes/naspi", "photos"), Path::new("/Volumes/naspi/photos"));
    assert_eq!(
        anchor("/Volumes/naspi", "photos/2026"),
        Path::new("/Volumes/naspi/photos/2026")
    );
}

#[test]
fn a_path_already_under_the_root_is_left_alone() {
    // Idempotence is what lets every call site anchor without checking first:
    // the panes send absolute paths, the dialog sends volume-relative ones, and
    // anchoring twice must not double the prefix.
    let once = anchor("/Volumes/naspi", "/Volumes/naspi/photos");
    assert_eq!(once, Path::new("/Volumes/naspi/photos"));
    assert_eq!(root_anchored(Path::new("/Volumes/naspi"), &once), once);
}

#[test]
fn a_sibling_mount_sharing_a_name_prefix_is_not_under_the_root() {
    // macOS mounts a second copy of a share at `/Volumes/naspi-1`, and `-1` is a
    // legal file name. A raw string compare would call this path "already under
    // the root" and address the wrong file; whole-component matching anchors it
    // instead. Same rule `SmbVolume::to_smb_path` follows.
    assert_eq!(
        anchor("/Volumes/naspi", "/Volumes/naspi-1/x"),
        Path::new("/Volumes/naspi/Volumes/naspi-1/x")
    );
}

#[test]
fn the_root_volume_keeps_every_absolute_path_as_it_is() {
    // Root `/` contains every absolute path, so the "already under the root"
    // arm covers the whole local filesystem: no re-rooting, no `//` doubling.
    assert_eq!(
        anchor("/", "/Users/david/notes.txt"),
        Path::new("/Users/david/notes.txt")
    );
    assert_eq!(anchor("/", "/"), Path::new("/"));
    assert_eq!(anchor("/", "Users/david"), Path::new("/Users/david"));
}

#[test]
fn the_volume_root_itself_is_the_answer_for_every_spelling_of_it() {
    for root_ish in ["", "/", "."] {
        assert_eq!(
            anchor("/Volumes/naspi", root_ish),
            Path::new("/Volumes/naspi"),
            "{root_ish:?} means the volume root"
        );
    }
}

#[test]
fn a_scheme_shaped_root_anchors_the_same_way() {
    // An MTP volume's whole path vocabulary is `mtp://device/storage/…`, which
    // `Path::is_absolute` calls relative. The dialog still sends `/DCIM`, and
    // the device only ever sees the inner path, so anchoring has to produce the
    // URL form rather than a bare `/DCIM`.
    assert_eq!(
        anchor("mtp://mtp-0-1/65537", "/DCIM/Camera"),
        Path::new("mtp://mtp-0-1/65537/DCIM/Camera")
    );
    assert_eq!(
        anchor("mtp://mtp-0-1/65537", "mtp://mtp-0-1/65537/DCIM"),
        Path::new("mtp://mtp-0-1/65537/DCIM")
    );
}
