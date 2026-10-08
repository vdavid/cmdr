use super::*;
use crate::search::index::{OptU64, SearchEntry};
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

    let mut entries = vec![
        SearchEntry {
            id: 1,
            parent_id: 0,
            name_offset: offsets[0].0,
            name_len: offsets[0].1,
            is_directory: true,
            size: OptU64::NONE,
            modified_at: OptU64::NONE,
        },
        SearchEntry {
            id: 2,
            parent_id: 1,
            name_offset: offsets[1].0,
            name_len: offsets[1].1,
            is_directory: true,
            size: OptU64::NONE,
            modified_at: OptU64::new(Some(1000)),
        },
        SearchEntry {
            id: 3,
            parent_id: 2,
            name_offset: offsets[2].0,
            name_len: offsets[2].1,
            is_directory: true,
            size: OptU64::NONE,
            modified_at: OptU64::new(Some(2000)),
        },
        SearchEntry {
            id: 4,
            parent_id: 3,
            name_offset: offsets[3].0,
            name_len: offsets[3].1,
            is_directory: true,
            size: OptU64::NONE,
            modified_at: OptU64::new(Some(3000)),
        },
        SearchEntry {
            id: 9,
            parent_id: 4,
            name_offset: offsets[4].0,
            name_len: offsets[4].1,
            is_directory: false,
            size: OptU64::new(Some(1000)),
            modified_at: OptU64::new(Some(4000)),
        },
        SearchEntry {
            id: 10,
            parent_id: 4,
            name_offset: offsets[5].0,
            name_len: offsets[5].1,
            is_directory: true,
            size: OptU64::NONE,
            modified_at: OptU64::new(Some(5000)),
        },
        SearchEntry {
            id: 11,
            parent_id: 10,
            name_offset: offsets[6].0,
            name_len: offsets[6].1,
            is_directory: false,
            size: OptU64::new(Some(500)),
            modified_at: OptU64::new(Some(6000)),
        },
        SearchEntry {
            id: 12,
            parent_id: 3,
            name_offset: offsets[7].0,
            name_len: offsets[7].1,
            is_directory: true,
            size: OptU64::NONE,
            modified_at: OptU64::new(Some(7000)),
        },
        SearchEntry {
            id: 13,
            parent_id: 12,
            name_offset: offsets[8].0,
            name_len: offsets[8].1,
            is_directory: false,
            size: OptU64::new(Some(200)),
            modified_at: OptU64::new(Some(8000)),
        },
    ];
    // Keep the arena in the order the real loader produces (`ORDER BY id`, segments
    // merged in range order): `index_of_id` binary-searches it.
    entries.sort_unstable_by_key(|e| e.id);
    SearchIndex {
        names,
        entries,
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
        sort_by: None,
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
        sort_by: None,
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
fn an_excluded_match_is_counted_not_just_dropped() {
    // The failure this prevents: a filtered count presented as the whole truth.
    // `pkg.json` matched the query and only the exclusion kept it out, so the
    // answer owes the caller that number — MCP turns it into "N more inside
    // system, cache, and build folders". A match that was never in scope is NOT
    // counted here (see the include-roots case below).
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
        sort_by: None,
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    assert_eq!(result.total_count, 2, "the excluded row stays out of the results");
    assert_eq!(result.hidden_by_excludes, 1, "and is reported rather than swallowed");
}

#[test]
fn a_match_outside_the_include_roots_is_not_counted_as_hidden() {
    // Scope is the question, not a filter over the answer: the user asked about
    // `projects`, so files elsewhere aren't "hidden matches" they could reveal by
    // flipping a flag. Only exclusions are.
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
        sort_by: None,
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    assert_eq!(result.hidden_by_excludes, 0);
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
        sort_by: None,
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
        sort_by: None,
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    // Should exclude config (under .git) but keep app.rs and pkg.json
    assert_eq!(result.total_count, 2);
    let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"app.rs"));
    assert!(names.contains(&"pkg.json"));
    assert!(!names.contains(&"config"));
}

// ── Newlines in directory names ─────────────────────────────────

