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

use crate::indexing::IndexFailureSignal;
use crate::indexing::aggregator::{self, AggregationProgress};
use crate::indexing::store::IndexStore;
use crate::pluralize::{pluralize, pluralize_with};

use super::deferred_repair::DeferredRepairs;
use super::repair::repair_dir_stats_upward;
use super::{AccumulatorMaps, AggSource, AggregationProgressEvent, phase_to_str};

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

/// Conservative cap on a hot dir's committed subtree size before the `Sql`
/// partial path will scope it. Measured in `recursive_file_count +
/// recursive_dir_count` from the dir's CURRENT `dir_stats` row.
///
/// The stability guard: the `Sql` path runs a SCOPED recursive CTE per hot dir
/// (real SQL work, O(subtree_size)), unlike the `Maps` path's pure in-memory
/// compute. A hot path near the volume root (e.g. a pane on the share root) would
/// otherwise trigger a near-whole-tree CTE on the single writer thread and stall
/// every queued insert — the exact class of writer-thread wedge we guard against.
/// Above the cap, the dir is skipped and the final `ComputeAllAggregates` (at
/// most seconds away) fills it.
///
/// 100K is deliberately conservative: three scoped CTEs (`children_stats`,
/// `child_dir_ids`, `listed_epochs`) plus the bottom-up compute over a
/// 100K-entry subtree stay well inside the per-pass budget on a release build,
/// with headroom so the writer never blocks. A future tuning pass with real
/// network-volume timings can raise it; don't raise it on a hunch.
const PARTIAL_AGG_SQL_MAX_SUBTREE: u64 = 100_000;

/// Mid-scan partial aggregation. Routes on `source`:
///
/// - `Maps`: today's behavior, byte-for-byte. Computes from the in-memory
///   accumulator maps (populated only by `InsertEntriesV2`), writes a
///   depth-capped + hot-path subset of `dir_stats`. Borrows the maps read-only —
///   never clears or mutates them, because the final `ComputeAllAggregates`
///   consumes the same maps for the exact totals (the differential test pins
///   this). Empty maps are a no-op with NO SQL fallback: the scanner sends
///   `ComputeAllAggregates` _before_ `scan_done` is set, so the progress reporter
///   can race one last `Maps` pass into the channel AFTER the final aggregation;
///   the final pass clears the maps, so that late pass sees empty maps and
///   no-ops. A SQL fallback here would overwrite the final `dir_stats` with a
///   depth-capped subset.
///
/// - `Sql`: the unified path. Recomputes from committed `entries` / `dir_stats`
///   rows scoped to the hot dirs, so it works on reconcile / network where the
///   maps are empty. It does NOT consult the maps, so a late `Sql` pass arriving
///   after the final aggregation is still safe: it's an idempotent
///   recompute-from-committed-rows (same values), not a stale-map subset.
pub(super) fn handle_compute_partial_aggregates(
    conn: &rusqlite::Connection,
    accumulator: &AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    hot_paths: Vec<String>,
    source: AggSource,
    signal: &IndexFailureSignal,
) {
    let t = Instant::now();
    let hot_paths_count = hot_paths.len();

    let result = match source {
        AggSource::Maps => {
            if accumulator.direct_stats.is_empty() {
                log::debug!("ComputePartialAggregates(Maps): maps empty, no-op");
                return;
            }
            aggregator::compute_partial_aggregates(
                conn,
                &accumulator.direct_stats,
                &accumulator.child_dirs,
                &hot_paths,
                PARTIAL_AGG_MAX_DEPTH,
            )
        }
        AggSource::Sql => aggregator::compute_partial_aggregates_sql(conn, &hot_paths, PARTIAL_AGG_SQL_MAX_SUBTREE),
    };

    match result {
        Ok(stats) => {
            log::info!(
                "ComputePartialAggregates({source:?}): {} dirs computed, {} rows written, \
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
                crate::indexing::reconcile::reconciler::emit_dir_updated(app, vec!["/".to_string()]);
            }
        }
        Err(e) => {
            signal.note(&e, &format!("compute_partial_aggregates({source:?})"));
        }
    }
    // No `bump_generation`: partial passes change no `entries` rows, only
    // transient `dir_stats`. Search-staleness detection cares about entry
    // existence, so a partial pass isn't a "mutation" for that purpose.
}

