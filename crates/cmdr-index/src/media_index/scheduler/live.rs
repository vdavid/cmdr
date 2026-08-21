//! Live enrichment: follow the index.
//!
//! Today a full pass enriches on a `ScanCompleted` edge, a user kick, or the importance
//! bridge — so a NEW or MODIFIED image waits for the next completed scan, and a DELETED
//! image's rows linger until a later pass GCs them. This module closes that gap by
//! subscribing to the SAME live dir-changed signal importance's incremental rescore
//! consumes ([`subscribe_dirs_changed`](crate::indexing::lifecycle::lifecycle_bus::subscribe_dirs_changed)) and running a SCOPED tick over just the touched
//! directories, mirroring importance's proven `start_incremental` pattern (throttled,
//! coalesced, subtree-scoped) rather than inventing a new one.
//!
//! ## What a tick does
//!
//! - **Gates before it walks.** The coverage gates resolve first, and the touched dirs are
//!   filtered down to the ones that could enrich
//!   ([`local_dir_may_be_covered`](super::lifecycle::local_dir_may_be_covered)). Walking a
//!   dir costs a `resolve_path` plus a `list_children_on`; deciding not to costs a prefix
//!   check. When nothing survives, the tick returns without opening the index at all.
//! - **Walks only the surviving dirs** ([`walk_image_entries_in_dirs`]), sibling-aware per
//!   directory, never the whole index.
//! - **Enriches the stale, covered images** through the SAME per-image loop as the full
//!   pass (`enrich_and_gc`), honoring the coverage gates, the live exclusion veto, and
//!   the `(path, mtime, size)` + stamp staleness key — so a modified image re-enriches
//!   and an untouched one is a no-op.
//! - **GCs deletions SCOPED to the dirs it WALKED** ([`GcScope::TouchedDirs`]). An index
//!   removal is a fact about the tree (like importance's subtree clear), not a scan-state
//!   inference, so deleting a confirmed-gone row here doesn't violate the
//!   GC-needs-a-complete-tree doctrine. Crucially it must NEVER whole-store GC against a
//!   scoped walk — that would delete every row outside the touched dirs.
//!
//! ❗ **The filtered set is ONE set with three consumers**: the walk, the GC scope, and
//! `coverage::patch_touched_dirs`. Filtering the walk alone would make every stored row
//! under a dropped dir "in scope, absent from the walk, therefore deleted" — every OCR
//! text, Vision tag, and CLIP embedding in it — and would replace every dropped dir's
//! cached count with zero. The walk hands back a
//! [`WalkedDirs`](super::enrich::WalkedDirs) token that the other two are the only
//! consumers of, so that divergence doesn't compile; `live_tests.rs` covers what it
//! would have cost.
//!
//! ## The guardrails
//!
//! - **Local only.** Wired from [`wire_volume`](super::lifecycle::wire_volume) after its kind early-returns, for
//!   `PassKind::Local` only: the tick treats stored paths as OS paths (no mount mapping),
//!   and SMB never publishes dir-changed batches (its live path only enqueues index
//!   writes), so wiring it for network would be dead. MTP is never background-swept.
//! - **Distinct coordinator key** ([`live_key`]): a live tick coalesces on
//!   `"{volume_id}#live"`, NEVER the full-pass key — else a `ScanCompleted` full pass
//!   coalescing into a tick's slot would silently downgrade a full pass to a scoped tick.
//! - **Skip while a full pass runs.** The full pass covers the touched dirs, so a tick
//!   that finds the full-pass key running drops its drained dirs and does nothing.
//! - **Silent for small ticks.** The top-right indicator is for sweeps, not two-image
//!   blips: a tick lights it up ONLY when its enrichable subset exceeds
//!   [`LIVE_INDICATOR_THRESHOLD`]. Below that BOTH the progress sink and the terminal
//!   guard are suppressed together — a lone terminal on a silent tick would clear a
//!   visible full-pass row.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::enrich::{self, EnrichGates, GcScope, PassHooks, enrich_and_gc_scoped, walk_image_entries_in_dirs};
use super::{
    BeginOutcome, EnrichProgressEmitter, EnrichProgressSink, EnrichTerminalGuard, FinishOutcome,
    MediaEnrichTerminalReason, MediaScheduler, NoopProgressSink, gate, load_statuses, local_should_enrich, network,
};
use crate::indexing::lifecycle::lifecycle_bus;