/// A two-entry index: one directory whose name contains a newline, holding one file.
/// `/we\nird/note.txt`
fn make_newline_dir_index() -> SearchIndex {
    let mut names = String::new();
    let offsets: Vec<(u32, u16)> = ["", "we\nird", "note.txt"]
        .iter()
        .map(|n| arena_push(&mut names, n))
        .collect();

    let mut entries = vec![
        SearchEntry {
            id: 1,
            parent_id: 0,
            name_offset: offsets[0].0,
            name_len: offsets[0].1,
            is_directory: true,
            size: OptU64::NONE,
            modified_at: OptU64::NONE,
        },
        SearchEntry {
            id: 2,
            parent_id: 1,
            name_offset: offsets[1].0,
            name_len: offsets[1].1,
            is_directory: true,
            size: OptU64::NONE,
            modified_at: OptU64::new(Some(1000)),
        },
        SearchEntry {
            id: 3,
            parent_id: 2,
            name_offset: offsets[2].0,
            name_len: offsets[2].1,
            is_directory: false,
            size: OptU64::new(Some(10)),
            modified_at: OptU64::new(Some(2000)),
        },
    ];
    // Keep the arena in the order the real loader produces (`ORDER BY id`, segments
    // merged in range order): `index_of_id` binary-searches it.
    entries.sort_unstable_by_key(|e| e.id);
    SearchIndex {
        names,
        entries,
        generation: 1,
    }
}

#[test]
fn a_wildcard_exclude_reaches_a_directory_name_with_a_newline_in_it() {
    // Same glob semantics the query bar's patterns get: `*` means any characters,
    // and a newline in a name is one of them. Without that, `!*ird*` would silently
    // fail to exclude the one directory the user typed it for.
    let index = make_newline_dir_index();
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(false),
        include_paths: None,
        exclude_dir_names: Some(vec!["*ird*".to_string()]),
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
        sort_by: None,
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    assert_eq!(result.total_count, 0, "note.txt sits under an excluded directory");
}

#[test]
fn matches_sharing_folders_get_the_same_verdict_as_a_fresh_walk() {
    // The scan memoizes each folder's verdict per chunk, so the second match in a
    // folder answers from the memo. A wrong propagation would leak an excluded file
    // (memo said "fine" below `node_modules`) or hide a kept one (memo said
    // "excluded" for the `b/c` that sits under `x`, not under `node_modules`).
    //
    // /a/node_modules/b/c/f.txt  excluded, walks c, b, node_modules
    // /a/node_modules/b/g.txt    excluded, answered by b's memo
    // /a/x/b/c/h.txt             kept, walks c, b, x, a
    // /a/x/b/i.txt               kept, answered by b's memo
    // /a/node_modules/j.txt      excluded, answered by node_modules' memo
    let rows: [(i64, i64, &str, bool); 13] = [
        (1, 0, "", true),
        (2, 1, "a", true),
        (3, 2, "node_modules", true),
        (4, 3, "b", true),
        (5, 4, "c", true),
        (6, 2, "x", true),
        (7, 6, "b", true),
        (8, 7, "c", true),
        (9, 5, "f.txt", false),
        (10, 4, "g.txt", false),
        (11, 8, "h.txt", false),
        (12, 7, "i.txt", false),
        (13, 3, "j.txt", false),
    ];
    let mut names = String::new();
    let entries = rows
        .iter()
        .map(|&(id, parent_id, name, is_directory)| {
            let (name_offset, name_len) = arena_push(&mut names, name);
            SearchEntry {
                id,
                parent_id,
                name_offset,
                name_len,
                is_directory,
                size: OptU64::NONE,
                modified_at: OptU64::NONE,
            }
        })
        .collect();
    let index = SearchIndex {
        names,
        entries,
        generation: 1,
    };
    let query = SearchQuery {
        name_pattern: Some("*.txt".to_string()),
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
        sort_by: None,
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    let mut kept: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
    kept.sort_unstable();
    assert_eq!(kept, ["h.txt", "i.txt"]);
    assert_eq!(result.hidden_by_excludes, 3);
}
