//! Shared helpers for indexing stress tests.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::file_system::listing::FileEntry;
use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
use crate::indexing::writer::IndexWriter;

/// Spawn writer + open read connection against a fresh temp DB.
pub fn setup_writer() -> (IndexWriter, Connection, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("stress-test.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
    let read_conn = IndexStore::open_read_connection(&db_path).expect("open read conn");
    (writer, read_conn, dir)
}

/// Build a synthetic tree of `EntryRow`s with correct parent/child IDs.
///
/// Shape: `levels` deep, `dirs_per_level` directories at each level,
/// `files_per_dir` files in each directory. File sizes are `file_size` bytes.
/// IDs start at 2 (ROOT_ID = 1 is the root sentinel).
pub fn build_synthetic_tree(
    levels: usize,
    dirs_per_level: usize,
    files_per_dir: usize,
    file_size: u64,
) -> Vec<EntryRow> {
    let mut entries = Vec::new();
    let mut next_id: i64 = 2;

    // Track directories at each level as (id, depth) so we can build children.
    // Start with ROOT_ID as the sole parent at depth 0.
    let mut current_parents: Vec<i64> = vec![ROOT_ID];

    for depth in 0..levels {
        let mut next_parents: Vec<i64> = Vec::new();

        for &parent_id in &current_parents {
            // Create directories at this level
            for d in 0..dirs_per_level {
                let dir_id = next_id;
                next_id += 1;
                entries.push(EntryRow {
                    id: dir_id,
                    parent_id,
                    name: format!("dir_L{depth}_D{d}"),
                    is_directory: true,
                    is_symlink: false,
                    logical_size: None,
                    physical_size: None,
                    modified_at: None,
                    inode: None,
                });
                next_parents.push(dir_id);
            }

            // Create files in this parent
            for f in 0..files_per_dir {
                let file_id = next_id;
                next_id += 1;
                entries.push(EntryRow {
                    id: file_id,
                    parent_id,
                    name: format!("file_L{depth}_F{f}.dat"),
                    is_directory: false,
                    is_symlink: false,
                    logical_size: Some(file_size),
                    physical_size: Some(file_size),
                    modified_at: Some(1_700_000_000),
                    inode: None,
                });
            }
        }

        current_parents = next_parents;
    }

    // Also add files to leaf directories (the last level's dirs have no children yet)
    for &parent_id in &current_parents {
        for f in 0..files_per_dir {
            let file_id = next_id;
            next_id += 1;
            entries.push(EntryRow {
                id: file_id,
                parent_id,
                name: format!("file_leaf_F{f}.dat"),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(file_size),
                physical_size: Some(file_size),
                modified_at: Some(1_700_000_000),
                inode: None,
            });
        }
    }

    entries
}

/// Verify DB consistency invariants after a test.
///
/// Checks:
/// 1. Every entry's parent_id points to an existing directory (or ROOT_PARENT_ID for the sentinel)
/// 2. Every directory has a dir_stats row
/// 3. dir_stats.recursive_size matches actual sum of descendant file sizes
/// 4. dir_stats.recursive_file_count and recursive_dir_count match actual counts
/// 5. No duplicate (parent_id, name) pairs
pub fn check_db_consistency(conn: &Connection) {
    // 1. Every entry's parent_id references an existing directory
    let orphans: Vec<(i64, i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.parent_id, e.name FROM entries e
                 WHERE e.parent_id != 0
                   AND NOT EXISTS (
                     SELECT 1 FROM entries p WHERE p.id = e.parent_id AND p.is_directory = 1
                   )",
            )
            .unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert!(
        orphans.is_empty(),
        "orphaned entries (parent_id points to non-existent directory): {orphans:?}"
    );

    // 2. Every directory has a dir_stats row
    let dirs_without_stats: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.name FROM entries e
                 WHERE e.is_directory = 1
                   AND NOT EXISTS (SELECT 1 FROM dir_stats ds WHERE ds.entry_id = e.id)",
            )
            .unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert!(
        dirs_without_stats.is_empty(),
        "directories missing dir_stats rows: {dirs_without_stats:?}"
    );

    // 3 & 4. dir_stats values match actual descendant counts.
    // Build in-memory tree, then compute expected stats bottom-up.
    let all_entries: Vec<EntryRow> = {
        let mut stmt = conn
            .prepare("SELECT id, parent_id, name, is_directory, is_symlink, logical_size, physical_size, modified_at, inode FROM entries")
            .unwrap();
        stmt.query_map([], |row| {
            Ok(EntryRow {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                is_directory: row.get::<_, i32>(3)? != 0,
                is_symlink: row.get::<_, i32>(4)? != 0,
                logical_size: row.get(5)?,
                physical_size: row.get(6)?,
                modified_at: row.get(7)?,
                inode: row.get(8)?,
            })
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    };

    // Build parent -> children map
    let mut children_map: HashMap<i64, Vec<&EntryRow>> = HashMap::new();
    for entry in &all_entries {
        children_map.entry(entry.parent_id).or_default().push(entry);
    }

    // Recursive function to compute expected stats
    fn compute_expected(entry_id: i64, children_map: &HashMap<i64, Vec<&EntryRow>>) -> (u64, u64, u64) {
        // (recursive_size, recursive_file_count, recursive_dir_count)
        let children = match children_map.get(&entry_id) {
            Some(c) => c,
            None => return (0, 0, 0),
        };

        let mut logical_size: u64 = 0;
        let mut file_count: u64 = 0;
        let mut dir_count: u64 = 0;

        for child in children {
            if child.is_directory {
                dir_count += 1;
                let (s, fc, dc) = compute_expected(child.id, children_map);
                logical_size += s;
                file_count += fc;
                dir_count += dc;
            } else {
                file_count += 1;
                logical_size += child.logical_size.unwrap_or(0);
            }
        }

        (logical_size, file_count, dir_count)
    }

    // Check each directory's dir_stats
    for entry in &all_entries {
        if !entry.is_directory {
            continue;
        }
        let stats = IndexStore::get_dir_stats_by_id(conn, entry.id)
            .unwrap()
            .unwrap_or_else(|| panic!("dir_stats missing for entry id={}, name={}", entry.id, entry.name));

        let (expected_size, expected_files, expected_dirs) = compute_expected(entry.id, &children_map);

        assert_eq!(
            stats.recursive_logical_size, expected_size,
            "dir_stats.recursive_logical_size mismatch for id={}, name='{}': got {}, expected {}",
            entry.id, entry.name, stats.recursive_logical_size, expected_size
        );
        assert_eq!(
            stats.recursive_file_count, expected_files,
            "dir_stats.recursive_file_count mismatch for id={}, name='{}': got {}, expected {}",
            entry.id, entry.name, stats.recursive_file_count, expected_files
        );
        assert_eq!(
            stats.recursive_dir_count, expected_dirs,
            "dir_stats.recursive_dir_count mismatch for id={}, name='{}': got {}, expected {}",
            entry.id, entry.name, stats.recursive_dir_count, expected_dirs
        );
    }

    // 5. No duplicate (parent_id, name) pairs
    let duplicates: Vec<(i64, String, i64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT parent_id, name, COUNT(*) as cnt FROM entries
                 GROUP BY parent_id, name COLLATE platform_case
                 HAVING cnt > 1",
            )
            .unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert!(
        duplicates.is_empty(),
        "duplicate (parent_id, name) pairs: {duplicates:?}"
    );
}

/// Create a `FileEntry` for enrichment testing.
pub fn make_file_entry(name: &str, path: &str, is_directory: bool) -> FileEntry {
    FileEntry {
        size: if is_directory { None } else { Some(100) },
        permissions: 0o755,
        ..FileEntry::new(name.to_string(), path.to_string(), is_directory, false)
    }
}
