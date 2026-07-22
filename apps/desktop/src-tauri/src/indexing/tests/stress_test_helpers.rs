//! Shared helpers for indexing stress and integration tests.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use rusqlite::Connection;

use crate::file_system::listing::FileEntry;
use crate::indexing::lifecycle::state::{INDEX_REGISTRY, IndexInstance, IndexPhase, IndexVolumeKind};
use crate::indexing::read::enrichment::ReadPool;
use crate::indexing::read::pending_sizes::PendingSizes;
use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
use crate::indexing::writer::IndexWriter;

/// A privately-registered per-volume `IndexInstance`, for tests that must NOT
/// assert on the process-global root `PENDING_SIZES` / `READ_POOL`.
///
/// **Why this exists.** `cargo test` runs a crate's tests as threads in ONE
/// process. Every `IndexWriter::spawn()` in the binary is a ROOT writer whose
/// end-of-drain hook CLEARS the global root `PENDING_SIZES`, and `reset_indexing_for_test`
/// clears the root `READ_POOL`. A test that installs one of those globals and
/// asserts a mark/pool survives is therefore clobbered by any OTHER test's
/// writer, flakes, and — worse — poisons the shared `*_TEST_MUTEX`, cascading
/// `.lock().unwrap()` panics into every other holder. A crate-wide lock can't
/// fix it: the clobbering writers are threads, not tests, and span the whole
/// crate. Registering a PRIVATE instance under a UNIQUE volume id routes the
/// state to a tracker + pool immune to any foreign root writer.
///
/// Removes its registry entry on drop — including on a failed assertion — so a
/// panicking test never leaks a stray instance into another test's registry
/// sweep. `freshness: None` keeps it out of the scheduler sweeps' `Fresh` filter
/// while it's registered.
///
/// See `writer/DETAILS.md` § "Test isolation".
pub struct TestInstanceGuard {
    /// The unique volume id this instance is registered under.
    pub volume_id: String,
    /// The private pending-sizes tracker this volume's writer/reads route to.
    pub tracker: Arc<PendingSizes>,
}

impl TestInstanceGuard {
    /// Register a private instance for `volume_id` over `db_path`, with an
    /// explicit `kind`. Use for tests that read the tracker DIRECTLY (the writer
    /// and reconciler cases); a `smb://…` id is conventional there.
    pub fn register(volume_id: impl Into<String>, db_path: &Path, kind: IndexVolumeKind) -> Self {
        let volume_id = volume_id.into();
        let tracker = Arc::new(PendingSizes::new());
        // The read pool lives in the registry instance; per-volume reads
        // (`get_read_pool_for` / `enrich_*_on_volume` / `get_dir_stats_on_volume`)
        // resolve it from there by volume id, so the guard doesn't retain a handle.
        let read_pool = Arc::new(ReadPool::new(db_path.to_path_buf()).expect("open read pool"));
        INDEX_REGISTRY.lock().unwrap_or_else(|e| e.into_inner()).insert(
            volume_id.clone(),
            IndexInstance {
                phase: IndexPhase::ShuttingDown,
                kind,
                read_pool,
                pending_sizes: Arc::clone(&tracker),
                freshness: Arc::new(std::sync::Mutex::new(None)),
            },
        );
        Self { volume_id, tracker }
    }

    /// Register a private instance whose read-side path mapping is IDENTITY for
    /// plain `/absolute` paths, so `get_dir_stats_on_volume` /
    /// `enrich_entries_with_index_on_volume` can be driven with the same paths a
    /// `root` test would use — but routed to a PRIVATE tracker + pool immune to
    /// foreign root writers.
    ///
    /// Implemented via an `mtp-` volume id: MTP's read side maps a plain `/path`
    /// unchanged (`paths::routing::index_read_path` → `mtp_index_relative_path`),
    /// which is the only non-root id kind that gives identity mapping with zero
    /// volume-manager setup. The MTP kind is incidental here; only the identity
    /// path mapping matters. `tag` disambiguates the id per test.
    pub fn register_identity_paths(tag: &str, db_path: &Path) -> Self {
        Self::register(format!("mtp-test-{tag}:1"), db_path, IndexVolumeKind::Mtp)
    }
}

impl Drop for TestInstanceGuard {
    fn drop(&mut self) {
        INDEX_REGISTRY
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.volume_id);
    }
}

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

