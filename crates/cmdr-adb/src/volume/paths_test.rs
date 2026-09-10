//! Turning the paths the app addresses this volume with into device paths.
//!
//! The shared rules are `cmdr_fs::volume::remote_paths`' cells; these prove this
//! volume is wired to them with its own prefix, and pin the one refusal that is
//! this backend's own (a `..` above the root).

use std::path::{Path, PathBuf};

use cmdr_fs::volume::{VolumeError, root_anchored};

use super::join_device_path;
use crate::volume::testing::{FIXTURE_SERIAL, detached_volume};

/// The prefix the fixture volume mints, spelled out so a cell reads as the app
/// would see it.
fn prefix() -> String {
    format!("adb://{FIXTURE_SERIAL}")
}

#[test]
fn every_spelling_of_the_root_is_the_root() {
    let volume = detached_volume();
    for spelling in [
        String::new(),
        ".".into(),
        "/".into(),
        prefix(),
        format!("{}/", prefix()),
    ] {
        assert_eq!(
            volume.to_device_path(Path::new(&spelling)).unwrap(),
            "/",
            "{spelling:?}"
        );
    }
}

#[test]
fn a_prefixed_and_a_relative_path_land_on_the_same_device_path() {
    let volume = detached_volume();
    let prefixed = format!("{}/sdcard/DCIM", prefix());
    assert_eq!(volume.to_device_path(Path::new(&prefixed)).unwrap(), "/sdcard/DCIM");
    assert_eq!(volume.to_device_path(Path::new("sdcard/DCIM")).unwrap(), "/sdcard/DCIM");
    assert_eq!(
        volume.to_device_path(Path::new(&format!("{prefixed}/"))).unwrap(),
        "/sdcard/DCIM"
    );
}

/// What a listing hands out is what a pane passes back, so the two directions
/// have to agree, root included.
#[test]
fn the_app_spelling_round_trips_and_the_root_has_no_trailing_slash() {
    let volume = detached_volume();
    assert_eq!(volume.to_app_path("/"), PathBuf::from(prefix()));
    assert_eq!(
        volume.to_app_path("/sdcard/DCIM"),
        PathBuf::from(format!("{}/sdcard/DCIM", prefix()))
    );
    assert_eq!(
        volume.display_path_for(Path::new("sdcard/DCIM")),
        Some(PathBuf::from(format!("{}/sdcard/DCIM", prefix())))
    );
}

/// ❗ **The transfer dialog's box is volume-relative** (`/sdcard`), and five app
/// sites run it through `root_anchored` against `Volume::root` before the volume
/// sees it. It has to land on the device's `/sdcard`, and a pane's already
/// prefixed path has to come through unchanged.
#[test]
fn a_dialog_path_anchored_at_the_root_lands_on_the_device_path() {
    let volume = detached_volume();
    let root = cmdr_fs::volume::Volume::root(&volume);
    let anchored = root_anchored(root, Path::new("/sdcard"));
    assert_eq!(anchored, PathBuf::from(format!("{}/sdcard", prefix())));
    assert_eq!(volume.to_device_path(&anchored).unwrap(), "/sdcard");
    let pane = PathBuf::from(format!("{}/sdcard", prefix()));
    assert_eq!(root_anchored(root, &pane), pane);
}

#[test]
fn dot_dot_is_resolved_lexically() {
    let volume = detached_volume();
    assert_eq!(
        volume
            .to_device_path(Path::new(&format!("{}/sdcard/DCIM/../Pictures", prefix())))
            .unwrap(),
        "/sdcard/Pictures"
    );
    assert_eq!(
        volume
            .to_device_path(Path::new(&format!("{}/sdcard/..", prefix())))
            .unwrap(),
        "/"
    );
}

#[test]
fn a_path_climbing_above_the_root_is_refused_not_anchored() {
    let volume = detached_volume();
    let prefixed_escape = format!("{}/sdcard/../../etc/passwd", prefix());
    for escape in ["..", "../etc", "sdcard/../../etc", prefixed_escape.as_str()] {
        let outcome = volume.to_device_path(Path::new(escape));
        assert!(
            matches!(outcome, Err(VolumeError::NotFound(ref p)) if p == escape),
            "{escape}: {outcome:?}"
        );
    }
}

/// A bare `/sdcard` reaching the volume means a caller skipped `root_anchored`,
/// and anchoring it here would hide that. Another device's path is never ours.
#[test]
fn a_bare_device_path_and_another_devices_path_are_refused() {
    let volume = detached_volume();
    for foreign in [
        "/sdcard/DCIM",
        "/etc/passwd",
        "adb://SOMEONE-ELSE/sdcard",
        "mtp://1-2/65537",
    ] {
        assert!(
            matches!(volume.to_device_path(Path::new(foreign)), Err(VolumeError::NotFound(_))),
            "{foreign}"
        );
    }
    let longer_serial = format!("{}0/sdcard", prefix());
    assert!(
        volume.to_device_path(Path::new(&longer_serial)).is_err(),
        "a serial that merely starts with ours is another device"
    );
}

#[test]
fn joining_never_doubles_the_root_slash() {
    assert_eq!(join_device_path("/", "sdcard"), "/sdcard");
    assert_eq!(join_device_path("/sdcard", "DCIM"), "/sdcard/DCIM");
}
