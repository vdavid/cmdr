//! Dir stats computation: bottom-up aggregation and incremental delta propagation.
//!
//! Three modes:
//! - **Full aggregation**: after a full scan, compute `dir_stats` for every directory (deepest first).
//! - **Subtree aggregation**: after a micro-scan, compute `dir_stats` only under a given root.
//! - **Delta propagation**: after a watcher event, walk up the ancestor chain updating counts.

use std::cmp::Reverse;
use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::indexing::store::{DirStats, IndexStore, IndexStoreError};

/// Compute `dir_stats` for ALL directories in the DB (bottom-up, deepest first).
///
/// Called after a full scan completes. Uses an in-memory map to avoid repeated DB reads
/// for child directory stats. Also computes a synthetic root (`/`) entry by summing
/// all top-level entries, since the scanner skips depth-0 and `/` is never in the
/// entries table. Returns the number of directories processed.
pub fn compute_all_aggregates(conn: &Connection) -> Result<u64, IndexStoreError> {
    let all_dirs = IndexStore::get_all_directory_paths(conn)?;
    if all_dirs.is_empty() {
        return Ok(0);
    }
    let count = compute_aggregates_for_dirs(conn, &all_dirs)?;

    // Compute root `/` dir_stats by summing top-level entries. The scanner skips
    // depth-0, so `/` itself is never in the entries table and won't be covered
    // by the bottom-up pass above.
    compute_root_stats(conn)?;

    Ok(count + 1) // +1 for the root entry
}

/// Compute `dir_stats` for directories under `root` only (bottom-up).
///
/// Called after a micro-scan completes. Returns the number of directories processed.
pub fn compute_subtree_aggregates(conn: &Connection, root: &str) -> Result<u64, IndexStoreError> {
    let dirs = IndexStore::get_directory_paths_under(conn, root)?;
    if dirs.is_empty() {
        return Ok(0);
    }
    compute_aggregates_for_dirs(conn, &dirs)
}

/// Propagate a size/count delta up the ancestor chain.
///
/// Called when a file is added, removed, or modified. Walks from the parent of the
/// given path up to the root, updating each ancestor's `dir_stats` in a single transaction.
pub fn propagate_delta(
    conn: &Connection,
    path: &str,
    size_delta: i64,
    file_count_delta: i32,
    dir_count_delta: i32,
) -> Result<(), IndexStoreError> {
    let tx = conn.unchecked_transaction()?;

    let mut current = parent_path(path);
    while let Some(ancestor) = current {
        // Try to read existing stats
        let mut stmt = tx.prepare_cached(
            "SELECT recursive_size, recursive_file_count, recursive_dir_count
             FROM dir_stats WHERE path = ?1",
        )?;
        let existing = stmt
            .query_row(params![ancestor], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
            })
            .ok();

        let (new_size, new_file_count, new_dir_count) = match existing {
            Some((size, files, dirs)) => (
                (size + size_delta).max(0),
                (files + i64::from(file_count_delta)).max(0),
                (dirs + i64::from(dir_count_delta)).max(0),
            ),
            None => (
                size_delta.max(0),
                i64::from(file_count_delta).max(0),
                i64::from(dir_count_delta).max(0),
            ),
        };

        tx.execute(
            "INSERT OR REPLACE INTO dir_stats
                 (path, recursive_size, recursive_file_count, recursive_dir_count)
             VALUES (?1, ?2, ?3, ?4)",
            params![ancestor, new_size, new_file_count, new_dir_count],
        )?;

        current = parent_path(&ancestor);
    }

    tx.commit()?;
    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────

