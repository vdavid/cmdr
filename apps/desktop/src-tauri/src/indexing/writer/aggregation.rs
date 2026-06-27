//! Aggregation handlers: thin delegation wrappers around `indexing::aggregator`.
//!
//! `ComputeAllAggregates` / `ComputePartialAggregates` / `ComputeSubtreeAggregates`
//! / `BackfillMissingDirStats` all land here. The heavy bottom-up compute lives in
//! `aggregator`; these wrappers pick the maps-vs-SQL path, emit progress events,
//! summarize the scan's UNIQUE-conflict skips, and walk the symlink flag up.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tauri::AppHandle;
use tauri_specta::Event;

use crate::indexing::aggregator::{self, AggregationProgress};
use crate::indexing::store::IndexStore;
use crate::pluralize::{pluralize, pluralize_with};

use super::delta::propagate_recursive_has_symlinks;
use super::{AccumulatorMaps, AggregationProgressEvent, phase_to_str};

/// Log severity for the count of rows a full scan skipped on a UNIQUE
/// `(parent_id, name_folded)` conflict (the `INSERT OR IGNORE` path).
#[derive(Debug, PartialEq, Eq)]
enum SkipSeverity {
    /// Nothing skipped: log nothing.
    None,
    /// Sparse skips, expected dedup (one dir reachable by two walk paths via a
    /// firmlink/symlink, or case/NFD sibling pairs on case-sensitive or
    /// cross-OS-synced trees). Not actionable: log at DEBUG.
    Benign,
    /// A large fraction of the scan skipped: the signature of two writer threads
    /// racing on one DB (the constraint's reason for being, a 1.83 TB ghost size
    /// was traced to exactly that). Actionable: log at WARN.
    Suspicious,
}

/// Classify a full scan's accumulated UNIQUE-conflict skips. The absolute floor
/// keeps a tiny tree with a couple genuine sibling collisions from tripping the
/// warning; the ratio separates a handful of dedup hits in a multi-million-row
/// scan from a racing writer (whose loser duplicates a large fraction of rows).
fn classify_skip_severity(inserted: u64, skipped: u64) -> SkipSeverity {
    const MIN_SUSPICIOUS_SKIPS: u64 = 50;
    const SUSPICIOUS_SKIP_RATIO: f64 = 0.01;
    if skipped == 0 {
        return SkipSeverity::None;
    }
    let total = inserted + skipped;
    let ratio = skipped as f64 / total as f64;
    if skipped >= MIN_SUSPICIOUS_SKIPS && ratio > SUSPICIOUS_SKIP_RATIO {
        SkipSeverity::Suspicious
    } else {
        SkipSeverity::Benign
    }
}

/// Maximum directory depth (from the scan root) that a partial-aggregation pass
/// writes `dir_stats` for. Depth from the scan root: `/Users` = 1,
/// `/Users/david` = 2, `~/Downloads` = 3. Covers onboarding browsing while
/// keeping each pass's write set to a few thousand rows rather than 100K+.
///
/// Real-volume measurement (Apple Silicon, 5.94M entries / 558K dirs, release
/// build): the depth-3 write set plus pane hot paths was 151–716 rows per pass,
/// and total per-pass cost (full in-memory bottom-up compute over every
/// scanned dir + the bounded write) ran 6–397 ms, p95 377 ms — comfortably
/// under the 500 ms budget across the whole scan. The compute dominates the
/// write (rows are trivial); it scales with dirs-scanned-so-far, which is why
/// the last passes near 558K dirs are the slowest. Lowering this depth would
/// shrink the write set but not the compute, so it isn't the lever to pull if a
/// future, larger volume breaches the budget — raise `PARTIAL_AGG_TICK_INTERVAL`
/// instead. (Note: an unoptimized debug build runs this compute ~20× slower,
/// p95 ~2.6 s — measure tuning against a release build, never `pnpm dev`.)
const PARTIAL_AGG_MAX_DEPTH: usize = 3;

