use super::super::*;
use super::*;
use crate::search::types::PatternType;

// ── Wildcard-free glob auto-wrapping (contains match) ────────────

#[test]
fn search_glob_plain_text_matches_substring() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: Some("ote".to_string()),
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
    // "ote" should match "notes.txt" as a substring
    assert_eq!(result.total_count, 1);
    assert_eq!(result.entries[0].name, "notes.txt");
}

#[test]
fn search_glob_plain_text_matches_prefix() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: Some("repo".to_string()),
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
    // "repo" should match "report.pdf" and "Q1-report.pdf"
    assert_eq!(result.total_count, 2);
    assert!(result.entries.iter().all(|e| e.name.contains("report")));
}

#[test]
fn search_glob_with_wildcards_not_auto_wrapped() {
    let index = make_test_index();
    // Explicit glob with wildcard should NOT be auto-wrapped
    let query = SearchQuery {
        name_pattern: Some("report*".to_string()),
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
    // "report*" matches "report.pdf" but NOT "Q1-report.pdf"
    assert_eq!(result.total_count, 1);
    assert_eq!(result.entries[0].name, "report.pdf");
}

// ── Glob matching ────────────────────────────────────────────────

#[test]
fn search_glob_star_pdf() {
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
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    assert_eq!(result.total_count, 2);
    assert!(result.entries.iter().all(|e| e.name.ends_with(".pdf")));
}

#[test]
fn search_glob_question_mark() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: Some("Q?-report.pdf".to_string()),
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
    assert_eq!(result.total_count, 1);
    assert_eq!(result.entries[0].name, "Q1-report.pdf");
}

#[cfg(target_os = "macos")]
#[test]
fn search_glob_case_insensitive_macos() {
    let index = make_test_index();
    let query = SearchQuery {
        // Use a wildcard pattern to test case-insensitivity specifically
        // (without wildcards, auto-wrapping would turn this into a contains match)
        name_pattern: Some("NOTES.*".to_string()),
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
    // On macOS, matching is case-insensitive
    assert_eq!(result.total_count, 1);
    assert_eq!(result.entries[0].name, "notes.txt");
}

// ── Regex matching ───────────────────────────────────────────────

#[test]
fn search_regex_alternation() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: Some(r"Q[1-4].*\.pdf".to_string()),
        pattern_type: PatternType::Regex,
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
    assert_eq!(result.total_count, 1);
    assert_eq!(result.entries[0].name, "Q1-report.pdf");
}

#[test]
fn search_invalid_regex() {
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: Some("[unclosed".to_string()),
        pattern_type: PatternType::Regex,
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
    let result = search(&index, &query, &ImportanceWeights::empty());
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid pattern"));
}
