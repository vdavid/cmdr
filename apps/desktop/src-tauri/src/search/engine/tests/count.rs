use super::super::*;
use super::*;
use crate::search::types::PatternType;

// ── Count-only mode ──────────────────────────────────────────────

#[test]
fn count_only_returns_total_and_empty_entries() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: Some("*.pdf".to_string()),
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: true,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // Exact total, no rows materialized.
    assert_eq!(result.total_count, 2);
    assert!(result.entries.is_empty());
}

#[test]
fn count_only_files_only_with_size_filter_is_exact() {
    let index = make_test_index();
    // Files-only: directories are excluded entirely, so no dir_stats round-trip
    // is needed and the count is exact with empty entries.
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: Some(1_000_000),
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(false),
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: true,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // report.pdf (1M), photo.jpg (5M), Q1-report.pdf (2M); notes.txt (500) excluded.
    assert_eq!(result.total_count, 3);
    assert!(result.entries.is_empty());
}

#[test]
fn count_only_size_filter_with_dirs_hands_dirs_to_caller() {
    let index = make_test_index();
    // Size filter + directories included: engine can't size-filter directories
    // (their sizes live in dir_stats, not the index), so it returns the matching
    // directories in `entries` for the caller to size-check, and `total_count`
    // counts every match (files already size-filtered + all matching dirs).
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: Some(1_000_000),
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: true,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // 3 files pass the size filter + 3 directories (Users, alice, Documents) that
    // the engine passes through unfiltered = 6.
    assert_eq!(result.total_count, 6);
    // The three directories are handed back for the caller's dir_stats size check.
    assert_eq!(result.entries.len(), 3);
    assert!(result.entries.iter().all(|e| e.is_directory));
}