/// Mid-scan partial aggregation: compute partial recursive sizes from the
/// accumulator maps and write a bounded subset of `dir_stats` rows so visible
/// listings show growing sizes during the scan.
///
/// Borrows the maps read-only — it must never clear or mutate them, because the
/// final `ComputeAllAggregates` consumes the same maps to produce the exact
/// totals. The differential test pins this invariant.
///
/// Empty maps are a no-op with no SQL fallback. This rule is load-bearing, not
/// hygiene: the scanner sends `ComputeAllAggregates` _before_ the manager's
/// completion handler sets `scan_done`, so the 500 ms progress reporter can race
/// one last `ComputePartialAggregates` into the channel _after_ the final
/// aggregation. Channel ordering doesn't prevent that. What makes it safe is
/// that the final pass clears the maps, so the late partial pass sees empty maps
/// and no-ops. A SQL fallback here would overwrite the just-computed final
/// `dir_stats` with a depth-capped partial subset.
pub(super) fn handle_compute_partial_aggregates(
    conn: &rusqlite::Connection,
    accumulator: &AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    hot_paths: Vec<String>,
) {
    if accumulator.direct_stats.is_empty() {
        log::debug!("ComputePartialAggregates: maps empty, no-op");
        return;
    }
    let t = Instant::now();
    let hot_paths_count = hot_paths.len();
    match aggregator::compute_partial_aggregates(
        conn,
        &accumulator.direct_stats,
        &accumulator.child_dirs,
        &hot_paths,
        PARTIAL_AGG_MAX_DEPTH,
    ) {
        Ok(stats) => {
            log::info!(
                "ComputePartialAggregates: {} dirs computed, {} rows written, \
                 {}/{hot_paths_count} hot paths resolved ({}ms)",
                stats.dirs_computed,
                stats.rows_written,
                stats.hot_paths_resolved,
                t.elapsed().as_millis(),
            );
            // Refresh both panes via the `/` full-refresh sentinel. Emitting from
            // inside the handler is correct by the same ordering argument as
            // `EmitDirUpdated`: the writes just committed on this thread, and
            // `writer_loop` wraps each message in `objc2::rc::autoreleasepool` on
            // macOS, so the ObjC-on-background-thread rule is satisfied.
            if let Some(app) = app_handle {
                crate::indexing::reconciler::emit_dir_updated(app, vec!["/".to_string()]);
            }
        }
        Err(e) => log::warn!("Index writer: compute_partial_aggregates failed: {e}"),
    }
    // No `bump_generation`: partial passes change no `entries` rows, only
    // transient `dir_stats`. Search-staleness detection cares about entry
    // existence, so a partial pass isn't a "mutation" for that purpose.
}

