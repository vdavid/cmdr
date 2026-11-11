use std::path::Path;

use super::*;

/// The real table of the machine the tests run on: the root is always mounted, and a fresh
/// temp folder under it isn't a mount of its own.
#[test]
fn the_table_lists_the_root_and_not_a_plain_folder() {
    let mounts = list().expect("the mount table reads");
    assert!(mounts.iter().any(|mount| mount == Path::new("/")), "{mounts:?}");
    assert_eq!(is_mount_point(Path::new("/")), Some(true));

    let dir = cmdr_fs::testing::TestDir::new("mounts_plain_folder");
    let plain = dir.join("plain");
    std::fs::create_dir(&plain).expect("plain folder");
    assert_eq!(is_mount_point(&plain), Some(false));
}

#[test]
fn a_user_mount_prefix_is_an_absolute_folder_path() {
    for prefix in cmdr_fs::mount_layout::user_mount_prefixes() {
        assert!(prefix.starts_with('/') && prefix.ends_with('/'), "{prefix}");
    }
}
