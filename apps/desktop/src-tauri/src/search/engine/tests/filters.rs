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
        sort_by: None,
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
        sort_by: None,
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
        sort_by: None,
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
        sort_by: None,
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
        sort_by: None,
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
        sort_by: None,
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
        sort_by: None,
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
        sort_by: None,
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
        sort_by: None,
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
        sort_by: None,
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
        sort_by: None,
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    assert_eq!(result.total_count, 4);
    assert!(result.entries.iter().all(|e| !e.is_directory));
}

// ── Directory size filters (the `dir_stats` half) ────────────────

/// A size filter over directories, with the sizes the DB would have supplied.
fn ranked_with_dir_sizes(index: &SearchIndex, query: &SearchQuery, sizes: &[(i64, u64)], is_filter: bool) -> Ranked {
    let map: HashMap<i64, u64> = sizes.iter().copied().collect();
    let dir_sizes = DirSizes::new(map, is_filter);
    search_ranked(index, query, &ImportanceWeights::empty(), "", Some(&dir_sizes)).expect("search_ranked")
}

#[test]
fn a_huge_but_stale_directory_survives_a_size_filter_the_ranking_would_have_cut() {
    // The bug this pins: `sizeMin: 50 GB` over a real drive returned four folders
    // and missed a 1.7 TB `~/Library`. A directory's size isn't in the arena, so
    // filtering AFTER the ranked cut asks a recency-ordered sample instead of the
    // drive. Here `Users` (mtime 1000, the oldest) is the only directory that
    // passes, and a limit of 1 means the ranking would have handed back the newest
    // one and then dropped it — answering "nothing that big" over a drive that has
    // one.
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: Some(50_000_000_000),
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(true),
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 1,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
        sort_by: None,
    };
    let ranked = ranked_with_dir_sizes(&index, &query, &[(2, 1_700_000_000_000)], true);
    assert_eq!(ranked.total_count, 1, "the count is exact, not a post-hoc subtraction");
    assert_eq!(ranked.entries.len(), 1);
    assert_eq!(ranked.entries[0].name, "Users");
}

#[test]
fn a_count_only_directory_size_filter_needs_no_correction_pass() {
    // The count comes straight off the scan now, so count-only returns no rows and
    // no longer has to hand every matching directory back for the caller to
    // subtract from.
    let index = make_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: Some(1),
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(true),
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: true,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
        sort_by: None,
    };
    let ranked = ranked_with_dir_sizes(&index, &query, &[(2, 10), (7, 20)], true);
    assert_eq!(ranked.total_count, 2);
    assert!(ranked.entries.is_empty(), "count-only carries no rows");
}

#[test]
fn sorting_by_size_ranks_files_and_directories_on_one_scale() {
    // "The biggest matches" means the biggest that exist, so an explicit sort
    // REPLACES the relevance ranking rather than reordering its top-k. A directory
    // is compared on its recursive size, a file on its own, and a directory the
    // index has no size for sorts last rather than leading as "biggest".
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
        sort_by: Some(SearchSort::Size),
    };
    // `Users` is enormous; `Documents` has no row at all.
    let ranked = ranked_with_dir_sizes(&index, &query, &[(2, 9_000_000_000)], false);
    assert_eq!(ranked.entries[0].name, "Users", "the biggest thing leads, dir or file");
    assert!(
        !ranked.entries.iter().any(|e| e.name == "Documents"),
        "an unknown-sized directory never outranks a known one"
    );
}