/// Above this many enrichable images, a live tick joins the top-right indicator like a
/// full pass; at or below it the tick stays fully silent (the indicator is for
/// sweeps, not blips). A silent tick still enriches and scoped-GCs; only the UI is
/// suppressed.
const LIVE_INDICATOR_THRESHOLD: u64 = 25;

/// The minimum spacing between two live ticks of the same volume under sustained change.
/// A tick resolves + lists each touched dir, so a constant FSEvent firehose (a busy boot
/// volume is never truly idle) would drive back-to-back ticks; debouncing to one tick per
/// window bounds that. The FIRST change of a burst still enriches promptly (leading edge),
/// so a single real edit re-indexes within seconds. Mirrors importance's
/// `INCREMENTAL_THROTTLE_WINDOW`.
const LIVE_THROTTLE_WINDOW: Duration = Duration::from_secs(60);

/// Ticks that enriched and GC'd nothing, rolled up to one line a minute per
/// volume. Most ticks on a developer's machine are these: the churn is build
/// output, not photos. The count keeps the "ticks fire, nothing qualifies"
/// evidence in an error-report bundle without a line per tick.
static IDLE_TICKS: cmdr_fs::log_rollup::LogRollup = cmdr_fs::log_rollup::LogRollup::new(Duration::from_secs(60));

/// Coalescing key for live ticks: distinct from the full-pass key (the bare volume id) so
/// a scoped tick and a full recompute for the same volume don't share a coordinator slot
/// — a `ScanCompleted` full pass must NEVER coalesce into a tick's slot and silently
/// downgrade to a scoped tick (they serialize at the writer thread anyway).
fn live_key(volume_id: &str) -> String {
    format!("{volume_id}#live")
}

/// Whether a live tick should light up the top-right indicator, or stay fully silent.
/// Loud ONLY when the enrichable subset is a real sweep
/// (`> LIVE_INDICATOR_THRESHOLD`) AND no full pass is running for the volume (whose own
/// indicator row this tick must not stomp). The threshold gates BOTH the progress sink and
/// the terminal guard together: a lone row-clearing terminal on an otherwise-silent tick
/// would clear a visible full-pass row. Pure, so the gating is unit-testable.
fn tick_is_loud(subset_total: u64, full_pass_running: bool) -> bool {
    subset_total > LIVE_INDICATOR_THRESHOLD && !full_pass_running
}

/// How long to wait before the next live tick of a volume may start, given when the
/// previous one for this run started. The FIRST tick of a burst (`last_started == None`)
/// runs immediately (leading edge); each further tick while change keeps arriving waits
/// out the window (trailing edge — at most one walk per window under sustained churn).
/// Pure, so the spacing is unit-testable without a runtime. Mirrors importance's
/// `incremental_debounce_wait`.
fn live_debounce_wait(last_started: Option<Instant>, now: Instant, window: Duration) -> Duration {
    match last_started {
        None => Duration::ZERO,
        Some(started) => window.saturating_sub(now.saturating_duration_since(started)),
    }
}

/// Record one tick that enriched and GC'd nothing — the normal case on a machine whose
/// churn is builds, not photos. It was ~1,500 lines an hour, so it rolls up into one line
/// a minute per volume instead. The zero case still reaches the bundle (that ticks fire
/// and enrich nothing is exactly the "why aren't my photos indexed?" evidence), just once
/// per window rather than once per tick.
fn log_idle_tick(volume_id: &str, touched: usize) {
    if let Some(batch) = IDLE_TICKS.record(volume_id) {
        let rolled_up = if batch.is_rolled_up() {
            format!(" ×{} in {}s", batch.count, batch.elapsed.as_secs())
        } else {
            String::new()
        };
        log::debug!(
            target: "media_index",
            "live tick of '{volume_id}': nothing to enrich or GC{rolled_up} ({touched} touched dir(s))",
        );
    }
}

impl MediaScheduler {
    /// Accumulate `dirs` (touched directory paths) into the volume's pending live set.
    fn accumulate_touched_dirs(&self, volume_id: &str, dirs: Vec<String>) {
        let mut pending = self.pending_touched_dirs.lock_ignore_poison();
        pending.entry(volume_id.to_string()).or_default().extend(dirs);
    }

