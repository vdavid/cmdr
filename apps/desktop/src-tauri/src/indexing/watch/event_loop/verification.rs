//! Post-replay background verification: a bidirectional readdir diff over the
//! directories the replay touched. `run_background_verification` runs off the
//! async pool after live mode starts; `verify_affected_dirs` does the lock-free
//! two-phase DB-vs-disk reconcile. Root-scoped (boot disk only), so it stays on
//! `BootDisk` / `ROOT_VOLUME_ID`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use tauri::AppHandle;

use super::verify_guard::{self, VerifyVerdict};
use crate::indexing::DEBUG_STATS;
use crate::indexing::ROOT_VOLUME_ID;
use crate::indexing::lifecycle::lifecycle_bus;
use crate::indexing::metadata;
use crate::indexing::paths::firmlinks;
use crate::indexing::read::enrichment::get_read_pool;
use crate::indexing::reconcile::reconciler;
use crate::indexing::scanner;
use crate::indexing::store::{self, IndexStore};
use crate::indexing::writer::{IndexWriter, WriteMessage};
use crate::pluralize::{pluralize, pluralize_with};

/// Run post-replay verification in the background.
///
/// Called after live mode starts so the app is responsive immediately.
/// Corrections found by verification go through the writer channel,
/// which serializes them with live writes.
pub(super) async fn run_background_verification(affected_paths: HashSet<String>, writer: IndexWriter, app: AppHandle) {
    DEBUG_STATS.verifying.store(true, Ordering::Relaxed);
    let verify_start = Instant::now();
    log::debug!(
        "Background verification started ({} affected dirs)",
        affected_paths.len(),
    );

    // Verify affected directories: FSEvents journal replay coalesces events,
    // so child deletions may only show as "parent dir modified," and new
    // children may not get individual creation events. Readdir each affected
    // parent and reconcile with DB.
    //
    // Run on the blocking pool: `verify_affected_dirs` is sync (Phase 1 SQLite
    // reads via `ReadPool`, Phase 2 `read_dir`/`symlink_metadata` per child).
    // On a typical home folder it takes seconds. Doing it inline on an async
    // worker pins that worker for the full duration; on macOS it also feeds
    // a burst of writer messages and event emits through the main thread,
    // which competes with user-initiated IPCs like `plugin:window|close`.
    // The blocking pool absorbs the sync work; the async runtime stays free
    // to serve UI requests responsively (top-5 principle #3 — UI must always
    // be responsive).
    let verify_writer = writer.clone();
    let verify_affected_paths = affected_paths.clone();
    let verify_result = match tauri::async_runtime::spawn_blocking(move || {
        verify_affected_dirs(&verify_affected_paths, &verify_writer)
    })
    .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("Background verification: verify_affected_dirs join failed: {e}");
            VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            }
        }
    };

    // Scan newly discovered directories (inserts children + computes subtree aggregates).
    // Skip excluded paths (system dirs like /System, /dev) that aren't in the index.
    if !verify_result.new_dir_paths.is_empty() {
        // Flush first: verify_affected_dirs sent UpsertEntryV2 for each new dir, but those
        // writes are still queued. scan_subtree opens a read connection to resolve the dir's
        // path → entry_id, which fails if the entry isn't committed yet.
        if let Err(e) = writer.flush().await {
            log::warn!("Background verification pre-scan flush failed: {e}");
        }

        // Guarded-walker-based parallel walk + sync writer-channel sends — same blocking-pool
        // reasoning as `verify_affected_dirs` above. A subtree scan can take many
        // seconds and saturates multiple rayon threads; keeping it off the async
        // pool is essential.
        let scan_writer = writer.clone();
        let scan_dirs = verify_result.new_dir_paths.clone();
        if let Err(e) = tauri::async_runtime::spawn_blocking(move || {
            let cancelled = AtomicBool::new(false);
            for dir_path in &scan_dirs {
                // Background verification is root-scoped (boot disk), so `BootDisk`.
                if scanner::should_exclude(dir_path, &scanner::ExclusionScope::boot_disk()) {
                    continue;
                }
                match scanner::scan_subtree(Path::new(dir_path), &scan_writer, &cancelled) {
                    Ok(summary) => {
                        log::debug!(
                            "Background verification: scanned new dir {dir_path} ({} entries, {}ms)",
                            summary.total_entries,
                            summary.duration_ms,
                        );
                    }
                    Err(e) => {
                        log::warn!("Background verification: scan_subtree({dir_path}) failed: {e}");
                    }
                }
            }
        })
        .await
        {
            log::warn!("Background verification: scan_subtree batch join failed: {e}");
        }
    }

    let has_changes =
        verify_result.stale_count > 0 || verify_result.new_file_count > 0 || !verify_result.new_dir_paths.is_empty();

    if has_changes {
        log::debug!(
            "Background verification found {} stale, {} new files, {} new dirs; flushing",
            verify_result.stale_count,
            verify_result.new_file_count,
            verify_result.new_dir_paths.len(),
        );
        if let Err(e) = writer.flush().await {
            log::warn!("Background verification flush failed: {e}");
        }

        // Tell the UI about the newly-scanned subtrees so open listings can
        // refresh them. Coalesced into a single emit: the scan loop above
        // already finished all subtrees before we get here (the loop is
        // synchronous), so emitting per-path here only paid the per-emit
        // macOS main-thread cost N times without giving the FE any new info.
        // The FE handler is throttled at 2 s per pane anyway, so N separate
        // emits and one batched emit produce the same UX. This keeps the main
        // thread free for user-initiated IPCs like `plugin:window|close`.
        // (Was the post-commit-66712c2d "1.83 TB ghost-size" fix; the
        // `affected_paths` problem it solved persists — we just batch the
        // emit instead of looping it.)
        let visible_new_dirs: Vec<String> = verify_result
            .new_dir_paths
            .iter()
            .filter(|p| !scanner::should_exclude(p, &scanner::ExclusionScope::boot_disk()))
            .cloned()
            .collect();
        if !visible_new_dirs.is_empty() {
            // Background verification is root-scoped (uses the root read pool), so
            // its live corrections publish under the local root for the importance
            // scheduler's incremental rescore (plan Decision 5).
            lifecycle_bus::publish_dirs_changed(ROOT_VOLUME_ID, &visible_new_dirs);
            reconciler::emit_dir_updated(&app, visible_new_dirs);
        }

        // No off-writer ancestor compensation for the new dirs: each `scan_subtree`
        // above sent `ComputeSubtreeAggregates`, whose handler repairs the ancestor
        // chain (sizes, counts, symlinks, AND coverage — which this path never
        // corrected before) on the writer thread, race-free and without the 2×
        // credit a read-then-`PropagateDeltaById` here caused (Leak A). The
        // repairs already committed under the `has_changes` flush above.

        // Final emit for the replay-affected paths whose stats were corrected
        // (stale-row deletions and new-file additions in the affected_paths set).
        // `new_dir_paths` are not included here — they were already emitted
        // progressively above as each subtree's scan finished.
        if !affected_paths.is_empty() {
            let changed: Vec<String> = affected_paths.into_iter().collect();
            lifecycle_bus::publish_dirs_changed(ROOT_VOLUME_ID, &changed);
            reconciler::emit_dir_updated(&app, changed);
        }
    }

    DEBUG_STATS.verifying.store(false, Ordering::Relaxed);
    log::debug!(
        "Background verification completed in {}ms",
        verify_start.elapsed().as_millis(),
    );
}