/// Build a synthetic tree that also injects symlink rows and a hardlink pair,
/// so tests exercise the `recursive_has_symlinks` flag and the dedup convention.
///
/// On top of `build_synthetic_tree`'s plain dirs/files, this appends, under each
/// leaf directory:
/// - one symlink (`is_symlink: true`, sizes `None`, like the scanner stores them),
/// - a hardlink pair: a primary link (real sizes) and a secondary link
///   (`logical_size: None`, `physical_size: None`) sharing one inode — matching
///   the scanner's dedup-at-insert convention, so each inode's bytes count once.
///
/// Keeps `check_db_consistency` valid (it sums `logical_size.unwrap_or(0)`, so the
/// `None` secondary contributes 0) and gives `check_recursive_has_symlinks`
/// something to assert.
pub fn build_synthetic_tree_with_symlinks_and_hardlinks(
    levels: usize,
    dirs_per_level: usize,
    files_per_dir: usize,
    file_size: u64,
) -> Vec<EntryRow> {
    let mut entries = build_synthetic_tree(levels, dirs_per_level, files_per_dir, file_size);

    // Next free id and the leaf directories (those that are no other entry's parent).
    let mut next_id = entries.iter().map(|e| e.id).max().unwrap_or(ROOT_ID) + 1;
    let parent_ids: std::collections::HashSet<i64> = entries.iter().map(|e| e.parent_id).collect();
    let leaf_dir_ids: Vec<i64> = entries
        .iter()
        .filter(|e| e.is_directory && !parent_ids.contains(&e.id))
        .map(|e| e.id)
        .collect();

    for (i, parent_id) in leaf_dir_ids.into_iter().enumerate() {
        // Symlink.
        entries.push(EntryRow {
            id: next_id,
            parent_id,
            name: format!("link_{i}"),
            is_directory: false,
            is_symlink: true,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        });
        next_id += 1;

        // Hardlink primary (carries the bytes).
        let shared_inode = 1_000_000 + i as u64;
        entries.push(EntryRow {
            id: next_id,
            parent_id,
            name: format!("hard_primary_{i}.dat"),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(file_size),
            physical_size: Some(file_size),
            modified_at: Some(1_700_000_000),
            inode: Some(shared_inode),
        });
        next_id += 1;

        // Hardlink secondary (None sizes — counted as a file, contributes 0 bytes).
        entries.push(EntryRow {
            id: next_id,
            parent_id,
            name: format!("hard_secondary_{i}.dat"),
            is_directory: false,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: Some(1_700_000_000),
            inode: Some(shared_inode),
        });
        next_id += 1;
    }

    entries
}

/// Assert that every directory's stored `recursive_has_symlinks` matches an
/// independent recompute from the `entries` table (a direct symlink child, or
/// any subdirectory whose subtree contains a symlink).
///
/// A separate helper, not part of `check_db_consistency`: that helper is shared
/// by every stress test, so widening it has a broad blast radius.
pub fn check_recursive_has_symlinks(conn: &Connection) {
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

    let mut children_map: HashMap<i64, Vec<&EntryRow>> = HashMap::new();
    for entry in &all_entries {
        children_map.entry(entry.parent_id).or_default().push(entry);
    }

    fn expected_has_symlinks(entry_id: i64, children_map: &HashMap<i64, Vec<&EntryRow>>) -> bool {
        let Some(children) = children_map.get(&entry_id) else {
            return false;
        };
        children
            .iter()
            .any(|child| child.is_symlink || (child.is_directory && expected_has_symlinks(child.id, children_map)))
    }

    for entry in &all_entries {
        if !entry.is_directory {
            continue;
        }
        let stats = IndexStore::get_dir_stats_by_id(conn, entry.id)
            .unwrap()
            .unwrap_or_else(|| panic!("dir_stats missing for entry id={}, name={}", entry.id, entry.name));
        let expected = expected_has_symlinks(entry.id, &children_map);
        assert_eq!(
            stats.recursive_has_symlinks, expected,
            "recursive_has_symlinks mismatch for id={}, name='{}': got {}, expected {}",
            entry.id, entry.name, stats.recursive_has_symlinks, expected
        );
    }
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
    // `(EntryRow, listed_epoch)`: `EntryRow` has no `listed_epoch` field, so the
    // per-dir epoch is carried alongside for the `min_subtree_epoch` oracle.
    let all_entries: Vec<(EntryRow, u64)> = {
        let mut stmt = conn
            .prepare("SELECT id, parent_id, name, is_directory, is_symlink, logical_size, physical_size, modified_at, inode, listed_epoch FROM entries")
            .unwrap();
        stmt.query_map([], |row| {
            Ok((
                EntryRow {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    name: row.get(2)?,
                    is_directory: row.get::<_, i32>(3)? != 0,
                    is_symlink: row.get::<_, i32>(4)? != 0,
                    logical_size: row.get(5)?,
                    physical_size: row.get(6)?,
                    modified_at: row.get(7)?,
                    inode: row.get(8)?,
                },
                row.get::<_, u64>(9)?,
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    };

    // Build parent -> children map and a dir_id -> listed_epoch map.
    let mut children_map: HashMap<i64, Vec<&EntryRow>> = HashMap::new();
    let mut listed_epoch_map: HashMap<i64, u64> = HashMap::new();
    for (entry, listed_epoch) in &all_entries {
        children_map.entry(entry.parent_id).or_default().push(entry);
        listed_epoch_map.insert(entry.id, *listed_epoch);
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

    /// Ground-truth `min_subtree_epoch`: 0-absorbing min of this dir's own
    /// `listed_epoch` and every child dir's expected `min_subtree_epoch`.
    fn compute_expected_min_epoch(
        entry_id: i64,
        children_map: &HashMap<i64, Vec<&EntryRow>>,
        listed_epoch_map: &HashMap<i64, u64>,
    ) -> u64 {
        let mut min_epoch = listed_epoch_map.get(&entry_id).copied().unwrap_or(0);
        if let Some(children) = children_map.get(&entry_id) {
            for child in children {
                if child.is_directory {
                    let child_epoch = compute_expected_min_epoch(child.id, children_map, listed_epoch_map);
                    min_epoch = if min_epoch == 0 || child_epoch == 0 {
                        0
                    } else {
                        min_epoch.min(child_epoch)
                    };
                }
            }
        }
        min_epoch
    }

    // Check each directory's dir_stats
    for (entry, _) in &all_entries {
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

        let expected_min_epoch = compute_expected_min_epoch(entry.id, &children_map, &listed_epoch_map);
        assert_eq!(
            stats.min_subtree_epoch, expected_min_epoch,
            "dir_stats.min_subtree_epoch mismatch for id={}, name='{}': got {}, expected {}",
            entry.id, entry.name, stats.min_subtree_epoch, expected_min_epoch
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
