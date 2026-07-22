//! Tests for the `split_parent_and_name` pure helper (lives in `event_loop::live`).

use crate::indexing::event_loop::live::split_parent_and_name;

#[test]
fn split_parent_and_name_handles_normal_paths() {
    assert_eq!(
        split_parent_and_name("/a/b/c"),
        Some(("/a/b".to_string(), "c".to_string()))
    );
    assert_eq!(
        split_parent_and_name("/Users/foo/bar.txt"),
        Some(("/Users/foo".to_string(), "bar.txt".to_string()))
    );
}

#[test]
fn split_parent_and_name_handles_root_child() {
    assert_eq!(
        split_parent_and_name("/foo"),
        Some(("/".to_string(), "foo".to_string()))
    );
}

#[test]
fn split_parent_and_name_strips_trailing_slash() {
    assert_eq!(
        split_parent_and_name("/a/b/c/"),
        Some(("/a/b".to_string(), "c".to_string()))
    );
}

#[test]
fn split_parent_and_name_rejects_root_only() {
    assert_eq!(split_parent_and_name("/"), None);
    assert_eq!(split_parent_and_name(""), None);
}
