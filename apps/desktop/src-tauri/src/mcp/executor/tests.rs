//! Tests for executor module.

use std::path::Path;

use super::search::{format_search_results, parse_human_size};
use super::*;

#[test]
fn test_tool_error_invalid_params() {
    let err = ToolError::invalid_params("test error");
    assert_eq!(err.code, INVALID_PARAMS);
    assert_eq!(err.message, "test error");
}

#[test]
fn test_tool_error_internal() {
    let err = ToolError::internal("internal error");
    assert_eq!(err.code, INTERNAL_ERROR);
    assert_eq!(err.message, "internal error");
}

#[test]
fn test_user_path_param_expands_tilde() {
    let home = dirs::home_dir().expect("home dir").to_string_lossy().to_string();

    // Pre-fix, `~/Downloads` passed through raw and failed the existence check.
    let params = json!({"path": "~/Downloads"});
    assert_eq!(user_path_param(&params, "path").unwrap(), format!("{home}/Downloads"));

    // Bare `~` expands to home itself
    let params = json!({"path": "~"});
    assert_eq!(user_path_param(&params, "path").unwrap(), home);
}

#[test]
fn test_user_path_param_missing_param() {
    let err = user_path_param(&json!({}), "path").unwrap_err();
    assert_eq!(err.code, INVALID_PARAMS);
    assert_eq!(err.message, "Missing 'path' parameter");
}

#[test]
fn test_expand_user_path_leaves_non_tilde_paths_untouched() {
    // Absolute paths
    assert_eq!(expand_user_path("/tmp"), "/tmp");
    // Virtual paths must never be expanded
    assert_eq!(expand_user_path("mtp://device-1/DCIM"), "mtp://device-1/DCIM");
    // `~` only expands as the leading segment
    assert_eq!(expand_user_path("/tmp/~/x"), "/tmp/~/x");
    // `~user` syntax is not supported, so it passes through
    assert_eq!(expand_user_path("~root/x"), "~root/x");
}

fn pane_state_with(
    files: Vec<(&str, bool)>,
    cursor_index: usize,
    selected: Vec<usize>,
) -> crate::mcp::pane_state::PaneState {
    crate::mcp::pane_state::PaneState {
        path: "/test".to_string(),
        files: files
            .into_iter()
            .map(|(name, is_directory)| crate::mcp::pane_state::PaneFileEntry {
                name: name.to_string(),
                path: format!("/test/{name}"),
                is_directory,
                size: None,
                recursive_size: None,
                recursive_size_pending: None,
                modified: None,
            })
            .collect(),
        cursor_index,
        selected_indices: selected,
        total_files: 0,
        ..Default::default()
    }
}

#[test]
fn test_empty_operation_error_selection_wins() {
    let mut state = pane_state_with(vec![("..", true), ("a.txt", false)], 0, vec![1]);
    state.total_files = 2;
    assert!(file_ops::empty_operation_error(&state, "left", "copy").is_none());
}

#[test]
fn test_empty_operation_error_cursor_on_parent() {
    // Pre-fix this surfaced as a misleading 1500 ms "frontend may be stalled" timeout.
    let mut state = pane_state_with(vec![("..", true), ("a.txt", false)], 0, vec![]);
    state.total_files = 2;
    let msg = file_ops::empty_operation_error(&state, "left", "delete").expect("should reject");
    assert!(msg.contains("Nothing to delete"));
    assert!(msg.contains("parent entry"));
}

#[test]
fn test_empty_operation_error_cursor_fallback_proceeds() {
    let mut state = pane_state_with(vec![("..", true), ("a.txt", false)], 1, vec![]);
    state.total_files = 2;
    assert!(file_ops::empty_operation_error(&state, "left", "copy").is_none());
}

#[test]
fn test_empty_operation_error_empty_pane() {
    // Synced empty volume root: zero files, zero total
    let state = pane_state_with(vec![], 0, vec![]);
    let msg = file_ops::empty_operation_error(&state, "right", "move").expect("should reject");
    assert!(msg.contains("the right pane shows no files"));
}

#[test]
fn test_empty_operation_error_empty_dir_with_unrendered_parent() {
    // Synced empty dir: the FE renders the empty-state overlay (no rows, not even `..`),
    // so the push has zero files while total_files still counts the parent entry.
    let mut state = pane_state_with(vec![], 0, vec![]);
    state.total_files = 1;
    let msg = file_ops::empty_operation_error(&state, "left", "copy").expect("should reject");
    assert!(msg.contains("shows no files"));
}

#[test]
fn test_empty_operation_error_unsynced_state_passes_through() {
    // Default state (no push yet, path empty): the FE is the authority, don't reject.
    let mut state = pane_state_with(vec![], 0, vec![]);
    state.path = String::new();
    assert!(file_ops::empty_operation_error(&state, "left", "copy").is_none());
}

#[test]
fn test_empty_operation_error_cursor_outside_loaded_window() {
    // Cursor at global index 5 but the loaded window starts at 100: we can't see the
    // entry, so the FE stays the authority and the operation proceeds.
    let mut state = pane_state_with(vec![("z.txt", false)], 5, vec![]);
    state.loaded_start = 100;
    state.loaded_end = 101;
    state.total_files = 200;
    assert!(file_ops::empty_operation_error(&state, "left", "copy").is_none());
}