    /// Drain and return the volume's pending touched dirs (empties the set).
    fn take_touched_dirs(&self, volume_id: &str) -> HashSet<String> {
        let mut pending = self.pending_touched_dirs.lock_ignore_poison();
        match pending.get_mut(volume_id) {
            Some(set) => std::mem::take(set),
            None => HashSet::new(),
        }
    }

    /// Run one SCOPED live tick for a volume: filter `touched_dirs` down to the ones
    /// coverage could reach, walk only those, enrich the stale covered images, and GC
    /// deletions SCOPED to the dirs it walked. Blocking (SQLite + backend), so the caller
    /// runs it off the async worker. Returns the number of images enriched.
    ///
    /// Gated on the master toggle (off ⇒ no-op) and skip-on-`None` for an unregistered
    /// read pool, like the full pass. It does NOT `mark_deferred_for_importance` on an
    /// unscored volume: the full-pass bridge covers that, and marking here would trigger a
    /// full re-walk on the next importance bump.
    pub(crate) fn run_live_tick_blocking(
        &self,
        volume_id: &str,
        touched_dirs: &HashSet<String>,
    ) -> Result<usize, String> {
        if !gate::is_enabled() {
            return Ok(0);
        }
        let Some(pool) = crate::indexing::get_read_pool_for(volume_id) else {
            return Ok(0);
        };

        // The SAME coverage gates as the full pass (scope + importance threshold +
        // override), read from the start-of-tick snapshot; the privacy exclusion is read
        // LIVE. A tick never marks the volume deferred-on-importance: it walks only the
        // touched dirs, so the bridge re-kick it would ask for belongs to a full pass.
        let threshold = gate::importance_threshold();
        let scores = super::lifecycle::pass_coverage(gate::scope(), || self.folder_scores(volume_id, threshold)).scores;
        let config = network::config::snapshot();

        // ONE filtered set, and every consumer below takes THIS one: the walk, the
        // scoped GC, and the coverage-counts patch. Filtering the walk alone would leave
        // the GC holding rows that are "in scope but not walked" and delete every one of
        // them (§ the scoped-GC trap in `DETAILS.md`), and would zero the cached counts
        // of every dir it dropped.
        let covered_dirs: HashSet<String> = touched_dirs
            .iter()
            .filter(|dir| super::lifecycle::local_dir_may_be_covered(dir, scores.as_deref(), &config, volume_id))
            .cloned()
            .collect();
        if covered_dirs.is_empty() {
            // Nothing here could enrich, so there is nothing to walk, nothing in GC
            // scope, and nothing to patch. The common case on a machine whose churn is
            // build output: this is the whole tick.
            log_idle_tick(volume_id, touched_dirs.len());
            return Ok(0);
        }

        // The scoped walk: only the covered dirs' qualifying images. Don't early-return on
        // an empty result — an emptied covered dir still needs its stored rows scoped-GC'd.
        // It hands back the dirs it covered as a `WalkedDirs` token, which is the ONLY
        // thing the GC scope and the counts patch below accept.
        let (walked, images) = pool
            .with_conn(|conn| walk_image_entries_in_dirs(conn, &covered_dirs))
            .map_err(|e| format!("read pool error: {e}"))??;

        let statuses = load_statuses(&self.data_dir, volume_id);
        let writer = self
            .writers
            .writer_for(&self.data_dir, volume_id)
            .map_err(|e| e.to_string())?;

        let should_enrich = |path: &str| -> bool { local_should_enrich(path, scores.as_deref(), &config, volume_id) };
        let is_excluded = |path: &str| -> bool { network::config::is_excluded(path) };
        let folder_score = |dir: &str| -> f64 { scores.as_ref().and_then(|m| m.get(dir)).copied().unwrap_or(0.0) };
        let ordered = enrich::prioritized(&images, &folder_score);

        // Progress honesty: light up the indicator ONLY when the enrichable
        // subset is a real sweep, and never over a running full pass (whose own row this
        // tick must not stomp). The threshold gates BOTH the progress sink and the terminal
        // guard together — a row-clearing terminal on a silent tick would clear a visible
        // full-pass row.
        let (subset_total, _) = enrich::enrichable_totals(&ordered, &should_enrich, &is_excluded);
        let loud = tick_is_loud(subset_total, self.is_enriching(volume_id));
        let (progress, mut terminal): (Box<dyn EnrichProgressSink>, EnrichTerminalGuard) = if loud {
            (
                Box::new(EnrichProgressEmitter::new(
                    Arc::clone(&self.events),
                    volume_id.to_string(),
                )),
                EnrichTerminalGuard::for_sink(Arc::clone(&self.events), volume_id.to_string()),
            )
        } else {
            (Box::new(NoopProgressSink), EnrichTerminalGuard::disabled())
        };

        let hooks = PassHooks {
            // Stop on the watchdog emergency stop OR a master-toggle OFF (§ gate), so a
            // disable halts even a live tick promptly.
            cancel: &gate::should_stop,
            progress: progress.as_ref(),
        };
        // A live tick CLIP-embeds a new/changed image too (two-part staleness), so the
        // just-indexed photo is semantically searchable without waiting for a full pass.
        let clip_stamp = crate::media_index::clip::current_stamp(&self.data_dir);
        let summary = enrich_and_gc_scoped(
            &ordered,
            &statuses,
            self.backend.as_ref(),
            &writer,
            &EnrichGates {
                should_enrich: &should_enrich,
                is_excluded: &is_excluded,
                // SCOPED GC: only rows under the dirs this tick WALKED are candidates, so a
                // row in a dir the tick never walked — filtered out, or never touched at
                // all — is never deleted (the scoped-GC data-safety trap).
                gc_scope: GcScope::TouchedDirs(walked),
                clip_stamp: clip_stamp.as_deref(),
            },
            &hooks,
        )?;
        terminal.set(if summary.cancelled {
            MediaEnrichTerminalReason::Cancelled
        } else {
            MediaEnrichTerminalReason::Completed {
                enriched: summary.enriched as u64,
                gc_count: summary.gc_count as u64,
            }
        });

        if summary.enriched > 0 || summary.gc_count > 0 {
            // Land the tick's buffered ANN ops before invalidating (plan M6; as the
            // full pass does). Best-effort.
            let _ = writer.flush_ann_index();
            // The volume's embeddings changed; drop the resident vector cache so the next
            // find-similar / dedup reloads (as the full pass does).
            super::super::vector::cache::invalidate(&super::super::store::media_db_path(&self.data_dir, volume_id));
            // The qualifying set shifted, but ONLY within the dirs this tick walked — patch
            // just those in the cached counts instead of invalidating the whole volume (a
            // full rebuild is the O(entries) cold walk the cache exists to avoid). A GC'd
            // deletion or a new/changed image both move a walked dir's qualifying count, so
            // this runs on the same condition as the vector invalidate. `images` is the
            // scoped walk over `covered_dirs`, so patching exactly those dirs is what keeps
            // the patch complete per dir; a no-op if no counts are cached yet.
            super::super::coverage::patch_touched_dirs(volume_id, walked, &images);
        }
        // A tick that changed something always says so; one that didn't rolls up.
        if summary.enriched > 0 || summary.gc_count > 0 {
            log::debug!(
                target: "media_index",
                "live tick '{volume_id}': {} enriched, {} GC'd across {} of {} touched dir(s)",
                summary.enriched,
                summary.gc_count,
                walked.len(),
                touched_dirs.len(),
            );
        } else {
            log_idle_tick(volume_id, touched_dirs.len());
        }
        Ok(summary.enriched)
    }
}

