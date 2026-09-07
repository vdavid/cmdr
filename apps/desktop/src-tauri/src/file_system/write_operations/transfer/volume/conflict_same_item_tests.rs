//! Identity tests for `item_identity.rs::is_the_same_volume_path`. A `#[path]`
//! child of `item_identity.rs`, so `super::` is `item_identity` and
//! `super::super::` is `volume` — the same one-level-shallower rule every
//! `*_tests.rs` in this directory follows.
//!
//! The leaf is folded (the question one destination listing answers); the
//! parents are not, because whether two differently-cased directories are one is
//! the backend's call. See the function's own doc comment for what each mistake
//! costs.

use super::*;

#[test]
fn the_same_path_names_the_same_item() {
    assert!(is_the_same_volume_path(
        Path::new("/DCIM/photo.jpg"),
        Path::new("/DCIM/photo.jpg")
    ));
}

#[test]
fn a_case_differing_leaf_names_the_same_item() {
    assert!(is_the_same_volume_path(
        Path::new("/DCIM/Photo.JPG"),
        Path::new("/DCIM/photo.jpg")
    ));
}

#[test]
fn an_nfd_leaf_names_the_same_item_as_its_nfc_twin() {
    assert!(is_the_same_volume_path(
        Path::new("/photos/cafe\u{301}.jpg"),
        Path::new("/photos/caf\u{e9}.jpg")
    ));
}

#[test]
fn a_case_differing_parent_names_a_different_item() {
    assert!(!is_the_same_volume_path(
        Path::new("/DCIM/photo.jpg"),
        Path::new("/dcim/photo.jpg")
    ));
}

#[test]
fn a_different_parent_names_a_different_item() {
    assert!(!is_the_same_volume_path(
        Path::new("/DCIM/photo.jpg"),
        Path::new("/backup/photo.jpg")
    ));
}
