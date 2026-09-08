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

// ── Mounts anchored INSIDE the share ──────────────────────────────────────────
//
// macOS follows a DFS referral by mounting the target underneath the namespace
// root, and a subdirectory mount looks the same: the mount is a directory inside
// the share, not the share root. The share is what TreeConnect gets; the anchor
// is what every path on the wire has to be joined onto. Getting this wrong is the
// "real request at a real, wrong place" failure this whole module guards against,
// so both directions are pinned. Reported as ERR-48RZX.

#[test]
fn an_anchored_mount_joins_its_share_root_onto_the_wire_path() {
    let vol = make_test_volume_anchored("lgs-net.com", "/Volumes/SYSVOL/lgs-net.com");
    assert_eq!(
        vol.to_smb_path(Path::new("/Volumes/SYSVOL/lgs-net.com/Policies"))
            .unwrap(),
        "lgs-net.com/Policies"
    );
    assert_eq!(
        vol.to_smb_path(Path::new("/Volumes/SYSVOL/lgs-net.com/Policies/GPT.INI"))
            .unwrap(),
        "lgs-net.com/Policies/GPT.INI"
    );
}

#[test]
fn an_anchored_mount_root_is_the_share_root_itself() {
    // The mount root is not the share root: asking for "" would list the whole
    // share instead of the directory the pane is actually showing.
    let vol = make_test_volume_anchored("lgs-net.com", "/Volumes/SYSVOL/lgs-net.com");
    assert_eq!(
        vol.to_smb_path(Path::new("/Volumes/SYSVOL/lgs-net.com")).unwrap(),
        "lgs-net.com"
    );
    assert_eq!(vol.to_smb_path(Path::new("/")).unwrap(), "lgs-net.com");
    assert_eq!(vol.to_smb_path(Path::new("")).unwrap(), "lgs-net.com");
}

#[test]
fn an_anchored_mount_joins_relative_paths_too() {
    // The trait contract's relative form is relative to the VOLUME root, which on
    // an anchored mount is already inside the share.
    let vol = make_test_volume_anchored("photos/2026", "/Volumes/2026");
    assert_eq!(
        vol.to_smb_path(Path::new("June/IMG_1.jpg")).unwrap(),
        "photos/2026/June/IMG_1.jpg"
    );
}

#[test]
fn an_anchored_mount_still_rejects_a_path_outside_its_mount() {
    // The anchor must not become a way to reach the rest of the share: a path
    // that isn't under this mount is still `NotFound`, not a join.
    let vol = make_test_volume_anchored("lgs-net.com", "/Volumes/SYSVOL/lgs-net.com");
    for outside in ["/Volumes/SYSVOL/other", "/Volumes/SYSVOL", "/Users/andrew/notes.txt"] {
        assert!(
            matches!(vol.to_smb_path(Path::new(outside)), Err(VolumeError::NotFound(_))),
            "{outside} is not on this mount"
        );
    }
}

#[test]
fn an_anchored_mount_strips_its_share_root_back_off_for_display() {
    // The inverse of `to_smb_path`: a watcher event or an error message names a
    // share-relative path, and the pane only knows mount-relative ones.
    let vol = make_test_volume_anchored("lgs-net.com", "/Volumes/SYSVOL/lgs-net.com");
    assert_eq!(vol.to_display_path("lgs-net.com"), "/Volumes/SYSVOL/lgs-net.com");
    assert_eq!(
        vol.to_display_path("lgs-net.com/Policies/GPT.INI"),
        "/Volumes/SYSVOL/lgs-net.com/Policies/GPT.INI"
    );
}

#[test]
fn the_two_directions_round_trip_on_an_anchored_mount() {
    // The pair has to compose, or a mutation patches a listing-cache key nothing
    // is watching and the pane goes stale after a write that worked.
    let vol = make_test_volume_anchored("photos/2026", "/Volumes/2026");
    for display in ["/Volumes/2026", "/Volumes/2026/June", "/Volumes/2026/June/IMG_1.jpg"] {
        let wire = vol.to_smb_path(Path::new(display)).expect("on this mount");
        assert_eq!(vol.to_display_path(&wire), display, "round trip through {wire}");
    }
}

#[test]
fn a_share_root_anchor_never_matches_a_sibling_by_name_prefix() {
    // The same whole-component rule the mount root gets: `lgs-net.com.old` starts
    // with `lgs-net.com` as a string, and stripping it would name a path on a
    // directory the user never mounted.
    let vol = make_test_volume_anchored("lgs-net.com", "/Volumes/SYSVOL/lgs-net.com");
    assert_eq!(
        vol.to_display_path("lgs-net.com.old/GPT.INI"),
        "/Volumes/SYSVOL/lgs-net.com/lgs-net.com.old/GPT.INI",
        "an unrelated share-relative path stays under the mount rather than being mis-stripped"
    );
}
