//! `is_mount_boundary` when the cheap device test can't be asked. The real
//! mounts are the disk-image lane's (`delete::real_image`); these answer the
//! mount table from the `transfer_sides` test hook.

use std::path::Path;

use super::is_mount_boundary;
use crate::transfer_sides::test_hook;

/// A parent that can't be read gives no device to compare, so the table
/// decides: a listed mount point is a boundary however its parent fares.
#[test]
fn a_mount_point_whose_parent_cant_be_read_is_still_a_boundary() {
    let path = Path::new("/cmdr-no-such-parent/mounted");
    let _hook = test_hook::answer_each(vec![(path.to_path_buf(), Some(true))]);

    assert!(is_mount_boundary(path, None), "the mount table lists it");
}

/// The same parent with a table that can't be read: the caller is about to
/// delete on the answer, so it's a boundary.
#[test]
fn an_unreadable_parent_and_an_unreadable_table_answer_boundary() {
    let path = Path::new("/cmdr-no-such-parent/unknown");
    let _hook = test_hook::answer_each(vec![(path.to_path_buf(), None)]);

    assert!(is_mount_boundary(path, None), "nothing says it isn't a mount");
}

/// And a table that answers "not listed" still clears it: an unreadable parent
/// alone is no mount.
#[test]
fn an_unreadable_parent_the_table_doesnt_list_is_no_boundary() {
    let path = Path::new("/cmdr-no-such-parent/plain");
    let _hook = test_hook::answer_each(vec![(path.to_path_buf(), Some(false))]);

    assert!(!is_mount_boundary(path, None));
}
