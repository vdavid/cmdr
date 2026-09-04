//! The path translation, on a volume rooted at a sub-collection.

use std::path::Path;

use cmdr_fs::volume::VolumeError;

use super::super::test_support::make_test_volume;
use super::root_remote_path;

const ROOT: &str = "/srv/data";

#[test]
fn every_spelling_of_the_root_is_the_root() {
    let volume = make_test_volume(ROOT);
    for spelling in ["/", "", "."] {
        assert_eq!(volume.to_remote_path(Path::new(spelling)).expect("the root"), ROOT);
    }
    assert_eq!(volume.to_remote_path(Path::new(ROOT)).expect("the root"), ROOT);
}

#[test]
fn a_relative_path_lands_under_the_root() {
    let volume = make_test_volume(ROOT);
    assert_eq!(
        volume
            .to_remote_path(Path::new("photos/a b.jpg"))
            .expect("under the root"),
        "/srv/data/photos/a b.jpg"
    );
}

#[test]
fn a_sibling_that_shares_a_prefix_is_refused() {
    let volume = make_test_volume(ROOT);
    assert!(matches!(
        volume.to_remote_path(Path::new("/srv/data-1/photos")),
        Err(VolumeError::NotFound(_))
    ));
}

#[test]
fn a_dot_dot_escape_is_refused_however_it_is_spelled() {
    let volume = make_test_volume(ROOT);
    for escape in ["photos/../../etc", "/srv/data/../data-1", "../data-1"] {
        assert!(
            matches!(volume.to_remote_path(Path::new(escape)), Err(VolumeError::NotFound(_))),
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