pub(super) fn handle_compute_all_aggregates(
    conn: &rusqlite::Connection,
    accumulator: &mut AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    volume_id: &str,
    expected_total_entries: &AtomicU64,
) {
    let t = Instant::now();
    let use_maps = !accumulator.direct_stats.is_empty();
    log::info!(
        "ComputeAllAggregates: using {} (direct_stats={} parents, child_dirs={} parents)",
        if use_maps { "in-memory maps" } else { "SQL fallback" },
        accumulator.direct_stats.len(),
        accumulator.child_dirs.len(),
    );
    let mut on_progress = build_progress_callback(app_handle, volume_id);
    let result = if !use_maps {
        aggregator::compute_all_aggregates_reported(conn, &mut on_progress)
    } else {
        aggregator::compute_all_aggregates_with_maps(
            conn,
            &accumulator.direct_stats,
            &accumulator.child_dirs,
            &mut on_progress,
        )
    };
    // Summarize the scan's UNIQUE-conflict skips once, here, instead of WARNing
    // per offending batch. Sparse skips are expected dedup; only a racing-writer
    // ratio is worth a WARN. Read before `clear()`.
    let inserted = accumulator.entries_inserted;
    let skipped = accumulator.entries_skipped;
    match classify_skip_severity(inserted, skipped) {
        SkipSeverity::None => {}
        SkipSeverity::Benign => log::debug!(
            "Index scan: {skipped} of {entries} skipped on UNIQUE conflict (expected dedup: firmlinks, case/NFD siblings)",
            entries = pluralize_with(inserted + skipped, "entry", "entries"),
        ),
        SkipSeverity::Suspicious => log::warn!(
            "Index scan: {skipped} of {entries} skipped on UNIQUE conflict ({pct:.1}%); a high ratio can mean two writers raced on one DB",
            entries = pluralize_with(inserted + skipped, "entry", "entries"),
            pct = skipped as f64 / (inserted + skipped) as f64 * 100.0,
        ),
    }
    // Maps are consumed; clear to free memory.
    // Reset expected_total so subtree-scan inserts don't emit
    // spurious saving_entries progress events after the full scan.
    accumulator.clear();
    expected_total_entries.store(0, Ordering::Relaxed);
    match result {
        Ok(count) => {
            log::info!(
                "ComputeAllAggregates: done, {} in {:.1}s",
                pluralize_with(count, "directory", "directories"),
                t.elapsed().as_secs_f64(),
            );
        }
        Err(e) => log::warn!("Index writer: compute_all_aggregates failed: {e}"),
    }
}

pub(super) fn handle_compute_subtree_aggregates(conn: &rusqlite::Connection, root: &str) {
    let t = Instant::now();
    match aggregator::compute_subtree_aggregates(conn, root) {
        Ok(count) => {
            log::debug!(
                "Index writer: computed subtree aggregates for {} under {root} ({}ms)",
                pluralize(count, "dir"),
                t.elapsed().as_millis(),
            );
            // The subtree's own `recursive_has_symlinks` was just computed.
            // Walk the parent chain so ancestors pick up changes (a symlink may
            // have just appeared inside the subtree, or the last one disappeared).
            if let Ok(Some(root_id)) = crate::indexing::store::resolve_path(conn, root)
                && let Ok(Some(parent_id)) = IndexStore::get_parent_id(conn, root_id)
                && parent_id != 0
            {
                propagate_recursive_has_symlinks(conn, parent_id);
            }
        }
        Err(e) => log::warn!("Index writer: compute_subtree_aggregates({root}) failed: {e}"),
    }
}

pub(super) fn handle_backfill_missing_dir_stats(conn: &rusqlite::Connection) {
    let t = Instant::now();
    match aggregator::backfill_missing_dir_stats(conn) {
        Ok(0) => {
            log::debug!("BackfillMissingDirStats: no dirs missing stats");
        }
        Ok(count) => {
            log::info!(
                "BackfillMissingDirStats: computed stats for {} in {:.1}s",
                pluralize(count, "dir"),
                t.elapsed().as_secs_f64(),
            );
        }
        Err(e) => log::warn!("BackfillMissingDirStats failed: {e}"),
    }
}

