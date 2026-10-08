//! A `ListObjectsV2` page turned into a folder's children: prefixes become
//! folders, objects become files, and a folder's own marker stays out.

use std::time::{Duration, SystemTime};

use super::{Child, FolderContents, children_of, folder_contents, settle};
use crate::xml::{ObjectEntry, ObjectPage, StorageClass};

fn object(key: &str, size: u64) -> ObjectEntry {
    ObjectEntry {
        key: key.to_string(),
        size,
        last_modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        etag: None,
        storage_class: StorageClass::Standard,
    }
}

fn page(prefixes: &[&str], objects: Vec<ObjectEntry>) -> ObjectPage {
    ObjectPage {
        prefixes: prefixes.iter().map(|p| (*p).to_string()).collect(),
        objects,
        is_truncated: false,
        next_continuation_token: None,
    }
}

fn names(children: &[Child]) -> Vec<(&str, bool)> {
    children.iter().map(|c| (c.name(), c.is_folder())).collect()
}

#[test]
fn prefixes_are_folders_and_objects_are_files() {
    let listed = children_of(&page(&["photos/2025/"], vec![object("photos/a.jpg", 10)]), "photos/");
    assert_eq!(names(&listed), [("2025", true), ("a.jpg", false)]);
    let Child::Object { size, modified, .. } = &listed[1] else {
        panic!("a.jpg is an object");
    };
    assert_eq!(*size, 10);
    assert!(modified.is_some(), "a listing's LastModified is the file's date");
}

#[test]
fn a_folders_own_marker_is_not_a_child_of_itself() {
    // The zero-byte `photos/` object the AWS console and rclone write for an
    // empty folder comes back in its own listing.
    let listed = children_of(
        &page(&[], vec![object("photos/", 0), object("photos/a.jpg", 1)]),
        "photos/",
    );
    assert_eq!(names(&listed), [("a.jpg", false)]);
}

#[test]
fn a_child_folder_marker_is_a_folder_not_a_file() {
    // A server that ignored the delimiter hands the child's marker back as an
    // object; it still names a folder.
    let listed = children_of(&page(&[], vec![object("photos/empty/", 0)]), "photos/");
    assert_eq!(names(&listed), [("empty", true)]);
}

#[test]
fn keys_deeper_down_collapse_into_one_folder() {
    // Same server, deeper keys: each names the folder it sits under, once.
    let listed = children_of(
        &page(
            &["photos/2025/"],
            vec![object("photos/2025/a.jpg", 1), object("photos/2025/b.jpg", 1)],
        ),
        "photos/",
    );
    assert_eq!(names(&listed), [("2025", true)]);
}

#[test]
fn a_key_and_a_prefix_of_one_name_list_both_the_file_suffixed() {
    // ❗ S3 lets `notes` and `notes/…` both exist; one name in a pane can't be
    // two entries (both would carry one path), so the file shows as
    // `notes (file)`.
    let listed = settle(children_of(&page(&["notes/"], vec![object("notes", 3)]), ""));
    assert_eq!(names(&listed), [("notes", true), ("notes (file)", false)]);
    assert!(matches!(
        &listed[1],
        Child::Object {
            beside_folder: true,
            size: 3,
            ..
        }
    ));
}

#[test]
fn a_key_and_a_prefix_on_different_pages_still_list_both() {
    // `notes` sorts before `notes/`, so a page break can fall between them.
    let mut children = children_of(&page(&[], vec![object("a.txt", 1), object("notes", 3)]), "");
    children.extend(children_of(&page(&["notes/"], vec![]), ""));
    let listed = settle(children);
    assert_eq!(
        names(&listed),
        [("notes", true), ("a.txt", false), ("notes (file)", false)]
    );
}

#[test]
fn a_file_with_no_folder_of_its_name_keeps_its_name() {
    let listed = settle(children_of(&page(&["photos/"], vec![object("notes", 3)]), ""));
    assert_eq!(names(&listed), [("photos", true), ("notes", false)]);
    assert!(matches!(
        &listed[1],
        Child::Object {
            beside_folder: false,
            ..
        }
    ));
}

#[test]
fn a_suffixed_name_that_is_taken_leaves_the_file_out() {
    // A real `notes (file)` already holds the name the shadowed file would
    // take; two rows on one path would break the pane, so it stays unlisted.
    let page = page(&["notes/"], vec![object("notes", 3), object("notes (file)", 5)]);
    let listed = settle(children_of(&page, ""));
    assert_eq!(names(&listed), [("notes", true), ("notes (file)", false)]);
    assert!(matches!(
        &listed[1],
        Child::Object {
            beside_folder: false,
            size: 5,
            ..
        }
    ));
}

#[test]
fn a_folder_named_twice_lists_once() {
    // A server that ignores the delimiter can name one folder on two pages.
    let mut children = children_of(&page(&[], vec![object("a/x", 1)]), "");
    children.extend(children_of(&page(&[], vec![object("a/y", 1)]), ""));
    assert_eq!(names(&settle(children)), [("a", true)]);
}

#[test]
fn names_that_cant_be_addressed_are_left_out() {
    // `.` and `..` resolve away in every URL parser, and an empty name (from
    // `a//b`) has no path of its own.
    let listed = children_of(
        &page(
            &["./", "../", "/"],
            vec![object(".", 1), object("..", 1), object("ok.txt", 1)],
        ),
        "",
    );
    assert_eq!(names(&listed), [("ok.txt", false)]);
}

#[test]
fn spaces_pluses_and_unicode_names_pass_through_untouched() {
    let nfd = "cafe\u{301}.txt";
    let listed = children_of(
        &page(
            &["a b+c/"],
            vec![object("x + y.txt", 1), object(nfd, 1), object(" padded ", 1)],
        ),
        "",
    );
    assert_eq!(
        names(&listed),
        [("a b+c", true), ("x + y.txt", false), (nfd, false), (" padded ", false)]
    );
}

#[test]
fn an_archived_object_says_so() {
    let mut cold = object("old.tar", 5);
    cold.storage_class = StorageClass::DeepArchive;
    let listed = children_of(&page(&[], vec![cold]), "");
    assert!(matches!(listed[0], Child::Object { archived: true, .. }));
}

/// What a delete reads off one capped listing of `name/`: ❗ anything but the
/// folder's own marker means it still holds something, and the delete refuses
/// rather than recursing.
#[test]
fn a_folder_holding_anything_but_its_marker_is_not_empty() {
    assert_eq!(
        folder_contents(&page(&[], vec![object("a/", 0), object("a/x.txt", 1)]), "a/"),
        FolderContents::Holds
    );
    assert_eq!(folder_contents(&page(&["a/sub/"], vec![]), "a/"), FolderContents::Holds);
    assert_eq!(
        folder_contents(&page(&[], vec![object("a/x.txt", 1)]), "a/"),
        FolderContents::Holds
    );
}

#[test]
fn a_folder_with_only_its_marker_is_empty_and_marked() {
    assert_eq!(
        folder_contents(&page(&[], vec![object("a/", 0)]), "a/"),
        FolderContents::MarkerOnly
    );
}

#[test]
fn nothing_under_the_prefix_is_no_folder() {
    assert_eq!(folder_contents(&page(&[], vec![]), "a/"), FolderContents::Nothing);
}
