use crate::mcp::pane_state::{FileEntry, PaneState, PaneStateStore};
use crate::mcp::tools::get_all_tools;

// =============================================================================
// PaneStateStore tests
// =============================================================================

#[test]
fn test_pane_state_store_initial_values() {
    let store = PaneStateStore::new();
    assert_eq!(store.get_focused_pane(), "left");
    assert_eq!(store.get_left().path, "");
    assert_eq!(store.get_right().path, "");
}

#[test]
fn test_pane_state_store_update_left() {
    let store = PaneStateStore::new();
    let state = PaneState {
        path: "/test/path".to_string(),
        volume_id: Some("test-vol".to_string()),
        volume_name: Some("Test Volume".to_string()),
        files: vec![FileEntry {
            name: "file1.txt".to_string(),
            path: "/test/path/file1.txt".to_string(),
            is_directory: false,
            size: Some(1024),
            recursive_size: None,
            modified: Some("2024-01-01T00:00:00Z".to_string()),
        }],
        cursor_index: 0,
        view_mode: "brief".to_string(),
        selected_indices: vec![],
        sort_field: "name".to_string(),
        sort_order: "asc".to_string(),
        total_files: 1,
        loaded_start: 0,
        loaded_end: 1,
        show_hidden: false,
        tabs: vec![],
    };

    store.set_left(state.clone());
    let left = store.get_left();

    assert_eq!(left.path, "/test/path");
    assert_eq!(left.volume_id, Some("test-vol".to_string()));
    assert_eq!(left.files.len(), 1);
}

#[test]
fn test_pane_state_store_focus_change() {
    let store = PaneStateStore::new();
    assert_eq!(store.get_focused_pane(), "left");

    store.set_focused_pane("right".to_string());
    assert_eq!(store.get_focused_pane(), "right");

    store.set_focused_pane("left".to_string());
    assert_eq!(store.get_focused_pane(), "left");
}

#[test]
fn test_pane_state_store_accepts_any_focus_value() {
    let store = PaneStateStore::new();

    // The store currently accepts any value - this documents the behavior
    // Future: we may want to validate and reject invalid values
    store.set_focused_pane("invalid".to_string());
    let focused = store.get_focused_pane();
    assert_eq!(focused, "invalid");

    // Reset to valid state
    store.set_focused_pane("left".to_string());
}

#[test]
fn test_pane_state_cursor_index_bounds() {
    let store = PaneStateStore::new();
    let state = PaneState {
        path: "/test".to_string(),
        volume_id: None,
        volume_name: None,
        files: vec![FileEntry {
            name: "file1.txt".to_string(),
            path: "/test/file1.txt".to_string(),
            is_directory: false,
            size: None,
            recursive_size: None,
            modified: None,
        }],
        cursor_index: 999, // Out of bounds
        view_mode: "brief".to_string(),
        selected_indices: vec![],
        sort_field: "name".to_string(),
        sort_order: "asc".to_string(),
        total_files: 1,
        loaded_start: 0,
        loaded_end: 1,
        show_hidden: false,
        tabs: vec![],
    };

    store.set_left(state);
    let left = store.get_left();

    // Should store as-is (bounds checking is done at query time)
    assert_eq!(left.cursor_index, 999);

    // But accessing the file should handle bounds
    let file_under_cursor = left.files.get(left.cursor_index);
    assert!(file_under_cursor.is_none());
}

#[test]
fn test_file_entry_serialization() {
    let entry = FileEntry {
        name: "test.txt".to_string(),
        path: "/path/to/test.txt".to_string(),
        is_directory: false,
        size: Some(42),
        recursive_size: None,
        modified: Some("2024-01-01T00:00:00Z".to_string()),
    };

    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["name"], "test.txt");
    assert_eq!(json["isDirectory"], false);
    assert_eq!(json["size"], 42);
}

#[test]
fn test_file_entry_optional_fields_omitted() {
    let entry = FileEntry {
        name: "dir".to_string(),
        path: "/path/dir".to_string(),
        is_directory: true,
        size: None,
        recursive_size: None,
        modified: None,
    };

    let json = serde_json::to_value(&entry).unwrap();
    // Optional fields with None should be omitted (per skip_serializing_if)
    assert!(json.get("size").is_none());
    assert!(json.get("modified").is_none());
}

// =============================================================================
// Edge case tests
// =============================================================================