/// Compute dir_stats for the root `/` by summing direct children stats and their
/// already-computed recursive dir_stats.
fn compute_root_stats(conn: &Connection) -> Result<(), IndexStoreError> {
    let (file_size_sum, file_count, child_dir_count) = IndexStore::get_children_stats(conn, "/")?;
    let child_dirs = get_child_directory_paths(conn, "/")?;

    let mut recursive_size = file_size_sum;
    let mut recursive_file_count = file_count;
    let mut recursive_dir_count = child_dir_count;

    // Add already-computed recursive stats from child directories
    for child_dir in &child_dirs {
        let mut stmt = conn.prepare_cached(
            "SELECT recursive_size, recursive_file_count, recursive_dir_count
             FROM dir_stats WHERE path = ?1",
        )?;
        if let Ok((size, files, dirs)) = stmt.query_row(params![child_dir], |row| {
            Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?, row.get::<_, u64>(2)?))
        }) {
            recursive_size += size;
            recursive_file_count += files;
            recursive_dir_count += dirs;
        }
    }

    IndexStore::upsert_dir_stats(
        conn,
        &[DirStats {
            path: "/".to_string(),
            recursive_size,
            recursive_file_count,
            recursive_dir_count,
        }],
    )?;

    Ok(())
}

/// Sort directories by depth (deepest first), compute stats bottom-up using
/// an in-memory map, then batch-write results.
fn compute_aggregates_for_dirs(conn: &Connection, dirs: &[String]) -> Result<u64, IndexStoreError> {
    // Sort by depth descending (most '/' characters first)
    let mut sorted: Vec<&str> = dirs.iter().map(String::as_str).collect();
    sorted.sort_by_key(|p| Reverse(depth(p)));

    // In-memory map of computed stats: avoids re-reading child dir stats from DB
    let mut computed: HashMap<&str, DirStats> = HashMap::with_capacity(sorted.len());

    for dir_path in &sorted {
        // Get direct children stats (file sizes, file count, subdir count)
        let (file_size_sum, file_count, child_dir_count) = IndexStore::get_children_stats(conn, dir_path)?;

        // Get child directory paths so we can look up their computed recursive stats
        let child_dirs = get_child_directory_paths(conn, dir_path)?;

        let mut recursive_size = file_size_sum;
        let mut recursive_file_count = file_count;
        let mut recursive_dir_count = child_dir_count;

        for child_dir in &child_dirs {
            if let Some(child_stats) = computed.get(child_dir.as_str()) {
                recursive_size += child_stats.recursive_size;
                recursive_file_count += child_stats.recursive_file_count;
                recursive_dir_count += child_stats.recursive_dir_count;
            }
        }

        computed.insert(
            dir_path,
            DirStats {
                path: (*dir_path).to_string(),
                recursive_size,
                recursive_file_count,
                recursive_dir_count,
            },
        );
    }

    // Batch-write all computed stats in chunks of 1000
    let all_stats: Vec<DirStats> = computed.into_values().collect();
    let count = all_stats.len() as u64;

    for chunk in all_stats.chunks(1000) {
        IndexStore::upsert_dir_stats(conn, chunk)?;
    }

    Ok(count)
}

