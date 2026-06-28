//! The LOCAL full-tree reconcile rescan for the jwalk-indexed local volume.
//!
//! A LOCAL rescan of an already-populated index reconciles in place instead of
//! truncating and rebuilding: it BFS-walks the tree from the volume root over
//! `std::fs::read_dir`, diffs each directory against the DB
//! ([`reconciler::diff_dir_against_db`], shared with the live `reconcile_subtree`
//! and the network `reconcile_volume_via_trait`), and writes only the changes — so
//! the last-good directory sizes stay visible (marked stale) throughout, and a
//! rescan never mints the large freelist a mass-DELETE + bulk-reinsert does. A
//! FIRST/empty scan keeps today's truncate + parallel-jwalk path (the onboarding
//! moment stays fast); the `manager::start_scan` predicate picks between them.
//!
//! ## Why a separate serial walk (not jwalk)
//!
//! jwalk's fast parallel build is kept for the fresh scan. The reconcile is a
//! separate serial BFS used only on the rare rescan (journal gap / overflow /
//! stale-on-launch / forced); it reuses proven per-dir diff code and a single
//! read connection, so there are no id races. Speed of the rare walk is secondary
//! to safety here.
//!
//! ## Integration shape
//!
//! [`start_local_reconcile`] returns the SAME `(ScanHandle, JoinHandle<Result<
//! ScanSummary, ScanError>>)` shape as [`scanner::scan_volume`] and runs the
//! synchronous walk on a `std::thread` (NOT a tokio task). `manager::start_scan`
//! swaps it in for the `scanner::scan_volume` call on the reconcile branch, so the
//! existing completion handler — FSEvents drain → replay → `run_live_event_loop` —
//! is reused LITERALLY UNCHANGED. The shared finish (marks → one
//! `ComputeAllAggregates`) runs IN-THREAD, exactly as `scan_volume` does its
//! marks + aggregate before the thread joins.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::firmlinks;
use super::metadata::extract_metadata;
use super::reconciler::{self, LiveChild};
use super::scanner::{ScanError, ScanHandle, ScanProgress, ScanSummary};
use super::store::{self, IndexStore};
use super::writer::{IndexWriter, WriteMessage};

/// Start a LOCAL full-tree reconcile on a background `std::thread`.
///
/// Mirrors [`scanner::scan_volume`]'s return shape so `manager::start_scan`'s
/// completion handler is reused unchanged: a [`ScanHandle`] for progress +
/// cancellation, and a `JoinHandle` the handler joins for the [`ScanSummary`].
pub(super) fn start_local_reconcile(
    root: PathBuf,
    writer: &IndexWriter,
) -> Result<(ScanHandle, std::thread::JoinHandle<Result<ScanSummary, ScanError>>), ScanError> {
    let progress = Arc::new(ScanProgress::new());
    let cancelled = Arc::new(AtomicBool::new(false));
    let handle = ScanHandle::new(Arc::clone(&progress), Arc::clone(&cancelled));

    let writer = writer.clone();
    let thread_handle = std::thread::Builder::new()
        .name("index-local-reconcile".into())
        .spawn(move || run_local_reconcile(&root, &writer, &progress, &cancelled))
        .map_err(ScanError::Io)?;

    Ok((handle, thread_handle))
}

/// Normalize one directory's filesystem children into source-agnostic
/// [`LiveChild`]s, accumulating the live counters the progress bar reads (so a
/// multi-minute reconcile doesn't show a frozen bar) and the summary totals.
fn build_live_children(
    fs_children: &[(String, std::fs::Metadata, bool)],
    total_entries: &mut u64,
    total_dirs: &mut u64,
    total_physical_bytes: &mut u64,
    progress: &ScanProgress,
) -> Vec<LiveChild> {
    let mut live = Vec::with_capacity(fs_children.len());
    for (name, meta, is_symlink) in fs_children {
        let is_dir = meta.is_dir();
        let snap = extract_metadata(meta, is_dir, *is_symlink);
        let entry_physical = snap.physical_size.unwrap_or(0);
        *total_physical_bytes += entry_physical;
        *total_entries += 1;
        progress.entries_scanned.fetch_add(1, Ordering::Relaxed);
        progress.bytes_scanned.fetch_add(entry_physical, Ordering::Relaxed);
        if is_dir {
            *total_dirs += 1;
            progress.dirs_found.fetch_add(1, Ordering::Relaxed);
        }
        live.push(LiveChild {
            name: name.clone(),
            is_directory: is_dir,
            is_symlink: *is_symlink,
            snap,
        });
    }
    live
}

