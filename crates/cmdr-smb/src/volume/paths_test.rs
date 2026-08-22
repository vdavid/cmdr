//! Path translation in both directions, for a share with no session behind it.
//!
//! Every case here is a pure function over the mount root, and each one pins a
//! way of guessing that once sent a real request to a real, wrong place.

use super::*;
use crate::volume::test_support::*;
use cmdr_fs::volume::Volume;

#[test]
fn to_smb_path_empty() {
    let vol = make_test_volume();
    assert_eq!(vol.to_smb_path(Path::new("")).unwrap(), "");
    assert_eq!(vol.to_smb_path(Path::new("/")).unwrap(), "");
    assert_eq!(vol.to_smb_path(Path::new(".")).unwrap(), "");
}

#[test]
fn to_smb_path_relative() {
    let vol = make_test_volume();
    assert_eq!(vol.to_smb_path(Path::new("Documents")).unwrap(), "Documents");
    assert_eq!(
        vol.to_smb_path(Path::new("Documents/report.pdf")).unwrap(),
        "Documents/report.pdf"
    );
}

#[test]
fn to_smb_path_absolute_under_mount() {
    let vol = make_test_volume();
    assert_eq!(
        vol.to_smb_path(Path::new("/Volumes/TestShare/Documents")).unwrap(),
        "Documents"
    );
    assert_eq!(
        vol.to_smb_path(Path::new("/Volumes/TestShare/Documents/report.pdf"))
            .unwrap(),
        "Documents/report.pdf"
    );
}

#[test]
fn to_smb_path_mount_root() {
    let vol = make_test_volume();
    assert_eq!(vol.to_smb_path(Path::new("/Volumes/TestShare")).unwrap(), "");
}

#[test]
fn to_smb_path_rejects_a_sibling_mount_that_merely_shares_a_name_prefix() {
    // macOS mounts a second copy of a share at `/Volumes/TestShare-1`. A raw
    // string prefix compare strips `/Volumes/TestShare` off that path and sends
    // the server the share-relative `-1/Documents`, which is a real file name
    // on the share. Matching whole path components is the only safe compare.
    let vol = make_test_volume();
    assert!(matches!(
        vol.to_smb_path(Path::new("/Volumes/TestShare-1/Documents")),
        Err(VolumeError::NotFound(_))
    ));
    assert!(matches!(
        vol.to_smb_path(Path::new("/Volumes/TestShareX")),
        Err(VolumeError::NotFound(_))
    ));
}

#[test]
fn a_root_anchored_dialog_destination_reaches_the_share() {
    // The transfer dialog's destination box is volume-relative (`/_todo_pics`),
    // and this backend answers `NotFound` for an absolute path outside the
    // mount, on purpose. Anchoring at the IPC boundary is what closes that gap:
    // a move into an SMB subfolder died in 2 ms without it (ERR-XCP5Q).
    let vol = make_test_volume();
    let anchored = cmdr_fs::volume::root_anchored(vol.root(), Path::new("/_todo_pics/Fiumei footage"));
    assert_eq!(
        vol.to_smb_path(&anchored).unwrap(),
        "_todo_pics/Fiumei footage",
        "an anchored destination converts to the share-relative wire path"
    );
}

#[test]
fn to_smb_path_rejects_a_path_outside_the_mount() {
    // Falling back to "strip the leading slash" turned `/Volumes/Other/x` into
    // the share-relative `Volumes/Other/x` and asked the server for it. A path
    // that isn't on this volume has to say so instead of guessing.
    let vol = make_test_volume();
    for outside in ["/Volumes/Other/x", "/Users/david/notes.txt", "/Volumes"] {
        assert!(
            matches!(vol.to_smb_path(Path::new(outside)), Err(VolumeError::NotFound(_))),
            "{outside} is not on this share"
        );
    }
}

#[test]
fn to_display_path_empty_is_mount_root() {
    let vol = make_test_volume();
    assert_eq!(vol.to_display_path(""), "/Volumes/TestShare");
}

#[test]
fn to_display_path_with_subpath() {
    let vol = make_test_volume();
    assert_eq!(
        vol.to_display_path("Documents/report.pdf"),
        "/Volumes/TestShare/Documents/report.pdf"
    );
}