/// Build a progress callback that emits `index-aggregation-progress` events via the AppHandle.
/// If no AppHandle is available, returns a no-op closure.
fn build_progress_callback<'a>(
    app_handle: &'a Option<AppHandle>,
    volume_id: &'a str,
) -> impl FnMut(AggregationProgress) + 'a {
    move |progress: AggregationProgress| {
        if let Some(app) = app_handle {
            let _ = AggregationProgressEvent {
                volume_id: volume_id.to_string(),
                phase: phase_to_str(progress.phase).to_string(),
                current: progress.current,
                total: progress.total,
            }
            .emit(app);
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
    use crate::indexing::writer::tests::setup_db;
    use crate::indexing::writer::{IndexWriter, WriteMessage};

    // ── Skip-severity classification ─────────────────────────────────

    #[test]
    fn skip_severity_none_when_nothing_skipped() {
        assert_eq!(classify_skip_severity(5_000_000, 0), SkipSeverity::None);
    }

    #[test]
    fn skip_severity_benign_for_sparse_dedup() {
        // A handful of firmlink double-visits / case-NFD siblings in a big scan: expected, not actionable.
        assert_eq!(classify_skip_severity(5_000_000, 3), SkipSeverity::Benign);
        assert_eq!(classify_skip_severity(5_000_000, 49), SkipSeverity::Benign);
    }

    #[test]
    fn skip_severity_benign_when_below_absolute_floor_even_at_high_ratio() {
        // Tiny tree with a couple genuine sibling collisions: high ratio but few skips, stay quiet.
        assert_eq!(classify_skip_severity(20, 10), SkipSeverity::Benign);
    }

    #[test]
    fn skip_severity_suspicious_for_racing_writer_signature() {
        // Two writers racing on one DB: the loser's inserts all conflict, so a large fraction skips.
        assert_eq!(classify_skip_severity(5_000_000, 5_000_000), SkipSeverity::Suspicious);
        // Just over both gates: 100 skips and >1% of the scan (100 / 9100 ≈ 1.1%).
        assert_eq!(classify_skip_severity(9_000, 100), SkipSeverity::Suspicious);
        // Exactly 1% does not trip it (the ratio gate is strict `>`): 100 / 10000.
        assert_eq!(classify_skip_severity(9_900, 100), SkipSeverity::Benign);
    }

    #[test]
    fn skip_severity_benign_when_over_floor_but_under_ratio() {
        // 50 skips clears the floor but is a vanishing fraction of a 5M scan: still benign.
        assert_eq!(classify_skip_severity(5_000_000, 50), SkipSeverity::Benign);
    }

    // ── Partial aggregation tests ────────────────────────────────────

    /// A fresh writer with no inserts has empty accumulator maps, so a partial
    /// pass must be a no-op: no `dir_stats` rows, and the writer's mutation
    /// counter unchanged (partial passes are not "mutations" for search-staleness
    /// purposes — they change no `entries` rows). The counter is asserted as a
    /// before/after delta on this one writer (nothing else sends to it), never as
    /// an absolute value and never via the global `WRITER_GENERATION`.
    #[test]
    fn partial_aggregates_no_op_on_empty_maps() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        let gen_before = writer.mutation_count();

        writer
            .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let dir_stats_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM dir_stats", [], |row| row.get(0))
            .unwrap();
        assert_eq!(dir_stats_count, 0, "empty maps must produce no dir_stats rows");

        let gen_after = writer.mutation_count();
        assert_eq!(
            gen_before, gen_after,
            "a partial pass must not bump the writer's mutation counter"
        );

        writer.shutdown();
    }

    /// Partial sums show up at shallow depth and grow across batches. A 3-level
    /// tree is inserted in two batches; a partial pass after batch 1 writes
    /// dir_stats reflecting only batch-1 contents, and a pass after batch 2 grows
    /// them.
    #[test]
    fn partial_aggregates_shallow_sums_grow_across_batches() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Tree (depths from ROOT_ID): /a (id=10, depth 1) -> /a/b (id=11, depth 2)
        //                             /a/b/c (id=12, depth 3) -> /a/b/c/f1 (file)
        // Batch 1 inserts /a, /a/b, /a/b/c and one 100-byte file under /a/b/c.
        let batch1 = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "a".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "b".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 12,
                parent_id: 11,
                name: "c".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 13,
                parent_id: 12,
                name: "f1.dat".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(100),
                physical_size: Some(100),
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(batch1)).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
            .unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            // Depth ≤ 3 dirs (ROOT_ID=0, /a=1, /a/b=2, /a/b/c=3) all get rows.
            let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
            assert_eq!(a.recursive_logical_size, 100, "/a should sum batch-1 file");
            assert_eq!(a.recursive_file_count, 1);
            assert_eq!(a.recursive_dir_count, 2, "/a has /a/b and /a/b/c beneath it");
            let c = IndexStore::get_dir_stats_by_id(&conn, 12).unwrap().unwrap();
            assert_eq!(c.recursive_logical_size, 100, "/a/b/c holds the file directly");
        }

        // Batch 2 adds a second 50-byte file under /a/b/c.
        let batch2 = vec![EntryRow {
            id: 14,
            parent_id: 12,
            name: "f2.dat".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(50),
            physical_size: Some(50),
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(batch2)).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
            .unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
            assert_eq!(a.recursive_logical_size, 150, "/a should grow to 100 + 50");
            assert_eq!(a.recursive_file_count, 2);
            let c = IndexStore::get_dir_stats_by_id(&conn, 12).unwrap().unwrap();
            assert_eq!(c.recursive_logical_size, 150);
            assert_eq!(c.recursive_file_count, 2);
        }

        writer.shutdown();
    }

    /// Dirs deeper than `PARTIAL_AGG_MAX_DEPTH` get no rows from a partial pass,
    /// but DO get rows from the final `ComputeAllAggregates`.
    #[test]
    fn partial_aggregates_depth_limiting() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Chain: /a(10,d1) -> /a/b(11,d2) -> /a/b/c(12,d3) -> /a/b/c/d(13,d4)
        // with a file under the depth-4 dir. d4 = MAX_DEPTH + 1.
        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "a".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "b".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 12,
                parent_id: 11,
                name: "c".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 13,
                parent_id: 12,
                name: "d".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 14,
                parent_id: 13,
                name: "deep.dat".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(70),
                physical_size: Some(70),
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
            .unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            // /a/b/c is at depth 3 (≤ MAX_DEPTH) — gets a row reflecting the file.
            let c = IndexStore::get_dir_stats_by_id(&conn, 12).unwrap().unwrap();
            assert_eq!(c.recursive_logical_size, 70, "depth-3 dir should sum the deep file");
            // /a/b/c/d is at depth 4 (> MAX_DEPTH) — no partial row.
            assert!(
                IndexStore::get_dir_stats_by_id(&conn, 13).unwrap().is_none(),
                "depth-4 dir must get no partial row"
            );
        }

        // The final pass writes every dir, including the depth-4 one.
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            let d = IndexStore::get_dir_stats_by_id(&conn, 13).unwrap().unwrap();
            assert_eq!(d.recursive_logical_size, 70, "final pass fills the depth-4 dir");
        }

        writer.shutdown();
    }

    /// A deep dir listed in `hot_paths` punches through the depth limit: it gets
    /// its own row plus rows for its direct children. An unresolvable hot path is
    /// skipped without error.
    #[test]
    fn partial_aggregates_hot_paths_punch_through_depth() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // /a(10,d1)/b(11,d2)/c(12,d3)/d(13,d4)/e(14,d5, child dir of d)
        // plus a 60-byte file under e. /a/b/c/d is the hot path (depth 4).
        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "a".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "b".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 12,
                parent_id: 11,
                name: "c".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 13,
                parent_id: 12,
                name: "d".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 14,
                parent_id: 13,
                name: "e".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 15,
                parent_id: 14,
                name: "x.dat".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(60),
                physical_size: Some(60),
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates {
                // The hot dir (depth 4) and one unresolvable path.
                hot_paths: vec!["/a/b/c/d".into(), "/does/not/exist".into()],
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        // /a/b/c/d (hot, depth 4) gets a row despite the cap.
        let d = IndexStore::get_dir_stats_by_id(&conn, 13).unwrap().unwrap();
        assert_eq!(d.recursive_logical_size, 60, "hot dir punches through the depth cap");
        // Its direct child /a/b/c/d/e (depth 5) also gets a row.
        let e = IndexStore::get_dir_stats_by_id(&conn, 14).unwrap().unwrap();
        assert_eq!(e.recursive_logical_size, 60, "hot dir's direct child gets a row");
        // The unresolvable hot path produced no error and no spurious rows: the
        // flush above returned cleanly, which is the assertion.

        writer.shutdown();
    }
}
