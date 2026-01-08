//! Tests for file system watcher

// Note: The watcher tests require async handling and app context
// which makes them difficult to unit test. Key functionality is tested via:
// 1. compute_diff tests in watcher.rs (unit tests for diff logic)
// 2. Manual testing of file watching in the actual app
// 3. Integration tests with the full Tauri app

// The start_watching/stop_watching functions require a running app context
// to emit events, so proper testing requires integration tests.

use super::operations::FileEntry;
use super::watcher::compute_diff;

fn make_entry(name: &str, size: Option<u64>) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: format!("/test/{}", name),
        is_directory: false,
        is_symlink: false,
        size,
        modified_at: None,
        created_at: None,
        added_at: None,
        opened_at: None,
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        icon_id: "ext:txt".to_string(),
        extended_metadata_loaded: true,
    }
}

#[test]
fn test_compute_diff_addition() {
    let old = vec![make_entry("a.txt", Some(100))];
    let new = vec![make_entry("a.txt", Some(100)), make_entry("b.txt", Some(200))];

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].change_type, "add");
    assert_eq!(diff[0].entry.name, "b.txt");
}

#[test]
fn test_compute_diff_removal() {
    let old = vec![make_entry("a.txt", Some(100)), make_entry("b.txt", Some(200))];
    let new = vec![make_entry("a.txt", Some(100))];

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].change_type, "remove");
    assert_eq!(diff[0].entry.name, "b.txt");
}

#[test]
fn test_compute_diff_modification() {
    let old = vec![make_entry("a.txt", Some(100))];
    let new = vec![make_entry("a.txt", Some(200))]; // Size changed

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].change_type, "modify");
    assert_eq!(diff[0].entry.size, Some(200));
}

#[test]
fn test_compute_diff_no_change() {
    let old = vec![make_entry("a.txt", Some(100))];
    let new = vec![make_entry("a.txt", Some(100))];

    let diff = compute_diff(&old, &new);
    assert!(diff.is_empty());
}