/// Subscribe a LOCAL volume to its dir-changed bus and run a scoped live tick for each
/// batch of listing changes. Coalesces overlapping batches per volume
/// (accumulating their touched dirs) so a burst of FSEvents collapses to one tick plus at
/// most one re-run, never a tick per event — mirroring importance's `start_incremental`.
pub(crate) fn start_live_follow(scheduler: Arc<MediaScheduler>, volume_id: String) {
    let mut rx = lifecycle_bus::subscribe_dirs_changed(&volume_id);
    crate::indexing::host::runtime::spawn(async move {
        // The retained initial value is the empty batch; `borrow_and_update` marks it seen
        // so only a real later change triggers.
        rx.borrow_and_update();
        while rx.changed().await.is_ok() {
            // The bus carries the ORIGIN dirs (those whose own listings changed), so a
            // tick walks exactly the directories that gained or lost entries.
            let origins = rx.borrow_and_update().origins.clone();
            if origins.is_empty() {
                continue;
            }
            spawn_live_tick(Arc::clone(&scheduler), volume_id.clone(), origins);
        }
    });
}

/// Request a coalesced live tick, accumulating `dirs` into the pending set. If this
/// request starts the tick, drive it (plus any coalesced re-run, draining whatever
/// accumulated meanwhile) on a blocking background task.
fn spawn_live_tick(scheduler: Arc<MediaScheduler>, volume_id: String, dirs: Vec<String>) {
    let key = live_key(&volume_id);
    scheduler.accumulate_touched_dirs(&volume_id, dirs);
    if scheduler.coordinator.request(&key) == BeginOutcome::Coalesced {
        return; // a tick is running; it drains the accumulated dirs on re-run.
    }
    crate::indexing::host::runtime::spawn(async move {
        let key = live_key(&volume_id);
        // Debounce across this run's ticks: the first runs immediately (leading edge), each
        // further one waits out the window so sustained churn drives at most one walk per
        // window. Requests during the wait coalesce (the slot stays running).
        let mut last_started: Option<Instant> = None;
        loop {
            let wait = live_debounce_wait(last_started, Instant::now(), LIVE_THROTTLE_WINDOW);
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            let dirs = scheduler.take_touched_dirs(&volume_id);
            // Skip the whole tick while a FULL pass runs for this volume: it re-walks the
            // whole index and covers these dirs, so the drained dirs are dropped (the next
            // completed pass or a later change heals any gap). Checking the full-pass key
            // (the bare volume id) here is why the live key must stay distinct.
            if !dirs.is_empty() && !scheduler.is_enriching(&volume_id) {
                last_started = Some(Instant::now());
                let sched = Arc::clone(&scheduler);
                let vid = volume_id.clone();
                let result =
                    crate::indexing::host::runtime::spawn_blocking(move || sched.run_live_tick_blocking(&vid, &dirs))
                        .await;
                match result {
                    // No line on success: `run_live_tick_blocking` already logged the
                    // outcome with the counts AND the touched-dir count, so this was a
                    // strictly-less-informative duplicate of every tick.
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => log::warn!(target: "media_index", "live tick of '{volume_id}' failed: {e}"),
                    Err(e) => log::warn!(target: "media_index", "live tick task for '{volume_id}' panicked: {e}"),
                }
            }
            if scheduler.coordinator.finish(&key) == FinishOutcome::Done {
                break;
            }
            // RunAgain: more dirs accumulated mid-tick; loop and drain them.
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_key_is_distinct_from_the_full_pass_key() {
        // The full pass coalesces on the bare volume id; the tick must NOT share that slot,
        // or a ScanCompleted full pass would silently downgrade to a scoped tick.
        assert_ne!(live_key("root"), "root");
        assert_eq!(live_key("root"), "root#live");
    }

    #[test]
    fn a_tick_is_silent_below_the_threshold_or_under_a_full_pass() {
        // A real sweep with no full pass running ⇒ loud.
        assert!(tick_is_loud(LIVE_INDICATOR_THRESHOLD + 1, false));
        // At or below the threshold ⇒ silent (a two-image blip never lights the indicator).
        assert!(!tick_is_loud(LIVE_INDICATOR_THRESHOLD, false));
        assert!(!tick_is_loud(1, false));
        // Even a big subset stays silent while a full pass runs, so the tick never stomps
        // the full pass's own indicator row.
        assert!(!tick_is_loud(LIVE_INDICATOR_THRESHOLD + 1_000, true));
    }

    #[test]
    fn debounce_runs_the_first_tick_immediately_then_spaces_the_rest() {
        let window = Duration::from_secs(60);
        let now = Instant::now();
        // Leading edge: nothing ran yet ⇒ go now.
        assert_eq!(live_debounce_wait(None, now, window), Duration::ZERO);
        // Trailing edge: a tick started 10 s ago ⇒ wait out the remaining ~50 s.
        let started = now - Duration::from_secs(10);
        let wait = live_debounce_wait(Some(started), now, window);
        assert!(wait > Duration::from_secs(49) && wait <= Duration::from_secs(50));
        // Fully elapsed ⇒ no wait.
        let old = now - Duration::from_secs(120);
        assert_eq!(live_debounce_wait(Some(old), now, window), Duration::ZERO);
    }
}
