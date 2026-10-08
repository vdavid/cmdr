//! What a path names on a bucket place and on the account root.

use std::path::Path;

use cmdr_fs::volume::{Volume, VolumeError};

use super::super::test_support::{PREFIX, make_test_volume};
use super::{Target, target_of};

#[test]
fn a_server_side_path_splits_into_bucket_and_key() {
    assert_eq!(target_of("/"), Target::Account);
    assert_eq!(target_of("/photos"), Target::Bucket("photos"));
    assert_eq!(
        target_of("/photos/2025/a b+c.jpg"),
        Target::Key {
            bucket: "photos",
            key: "2025/a b+c.jpg"
        }
    );
}

#[test]
fn a_bucket_place_is_rooted_at_its_bucket() {
    let volume = make_test_volume(Some("photos"));
    assert_eq!(volume.root(), Path::new(&format!("{PREFIX}/photos")));
    for spelling in ["/", "", "."] {
        assert_eq!(volume.to_remote_path(Path::new(spelling)).expect("the root"), "/photos");
    }
    assert_eq!(
        volume
            .to_remote_path(Path::new(&format!("{PREFIX}/photos/2025/a.jpg")))
            .expect("a path in the bucket"),
        "/photos/2025/a.jpg"
    );
}

#[test]
fn a_bucket_place_refuses_another_buckets_path() {
    // ❗ The same account's other bucket is another place; a pane must never
    // reach it through this one.
    let volume = make_test_volume(Some("photos"));
    let refused = volume.to_remote_path(Path::new(&format!("{PREFIX}/backups/a.tar")));
    assert!(matches!(refused, Err(VolumeError::NotFound(_))), "got {refused:?}");
}

#[test]
fn the_account_root_reaches_every_bucket() {
    let volume = make_test_volume(None);
    assert_eq!(volume.root(), Path::new(&format!("{PREFIX}/")));
    assert_eq!(
        volume
            .to_remote_path(Path::new(&format!("{PREFIX}/backups/a.tar")))
            .expect("any bucket"),
        "/backups/a.tar"
    );
}

#[test]
fn a_bare_server_path_is_refused() {
    let volume = make_test_volume(None);
    assert!(volume.to_remote_path(Path::new("/photos/a.jpg")).is_err());
}
