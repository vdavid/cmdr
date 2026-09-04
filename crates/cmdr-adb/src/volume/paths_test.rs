use std::path::Path;

use cmdr_fs::volume::VolumeError;

use super::join_device_path;
use crate::volume::testing::detached_volume;

#[test]
fn every_spelling_of_the_root_is_the_root() {
    let volume = detached_volume();
    for spelling in ["", ".", "/", "//", "/./"] {
        assert_eq!(volume.to_device_path(Path::new(spelling)).unwrap(), "/", "{spelling:?}");
    }
}

#[test]
fn anchoring_is_idempotent() {
    let volume = detached_volume();
    assert_eq!(volume.to_device_path(Path::new("sdcard/DCIM")).unwrap(), "/sdcard/DCIM");
    assert_eq!(
        volume.to_device_path(Path::new("/sdcard/DCIM")).unwrap(),
        "/sdcard/DCIM"
    );
    assert_eq!(
        volume.to_device_path(Path::new("/sdcard/DCIM/")).unwrap(),
        "/sdcard/DCIM"
    );
}

#[test]
fn dot_dot_is_resolved_lexically() {
    let volume = detached_volume();
    assert_eq!(
        volume.to_device_path(Path::new("/sdcard/DCIM/../Pictures")).unwrap(),
        "/sdcard/Pictures"
    );
    assert_eq!(volume.to_device_path(Path::new("/sdcard/..")).unwrap(), "/");
}

#[test]
fn a_path_climbing_above_the_root_is_refused_not_anchored() {
    let volume = detached_volume();
    for escape in ["/..", "../etc", "/sdcard/../../etc/passwd"] {
        let outcome = volume.to_device_path(Path::new(escape));
        assert!(
            matches!(outcome, Err(VolumeError::NotFound(ref p)) if p == escape),
            "{escape}: {outcome:?}"
        );
    }
}

#[test]
fn joining_never_doubles_the_root_slash() {
    assert_eq!(join_device_path("/", "sdcard"), "/sdcard");
    assert_eq!(join_device_path("/sdcard", "DCIM"), "/sdcard/DCIM");
}