#[test]
fn test_is_virtual_path() {
    // Scheme-prefixed virtual paths skip the local existence check
    assert!(is_virtual_path("mtp://device-1/DCIM"));
    assert!(is_virtual_path("smb://nas.local/share/folder"));
    // Local paths don't
    assert!(!is_virtual_path("/Users/jane/Documents"));
    assert!(!is_virtual_path("~/Downloads"));
    assert!(!is_virtual_path(""));
    // A "://" that isn't a scheme prefix isn't virtual
    assert!(!is_virtual_path("/tmp/weird://name"));
    assert!(!is_virtual_path("://no-scheme"));
}

#[tokio::test]
async fn test_validate_path_exists() {
    // Local existing path passes
    assert!(validate_path_exists("/tmp").await.is_ok());
    // Local missing path is invalid_params
    let err = validate_path_exists("/nonexistent/path/xyz").await.unwrap_err();
    assert_eq!(err.code, INVALID_PARAMS);
    // Virtual paths skip the check entirely
    assert!(validate_path_exists("mtp://device/DCIM").await.is_ok());
    assert!(validate_path_exists("smb://server/share/missing").await.is_ok());
}

#[test]
fn test_path_exists_validation() {
    // Test that Path::new().exists() works as expected for our validation
    assert!(Path::new("/").exists(), "Root should exist");
    assert!(Path::new("/tmp").exists(), "Temp dir should exist");
    assert!(
        !Path::new("/nonexistent/path/that/does/not/exist").exists(),
        "Nonexistent path should not exist"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_volume_list_not_empty() {
    // Verify we can list volumes for validation
    let locations = crate::volumes::list_locations();
    assert!(!locations.is_empty(), "Should have at least one volume");
    // Should have a main volume
    assert!(
        locations
            .iter()
            .any(|l| l.category == crate::volumes::LocationCategory::MainVolume),
        "Should have main volume"
    );
}

#[test]
fn test_parse_human_size_with_space() {
    assert_eq!(parse_human_size("1 MB").unwrap(), 1_048_576);
    assert_eq!(parse_human_size("500 KB").unwrap(), 512_000);
    assert_eq!(parse_human_size("2 GB").unwrap(), 2_147_483_648);
    assert_eq!(parse_human_size("1 TB").unwrap(), 1_099_511_627_776);
    assert_eq!(parse_human_size("100 B").unwrap(), 100);
}

#[test]
fn test_parse_human_size_no_space() {
    assert_eq!(parse_human_size("1MB").unwrap(), 1_048_576);
    assert_eq!(parse_human_size("500KB").unwrap(), 512_000);
    assert_eq!(parse_human_size("2GB").unwrap(), 2_147_483_648);
}

#[test]
fn test_parse_human_size_case_insensitive() {
    assert_eq!(parse_human_size("1 mb").unwrap(), 1_048_576);
    assert_eq!(parse_human_size("500 kb").unwrap(), 512_000);
    assert_eq!(parse_human_size("1 Mb").unwrap(), 1_048_576);
}

#[test]
fn test_parse_human_size_decimal() {
    assert_eq!(parse_human_size("1.5 MB").unwrap(), 1_572_864);
    assert_eq!(parse_human_size("0.5 GB").unwrap(), 536_870_912);
}

#[test]
fn test_parse_human_size_invalid() {
    assert!(parse_human_size("abc").is_err());
    assert!(parse_human_size("MB").is_err());
}

#[test]
fn test_format_search_results_empty() {
    use crate::search::SearchResult;
    let result = SearchResult {
        entries: Vec::new(),
        total_count: 0,
    };
    assert_eq!(format_search_results(&result, 30), "No files found matching the query.");
}

#[test]
fn test_format_search_results_with_entries() {
    use crate::search::{SearchResult, SearchResultEntry};
    let result = SearchResult {
        entries: vec![SearchResultEntry {
            name: "test.pdf".to_string(),
            path: "/Users/test/Documents/test.pdf".to_string(),
            parent_path: "~/Documents".to_string(),
            is_directory: false,
            size: Some(340_000),
            modified_at: Some(1_735_689_600),
            icon_id: "pdf".to_string(),
            entry_id: 1,
        }],
        total_count: 1,
    };
    let formatted = format_search_results(&result, 30);
    assert!(formatted.contains("1 of 1 results:"));
    assert!(formatted.contains("test.pdf"));
    assert!(formatted.contains("~/Documents"));
}

#[test]
fn test_format_search_results_directory_trailing_slash() {
    use crate::search::{SearchResult, SearchResultEntry};
    let result = SearchResult {
        entries: vec![SearchResultEntry {
            name: "Projects".to_string(),
            path: "/Users/test/Projects".to_string(),
            parent_path: "~".to_string(),
            is_directory: true,
            size: Some(1_200_000),
            modified_at: Some(1_735_689_600),
            icon_id: "dir".to_string(),
            entry_id: 2,
        }],
        total_count: 1,
    };
    let formatted = format_search_results(&result, 30);
    assert!(formatted.contains("Projects/"));
}
