//! Turning the paths the app addresses this volume with into remote paths.
//!
//! The rules themselves are `cmdr_fs::volume::remote_paths`' cells; these prove
//! this volume is wired to them, with its own prefix and its own root.

use super::*;
use crate::volume::test_support::*;
use cmdr_fs::volume::Volume;

/// The prefix this crate's test volume mints, spelled out so a cell reads as the
/// app would see it.
const PREFIX: &str = "sftp://ada@127.0.0.1:12599";

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

/// The two spellings the app uses land on one server path, and the app-facing
/// one comes back out.
#[test]
fn a_prefixed_and_a_relative_path_land_on_the_same_server_path_and_round_trip() {
    let vol = make_test_volume();
    let prefixed = vol
        .to_remote_path(Path::new(&format!("{PREFIX}/srv/data/photos")))
        .expect("a path under this volume's own prefix is on this volume");
    assert_eq!(prefixed, "/srv/data/photos");
    assert_eq!(prefixed, vol.to_remote_path(Path::new("photos")).unwrap());
    assert_eq!(
        vol.display_path_for(Path::new("photos")),
        Some(PathBuf::from(format!("{PREFIX}/srv/data/photos"))),
        "what the listing-cache patcher spells is what a pane holds"
    );
}

/// ❗ **A bare server-absolute path is refused, not anchored.**
///
/// Anchoring turns `/etc/passwd` into `/srv/data/etc/passwd`, which is a real
/// path on a real server and quietly the wrong one. And a bare path that IS
/// under the root buys only the `root_anchored` hole: five app sites join
/// anything not under the root onto it, so `/srv/data/x` would arrive doubled
/// and strip back to a real, wrong server path.
#[test]
fn a_bare_server_absolute_path_is_refused() {
    let vol = make_test_volume();
    for bare in ["/etc/passwd", "/srv", "/srv/data/photos"] {
        assert!(
            matches!(vol.to_remote_path(Path::new(bare)), Err(VolumeError::NotFound(_))),
            "a path with no prefix names nothing on this volume: {bare}"
        );
    }
}

#[test]
fn a_prefixed_path_outside_the_root_is_refused() {
    let vol = make_test_volume();
    assert!(matches!(
        vol.to_remote_path(Path::new(&format!("{PREFIX}/etc/passwd"))),
        Err(VolumeError::NotFound(_))
    ));
}

/// Another account on the same server is another volume, and its paths are not
/// this one's.
#[test]
fn another_accounts_prefix_is_refused() {
    let vol = make_test_volume();
    assert!(matches!(
        vol.to_remote_path(Path::new("sftp://grace@127.0.0.1:12599/srv/data/photos")),
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
        vol.to_remote_path(Path::new(&format!("{PREFIX}/srv/data-1/photos"))),
        Err(VolumeError::NotFound(_))
    ));
    assert!(matches!(
        vol.to_remote_path(Path::new(&format!("{PREFIX}/srv/dataX"))),
        Err(VolumeError::NotFound(_))
    ));
}

#[test]
fn a_volume_rooted_at_the_server_root_reaches_everything() {
    // Rooting at `/` is a legitimate choice, and then every path under the
    // prefix is inside the root by definition.
    let vol = make_test_volume_at("/");
    assert_eq!(
        vol.to_remote_path(Path::new(&format!("{PREFIX}/etc/passwd"))).unwrap(),
        "/etc/passwd"
    );
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

/// ❗ The root the app sees carries the scheme, which is what makes a restored
/// tab, a favorite, and a copied path resolve back to this server rather than to
/// the boot disk.
#[test]
fn the_volume_root_is_the_app_spelling() {
    let vol = make_test_volume();
    assert_eq!(vol.root(), Path::new(&format!("{PREFIX}{TEST_ROOT}")));
}