/// Persist the ledger-heal marker after a SUCCESSFUL full aggregate, and disarm
/// the latch — but only when the latch is armed AND the aggregate succeeded.
///
/// This is the whole heal-latch policy in one place: a failed aggregate
/// (`aggregate_ok == false`) leaves the key unset so the heal re-arms next
/// launch; a disarmed latch (an already-healed DB's routine aggregate) writes
/// nothing. A meta-write failure leaves the latch armed to retry.
fn set_heal_key_on_success(conn: &rusqlite::Connection, heal_latch: &mut bool, aggregate_ok: bool) {
    if !*heal_latch || !aggregate_ok {
        return;
    }
    match IndexStore::mark_ledger_heal_done(conn) {
        Ok(()) => {
            *heal_latch = false;
            log::info!("Ledger heal: rebuilt dir_stats aggregates and marked this index healed");
        }
        // Leave the latch armed so a later aggregate (or the next launch) retries.
        Err(e) => log::warn!("Ledger heal: failed to persist the healed marker (will retry): {e}"),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "writer handler: ambient state + the failure signal"
)]
pub(super) fn handle_compute_all_aggregates(
    conn: &rusqlite::Connection,
    accumulator: &mut AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    volume_id: &str,
    expected_total_entries: &AtomicU64,
    source: AggSource,
    heal_latch: &mut bool,
    signal: &IndexFailureSignal,
) {
    let t = Instant::now();
    // Only a `Maps` sender may read the accumulator, and only when it's non-empty
    // (an empty `Maps` sender whose maps were already consumed falls back to SQL,
    // never treats "empty" as "everything is zero"). A `Sql` sender ignores the
    // accumulator entirely — the Leak-D defense: a verification subtree scan can
    // leave the maps holding subtree-only data, and rolling every out-of-subtree
    // dir up from that would zero the whole tree outside the subtree.
    let use_maps = matches!(source, AggSource::Maps) && !accumulator.direct_stats.is_empty();
    log::info!(
        "ComputeAllAggregates({source:?}): using {} (direct_stats={} parents, child_dirs={} parents)",
        if use_maps { "in-memory maps" } else { "SQL" },
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
    // Clear the accumulator to free memory. A `Maps` run consumed it; a `Sql` run
    // ignored it, but clearing here is still correct — a full aggregate ends a
    // scan phase, and dropping any subtree-scan pollution left behind is harmless
    // (the next fresh scan's `TruncateData` clears it anyway, and the subtree
    // handler deliberately doesn't). Reset expected_total so subtree-scan inserts
    // don't emit spurious saving_entries progress events after the full scan.
    accumulator.clear();
    expected_total_entries.store(0, Ordering::Relaxed);
    match &result {
        Ok(count) => {
            log::info!(
                "ComputeAllAggregates: done, {} in {:.1}s",
                pluralize_with(*count, "directory", "directories"),
                t.elapsed().as_secs_f64(),
            );
        }
        Err(e) => {
            signal.note(e, "compute_all_aggregates");
        }
    }
    // Consume the one-shot ledger-heal latch, but ONLY on success: a full
    // aggregate recomputes every `dir_stats` row from the committed `entries`, so
    // an `Ok` here means this DB's drift is now healed. A failed rebuild leaves
    // the latch armed and the key unset, so the heal re-runs next launch.
    set_heal_key_on_success(conn, heal_latch, result.is_ok());
}

pub(super) fn handle_compute_subtree_aggregates(
    conn: &rusqlite::Connection,
    root_id: i64,
    repairs: &DeferredRepairs,
    signal: &IndexFailureSignal,
) {
    let t = Instant::now();
    match aggregator::compute_subtree_aggregates(conn, root_id) {
        Ok(count) => {
            log::debug!(
                "Index writer: computed subtree aggregates for {} under id={root_id} ({}ms)",
                pluralize(count, "dir"),
                t.elapsed().as_millis(),
            );
            // Repair the ancestor chain from the subtree root's PARENT. The scoped
            // recompute above already wrote the subtree root's fresh totals, so
            // repairing from the root itself would short-circuit immediately (its
            // row already agrees with its children); we must start one level up.
            // One writer-thread walk rolls up sizes, counts, `recursive_has_symlinks`,
            // AND `min_subtree_epoch` coverage at once — subsuming the former
            // symlink-only ancestor walk and both off-writer `PropagateDeltaById` /
            // `PropagateMinSubtreeEpoch` compensation blocks (deleted). Running here,
            // in the same message, makes it race-free by construction.
            if let Ok(Some(parent_id)) = IndexStore::get_parent_id(conn, root_id)
                && parent_id != 0
            {
                repair_dir_stats_upward(conn, parent_id, repairs);
            }
        }
        Err(e) => {
            signal.note(&e, &format!("compute_subtree_aggregates(id={root_id})"));
        }
    }
}

pub(super) fn handle_backfill_missing_dir_stats(
    conn: &rusqlite::Connection,
    repairs: &DeferredRepairs,
    signal: &IndexFailureSignal,
) {
    let t = Instant::now();
    match aggregator::backfill_missing_dir_stats(conn) {
        Ok(outcome) if outcome.backfilled == 0 => {
            log::debug!("BackfillMissingDirStats: no dirs missing stats");
            debug_assert!(outcome.parents_to_repair.is_empty());
        }
        Ok(outcome) => {
            // Leak C: backfill wrote the missing rows but never credited the
            // ancestors above them (a missing row means no delta ever walked
            // through that dir). Repair each missing root's parent upward on the
            // writer thread — idempotent, so the deduped chains just short-circuit
            // where they already agree.
            for parent_id in outcome.parents_to_repair {
                repair_dir_stats_upward(conn, parent_id, repairs);
            }
            log::info!(
                "BackfillMissingDirStats: computed stats for {} in {:.1}s",
                pluralize(outcome.backfilled, "dir"),
                t.elapsed().as_secs_f64(),
            );
        }
        Err(e) => {
            signal.note(&e, "backfill_missing_dir_stats");
        }
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
mod partial_tests;
#[cfg(test)]
mod tests;
