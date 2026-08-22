//! Turning the paths the app addresses this volume with into remote paths.
//!
//! Every cell pins a way of guessing that would send a real request to a real,
//! wrong place.

use super::*;
use crate::volume::test_support::*;
use cmdr_fs::volume::Volume;

#[test]
fn the_three_spellings_of_the_root_all_mean_the_root() {
    let vol = make_test_volume();
    assert_eq!(vol.to_remote_path(Path::new("")).unwrap(), "/srv/data");
    assert_eq!(vol.to_remote_path(Path::new(".")).unwrap(), "/srv/data");
    assert_eq!(vol.to_remote_path(Path::new("/")).unwrap(), "/srv/data");
}

#[test]
fn a_relative_path_hangs_off_the_root() {
    let vol = make_test_volume();
    assert_eq!(vol.to_remote_path(Path::new("photos")).unwrap(), "/srv/data/photos");
    assert_eq!(
        vol.to_remote_path(Path::new("photos/2026/trip.jpg")).unwrap(),
        "/srv/data/photos/2026/trip.jpg"
    );
}

#[test]
fn an_absolute_path_inside_the_root_is_already_the_remote_path() {
    let vol = make_test_volume();
    assert_eq!(
        vol.to_remote_path(Path::new("/srv/data/photos")).unwrap(),
        "/srv/data/photos"
    );
    assert_eq!(vol.to_remote_path(Path::new("/srv/data")).unwrap(), "/srv/data");
}

#[test]
fn a_path_outside_the_root_is_refused_rather_than_anchored() {
    // ❗ The whole reason this doesn't use `cmdr_fs::volume::root_anchored`:
    // anchoring turns `/etc/passwd` into `/srv/data/etc/passwd`, which is a real
    // path on a real server and quietly the wrong one. Refusing says so.
    let vol = make_test_volume();
    assert!(matches!(
        vol.to_remote_path(Path::new("/etc/passwd")),
        Err(VolumeError::NotFound(_))
    ));
    assert!(matches!(
        vol.to_remote_path(Path::new("/srv")),
        Err(VolumeError::NotFound(_))
    ));
}

#[test]
fn the_root_is_matched_by_whole_components() {
    // The trap: a sibling directory whose name merely starts with the root's.
    // A raw string prefix compare strips `/srv/data` off `/srv/data-1/photos`
    // and sends the server `-1/photos`, which is a real name on a real server.
    let vol = make_test_volume();
    assert!(matches!(
        vol.to_remote_path(Path::new("/srv/data-1/photos")),
        Err(VolumeError::NotFound(_))
    ));
    assert!(matches!(
        vol.to_remote_path(Path::new("/srv/dataX")),
        Err(VolumeError::NotFound(_))
    ));
}

#[test]
fn a_volume_rooted_at_the_server_root_reaches_everything() {
    // Rooting at `/` is a legitimate choice, and then every absolute path is
    // inside the root by definition.
    let vol = make_test_volume_at("/");
    assert_eq!(vol.to_remote_path(Path::new("/etc/passwd")).unwrap(), "/etc/passwd");
    assert_eq!(vol.to_remote_path(Path::new("etc/passwd")).unwrap(), "/etc/passwd");
    assert_eq!(vol.to_remote_path(Path::new("/")).unwrap(), "/");
}

#[test]
fn a_relative_path_cannot_climb_out_of_the_root() {
    // `..` in a relative path is the same escape by another spelling, and the
    // server would happily resolve it.
    let vol = make_test_volume();
    assert!(matches!(
        vol.to_remote_path(Path::new("../secrets")),
        Err(VolumeError::NotFound(_))
    ));
    assert!(matches!(
        vol.to_remote_path(Path::new("photos/../../secrets")),
        Err(VolumeError::NotFound(_))
    ));
}

#[test]
fn the_volume_root_is_what_the_trait_reports() {
    let vol = make_test_volume();
    assert_eq!(vol.root(), Path::new(TEST_ROOT));
}
