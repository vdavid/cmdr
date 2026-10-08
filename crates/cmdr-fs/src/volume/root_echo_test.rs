//! Tests for [`super::root_echo`], the warning-side twin of `root_anchored`.

use super::root_echo;
use std::path::Path;

const SERVER_ROOT: &str = "sftp://ada@nas:22/srv/data";

#[test]
fn a_path_repeating_the_server_folder_is_flagged_with_both_readings() {
    // The trap from #164: the user types the full server path they know, and
    // anchoring glues it onto the root. Anchoring stays; the echo names what
    // happens and offers the stripped reading.
    let echo = root_echo(Path::new(SERVER_ROOT), Path::new("/srv/data/photos")).expect("an echo");
    assert_eq!(echo.root_folder, Path::new("/srv/data"));
    assert_eq!(echo.resolved, Path::new("/srv/data/srv/data/photos"));
    assert_eq!(echo.stripped, Path::new("/photos"));
}

#[test]
fn the_bare_server_folder_strips_to_the_place_root() {
    let echo = root_echo(Path::new(SERVER_ROOT), Path::new("/srv/data")).expect("an echo");
    assert_eq!(echo.resolved, Path::new("/srv/data/srv/data"));
    assert_eq!(echo.stripped, Path::new("/"));
}

#[test]
fn a_volume_relative_path_is_not_flagged() {
    assert!(root_echo(Path::new(SERVER_ROOT), Path::new("/photos")).is_none());
    assert!(root_echo(Path::new(SERVER_ROOT), Path::new("photos/2026")).is_none());
}

#[test]
fn a_sibling_folder_sharing_a_name_prefix_is_not_flagged() {
    // Whole components, the same rule `root_anchored` follows: `/srv/data-1` is
    // a legal folder name under the place, not a repeat of `/srv/data`.
    assert!(root_echo(Path::new(SERVER_ROOT), Path::new("/srv/data-1/x")).is_none());
}

#[test]
fn every_spelling_of_the_root_and_a_full_app_path_are_not_flagged() {
    for root_ish in ["", ".", "/"] {
        assert!(
            root_echo(Path::new(SERVER_ROOT), Path::new(root_ish)).is_none(),
            "{root_ish:?}"
        );
    }
    // A pane's own spelling passes through anchoring untouched, so it's unambiguous.
    assert!(
        root_echo(
            Path::new(SERVER_ROOT),
            Path::new("sftp://ada@nas:22/srv/data/srv/data/x")
        )
        .is_none()
    );
}

#[test]
fn a_place_rooted_at_the_server_root_never_echoes() {
    // Root `/` on the server: every path is volume-relative and server-absolute
    // at once, and both readings land in the same place.
    assert!(root_echo(Path::new("sftp://ada@nas:22/"), Path::new("/srv/data")).is_none());
    assert!(root_echo(Path::new("adb://R58M1"), Path::new("/sdcard")).is_none());
}

#[test]
fn a_mounted_volume_never_echoes() {
    // A local or OS-mounted root has no separate server folder: a path starting
    // with the mount passes through anchoring untouched.
    assert!(root_echo(Path::new("/Volumes/naspi"), Path::new("/Volumes/naspi/photos")).is_none());
    assert!(root_echo(Path::new("/"), Path::new("/Users/david")).is_none());
}

#[test]
fn a_trailing_slash_on_the_root_still_matches() {
    let echo = root_echo(Path::new("webdav://ada@nas:443/dav/files/"), Path::new("/dav/files/a")).expect("an echo");
    assert_eq!(echo.root_folder, Path::new("/dav/files"));
    assert_eq!(echo.stripped, Path::new("/a"));
}
