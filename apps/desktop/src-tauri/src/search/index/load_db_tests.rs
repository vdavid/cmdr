//! The loader against a real index database: what a row's NULLs read back as, and
//! that a load can be sized and cancelled.

use std::sync::atomic::AtomicBool;

use cmdr_index::ReadPool;
use cmdr_index::store::{IndexStore, ROOT_ID};

use super::load_search_index;

/// A hardlink-deduped row (NULL `logical_size`) has to read back as `None`, not as
/// `0`. The index counts an inode's bytes once and stores NULL on every other name,
/// so collapsing the two would change what folder totals and size filters report on
/// a hardlink-heavy tree — silently, and only on the rows nobody looks at twice.
#[test]
fn a_hardlink_deduped_row_loads_back_as_unknown_size() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("test-index.db");
    let _store = IndexStore::open(&db_path).expect("failed to open store");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Two names for one inode, the way the scanner writes them: the first carries the
    // bytes, the second carries NULL. Plus an empty file, which is NOT the same thing.
    let inode = Some(4_242_424_242);
    IndexStore::insert_entry_v2(
        &conn,
        ROOT_ID,
        "first-link",
        false,
        false,
        Some(4096),
        Some(4096),
        Some(1),
        inode,
    )
    .unwrap();
    IndexStore::insert_entry_v2(&conn, ROOT_ID, "second-link", false, false, None, None, Some(1), inode).unwrap();
    IndexStore::insert_entry_v2(&conn, ROOT_ID, "empty.txt", false, false, Some(0), Some(0), None, None).unwrap();

    let pool = ReadPool::new(db_path).unwrap();
    let index = load_search_index(&pool, &AtomicBool::new(false)).unwrap();
    let size_of = |name: &str| {
        index.size(
            (0..index.len())
                .find(|&i| index.name(i) == name)
                .unwrap_or_else(|| panic!("{name} should be in the arena")),
        )
    };

    assert_eq!(
        size_of("first-link"),
        Some(4096),
        "the name carrying the bytes keeps them"
    );
    assert_eq!(
        size_of("second-link"),
        None,
        "a deduped name is sizeless, not zero-sized"
    );
    assert_eq!(
        size_of("empty.txt"),
        Some(0),
        "an empty file is zero-sized, not sizeless"
    );
}

/// A NULL `modified_at` is "unknown", and must not read back as the epoch: the
/// ranker treats unknown as oldest-possible, and `sortBy: modified` sorts unknown
/// keys last rather than pretending they're from 1970.
#[test]
fn an_unknown_modified_time_loads_back_as_none() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("test-index.db");
    let _store = IndexStore::open(&db_path).expect("failed to open store");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    IndexStore::insert_entry_v2(
        &conn,
        ROOT_ID,
        "undated.txt",
        false,
        false,
        Some(7),
        Some(7),
        None,
        None,
    )
    .unwrap();
    IndexStore::insert_entry_v2(
        &conn,
        ROOT_ID,
        "epoch.txt",
        false,
        false,
        Some(7),
        Some(7),
        Some(0),
        None,
    )
    .unwrap();

    let pool = ReadPool::new(db_path).unwrap();
    let index = load_search_index(&pool, &AtomicBool::new(false)).unwrap();
    let modified = |name: &str| {
        index.modified_at(
            (0..index.len())
                .find(|&i| index.name(i) == name)
                .unwrap_or_else(|| panic!("{name} should be in the arena")),
        )
    };

    assert_eq!(modified("undated.txt"), None, "an unknown time stays unknown");
    assert_eq!(modified("epoch.txt"), Some(0), "a real epoch timestamp isn't 'unknown'");
}

// ── Integration test: load from real SQLite DB ───────────────────

#[test]
fn integration_load_and_search() {
    use cmdr_search::engine::search;
    use cmdr_search::ranking::ImportanceWeights;
    use cmdr_search::types::{PatternType, SearchQuery};

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("test-index.db");
    let _store = IndexStore::open(&db_path).expect("failed to open store");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Insert test entries
    let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
    let alice_id =
        IndexStore::insert_entry_v2(&conn, users_id, "test_user_4", true, false, None, None, None, None).unwrap();
    let _pdf_id = IndexStore::insert_entry_v2(
        &conn,
        alice_id,
        "report.pdf",
        false,
        false,
        Some(1_000_000),
        Some(1_000_000),
        Some(1700000000),
        None,
    )
    .unwrap();
    let _txt_id = IndexStore::insert_entry_v2(
        &conn,
        alice_id,
        "notes.txt",
        false,
        false,
        Some(500),
        Some(500),
        Some(1700000100),
        None,
    )
    .unwrap();

    // Load the index using ReadPool
    let pool = ReadPool::new(db_path).unwrap();
    let cancel = AtomicBool::new(false);
    let index = load_search_index(&pool, &cancel).unwrap();

    // Root sentinel + 4 entries
    assert_eq!(index.len(), 5);

    // Search for PDFs
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
        sort_by: None,
    };
    let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
    assert_eq!(result.total_count, 1);
    assert_eq!(result.entries[0].name, "report.pdf");
    assert_eq!(result.entries[0].path, "/Users/test_user_4/report.pdf");
}

#[test]
fn load_rightsizes_arena_from_row_count() {
    // A small index must not pre-allocate the ~5M-entry / ~100 MB worst-case
    // arena. Capacity should track the actual row count.
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("test-index.db");
    let _store = IndexStore::open(&db_path).expect("failed to open store");

    let pool = ReadPool::new(db_path).unwrap();
    let cancel = AtomicBool::new(false);
    let index = load_search_index(&pool, &cancel).unwrap();

    // Root sentinel only: before right-sizing this was Vec::with_capacity(5_000_000).
    assert!(
        index.len() + index.spare_rows_left() < 1000,
        "entries capacity {} should track the row count, not the 5M worst case",
        index.len() + index.spare_rows_left()
    );
}

#[test]
fn load_cancellation() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("test-index.db");
    let _store = IndexStore::open(&db_path).expect("failed to open store");

    let pool = ReadPool::new(db_path).unwrap();
    let cancel = AtomicBool::new(true); // Pre-cancelled
    let result = load_search_index(&pool, &cancel);
    // With only the root sentinel, cancellation check happens at row 0, but CANCEL_CHECK_INTERVAL
    // is 100K so the first check is at row 0 (0 % 100K == 0). The load should be cancelled.
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("cancelled"));
}