/// Get paths of direct child directories for a parent path.
fn get_child_directory_paths(conn: &Connection, parent: &str) -> Result<Vec<String>, IndexStoreError> {
    let mut stmt = conn.prepare_cached("SELECT path FROM entries WHERE parent_path = ?1 AND is_directory = 1")?;
    let rows = stmt.query_map(params![parent], |row| row.get(0))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Count the depth of a path (number of '/' characters).
fn depth(path: &str) -> usize {
    path.chars().filter(|&c| c == '/').count()
}

/// Extract the parent path, returning `None` for root paths like "/" or "".
fn parent_path(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        return None;
    }
    match path.rfind('/') {
        Some(0) => {
            // Parent is root "/"
            if path == "/" { None } else { Some("/".to_string()) }
        }
        Some(pos) => Some(path[..pos].to_string()),
        None => None,
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{IndexStore, ScannedEntry};

    /// Open a write connection to a temp DB with schema initialized.
    fn open_temp_conn() -> (Connection, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let store = IndexStore::open(&db_path).expect("failed to open store");
        let conn = IndexStore::open_write_connection(store.db_path()).expect("failed to open write conn");
        // Drop store so the read connection is closed; we only need the write conn for tests
        drop(store);
        (conn, dir)
    }

    /// Insert a batch of test entries.
    fn insert_entries(conn: &Connection, entries: &[ScannedEntry]) {
        IndexStore::insert_entries_batch(conn, entries).expect("insert failed");
    }

    fn make_dir(path: &str, parent: &str, name: &str) -> ScannedEntry {
        ScannedEntry {
            path: path.into(),
            parent_path: parent.into(),
            name: name.into(),
            is_directory: true,
            is_symlink: false,
            size: None,
            modified_at: None,
        }
    }

    fn make_file(path: &str, parent: &str, name: &str, size: u64) -> ScannedEntry {
        ScannedEntry {
            path: path.into(),
            parent_path: parent.into(),
            name: name.into(),
            is_directory: false,
            is_symlink: false,
            size: Some(size),
            modified_at: None,
        }
    }

    fn get_stats(conn: &Connection, path: &str) -> Option<DirStats> {
        let mut stmt = conn
            .prepare(
                "SELECT path, recursive_size, recursive_file_count, recursive_dir_count FROM dir_stats WHERE path = ?1",
            )
            .unwrap();
        stmt.query_row(params![path], |row| {
            Ok(DirStats {
                path: row.get(0)?,
                recursive_size: row.get(1)?,
                recursive_file_count: row.get(2)?,
                recursive_dir_count: row.get(3)?,
            })
        })
        .ok()
    }

    // ── parent_path tests ────────────────────────────────────────────

    #[test]
    fn parent_path_of_nested() {
        assert_eq!(parent_path("/a/b/c"), Some("/a/b".to_string()));
    }

    #[test]
    fn parent_path_of_top_level() {
        assert_eq!(parent_path("/a"), Some("/".to_string()));
    }

    #[test]
    fn parent_path_of_root() {
        assert_eq!(parent_path("/"), None);
    }

    #[test]
    fn parent_path_of_empty() {
        assert_eq!(parent_path(""), None);
    }

    // ── compute_all_aggregates tests ─────────────────────────────────

    #[test]
    fn aggregate_simple_tree() {
        let (conn, _dir) = open_temp_conn();

        // Tree: /root
        //       /root/a.txt (100 bytes)
        //       /root/b.txt (200 bytes)
        //       /root/sub/
        //       /root/sub/c.txt (50 bytes)
        insert_entries(
            &conn,
            &[
                make_dir("/root", "/", "root"),
                make_file("/root/a.txt", "/root", "a.txt", 100),
                make_file("/root/b.txt", "/root", "b.txt", 200),
                make_dir("/root/sub", "/root", "sub"),
                make_file("/root/sub/c.txt", "/root/sub", "c.txt", 50),
            ],
        );

        let count = compute_all_aggregates(&conn).unwrap();
        assert_eq!(count, 3); // 2 directories + 1 synthetic root

        let sub_stats = get_stats(&conn, "/root/sub").unwrap();
        assert_eq!(sub_stats.recursive_size, 50);
        assert_eq!(sub_stats.recursive_file_count, 1);
        assert_eq!(sub_stats.recursive_dir_count, 0);

        let root_stats = get_stats(&conn, "/root").unwrap();
        assert_eq!(root_stats.recursive_size, 350); // 100 + 200 + 50
        assert_eq!(root_stats.recursive_file_count, 3);
        assert_eq!(root_stats.recursive_dir_count, 1);

        // Root "/" should now have stats summing all top-level entries
        let volume_root_stats = get_stats(&conn, "/").unwrap();
        assert_eq!(volume_root_stats.recursive_size, 350);
        assert_eq!(volume_root_stats.recursive_file_count, 3);
        assert_eq!(volume_root_stats.recursive_dir_count, 2); // /root + /root/sub
    }

    #[test]
    fn aggregate_deep_tree() {
        let (conn, _dir) = open_temp_conn();

        // Tree: /a/b/c/d/file.txt (1000 bytes)
        insert_entries(
            &conn,
            &[
                make_dir("/a", "/", "a"),
                make_dir("/a/b", "/a", "b"),
                make_dir("/a/b/c", "/a/b", "c"),
                make_dir("/a/b/c/d", "/a/b/c", "d"),
                make_file("/a/b/c/d/file.txt", "/a/b/c/d", "file.txt", 1000),
            ],
        );

        compute_all_aggregates(&conn).unwrap();

        // Each ancestor should have the file's size propagated up
        for path in &["/a/b/c/d", "/a/b/c", "/a/b", "/a"] {
            let stats = get_stats(&conn, path).unwrap();
            assert_eq!(stats.recursive_size, 1000, "wrong size for {path}");
            assert_eq!(stats.recursive_file_count, 1, "wrong file count for {path}");
        }

        // Dir counts should increase as we go up
        assert_eq!(get_stats(&conn, "/a/b/c/d").unwrap().recursive_dir_count, 0);
        assert_eq!(get_stats(&conn, "/a/b/c").unwrap().recursive_dir_count, 1);
        assert_eq!(get_stats(&conn, "/a/b").unwrap().recursive_dir_count, 2);
        assert_eq!(get_stats(&conn, "/a").unwrap().recursive_dir_count, 3);
    }

    #[test]
    fn aggregate_empty_db() {
        let (conn, _dir) = open_temp_conn();
        let count = compute_all_aggregates(&conn).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn aggregate_dir_with_no_files() {
        let (conn, _dir) = open_temp_conn();

        insert_entries(&conn, &[make_dir("/empty", "/", "empty")]);

        compute_all_aggregates(&conn).unwrap();

        let stats = get_stats(&conn, "/empty").unwrap();
        assert_eq!(stats.recursive_size, 0);
        assert_eq!(stats.recursive_file_count, 0);
        assert_eq!(stats.recursive_dir_count, 0);
    }

    // ── compute_subtree_aggregates tests ─────────────────────────────

    #[test]
    fn subtree_aggregation() {
        let (conn, _dir) = open_temp_conn();

        // Two separate subtrees under /
        insert_entries(
            &conn,
            &[
                make_dir("/a", "/", "a"),
                make_file("/a/f.txt", "/a", "f.txt", 100),
                make_dir("/b", "/", "b"),
                make_dir("/b/sub", "/b", "sub"),
                make_file("/b/sub/g.txt", "/b/sub", "g.txt", 200),
            ],
        );

        // Only aggregate /b subtree
        let count = compute_subtree_aggregates(&conn, "/b").unwrap();
        assert_eq!(count, 2); // /b and /b/sub

        // /b/sub should have stats
        let sub_stats = get_stats(&conn, "/b/sub").unwrap();
        assert_eq!(sub_stats.recursive_size, 200);

        // /b should have stats
        let b_stats = get_stats(&conn, "/b").unwrap();
        assert_eq!(b_stats.recursive_size, 200);
        assert_eq!(b_stats.recursive_file_count, 1);
        assert_eq!(b_stats.recursive_dir_count, 1);

        // /a should NOT have stats (not in subtree)
        assert!(get_stats(&conn, "/a").is_none());
    }

    #[test]
    fn subtree_aggregation_nonexistent_root() {
        let (conn, _dir) = open_temp_conn();
        let count = compute_subtree_aggregates(&conn, "/nonexistent").unwrap();
        assert_eq!(count, 0);
    }

    // ── propagate_delta tests ────────────────────────────────────────

    #[test]
    fn propagate_file_added() {
        let (conn, _dir) = open_temp_conn();

        // Pre-populate dir_stats for ancestors
        IndexStore::upsert_dir_stats(
            &conn,
            &[
                DirStats {
                    path: "/a".into(),
                    recursive_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 0,
                },
                DirStats {
                    path: "/a/b".into(),
                    recursive_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 0,
                },
            ],
        )
        .unwrap();

        // Add a file at /a/b/new.txt (500 bytes)
        propagate_delta(&conn, "/a/b/new.txt", 500, 1, 0).unwrap();

        let b_stats = get_stats(&conn, "/a/b").unwrap();
        assert_eq!(b_stats.recursive_size, 500);
        assert_eq!(b_stats.recursive_file_count, 1);

        let a_stats = get_stats(&conn, "/a").unwrap();
        assert_eq!(a_stats.recursive_size, 500);
        assert_eq!(a_stats.recursive_file_count, 1);
    }

    #[test]
    fn propagate_file_removed() {
        let (conn, _dir) = open_temp_conn();

        // Pre-populate with existing data
        IndexStore::upsert_dir_stats(
            &conn,
            &[
                DirStats {
                    path: "/".into(),
                    recursive_size: 1000,
                    recursive_file_count: 5,
                    recursive_dir_count: 2,
                },
                DirStats {
                    path: "/a".into(),
                    recursive_size: 500,
                    recursive_file_count: 3,
                    recursive_dir_count: 1,
                },
            ],
        )
        .unwrap();

        // Remove a file at /a/old.txt (200 bytes)
        propagate_delta(&conn, "/a/old.txt", -200, -1, 0).unwrap();

        let a_stats = get_stats(&conn, "/a").unwrap();
        assert_eq!(a_stats.recursive_size, 300);
        assert_eq!(a_stats.recursive_file_count, 2);

        let root_stats = get_stats(&conn, "/").unwrap();
        assert_eq!(root_stats.recursive_size, 800);
        assert_eq!(root_stats.recursive_file_count, 4);
    }

    #[test]
    fn propagate_delta_clamps_to_zero() {
        let (conn, _dir) = open_temp_conn();

        IndexStore::upsert_dir_stats(
            &conn,
            &[DirStats {
                path: "/a".into(),
                recursive_size: 10,
                recursive_file_count: 1,
                recursive_dir_count: 0,
            }],
        )
        .unwrap();

        // Remove more than exists (edge case from race conditions or stale data)
        propagate_delta(&conn, "/a/file.txt", -100, -5, 0).unwrap();

        let stats = get_stats(&conn, "/a").unwrap();
        assert_eq!(stats.recursive_size, 0);
        assert_eq!(stats.recursive_file_count, 0);
    }

    #[test]
    fn propagate_creates_missing_ancestor_stats() {
        let (conn, _dir) = open_temp_conn();

        // No pre-existing dir_stats. Delta should create entries.
        propagate_delta(&conn, "/x/y/z/file.txt", 100, 1, 0).unwrap();

        let z_stats = get_stats(&conn, "/x/y/z").unwrap();
        assert_eq!(z_stats.recursive_size, 100);
        assert_eq!(z_stats.recursive_file_count, 1);

        let y_stats = get_stats(&conn, "/x/y").unwrap();
        assert_eq!(y_stats.recursive_size, 100);

        let x_stats = get_stats(&conn, "/x").unwrap();
        assert_eq!(x_stats.recursive_size, 100);
    }

    #[test]
    fn propagate_multiple_deltas_accumulate() {
        let (conn, _dir) = open_temp_conn();

        IndexStore::upsert_dir_stats(
            &conn,
            &[DirStats {
                path: "/d".into(),
                recursive_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
            }],
        )
        .unwrap();

        propagate_delta(&conn, "/d/a.txt", 100, 1, 0).unwrap();
        propagate_delta(&conn, "/d/b.txt", 200, 1, 0).unwrap();
        propagate_delta(&conn, "/d/c.txt", 300, 1, 0).unwrap();

        let stats = get_stats(&conn, "/d").unwrap();
        assert_eq!(stats.recursive_size, 600);
        assert_eq!(stats.recursive_file_count, 3);
    }

    #[test]
    fn propagate_dir_added() {
        let (conn, _dir) = open_temp_conn();

        IndexStore::upsert_dir_stats(
            &conn,
            &[DirStats {
                path: "/p".into(),
                recursive_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
            }],
        )
        .unwrap();

        // A new directory /p/newdir was created
        propagate_delta(&conn, "/p/newdir", 0, 0, 1).unwrap();

        let stats = get_stats(&conn, "/p").unwrap();
        assert_eq!(stats.recursive_dir_count, 1);
        assert_eq!(stats.recursive_size, 0);
    }

    // ── depth / parent_path edge cases ───────────────────────────────

    #[test]
    fn depth_counts_slashes() {
        assert_eq!(depth("/"), 1);
        assert_eq!(depth("/a"), 1);
        assert_eq!(depth("/a/b"), 2);
        assert_eq!(depth("/a/b/c"), 3);
    }
}
