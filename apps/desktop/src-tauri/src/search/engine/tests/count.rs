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
        sort_by: None,
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
        sort_by: None,
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // report.pdf (1M), photo.jpg (5M), Q1-report.pdf (2M); notes.txt (500) excluded.
    assert_eq!(result.total_count, 3);
    assert!(result.entries.is_empty());
}
