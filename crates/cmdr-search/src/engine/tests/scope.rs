use super::*;
use crate::arena::{ArenaBuilder, arena_of};
use crate::types::PatternType;

// ── Scope filtering in search ───────────────────────────────────

/// Build a test index representing:
/// /Users/alice/projects/app.rs         (id=9)
/// /Users/alice/projects/node_modules/pkg.json (id=11)
/// /Users/alice/.git/config             (id=13)
fn make_scope_test_index() -> SearchIndex {
    const NAMES: [&str; 9] = [
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
    arena_of(
        &[
            (1, 0, NAMES[0], true, None, None),
            (2, 1, NAMES[1], true, None, Some(1000)),
            (3, 2, NAMES[2], true, None, Some(2000)),
            (4, 3, NAMES[3], true, None, Some(3000)),
            (9, 4, NAMES[4], false, Some(1000), Some(4000)),
            (10, 4, NAMES[5], true, None, Some(5000)),
            (11, 10, NAMES[6], false, Some(500), Some(6000)),
            (12, 3, NAMES[7], true, None, Some(7000)),
            (13, 12, NAMES[8], false, Some(200), Some(8000)),
        ],
        1,
    )
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

/// An exclusion the user typed that can't be compiled must fail the search, never be
/// dropped: a dropped exclude answers with exactly the folders the user asked to hide.
/// A glob this long blows the regex crate's compiled-size limit.
#[test]
fn an_exclude_glob_that_cannot_compile_fails_the_search() {
    let index = make_scope_test_index();
    let huge_glob = format!("{}*", "?".repeat(200_000));
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: Some(false),
        include_paths: None,
        exclude_dir_names: Some(vec![huge_glob]),
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
        sort_by: None,
    };
    let error = search(&index, &query, &ImportanceWeights::empty()).expect_err("an uncompilable exclude must fail");
    assert!(!error.is_empty());
}

// ── Newlines in directory names ─────────────────────────────────

/// A two-entry index: one directory whose name contains a newline, holding one file.
/// `/we\nird/note.txt`
fn make_newline_dir_index() -> SearchIndex {
    const NAMES: [&str; 3] = ["", "we\nird", "note.txt"];
    arena_of(
        &[
            (1, 0, NAMES[0], true, None, None),
            (2, 1, NAMES[1], true, None, Some(1000)),
            (3, 2, NAMES[2], false, Some(10), Some(2000)),
        ],
        1,
    )
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
    let mut builder = ArenaBuilder::new();
    for &(id, parent_id, name, is_directory) in &rows {
        builder.push(id, parent_id, name, is_directory, None, None);
    }
    let index = builder.finish(1);
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