/// Phase 1's snapshot: affected parent path → (its entry id, its DB children).
type DbSnapshot = HashMap<String, (i64, Vec<store::EntryRow>)>;

/// Result of `verify_affected_dirs`.
struct VerifyResult {
    /// Entries in DB but not on disk (deleted).
    stale_count: u64,
    /// Files on disk but not in DB (inserted with delta propagation).
    new_file_count: u64,
    /// Directories on disk but not in DB (inserted, need subtree scan by caller).
    new_dir_paths: Vec<String>,
}

/// Verify that DB entries for affected directories match what's on disk.
///
/// FSEvents journal replay coalesces events: child deletions may appear as
/// "parent directory modified" without individual removal events. Similarly,
/// new children may not get individual creation events.
///
/// Two-phase approach, no `INDEXING` lock needed:
///
/// **Phase 1 (ReadPool, no lock):** Resolve each affected path to its entry ID,
/// list children as `EntryRow` (integer-keyed), and snapshot into a `HashMap`.
/// Uses `get_read_pool()` + `pool.with_conn()` for lock-free DB reads.
///
/// **Phase 2 (no lock):** Walk the snapshot, check the filesystem
/// (`Path::exists`, `read_dir`, `symlink_metadata`), and send corrections to
/// the writer channel using integer-keyed write messages:
/// 1. **Stale entries**: DB children that no longer exist on disk get
///    `DeleteEntryById`/`DeleteSubtreeById` (auto-propagates deltas).
/// 2. **Missing entries**: Disk children not in DB get `UpsertEntryV2`. New files also get
///    `PropagateDeltaById`. New directories are collected in `new_dir_paths` for the caller to scan
///    via `scan_subtree`.
///
/// **Both phases are cost-guarded** (`verify_guard`): a directory with more than
/// `HUGE_DIR_CHILDREN` index children is declined before Phase 1 snapshots it, and
/// Phase 2's `read_dir` loop stops after that many iterations. See
/// `indexing/DETAILS.md` § "Bounding verification cost (the two teeth)" for the
/// trade this makes.
fn verify_affected_dirs(affected_paths: &HashSet<String>, writer: &IndexWriter) -> VerifyResult {
    verify_affected_dirs_with(affected_paths, writer, verify_guard::HUGE_DIR_CHILDREN)
}

