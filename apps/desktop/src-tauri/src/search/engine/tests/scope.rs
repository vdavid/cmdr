use std::collections::HashMap;

use super::super::*;
use super::*;
use crate::search::index::SearchEntry;
use crate::search::types::PatternType;

// ── Scope filtering in search ───────────────────────────────────

/// Build a test index representing:
/// /Users/alice/projects/app.rs         (id=9)
/// /Users/alice/projects/node_modules/pkg.json (id=11)
/// /Users/alice/.git/config             (id=13)
fn make_scope_test_index() -> SearchIndex {
    let mut names = String::new();
    let test_names = [
        "",             // 0: root
        "Users",        // 1
        "alice",        // 2
        "projects",     // 3
        "app.rs",       // 4
        "node_modules", // 5
        "pkg.json",     // 6
        ".git",         // 7
        "config",       // 8
    ];
    let offsets: Vec<(u32, u16)> = test_names.iter().map(|n| arena_push(&mut names, n)).collect();

    let entries = vec![
        SearchEntry {
            id: 1,
            parent_id: 0,
            name_offset: offsets[0].0,
            name_len: offsets[0].1,
            is_directory: true,
            size: None,
            modified_at: None,
        },
        SearchEntry {
            id: 2,
            parent_id: 1,
            name_offset: offsets[1].0,
            name_len: offsets[1].1,
            is_directory: true,
            size: None,
            modified_at: Some(1000),
        },
        SearchEntry {
            id: 3,
            parent_id: 2,
            name_offset: offsets[2].0,
            name_len: offsets[2].1,
            is_directory: true,
            size: None,
            modified_at: Some(2000),
        },
        SearchEntry {
            id: 4,
            parent_id: 3,
            name_offset: offsets[3].0,
            name_len: offsets[3].1,
            is_directory: true,
            size: None,
            modified_at: Some(3000),
        },
        SearchEntry {
            id: 9,
            parent_id: 4,
            name_offset: offsets[4].0,
            name_len: offsets[4].1,
            is_directory: false,
            size: Some(1000),
            modified_at: Some(4000),
        },
        SearchEntry {
            id: 10,
            parent_id: 4,
            name_offset: offsets[5].0,
            name_len: offsets[5].1,
            is_directory: true,
            size: None,
            modified_at: Some(5000),
        },
        SearchEntry {
            id: 11,
            parent_id: 10,
            name_offset: offsets[6].0,
            name_len: offsets[6].1,
            is_directory: false,
            size: Some(500),
            modified_at: Some(6000),
        },
        SearchEntry {
            id: 12,
            parent_id: 3,
            name_offset: offsets[7].0,
            name_len: offsets[7].1,
            is_directory: true,
            size: None,
            modified_at: Some(7000),
        },
        SearchEntry {
            id: 13,
            parent_id: 12,
            name_offset: offsets[8].0,
            name_len: offsets[8].1,
            is_directory: false,
            size: Some(200),
            modified_at: Some(8000),
        },
    ];
    let mut id_to_index = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        id_to_index.insert(e.id, i);
    }
    SearchIndex {
        names,
        entries,
        id_to_index,
        generation: 1,
    }
}

#[test]
fn search_with_include_path_filter() {
    let index = make_scope_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(false),
        include_paths: Some(vec!["/Users/alice/projects".to_string()]),
        exclude_dir_names: None,
        include_path_ids: Some(vec![4]),
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // Should find app.rs and pkg.json (both under /Users/alice/projects)
    // but NOT config (under /Users/alice/.git)
    assert_eq!(result.total_count, 2);
    let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"app.rs"));
    assert!(names.contains(&"pkg.json"));
}

#[test]
fn search_with_exclude_pattern() {
    let index = make_scope_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(false),
        include_paths: None,
        exclude_dir_names: Some(vec!["node_modules".to_string()]),
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // Should find app.rs and config, but NOT pkg.json (under node_modules)
    assert_eq!(result.total_count, 2);
    let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"app.rs"));
    assert!(names.contains(&"config"));
    assert!(!names.contains(&"pkg.json"));
}

#[test]
fn search_with_include_and_exclude() {
    let index = make_scope_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(false),
        include_paths: Some(vec!["/Users/alice/projects".to_string()]),
        exclude_dir_names: Some(vec!["node_modules".to_string()]),
        include_path_ids: Some(vec![4]),
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // Only app.rs: under projects but not under node_modules
    assert_eq!(result.total_count, 1);
    assert_eq!(result.entries[0].name, "app.rs");
}

#[test]
fn search_with_wildcard_exclude() {
    let index = make_scope_test_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(false),
        include_paths: None,
        exclude_dir_names: Some(vec![".*".to_string()]),
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // Should exclude config (under .git) but keep app.rs and pkg.json
    assert_eq!(result.total_count, 2);
    let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"app.rs"));
    assert!(names.contains(&"pkg.json"));
    assert!(!names.contains(&"config"));
}
