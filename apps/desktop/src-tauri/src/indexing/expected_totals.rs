//! Index-derived expected totals for write operations.
//!
//! Looks up pre-aggregated `dir_stats` for a set of source paths so the FE
//! can render a real progress bar from second one of a scan, before the
//! foolproof re-scan finishes. The re-scan still runs and the verified
//! totals replace these when complete.
//!
//! Returns `None` if the index isn't covering all sources: partial totals
//! would be misleading.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::enrichment::get_read_pool;
use super::firmlinks;
use super::store::{self, EntryRow, IndexStore};

/// Aggregate "what the scan is expected to find" totals, sourced from the
/// index's pre-computed `dir_stats`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExpectedTotals {
    pub files: u64,
    pub bytes: u64,
}

/// Look up index-derived expected totals for a set of source paths.
///
/// Returns `None` when the index isn't available, or when any source isn't
/// covered (directory not yet indexed, missing `dir_stats`, or file without a
/// recorded `logical_size`). A partial total would underrepresent the
/// denominator and let the progress bar shoot past 100%; better to fall back
/// to a tally-only display.
pub fn expected_totals_for_sources(sources: &[PathBuf]) -> Option<ExpectedTotals> {
    let pool = get_read_pool()?;
    pool.with_conn(|conn| sum_expected_totals(conn, sources)).ok()?
}

/// Pure implementation: given a SQLite connection and source paths, sum the
/// expected totals. Returns `None` for empty input or if any source isn't
/// covered. Split out so tests can use a temp DB without the global read pool.
pub(crate) fn sum_expected_totals(conn: &Connection, sources: &[PathBuf]) -> Option<ExpectedTotals> {
    if sources.is_empty() {
        return None;
    }
    let mut totals = ExpectedTotals::default();
    for source in sources {
        let contribution = per_source_contribution(conn, source)?;
        totals.files = totals.files.checked_add(contribution.files)?;
        totals.bytes = totals.bytes.checked_add(contribution.bytes)?;
    }
    Some(totals)
}

fn per_source_contribution(conn: &Connection, source: &Path) -> Option<ExpectedTotals> {
    let normalized = firmlinks::normalize_path(&source.to_string_lossy());
    let entry_id = store::resolve_path(conn, &normalized).ok().flatten()?;
    let entry: EntryRow = IndexStore::get_entry_by_id(conn, entry_id).ok().flatten()?;

    if entry.is_directory && !entry.is_symlink {
        let stats = IndexStore::get_dir_stats_by_id(conn, entry_id).ok().flatten()?;
        Some(ExpectedTotals {
            files: stats.recursive_file_count,
            bytes: stats.recursive_logical_size,
        })
    } else {
        // File or symlink: count as one item. Symlinks contribute their own
        // size, never the target's, matching what the walker does.
        let size = entry.logical_size?;
        Some(ExpectedTotals { files: 1, bytes: size })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{DirStatsById, IndexStore, ROOT_ID};
    use rusqlite::Connection;
    use tempfile::TempDir;

    /// Open a fresh write connection backed by a temp DB file. Returns the
    /// connection plus the `TempDir` (kept alive by the caller).
    fn open_test_conn() -> (Connection, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test-index.db");
        // `open` initializes the schema and root sentinel.
        let _store = IndexStore::open(&db_path).unwrap();
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        (conn, dir)
    }

    fn insert_dir(conn: &Connection, parent_id: i64, name: &str) -> i64 {
        IndexStore::insert_entry_v2(conn, parent_id, name, true, false, None, None, None, None).unwrap()
    }

    fn insert_file(conn: &Connection, parent_id: i64, name: &str, size: Option<u64>) -> i64 {
        IndexStore::insert_entry_v2(conn, parent_id, name, false, false, size, size, None, None).unwrap()
    }

    fn upsert_stats(conn: &Connection, id: i64, files: u64, bytes: u64) {
        IndexStore::upsert_dir_stats_by_id(
            conn,
            &[DirStatsById {
                entry_id: id,
                recursive_logical_size: bytes,
                recursive_physical_size: bytes,
                recursive_file_count: files,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            }],
        )
        .unwrap();
    }

    #[test]
    fn empty_input_returns_none() {
        let (conn, _dir) = open_test_conn();
        assert_eq!(sum_expected_totals(&conn, &[]), None);
    }

    #[test]
    fn single_indexed_dir_returns_dir_stats() {
        let (conn, _dir) = open_test_conn();
        let users_id = insert_dir(&conn, ROOT_ID, "Users");
        let alice_id = insert_dir(&conn, users_id, "alice");
        upsert_stats(&conn, alice_id, 42, 1024 * 1024);

        let totals = sum_expected_totals(&conn, &[PathBuf::from("/Users/alice")]).unwrap();
        assert_eq!(totals.files, 42);
        assert_eq!(totals.bytes, 1024 * 1024);
    }

    #[test]
    fn single_indexed_file_counts_as_one_item() {
        let (conn, _dir) = open_test_conn();
        let users_id = insert_dir(&conn, ROOT_ID, "Users");
        insert_file(&conn, users_id, "readme.md", Some(500));

        let totals = sum_expected_totals(&conn, &[PathBuf::from("/Users/readme.md")]).unwrap();
        assert_eq!(totals.files, 1);
        assert_eq!(totals.bytes, 500);
    }

    #[test]
    fn mixed_files_and_dirs_sum() {
        let (conn, _dir) = open_test_conn();
        let users_id = insert_dir(&conn, ROOT_ID, "Users");
        let alice_id = insert_dir(&conn, users_id, "alice");
        upsert_stats(&conn, alice_id, 100, 5000);
        insert_file(&conn, users_id, "note.txt", Some(200));

        let totals = sum_expected_totals(
            &conn,
            &[PathBuf::from("/Users/alice"), PathBuf::from("/Users/note.txt")],
        )
        .unwrap();
        assert_eq!(totals.files, 101);
        assert_eq!(totals.bytes, 5200);
    }

    #[test]
    fn missing_path_returns_none() {
        let (conn, _dir) = open_test_conn();
        let users_id = insert_dir(&conn, ROOT_ID, "Users");
        let alice_id = insert_dir(&conn, users_id, "alice");
        upsert_stats(&conn, alice_id, 1, 1);

        let totals = sum_expected_totals(
            &conn,
            &[PathBuf::from("/Users/alice"), PathBuf::from("/Users/never-indexed")],
        );
        assert_eq!(totals, None);
    }

    #[test]
    fn dir_without_stats_returns_none() {
        let (conn, _dir) = open_test_conn();
        let users_id = insert_dir(&conn, ROOT_ID, "Users");
        insert_dir(&conn, users_id, "alice");

        let totals = sum_expected_totals(&conn, &[PathBuf::from("/Users/alice")]);
        assert_eq!(totals, None);
    }

    #[test]
    fn file_without_logical_size_returns_none() {
        let (conn, _dir) = open_test_conn();
        let users_id = insert_dir(&conn, ROOT_ID, "Users");
        insert_file(&conn, users_id, "no-size.bin", None);

        let totals = sum_expected_totals(&conn, &[PathBuf::from("/Users/no-size.bin")]);
        assert_eq!(totals, None);
    }
}