/// [`verify_affected_dirs`] with the guard threshold injected, so tests can drive
/// both teeth with tiny fixtures instead of a million-file directory.
fn verify_affected_dirs_with(affected_paths: &HashSet<String>, writer: &IndexWriter, threshold: usize) -> VerifyResult {
    // ── Phase 1: Bulk-read DB state via ReadPool (no lifecycle/registry lock) ──
    // Snapshot: parent_path → (parent_id, Vec<EntryRow>)
    let pool = match get_read_pool() {
        Some(p) => p,
        None => {
            return VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            };
        }
    };

    let (db_snapshot, declined): (DbSnapshot, Vec<String>) = match pool.with_conn(|conn| {
        let mut snapshot = HashMap::with_capacity(affected_paths.len());
        let mut declined = Vec::new();
        for parent_path in affected_paths {
            let parent_id = match store::resolve_path(conn, parent_path) {
                Ok(Some(id)) => id,
                _ => continue, // Path not in index, skip
            };
            // ── Guard tooth 1 ────────────────────────────────────────
            // Probe the child count with `LIMIT threshold + 1` BEFORE
            // `list_children_on`. The snapshot below owns every `EntryRow` it
            // reads, so a 1.14M-child directory costs hundreds of MB and
            // minutes of writer traffic before a single child is examined —
            // the guard has to sit here, not around the upsert.
            // A probe that errors falls through to the diff: refusing to
            // verify on a transient read failure would be the worse default.
            let probe =
                IndexStore::count_children_capped(parent_id, conn, verify_guard::probe_limit(threshold)).unwrap_or(0);
            if verify_guard::classify_db_children(probe, threshold) == VerifyVerdict::Decline {
                declined.push(parent_path.clone());
                continue;
            }
            match IndexStore::list_children_on(parent_id, conn) {
                Ok(entries) => {
                    snapshot.insert(parent_path.clone(), (parent_id, entries));
                }
                Err(_) => {
                    // Insert empty vec so Phase 2 still checks disk for new entries
                    snapshot.insert(parent_path.clone(), (parent_id, Vec::new()));
                }
            }
        }
        (snapshot, declined)
    }) {
        Ok(pair) => pair,
        Err(e) => {
            log::warn!("verify_affected_dirs: ReadPool error: {e}");
            return VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            };
        }
    };

    if !declined.is_empty() {
        DEBUG_STATS
            .verify_declined_dirs
            .fetch_add(declined.len() as u64, Ordering::Relaxed);
        // ❌ Do NOT mark a declined dir unlisted here. Affected dirs carry a
        // positive `listed_epoch` from the scan, and `absorbing_min_epoch`
        // propagates a zero to every ancestor, so one declined temp directory
        // would render the whole home folder incomplete and make `expected_totals`
        // return `None` for every copy of `~`. Leave the epoch untouched.
        //
        // One line per episode with a bounded sample: the declined set can be
        // hundreds of paths, and this runs on the cold-start path.
        log::info!(
            "verify_affected_dirs: declined {} (over {} index children, a diff would cost O(children)): {}",
            pluralize(declined.len() as u64, "dir"),
            threshold,
            declined.iter().take(10).cloned().collect::<Vec<_>>().join(", "),
        );
    }

    // ── Phase 2: Filesystem checks without the lock ──────────────────
    let mut stale_count = 0u64;
    let mut new_file_count = 0u64;
    let mut new_dir_paths = Vec::<String>::new();

    for (parent_path, (parent_id, db_children)) in &db_snapshot {
        // Build a set of normalized DB child names for fast lookup
        let db_child_names: HashSet<String> = db_children
            .iter()
            .map(|c| store::normalize_for_comparison(&c.name))
            .collect();

        // Build child path from parent_path + name for filesystem checks
        let parent_prefix = if parent_path == "/" {
            String::new()
        } else {
            parent_path.clone()
        };

        // Detect stale entries (in DB but not on disk)
        for child in db_children {
            let child_path = format!("{}/{}", parent_prefix, child.name);
            if !Path::new(&child_path).exists() {
                if child.is_directory {
                    let _ = writer.send(WriteMessage::DeleteSubtreeById(child.id));
                } else {
                    let _ = writer.send(WriteMessage::DeleteEntryById(child.id));
                }
                stale_count += 1;
            }
        }

        // Detect missing entries (on disk but not in DB)
        let read_dir = match std::fs::read_dir(parent_path) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        // ── Guard tooth 2 ────────────────────────────────────────────────
        // Cap ITERATIONS, not upserts. The loop below `continue`s past every
        // DB-known child before doing any work, so an already-indexed
        // pathological directory produces ~zero upserts while iterating 1.14M
        // times — an upsert cap would be a no-op on the measured incident. This
        // tooth also covers the directory that is small in the DB (so it passes
        // tooth 1) but huge on disk.
        for (iterations, dir_entry) in read_dir.flatten().enumerate() {
            if verify_guard::classify_iteration(iterations, threshold) == VerifyVerdict::Decline {
                DEBUG_STATS.verify_truncated_dirs.fetch_add(1, Ordering::Relaxed);
                log::info!(
                    "verify_affected_dirs: stopped diffing {parent_path} after {} on disk (partial diff)",
                    pluralize_with(threshold as u64, "entry", "entries"),
                );
                break;
            }

            let child_path = dir_entry.path();
            let child_path_str = child_path.to_string_lossy().to_string();
            let normalized = firmlinks::normalize_path(&child_path_str);

            let name = dir_entry.file_name().to_string_lossy().to_string();
            if db_child_names.contains(&store::normalize_for_comparison(&name)) {
                continue;
            }

            // Skip excluded system paths (e.g. /System, /dev, /Volumes).
            // Root-scoped background verification (boot disk), so `BootDisk`.
            if scanner::should_exclude(&normalized, &scanner::ExclusionScope::boot_disk()) {
                continue;
            }

            let metadata = match std::fs::symlink_metadata(&child_path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let is_dir = metadata.is_dir();
            let is_symlink = metadata.is_symlink();
            let snap = metadata::extract_metadata(&metadata, is_dir, is_symlink);

            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id: *parent_id,
                name,
                is_directory: is_dir,
                is_symlink,
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                inode: snap.inode,
                nlink: snap.nlink,
            });

            // UpsertEntryV2 auto-propagates deltas in the writer.
            if is_dir {
                log::debug!("verify_affected_dirs: new dir on disk: {normalized} (parent_id={parent_id})");
                new_dir_paths.push(normalized);
            } else {
                new_file_count += 1;
            }
        }
    }

    if stale_count > 0 || new_file_count > 0 || !new_dir_paths.is_empty() {
        log::debug!(
            "Replay verification: {stale_count} stale, {}, {} across {}",
            pluralize(new_file_count, "new file"),
            pluralize(new_dir_paths.len() as u64, "new dir"),
            pluralize(affected_paths.len() as u64, "affected dir"),
        );
    }

    VerifyResult {
        stale_count,
        new_file_count,
        new_dir_paths,
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    //! The guard's integration tier: real `verify_affected_dirs_with` over real
    //! fixtures and a real writer, with the threshold injected so the "over
    //! threshold" case is 5 files rather than 1.14M.
    //!
    //! Every test asserts BOTH halves: the oversized directory is left alone AND
    //! a normal directory in the same batch is still fully diffed. Without the
    //! second half the test would pass if `verify_affected_dirs` were replaced
    //! with `return` (the no-op-fixture anti-pattern in `docs/testing.md`).
    //!
    //! Pool installation follows `verifier.rs::tests`: a root `ReadPool` under
    //! `READ_POOL_TEST_MUTEX`.

    use super::*;
    use crate::ignore_poison::IgnorePoison;
    use crate::indexing::read::enrichment::{READ_POOL, READ_POOL_TEST_MUTEX, ReadPool};
    use crate::indexing::store::{DirStatsById, EntryRow, ROOT_ID};
    use std::fs;
    use std::sync::Arc;

    /// Temp dir inside the crate root, not `/tmp/`: on Linux `/tmp/` is an
    /// excluded prefix, so `should_exclude` would filter every fixture child out
    /// and the "normal dir is still diffed" half would pass vacuously.
    fn test_tempdir() -> tempfile::TempDir {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        tempfile::Builder::new()
            .prefix("cmdr-verifyguard-")
            .tempdir_in(base)
            .expect("create temp dir")
    }

    fn setup_writer() -> (IndexWriter, std::path::PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
        (writer, db_path, dir)
    }

    fn install_read_pool(db_path: &Path) {
        let pool = Arc::new(ReadPool::new(db_path.to_path_buf()).unwrap());
        *READ_POOL.lock_ignore_poison() = Some(pool);
    }

    fn remove_read_pool() {
        *READ_POOL.lock_ignore_poison() = None;
    }

    /// Insert the directory chain for `path` and return the deepest dir's id.
    fn ensure_path_in_db(db_path: &Path, path: &Path, writer: &IndexWriter) -> i64 {
        let conn = IndexStore::open_write_connection(db_path).unwrap();
        let path_str = path.to_string_lossy();
        let mut parent_id = ROOT_ID;
        for component in path_str.split('/').filter(|c| !c.is_empty()) {
            parent_id = match IndexStore::resolve_component(&conn, parent_id, component) {
                Ok(Some(id)) => id,
                _ => IndexStore::insert_entry_v2(&conn, parent_id, component, true, false, None, None, None, None)
                    .unwrap(),
            };
        }
        let db_next_id = IndexStore::get_next_id(&conn).unwrap();
        writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
        parent_id
    }

    /// Index everything currently on disk under `dir_path` as children of `parent_id`.
    fn insert_children_from_disk(writer: &IndexWriter, parent_id: i64, dir_path: &Path) {
        for entry in fs::read_dir(dir_path).unwrap().flatten() {
            let meta = fs::symlink_metadata(entry.path()).unwrap();
            let is_dir = meta.is_dir();
            let is_symlink = meta.is_symlink();
            let snap = metadata::extract_metadata(&meta, is_dir, is_symlink);
            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id,
                name: entry.file_name().to_string_lossy().to_string(),
                is_directory: is_dir,
                is_symlink,
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                inode: snap.inode,
                nlink: snap.nlink,
            });
        }
        writer.flush_blocking().unwrap();
    }

    fn db_children(db_path: &Path, parent_id: i64) -> Vec<EntryRow> {
        let conn = IndexStore::open_read_connection(db_path).unwrap();
        IndexStore::list_children_on(parent_id, &conn).unwrap()
    }

    fn write_files(dir: &Path, names: &[&str]) {
        for name in names {
            fs::write(dir.join(name), "x").unwrap();
        }
    }

    /// A batch with one over-threshold directory and one normal one.
    ///
    /// Tooth 1 (DB-side): the oversized directory must produce zero per-child
    /// upserts, and the normal one must still be fully diffed.
    #[test]
    fn an_over_threshold_dir_is_declined_while_a_normal_dir_is_still_diffed() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock_ignore_poison();
        let root = test_tempdir();
        let huge = root.path().join("huge");
        let normal = root.path().join("normal");
        fs::create_dir(&huge).unwrap();
        fs::create_dir(&normal).unwrap();
        write_files(&huge, &["a", "b", "c", "d", "e"]);
        write_files(&normal, &["p", "q"]);

        let (writer, db_path, _db_dir) = setup_writer();
        let huge_id = ensure_path_in_db(&db_path, &huge, &writer);
        let normal_id = ensure_path_in_db(&db_path, &normal, &writer);
        insert_children_from_disk(&writer, huge_id, &huge);
        insert_children_from_disk(&writer, normal_id, &normal);
        install_read_pool(&db_path);

        // Drift on BOTH sides of both directories, so a working diff has
        // something to find in each.
        fs::write(huge.join("new-in-huge"), "x").unwrap();
        fs::remove_file(huge.join("a")).unwrap();
        fs::write(normal.join("new-in-normal"), "x").unwrap();
        fs::remove_file(normal.join("p")).unwrap();

        let affected: HashSet<String> = [huge.to_string_lossy().to_string(), normal.to_string_lossy().to_string()]
            .into_iter()
            .collect();
        // Threshold 3: `huge` has 5 index children (over), `normal` has 2 (under).
        let result = verify_affected_dirs_with(&affected, &writer, 3);
        writer.flush_blocking().unwrap();

        let huge_after = db_children(&db_path, huge_id);
        let normal_after = db_children(&db_path, normal_id);

        // Declined: not one row changed, in either direction.
        let huge_names: Vec<&str> = huge_after.iter().map(|e| e.name.as_str()).collect();
        assert!(
            !huge_names.contains(&"new-in-huge"),
            "a declined dir must produce zero per-child upserts, got {huge_names:?}"
        );
        assert!(
            huge_names.contains(&"a"),
            "a declined dir's stale rows are knowingly left behind (the documented cost), got {huge_names:?}"
        );

        // Still diffed: the normal dir in the SAME batch is fully reconciled.
        let normal_names: Vec<&str> = normal_after.iter().map(|e| e.name.as_str()).collect();
        assert!(
            normal_names.contains(&"new-in-normal"),
            "the normal dir must still gain its new file, got {normal_names:?}"
        );
        assert!(
            !normal_names.contains(&"p"),
            "the normal dir must still lose its stale row, got {normal_names:?}"
        );
        assert_eq!(result.new_file_count, 1, "exactly the normal dir's new file");
        assert_eq!(result.stale_count, 1, "exactly the normal dir's stale row");

        remove_read_pool();
        writer.shutdown();
    }

    /// Tooth 2 (disk-side): a directory that is small in the index but huge on
    /// disk passes the DB probe, so only the iteration cap stands between it and
    /// a full re-insertion.
    #[test]
    fn a_dir_thats_small_in_the_db_but_huge_on_disk_is_truncated_not_fully_diffed() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock_ignore_poison();
        let root = test_tempdir();
        let sparse = root.path().join("sparse");
        let normal = root.path().join("normal");
        fs::create_dir(&sparse).unwrap();
        fs::create_dir(&normal).unwrap();
        write_files(&sparse, &["known-1", "known-2"]);
        write_files(&normal, &["p"]);

        let (writer, db_path, _db_dir) = setup_writer();
        let sparse_id = ensure_path_in_db(&db_path, &sparse, &writer);
        let normal_id = ensure_path_in_db(&db_path, &normal, &writer);
        insert_children_from_disk(&writer, sparse_id, &sparse);
        insert_children_from_disk(&writer, normal_id, &normal);
        install_read_pool(&db_path);

        // 20 files appear on disk; the index still holds only the original 2, so
        // tooth 1 (threshold 3) waves this directory through.
        let extra: Vec<String> = (0..20).map(|i| format!("disk-{i:02}")).collect();
        write_files(&sparse, &extra.iter().map(String::as_str).collect::<Vec<_>>());
        fs::write(normal.join("new-in-normal"), "x").unwrap();

        let before = DEBUG_STATS.verify_truncated_dirs.load(Ordering::Relaxed);
        let affected: HashSet<String> = [
            sparse.to_string_lossy().to_string(),
            normal.to_string_lossy().to_string(),
        ]
        .into_iter()
        .collect();
        verify_affected_dirs_with(&affected, &writer, 3);
        writer.flush_blocking().unwrap();

        let sparse_after = db_children(&db_path, sparse_id);
        // 3 iterations max, and 2 of the 22 disk entries are already known, so at
        // most 3 rows can be added on top of the original 2.
        assert!(
            sparse_after.len() <= 5,
            "the iteration cap must stop the diff early, got {} rows",
            sparse_after.len()
        );
        assert!(
            sparse_after.len() < 22,
            "sanity: the cap has to actually bite, got {} rows",
            sparse_after.len()
        );
        assert!(
            DEBUG_STATS.verify_truncated_dirs.load(Ordering::Relaxed) > before,
            "a truncated dir must be counted on the debug surface"
        );

        // The normal dir in the same batch is untouched by the cap.
        let normal_after = db_children(&db_path, normal_id);
        let normal_names: Vec<&str> = normal_after.iter().map(|e| e.name.as_str()).collect();
        assert!(
            normal_names.contains(&"new-in-normal"),
            "the normal dir must still be fully diffed, got {normal_names:?}"
        );

        remove_read_pool();
        writer.shutdown();
    }

    /// Regression guard, NOT TDD: this can't go red, because the code never wrote
    /// the epoch. It exists so a future "let's mark declined dirs honest-stale"
    /// change fails loudly.
    ///
    /// Writing `listed_epoch = 0` for a declined dir would look like honesty and
    /// be the opposite: `absorbing_min_epoch` propagates the zero to every
    /// ancestor, `recursive_size_complete` is derived as `min_subtree_epoch > 0`,
    /// so one declined temp directory would render the whole home folder
    /// incomplete and make `expected_totals` return `None` for every copy of `~`.
    #[test]
    fn a_declined_dir_leaves_its_epoch_and_every_ancestor_epoch_untouched() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock_ignore_poison();
        let root = test_tempdir();
        let huge = root.path().join("huge");
        fs::create_dir(&huge).unwrap();
        write_files(&huge, &["a", "b", "c", "d", "e"]);

        let (writer, db_path, _db_dir) = setup_writer();
        let huge_id = ensure_path_in_db(&db_path, &huge, &writer);
        insert_children_from_disk(&writer, huge_id, &huge);

        // Stamp the dir and give it plus its ancestor chain a positive coverage
        // epoch, exactly as a scan would.
        let epoch = 7u64;
        let ancestors: Vec<i64> = {
            let conn = IndexStore::open_read_connection(&db_path).unwrap();
            let mut chain = Vec::new();
            let mut id = huge_id;
            while let Ok(Some(parent)) = IndexStore::get_parent_id(&conn, id) {
                chain.push(parent);
                if parent == ROOT_ID {
                    break;
                }
                id = parent;
            }
            chain
        };
        let _ = writer.send(WriteMessage::MarkDirsListed {
            ids: std::iter::once(huge_id).chain(ancestors.iter().copied()).collect(),
            epoch,
        });
        writer.flush_blocking().unwrap();
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            let stats: Vec<DirStatsById> = std::iter::once(huge_id)
                .chain(ancestors.iter().copied())
                .map(|entry_id| DirStatsById {
                    entry_id,
                    recursive_logical_size: 5,
                    recursive_physical_size: 5,
                    recursive_file_count: 5,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: epoch,
                })
                .collect();
            IndexStore::upsert_dir_stats_by_id(&conn, &stats).unwrap();
        }
        install_read_pool(&db_path);

        // Drift, so a non-declining verification would definitely write here.
        fs::write(huge.join("new-in-huge"), "x").unwrap();
        fs::remove_file(huge.join("a")).unwrap();

        let affected: HashSet<String> = std::iter::once(huge.to_string_lossy().to_string()).collect();
        verify_affected_dirs_with(&affected, &writer, 3);
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        assert_eq!(
            IndexStore::get_listed_epoch_by_id(&conn, huge_id).unwrap(),
            Some(epoch),
            "a declined dir keeps its listed_epoch; writing 0 would drag every ancestor to incomplete"
        );
        for ancestor in std::iter::once(huge_id).chain(ancestors.iter().copied()) {
            let stats = IndexStore::get_dir_stats_by_id(&conn, ancestor).unwrap();
            assert_eq!(
                stats.map(|s| s.min_subtree_epoch),
                Some(epoch),
                "ancestor {ancestor} must keep min_subtree_epoch = {epoch}"
            );
        }

        remove_read_pool();
        writer.shutdown();
    }

    /// The census hook the walker and the reconcile walk share must be reachable
    /// from the guard's own decline path too — otherwise `verify_declined_dirs`
    /// reads zero on the machine that motivated all of this.
    #[test]
    fn a_declined_dir_is_counted_on_the_debug_surface() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock_ignore_poison();
        let root = test_tempdir();
        write_files(root.path(), &["a", "b", "c", "d", "e"]);

        let (writer, db_path, _db_dir) = setup_writer();
        let dir_id = ensure_path_in_db(&db_path, root.path(), &writer);
        insert_children_from_disk(&writer, dir_id, root.path());
        install_read_pool(&db_path);

        let before = DEBUG_STATS.verify_declined_dirs.load(Ordering::Relaxed);
        let affected: HashSet<String> = std::iter::once(root.path().to_string_lossy().to_string()).collect();
        verify_affected_dirs_with(&affected, &writer, 3);

        assert!(
            DEBUG_STATS.verify_declined_dirs.load(Ordering::Relaxed) > before,
            "the decline must be visible without shipping logs"
        );

        remove_read_pool();
        writer.shutdown();
    }
}