fn summary(entries: u64, dirs: u64, physical_bytes: u64, start: Instant, cancelled: bool) -> ScanSummary {
    ScanSummary {
        total_entries: entries,
        total_dirs: dirs,
        total_physical_bytes: physical_bytes,
        duration_ms: start.elapsed().as_millis() as u64,
        was_cancelled: cancelled,
    }
}

/// The synchronous LOCAL reconcile walk. Runs on the scanner thread.
///
/// Serial BFS from the volume root over `std::fs::read_dir`, diffing each dir
/// against the DB ([`reconciler::diff_dir_against_db`]) and writing only changes,
/// then the shared finish (marks → one `ComputeAllAggregates`, I1/I7). Honors the
/// cancel flag (I12), the empty-root guard (I8), the read-only connection (I11),
/// the `(parent_id, name)` new-dir resolution + shared id counter (I6/I10), and the
/// recurse-into-every-matched-child-dir rule (I5). Keeps the read connection in
/// autocommit (no long-lived `BEGIN` read txn) so post-flush new-dir resolves see
/// fresh rows.
fn run_local_reconcile(
    root: &Path,
    writer: &IndexWriter,
    progress: &ScanProgress,
    cancelled: &AtomicBool,
) -> Result<ScanSummary, ScanError> {
    let start = Instant::now();
    let db_path = writer.db_path();

    // I11: a READ connection. A write-mode connection's pragmas can `SQLITE_BUSY`
    // and silently kill live indexing.
    let conn = IndexStore::open_read_connection(&db_path).map_err(|e| ScanError::WriterSend(e.to_string()))?;
    // `start_scan` already bumped + flushed `current_epoch` before spawning this
    // walk, so read the bumped value back and stamp every re-listed dir with it.
    let epoch = IndexStore::read_current_epoch(&conn).map_err(|e| ScanError::WriterSend(e.to_string()))?;

    // The volume root maps to its DB id (`ROOT_ID` in production, since
    // `resolve_path("/")` is `ROOT_ID`). Resolving it (rather than hardcoding
    // `ROOT_ID`) also lets the walker be exercised from any root in tests.
    let root_str = firmlinks::normalize_path(&root.to_string_lossy());
    let root_id = match store::resolve_path(&conn, &root_str).map_err(|e| ScanError::WriterSend(e.to_string()))? {
        Some(id) => id,
        None => {
            return Err(ScanError::Io(std::io::Error::other(
                "local reconcile: root is not in the index",
            )));
        }
    };

    let mut listed_ids: Vec<i64> = Vec::new();
    let mut total_entries = 0u64;
    let mut total_dirs = 0u64;
    let mut total_physical_bytes = 0u64;
    let (mut added, mut removed, mut updated) = (0u64, 0u64, 0u64);

    // BFS by (absolute dir path, its DB id). New dirs discovered this pass are
    // resolved to ids after a writer flush before we recurse into them.
    let mut queue: VecDeque<(PathBuf, i64)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), root_id));
    // (parent dir path, parent DB id, child name): resolved by `(parent_id, name)`
    // after a level's flush, never by absolute path (I6).
    let mut new_dirs: Vec<(PathBuf, i64, String)> = Vec::new();

    while let Some((dir_path, dir_id)) = queue.pop_front() {
        if cancelled.load(Ordering::Relaxed) {
            // Cancel (I12): leave the prior index intact (no truncate ran) and send
            // NO marks/aggregate. Any partial entry writes already applied stay
            // size-consistent (`UpsertEntryV2`/`Delete*` auto-propagate dir_stats);
            // with no `scan_completed_at`, the next launch re-reconciles.
            return Ok(summary(total_entries, total_dirs, total_physical_bytes, start, true));
        }

        let fs_children = match reconciler::read_fs_children(&dir_path) {
            Some(c) => c,
            None => {
                if dir_path == *root {
                    // The ROOT itself is unlistable: nothing to reconcile from.
                    // Surface as a failed rescan so the completion handler writes no
                    // `scan_completed_at`; the prior index is untouched.
                    return Err(ScanError::Io(std::io::Error::other(
                        "local reconcile: root directory is unlistable",
                    )));
                }
                // A sub-directory we can't list: skip it. It keeps its old
                // `listed_epoch` (honest "stale/unknown") and heals on a later pass.
                continue;
            }
        };

        // Empty-root guard (I8): if the VOLUME ROOT lists empty, bail BEFORE diffing
        // it — otherwise the diff sees an empty live listing and DELETES every
        // existing child, blanking the index. A reconcile only runs over an
        // already-populated index, so an empty root is a transient half-dead `/`,
        // not a real "everything deleted". A non-root dir that lists empty is a
        // genuine empty subdir and reconciles normally (its stale children are swept).
        if dir_path == *root && fs_children.is_empty() {
            log::warn!(
                "local reconcile: root listed empty for {} — treating as a failed rescan, keeping prior index",
                dir_path.display()
            );
            return Err(ScanError::EmptyRoot);
        }

        // This dir's listing succeeded (incl. empty) — stamp it after the walk.
        listed_ids.push(dir_id);

        let db_children =
            IndexStore::list_children_on(dir_id, &conn).map_err(|e| ScanError::WriterSend(e.to_string()))?;
        let live_children = build_live_children(
            &fs_children,
            &mut total_entries,
            &mut total_dirs,
            &mut total_physical_bytes,
            progress,
        );

        let diff = reconciler::diff_dir_against_db(dir_id, &live_children, &db_children, writer);
        added += diff.added;
        removed += diff.removed;
        updated += diff.updated;
        // I5: recurse into EVERY matched child dir (changed or not).
        for (child_id, child_name) in diff.matched_child_dirs {
            queue.push_back((dir_path.join(child_name), child_id));
        }
        for child_name in diff.new_child_dir_names {
            new_dirs.push((dir_path.clone(), dir_id, child_name));
        }

        // Level drained + new dirs created: flush so the read connection sees their
        // freshly-assigned ids, then queue them for recursion. ❌ Don't wrap the walk
        // in one `BEGIN` read txn — autocommit per-dir reads keep the snapshot fresh
        // so these post-flush resolves see the new rows (and avoid freelist pinning).
        if !new_dirs.is_empty() && queue.is_empty() {
            if let Err(e) = writer.flush_blocking() {
                log::warn!("local reconcile: flush before resolving new dirs failed: {e}");
            }
            for (parent_path, parent_id, child_name) in new_dirs.drain(..) {
                let child_path = parent_path.join(&child_name);
                // Resolve by `(parent_id, name)` (I6): single-component lookup under
                // the id we already hold, robust to any root.
                match IndexStore::resolve_component(&conn, parent_id, &child_name) {
                    Ok(Some(id)) => queue.push_back((child_path, id)),
                    Ok(None) => log::debug!(
                        "local reconcile: couldn't resolve new dir after flush: {}",
                        child_path.display()
                    ),
                    Err(e) => log::warn!(
                        "local reconcile: resolve_component failed for {}: {e}",
                        child_path.display()
                    ),
                }
            }
        }
    }

    // Clean finish (I1/I7): stamp every re-listed dir, then ONE `ComputeAllAggregates`
    // (never per-dir propagation), then trim the post-rescan WAL spike.
    reconciler::finish_reconcile(&listed_ids, epoch, writer).map_err(|e| ScanError::WriterSend(e.to_string()))?;
    writer
        .send(WriteMessage::WalCheckpoint)
        .map_err(|e| ScanError::WriterSend(e.to_string()))?;

    log::info!(
        "local reconcile: complete for {}: +{added} -{removed} ~{updated} ({} re-listed) in {}ms",
        root.display(),
        crate::pluralize::pluralize(total_dirs, "dir"),
        start.elapsed().as_millis()
    );

    Ok(summary(total_entries, total_dirs, total_physical_bytes, start, false))
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};

    use rusqlite::Connection;

    use super::*;
    use crate::indexing::store::{self, DirStatsById, IndexStore, ROOT_ID};
    use crate::indexing::writer::{IndexWriter, WriteMessage};

    struct Harness {
        writer: IndexWriter,
        db_path: PathBuf,
        _dir: tempfile::TempDir,
    }

    fn setup() -> Harness {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("local-reconcile.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
        // Seed current_epoch = 1 so the first reconcile stamps a real value.
        writer
            .send(WriteMessage::UpdateMeta {
                key: store::CURRENT_EPOCH_KEY.to_string(),
                value: "1".to_string(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();
        Harness {
            writer,
            db_path,
            _dir: dir,
        }
    }

    /// A tree root under CWD (not /tmp — excluded on Linux, an alias on macOS).
    fn tree_root() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("cmdr_local_reconcile_")
            .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .expect("tempdir in cwd")
    }

    fn norm(p: &Path) -> String {
        firmlinks::normalize_path(&p.to_string_lossy())
    }

    fn conn(h: &Harness) -> Connection {
        IndexStore::open_read_connection(&h.db_path).expect("read conn")
    }

    /// Seed every component of `abs_path` as a directory entry (mirrors what a full
    /// scan does before live events). Syncs the writer's next_id afterward.
    fn ensure_path_in_db(h: &Harness, abs_path: &str) {
        let wconn = IndexStore::open_write_connection(&h.db_path).unwrap();
        let mut current_id = ROOT_ID;
        for component in abs_path.strip_prefix('/').unwrap_or(abs_path).split('/') {
            if component.is_empty() {
                continue;
            }
            match IndexStore::resolve_component(&wconn, current_id, component).unwrap() {
                Some(id) => current_id = id,
                None => {
                    current_id =
                        IndexStore::insert_entry_v2(&wconn, current_id, component, true, false, None, None, None, None)
                            .unwrap();
                }
            }
        }
        let db_next_id = IndexStore::get_next_id(&wconn).unwrap();
        h.writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
    }

    /// Insert a single child entry directly (simulating a prior populated index),
    /// then sync the writer's next_id.
    fn insert_child(h: &Harness, parent_id: i64, name: &str, is_dir: bool, size: Option<u64>) {
        let wconn = IndexStore::open_write_connection(&h.db_path).unwrap();
        IndexStore::insert_entry_v2(&wconn, parent_id, name, is_dir, false, size, size, None, None).unwrap();
        let db_next_id = IndexStore::get_next_id(&wconn).unwrap();
        h.writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
    }

    fn resolve(h: &Harness, p: &Path) -> Option<i64> {
        store::resolve_path(&conn(h), &norm(p)).unwrap()
    }

    fn listed_epoch(h: &Harness, id: i64) -> Option<u64> {
        IndexStore::get_listed_epoch_by_id(&conn(h), id).unwrap()
    }

    fn dir_stats(h: &Harness, id: i64) -> Option<DirStatsById> {
        IndexStore::get_dir_stats_by_id(&conn(h), id).unwrap()
    }

    fn bump_epoch(h: &Harness) -> u64 {
        let wconn = IndexStore::open_write_connection(&h.db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap()
    }

    /// Run the reconcile walk synchronously and flush. `cancel` pre-trips the cancel
    /// flag (deterministic interruption).
    fn run_reconcile(h: &Harness, root: &Path, cancel: bool) -> Result<ScanSummary, ScanError> {
        let progress = ScanProgress::new();
        let flag = AtomicBool::new(cancel);
        let result = run_local_reconcile(root, &h.writer, &progress, &flag);
        h.writer.flush_blocking().unwrap();
        result
    }

    /// I5: a reconcile must descend into existing child dirs whose own metadata
    /// did NOT change — recursion is decoupled from the write decision. Pinned by
    /// re-listing every dir to the new epoch on a no-change rescan.
    #[test]
    fn reconcile_descends_into_existing_unchanged_child_dirs() {
        let h = setup();
        let root = tree_root();
        let rp = root.path();
        ensure_path_in_db(&h, &norm(rp));

        std::fs::create_dir_all(rp.join("a/deep")).unwrap();
        std::fs::write(rp.join("a/deep/leaf.txt"), b"x").unwrap();

        // Epoch 1: build the index from disk.
        run_reconcile(&h, rp, false).expect("first reconcile");
        let a = resolve(&h, &rp.join("a")).expect("a indexed");
        let deep = resolve(&h, &rp.join("a/deep")).expect("deep indexed");
        assert_eq!(listed_epoch(&h, deep), Some(1), "deep listed at epoch 1");

        // Epoch 2 reconcile with NO disk changes: every dir must be re-listed.
        bump_epoch(&h);
        run_reconcile(&h, rp, false).expect("second reconcile");
        assert_eq!(listed_epoch(&h, a), Some(2), "/a re-listed");
        assert_eq!(
            listed_epoch(&h, deep),
            Some(2),
            "unchanged child dir must still be re-listed (I5)"
        );
    }

    /// Deletion sweep (data-safety): an entry removed on disk is removed from the
    /// index after a reconcile, including one nested under a subdir (proving the
    /// walk recurses to reach it).
    #[test]
    fn reconcile_removes_entry_deleted_on_disk() {
        let h = setup();
        let root = tree_root();
        let rp = root.path();
        ensure_path_in_db(&h, &norm(rp));

        std::fs::create_dir(rp.join("sub")).unwrap();
        std::fs::write(rp.join("sub/keep.txt"), b"keep").unwrap();
        std::fs::write(rp.join("sub/gone.txt"), b"gone-soon").unwrap();

        run_reconcile(&h, rp, false).expect("first reconcile");
        let sub = resolve(&h, &rp.join("sub")).expect("sub indexed");
        assert_eq!(dir_stats(&h, sub).expect("sub stats").recursive_file_count, 2);

        std::fs::remove_file(rp.join("sub/gone.txt")).unwrap();
        bump_epoch(&h);
        run_reconcile(&h, rp, false).expect("second reconcile");

        assert!(
            resolve(&h, &rp.join("sub/gone.txt")).is_none(),
            "deleted file gone from index"
        );
        assert!(
            resolve(&h, &rp.join("sub/keep.txt")).is_some(),
            "kept file still present"
        );
        assert_eq!(
            dir_stats(&h, sub).expect("sub stats after").recursive_file_count,
            1,
            "file count drops by one"
        );
    }

    /// A modified file's new size propagates up into ancestor dir_stats.
    #[test]
    fn reconcile_modified_file_propagates_to_ancestor_dir_stats() {
        let h = setup();
        let root = tree_root();
        let rp = root.path();
        ensure_path_in_db(&h, &norm(rp));

        std::fs::create_dir(rp.join("sub")).unwrap();
        std::fs::write(rp.join("sub/f.txt"), b"orig").unwrap();

        run_reconcile(&h, rp, false).expect("first reconcile");
        let root_id = resolve(&h, rp).expect("root in db");
        let size0 = dir_stats(&h, root_id).expect("root stats").recursive_logical_size;

        let bigger = b"a much longer body than before";
        std::fs::write(rp.join("sub/f.txt"), bigger).unwrap();
        bump_epoch(&h);
        run_reconcile(&h, rp, false).expect("second reconcile");

        let size1 = dir_stats(&h, root_id).expect("root stats after").recursive_logical_size;
        assert!(
            size1 > size0,
            "ancestor size grew after the file grew ({size0} -> {size1})"
        );
        assert_eq!(size1, bigger.len() as u64, "ancestor size equals the new file size");
    }

    /// Empty-root guard (I8, data-safety): when the volume root lists empty, the
    /// reconcile returns `EmptyRoot` and does NOT blank the prior index. The
    /// completion handler maps `Err` to "no scan_completed_at" (unchanged manager
    /// logic), so the index heals on the next launch.
    #[test]
    fn reconcile_empty_root_keeps_prior_index_and_signals_empty_root() {
        let h = setup();
        let root = tree_root();
        let rp = root.path(); // empty on disk
        ensure_path_in_db(&h, &norm(rp));
        let rp_id = resolve(&h, rp).expect("root seeded");
        // Simulate a prior populated index: a child the live (empty) root no longer shows.
        insert_child(&h, rp_id, "ghost.txt", false, Some(10));
        h.writer.flush_blocking().unwrap();
        assert!(
            resolve(&h, &rp.join("ghost.txt")).is_some(),
            "precondition: ghost present"
        );

        let result = run_reconcile(&h, rp, false);
        assert!(
            matches!(result, Err(ScanError::EmptyRoot)),
            "empty root must surface EmptyRoot, got {result:?}"
        );
        assert!(
            resolve(&h, &rp.join("ghost.txt")).is_some(),
            "the prior index must NOT be blanked when the root lists empty"
        );
    }

    /// Cancel (I12, data-safety): a cancelled reconcile returns `was_cancelled`
    /// (so the completion handler writes no scan_completed_at) and sends no
    /// marks/aggregate, leaving the prior index intact and un-restamped.
    #[test]
    fn cancelled_reconcile_leaves_prior_index_and_writes_no_marks() {
        let h = setup();
        let root = tree_root();
        let rp = root.path();
        ensure_path_in_db(&h, &norm(rp));

        std::fs::create_dir(rp.join("sub")).unwrap();
        std::fs::write(rp.join("sub/f.txt"), b"body").unwrap();

        run_reconcile(&h, rp, false).expect("first reconcile");
        let sub = resolve(&h, &rp.join("sub")).expect("sub indexed");
        assert_eq!(listed_epoch(&h, sub), Some(1), "sub listed at epoch 1");

        bump_epoch(&h); // -> 2
        let summary = run_reconcile(&h, rp, true).expect("cancelled reconcile returns Ok");
        assert!(summary.was_cancelled, "cancel must report was_cancelled");

        assert!(
            resolve(&h, &rp.join("sub/f.txt")).is_some(),
            "index not blanked by cancel"
        );
        assert_eq!(
            listed_epoch(&h, sub),
            Some(1),
            "no marks sent on cancel: listed_epoch stays at the old value"
        );
    }
}
