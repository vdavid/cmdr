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
