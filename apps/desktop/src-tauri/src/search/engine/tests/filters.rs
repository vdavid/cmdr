use super::*;
use crate::search::types::PatternType;

// ── Size filters ─────────────────────────────────────────────────

#[test]
fn search_min_size() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: Some(2_000_000),
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(false),
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // photo.jpg (5M) and Q1-report.pdf (2M)
    assert_eq!(result.total_count, 2);
    assert!(result.entries.iter().all(|e| e.size.unwrap() >= 2_000_000));
}

#[test]
fn search_max_size() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: Some(1000),
        modified_after: None,
        modified_before: None,
        is_directory: Some(false),
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    assert_eq!(result.total_count, 1);
    assert_eq!(result.entries[0].name, "notes.txt");
}

#[test]
fn search_size_range() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: Some(500_000),
        max_size: Some(3_000_000),
        modified_after: None,
        modified_before: None,
        is_directory: Some(false),
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // report.pdf (1M) and Q1-report.pdf (2M)
    assert_eq!(result.total_count, 2);
}

// ── Date filters ─────────────────────────────────────────────────

#[test]
fn search_modified_after() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: Some(4000),
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // photo.jpg (4000), notes.txt (5000), Q1-report.pdf (6000)
    assert_eq!(result.total_count, 3);
}

#[test]
fn search_modified_before() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: Some(2000),
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // Users (1000), alice (2000), Documents (1500)
    assert_eq!(result.total_count, 3);
}

#[test]
fn search_date_range() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: Some(3000),
        modified_before: Some(5000),
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // report.pdf (3000), photo.jpg (4000), notes.txt (5000)
    assert_eq!(result.total_count, 3);
}

// ── Combined filters ─────────────────────────────────────────────

#[test]
fn search_combined_name_and_size() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: Some("*.pdf".to_string()),
        pattern_type: PatternType::Glob,
        min_size: Some(1_500_000),
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    assert_eq!(result.total_count, 1);
    assert_eq!(result.entries[0].name, "Q1-report.pdf");
}

// ── Empty query (returns all by recency) ─────────────────────────

#[test]
fn search_empty_query_returns_by_recency() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // All entries except root sentinel (7 entries)
    assert_eq!(result.total_count, 7);
    // First result should be most recent (Q1-report.pdf, modified_at=6000)
    assert_eq!(result.entries[0].name, "Q1-report.pdf");
}

// ── Limit and total_count ────────────────────────────────────────

#[test]
fn search_limit_and_total_count() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 3,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    assert_eq!(result.entries.len(), 3);
    assert_eq!(result.total_count, 7); // total matches, not limited
}

// ── Directory filter ─────────────────────────────────────────────

#[test]
fn search_directories_only() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(true),
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // Users, alice, Documents (root excluded)
    assert_eq!(result.total_count, 3);
    assert!(result.entries.iter().all(|e| e.is_directory));
}

#[test]
fn search_files_only() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(false),
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    assert_eq!(result.total_count, 4);
    assert!(result.entries.iter().all(|e| !e.is_directory));
}