#[test]
fn test_tool_names_are_case_sensitive() {
    let tools = get_all_tools();

    // Should find move_cursor
    assert!(tools.iter().any(|t| t.name == "move_cursor"));

    // Should NOT find MOVE_CURSOR or Move_Cursor
    assert!(!tools.iter().any(|t| t.name == "MOVE_CURSOR"));
    assert!(!tools.iter().any(|t| t.name == "Move_Cursor"));
}

#[test]
fn test_unicode_in_file_entries() {
    // The store should handle Unicode filenames correctly
    let entry = FileEntry {
        name: "文件.txt".to_string(),
        path: "/path/文件.txt".to_string(),
        is_directory: false,
        size: Some(100),
        recursive_size: None,
        modified: None,
    };

    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["name"], "文件.txt");
}

#[test]
fn test_special_chars_in_file_paths() {
    // Paths can contain special characters
    let entries = vec![
        FileEntry {
            name: "file with spaces.txt".to_string(),
            path: "/path/file with spaces.txt".to_string(),
            is_directory: false,
            size: None,
            recursive_size: None,
            modified: None,
        },
        FileEntry {
            name: "file'with'quotes.txt".to_string(),
            path: "/path/file'with'quotes.txt".to_string(),
            is_directory: false,
            size: None,
            recursive_size: None,
            modified: None,
        },
        FileEntry {
            name: "file\"doublequotes\".txt".to_string(),
            path: "/path/file\"doublequotes\".txt".to_string(),
            is_directory: false,
            size: None,
            recursive_size: None,
            modified: None,
        },
    ];

    for entry in entries {
        // Should serialize without panic
        let json = serde_json::to_value(&entry).unwrap();
        assert!(json["name"].is_string());
    }
}

#[test]
fn test_empty_file_list() {
    let state = PaneState {
        path: "/empty".to_string(),
        volume_id: None,
        volume_name: None,
        files: vec![],
        cursor_index: 0,
        view_mode: "brief".to_string(),
        selected_indices: vec![],
        sort_field: "name".to_string(),
        sort_order: "asc".to_string(),
        total_files: 0,
        loaded_start: 0,
        loaded_end: 0,
        show_hidden: false,
        tabs: vec![],
    };

    let json = serde_json::to_value(&state).unwrap();
    assert!(json["files"].as_array().unwrap().is_empty());
}

#[test]
fn test_large_file_count() {
    // Simulate a directory with many files
    let files: Vec<FileEntry> = (0..1000)
        .map(|i| FileEntry {
            name: format!("file{i:04}.txt"),
            path: format!("/test/file{i:04}.txt"),
            is_directory: false,
            size: Some(i as u64 * 100),
            recursive_size: None,
            modified: None,
        })
        .collect();

    let state = PaneState {
        path: "/test".to_string(),
        volume_id: None,
        volume_name: None,
        files,
        cursor_index: 500,
        view_mode: "full".to_string(),
        selected_indices: vec![1, 5, 10], // Some selected files
        sort_field: "name".to_string(),
        sort_order: "asc".to_string(),
        total_files: 1000,
        loaded_start: 0,
        loaded_end: 1000,
        show_hidden: false,
        tabs: vec![],
    };

    // Should serialize reasonably fast
    let start = std::time::Instant::now();
    let json = serde_json::to_value(&state).unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed.as_millis() < 100, "Serialization took too long: {:?}", elapsed);
    assert_eq!(json["files"].as_array().unwrap().len(), 1000);
}

// =============================================================================
// Concurrent access tests
// =============================================================================

#[test]
fn test_pane_state_store_thread_safety() {
    use std::sync::Arc;
    use std::thread;

    let store = Arc::new(PaneStateStore::new());
    let mut handles = vec![];

    // Spawn multiple threads that read and write concurrently
    for i in 0..10 {
        let store_clone = Arc::clone(&store);
        handles.push(thread::spawn(move || {
            // Each thread does a mix of reads and writes
            for j in 0..100 {
                if j % 2 == 0 {
                    store_clone.set_focused_pane(if i % 2 == 0 { "left" } else { "right" }.to_string());
                } else {
                    let _ = store_clone.get_focused_pane();
                    let _ = store_clone.get_left();
                    let _ = store_clone.get_right();
                }
            }
        }));
    }

    // All threads should complete without panic or deadlock
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Store should still be in a valid state
    let focused = store.get_focused_pane();
    assert!(focused == "left" || focused == "right");
}
