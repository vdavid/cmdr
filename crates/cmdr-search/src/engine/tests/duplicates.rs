//! One path, one row: the arena can hold a path twice, and a result list can't.
//!
//! The arena is read without one snapshot across its load workers, and a warm one
//! keeps rows the index has since deleted (the host's `search/DETAILS.md` §
//! "Decision 12"). So a file deleted and created again can sit in it under both its
//! old id and its new one. Every consumer keys a row by its path, and the dialog's
//! list throws on a repeated key, so the engine is where the repeat stops.

use super::*;
use crate::types::PatternType;

fn query(pattern: &str) -> SearchQuery {
    SearchQuery {
        name_pattern: Some(pattern.to_string()),
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
    }
}

fn paths(result: &crate::types::SearchResult) -> Vec<&str> {
    result.entries.iter().map(|e| e.path.as_str()).collect()
}

#[test]
fn a_file_the_arena_holds_twice_is_returned_once_as_its_newest_row() {
    // `notes.txt` was deleted and created again: id 4 is the row that went, id 6 the
    // one that's there now.
    let index = arena_of(
        &[
            (1, 0, "", true, None, None),
            (2, 1, "docs", true, None, Some(1000)),
            (4, 2, "notes.txt", false, Some(100), Some(2000)),
            (5, 2, "other.txt", false, Some(300), Some(2500)),
            (6, 2, "notes.txt", false, Some(200), Some(3000)),
        ],
        1,
    );

    let result = search(&index, &query("notes.txt"), &ImportanceWeights::empty()).unwrap();

    assert_eq!(paths(&result), ["/docs/notes.txt"]);
    assert_eq!(result.entries[0].entry_id, 6, "the row that exists now");
    assert_eq!(result.entries[0].size, Some(200));
    assert_eq!(result.total_count, 1, "the count is of files, and it's one file");
}

#[test]
fn a_file_under_a_folder_the_arena_holds_twice_is_returned_once() {
    // The folder itself was created again, so the same path is reached through two
    // different parents.
    let index = arena_of(
        &[
            (1, 0, "", true, None, None),
            (2, 1, "docs", true, None, Some(1000)),
            (3, 2, "notes.txt", false, Some(100), Some(2000)),
            (4, 1, "docs", true, None, Some(3000)),
            (5, 4, "notes.txt", false, Some(200), Some(4000)),
        ],
        1,
    );

    let result = search(&index, &query("notes.txt"), &ImportanceWeights::empty()).unwrap();

    assert_eq!(paths(&result), ["/docs/notes.txt"]);
    assert_eq!(result.entries[0].entry_id, 5);
    assert_eq!(result.total_count, 1);
}

#[test]
fn rows_with_distinct_paths_keep_their_ranked_order() {
    let index = make_test_index();
    let result = search(&index, &query("*.pdf"), &ImportanceWeights::empty()).unwrap();
    assert_eq!(
        paths(&result),
        ["/Users/Documents/Q1-report.pdf", "/Users/test_user_4/report.pdf"],
        "newest first, untouched"
    );
    assert_eq!(result.total_count, 2);
}
