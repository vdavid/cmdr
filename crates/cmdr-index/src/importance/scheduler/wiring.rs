//! Building the scheduler and wiring it to the volume lifecycle.
//!
//! `ImportanceScheduler` (in `mod.rs`) owns scoring passes and their
//! coordination; this module owns everything around them: constructing the
//! singleton, subscribing to the registration bus before the startup sweep so a
//! share mounting mid-sweep isn't dropped in the gap, deciding whether a volume
//! needs an initial full pass, and the debounced spawn paths for incremental and
//! full recompute work.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{BeginOutcome, FinishOutcome, ImportanceScheduler, ScoringPolicy};
use crate::IndexVolumeKind;
use crate::importance::scorer::SignalSet;
use crate::indexing::lifecycle::lifecycle_bus;

/// Build and wire the scheduler, behind [`ImportanceScheduler::start`], which carries
/// the contract this fulfils. The registration bus catches a share mounted
/// MID-SESSION; the startup sweep catches volumes already ready at launch —
/// subscribing to the bus BEFORE the sweep closes the gap so no registration is
/// missed.
pub(super) fn build_and_wire() -> Option<Arc<ImportanceScheduler>> {
    let data_dir = match crate::indexing::host::config::data_dir() {
        Ok(d) => d,
        Err(e) => {
            log::warn!(target: "importance", "importance scheduler not started: {e}");
            return None;
        }
    };
    let scheduler = Arc::new(ImportanceScheduler::new(data_dir));

    // Subscribe to registrations FIRST (before the sweep), so a volume that
    // registers during the sweep isn't dropped in the gap. Each registration
    // wires that volume's per-volume subscriptions and scores it if it's
    // already ready.
    let reg_scheduler = Arc::clone(&scheduler);
    let mut reg_rx = lifecycle_bus::subscribe_registrations();
    crate::indexing::host::runtime::spawn(async move {
        loop {
            match reg_rx.recv().await {
                Ok(reg) => wire_volume(Arc::clone(&reg_scheduler), reg.volume_id, reg.kind),
                // A lag only skips a registration the next scan-completion covers
                // anyway; keep listening. A closed bus (never, it's process-global)
                // ends the task.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Startup sweep: any volume already ready at launch (loaded from its persisted
    // scan_completed_at) never re-fires ScanCompleted, so catch it here — WITH its
    // typed kind so MTP is excluded and SMB degrades correctly. Wiring alone only
    // sets up subscriptions (the retained bus value stays `Pending`); the
    // initial-pass trigger is what actually scores a fresh / recreated store.
    for (volume_id, kind) in crate::indexing::lifecycle::state::ready_volumes_with_kind() {
        wire_volume(Arc::clone(&scheduler), volume_id.clone(), kind);
        enqueue_initial_full_pass_if_unscored(Arc::clone(&scheduler), volume_id, kind);
    }

    // The caller owns the handle: the app keeps it in Tauri state so `record_visit`
    // can route its write through the shared per-volume writer the scheduler owns
    // (one writer thread per DB) rather than spawning one per navigation.
    Some(scheduler)
}

/// For a volume READY at launch (Fresh, so no `ScanCompleted` will fire), enqueue a
/// full recompute IFF its store has no generation yet — a fresh install, a
/// schema-recreated store (the prod schema-3 upgrade), or one maintained only by
/// incremental rescores (which never stamp a generation). An already-scored volume is
/// left alone; an unconditional kick would rescore every volume on every launch
/// (importance's policy differs from media's cheap unconditional kick).
///
/// The "unscored?" decision binds to the WRITE-path store open via
/// [`crate::importance::store::needs_initial_full_pass`], which forces the lazy schema recreate
/// BEFORE reading the generation — never a sweep-time read probe, which would read the
/// outgoing schema's stamped generation and skip, only for the recreate to wipe it
/// moments later (the prod-upgrade ordering trap). The probe (a DB open) runs on a
/// blocking task; when unscored it hands off to the normal coordinated
/// [`spawn_recompute`], so a concurrent `ScanCompleted` coalesces correctly.
pub(super) fn enqueue_initial_full_pass_if_unscored(
    scheduler: Arc<ImportanceScheduler>,
    volume_id: String,
    kind: IndexVolumeKind,
) {
    let available = match ScoringPolicy::for_kind(kind) {
        ScoringPolicy::Scored { available } => available,
        // MTP: on-demand only, never background-scored.
        ScoringPolicy::Excluded => return,
    };
    crate::indexing::host::runtime::spawn(async move {
        let data_dir = scheduler.data_dir().to_path_buf();
        let vid = volume_id.clone();
        let needs = crate::indexing::host::runtime::spawn_blocking(move || {
            should_enqueue_initial_full_pass(kind, &data_dir, &vid)
        })
        .await;
        match needs {
            Ok(Ok(true)) => {
                log::info!(
                    target: "importance",
                    "volume '{volume_id}' ready at launch with no generation (fresh/recreated); enqueuing an initial full recompute"
                );
                spawn_recompute(scheduler, volume_id, available);
            }
            Ok(Ok(false)) => {} // already scored — leave it.
            Ok(Err(e)) => log::warn!(target: "importance", "initial-pass probe for '{volume_id}' failed: {e}"),
            Err(e) => log::warn!(target: "importance", "initial-pass probe task for '{volume_id}' panicked: {e}"),
        }
    });
}

/// Whether a volume ready at launch needs an initial full recompute enqueued: its kind
/// is background-scored (not MTP) AND its store carries no generation yet (fresh /
/// schema-recreated / incremental-only). Binds the "unscored?" check to the write-path
/// store open ([`crate::importance::store::needs_initial_full_pass`]), which forces any lazy schema
/// recreate before reading the generation. Extracted from
/// [`enqueue_initial_full_pass_if_unscored`] so the combined kind + store-state decision
/// is testable without spawning the recompute (which needs a read pool).
pub(super) fn should_enqueue_initial_full_pass(
    kind: IndexVolumeKind,
    data_dir: &std::path::Path,
    volume_id: &str,
) -> Result<bool, crate::importance::store::ImportanceStoreError> {
    if matches!(ScoringPolicy::for_kind(kind), ScoringPolicy::Excluded) {
        return Ok(false); // MTP: on-demand only, never background-scored.
    }
    crate::importance::store::needs_initial_full_pass(data_dir, volume_id)
}

/// Wire one volume into the scheduler by its typed kind: skip MTP (on-demand
/// only), and for Local/SMB set up its scan-completion subscription (full
/// recompute) and its dir-changed subscription (incremental rescore), then score
/// it once if it's already ready.
///
/// Idempotent per volume in practice: the coalescing coordinator collapses a
/// re-wire's duplicate recompute into the running one, and the underlying `watch`
/// buses are per-volume, so re-subscribing spawns a second listener but each drives
/// the same coalesced pass. A volume is wired from at most two places (the sweep
/// and one registration), so no unbounded listener growth.
fn wire_volume(scheduler: Arc<ImportanceScheduler>, volume_id: String, kind: IndexVolumeKind) {
    let available = match ScoringPolicy::for_kind(kind) {
        ScoringPolicy::Scored { available } => available,
        // MTP: on-demand only, never background-scored (a typed exclusion).
        ScoringPolicy::Excluded => {
            log::debug!(target: "importance", "importance skips '{volume_id}' ({kind:?}): on-demand only");
            return;
        }
    };

    // Incremental recompute: rescore only the touched subtrees + capped ancestor
    // chains as live listing changes land. Full-volume recompute
    // stays the scan-completion default below.
    start_incremental(Arc::clone(&scheduler), volume_id.clone(), available);

    // And a slow full pass, which is what BOUNDS the staleness the incremental path
    // deliberately accepts (see below).
    start_periodic_full_refresh(Arc::clone(&scheduler), volume_id.clone(), available);

    // Subscribe to the scan bus for this volume; a subscription retains the last
    // state, so a ScanCompleted fired before this line is still observed
    // (late-subscriber replay). Recompute on each completion.
    //
    // And to home coverage, which is the EARLY half of the same signal: a volume
    // covered in phases reaches "home is walked" minutes before it reaches "the
    // drive is walked", and home is all this needs. A volume walked whole never
    // fires it, so nothing changes there.
    let sub_scheduler = Arc::clone(&scheduler);
    let sub_volume = volume_id.clone();
    let mut rx = lifecycle_bus::subscribe(&volume_id);
    let mut home_rx = lifecycle_bus::subscribe_home_covered(&volume_id);
    crate::indexing::host::runtime::spawn(async move {
        // Observe the retained values first (covers a signal fired before subscribe,
        // and a sweep-ready volume that already loaded Completed).
        if matches!(*rx.borrow_and_update(), lifecycle_bus::ScanState::Completed { .. }) || *home_rx.borrow_and_update()
        {
            spawn_recompute(Arc::clone(&sub_scheduler), sub_volume.clone(), available);
        }
        loop {
            let woken = tokio::select! {
                changed = rx.changed() => changed
                    .is_ok()
                    .then(|| matches!(*rx.borrow_and_update(), lifecycle_bus::ScanState::Completed { .. })),
                changed = home_rx.changed() => changed.is_ok().then(|| *home_rx.borrow_and_update()),
            };
            match woken {
                // A sender is gone, which for a process-global bus means shutdown.
                None => break,
                Some(true) => spawn_recompute(Arc::clone(&sub_scheduler), sub_volume.clone(), available),
                Some(false) => {}
            }
        }
    });
}

/// How often a volume gets a full recompute regardless of what changed.
///
/// The incremental path accepts two bounded stalenesses on purpose, and this is the
/// thing that bounds them:
///
/// - A row whose SIGNALS didn't move isn't rewritten, so its score keeps the
///   `now_secs` it was last written at and its recency decay pauses
///   (`../writer.rs`, `fate_of_stored_row`).
/// - A demoted origin seeds `has_marker_below` from its stored row, which can only
///   ADD marker presence, so the last marker LEAVING a big subtree reads stale
///   (`scoped_walk.rs`, § "The accepted lossiness").
///
/// Both correct themselves here. One hour is David's call over the daily cadence
/// this was first sized for, and it is affordable: measured against the real indexes
/// on 2026-08-04 (release build, `importance-measure`), a full pass costs **5.8 s CPU
/// for the 694,963-directory boot volume** and **1.6 s for the 71,365-directory NAS**,
/// so hourly is ~0.2% of one core. The walk reads the per-volume index DB, never the
/// volume, so a network volume costs no traffic and doesn't need to be awake.
///
/// ⚠️ The real cost is the transient allocation, not CPU: the boot-volume walk grows
/// `phys_footprint` by ~166 MB while it runs. Shortening this interval multiplies
/// that spike, not the CPU. ❌ Don't take it anywhere near
/// [`INCREMENTAL_THROTTLE_WINDOW`]: a full pass per minute is exactly the treadmill
/// `docs/notes/importance-treadmill-2026-08-04.md` exists to document, which cost
/// 17.6% of a 10.5-hour session's wall clock.
const FULL_REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Run a full recompute every [`FULL_REFRESH_INTERVAL`], forever.
///
/// Deliberately fires on the interval rather than at once: the scan-completion
/// subscription in [`wire_volume`] already covers startup, so an immediate tick would
/// only duplicate it. [`spawn_recompute`] coalesces on the full-pass key, so a tick
/// landing inside a running pass is absorbed rather than queued.
fn start_periodic_full_refresh(scheduler: Arc<ImportanceScheduler>, volume_id: String, available: SignalSet) {
    crate::indexing::host::runtime::spawn(async move {
        loop {
            tokio::time::sleep(FULL_REFRESH_INTERVAL).await;
            log::debug!(target: "importance", "periodic full refresh for '{volume_id}'");
            spawn_recompute(Arc::clone(&scheduler), volume_id.clone(), available);
        }
    });
}

/// Subscribe to a volume's dir-changed bus and run a bounded incremental rescore
/// for each batch of live listing changes. Coalesces overlapping
/// batches per volume (accumulating their paths) so a burst of FSEvents collapses
/// to one pass plus at most one re-run, never a pass per event.
fn start_incremental(scheduler: Arc<ImportanceScheduler>, volume_id: String, available: SignalSet) {
    let mut rx = lifecycle_bus::subscribe_dirs_changed(&volume_id);
    crate::indexing::host::runtime::spawn(async move {
        // The retained initial value is the empty batch (nothing published yet);
        // `borrow_and_update` marks it seen so the first real change triggers.
        rx.borrow_and_update();
        while rx.changed().await.is_ok() {
            // The bus carries the ORIGIN dirs (those whose own listings changed), not
            // their ancestor closure, so the rescore's downward subtree expansion
            // stays proportional to what actually changed.
            let origins = rx.borrow_and_update().origins.clone();
            if origins.is_empty() {
                continue;
            }
            spawn_incremental(Arc::clone(&scheduler), volume_id.clone(), available, origins);
        }
    });
}

/// Coalescing key for incremental passes: distinct from the full-pass key so an
/// incremental rescore and a full recompute for the same volume don't block each
/// other in the coordinator (they serialize at the writer thread anyway).
fn incremental_key(volume_id: &str) -> String {
    format!("{volume_id}#incremental")
}

/// The minimum spacing between two incremental rescores of the same volume under
/// sustained change. A busy boot volume is never truly idle, so without a window
/// the FSEvent firehose would drive back-to-back passes forever.
///
/// What the window paces is the store write plus its WAL checkpoint. It is NOT the
/// walk (the scoped walk made a typical pass microseconds — `scoped_walk.rs`), and it
/// is no longer the weight reload either: `notify_recompute_completed` now ships a
/// DELTA, so `search::volumes` patches its map in O(changed) instead of rebuilding it.
/// **So the window CAN come down — but that's David's call, not a side effect.**
/// Rationale and the measured numbers: `DETAILS.md` § Throttle.
///
/// Importance is a background signal, so a lag of this order is invisible to its
/// consumers.
const INCREMENTAL_THROTTLE_WINDOW: Duration = Duration::from_secs(60);

/// Rescores whose whole batch was filtered out before the walk, rolled up to one
/// line a minute per volume. On a machine running cargo that's nearly every pass:
/// the churn is all under floored build trees. Policy: `docs/tooling/logging.md`.
static EMPTY_RESCORES: cmdr_fs::log_rollup::LogRollup = cmdr_fs::log_rollup::LogRollup::new(Duration::from_secs(60));

/// How long to wait before the next incremental rescore of a volume may start,
/// given when the previous one for this run started. The FIRST pass of a burst
/// (`last_started == None`) runs immediately (leading edge — a real edit scores
/// promptly); each further pass while change keeps arriving waits out the window
/// (trailing edge — at most one walk per window under sustained churn). Pure so the
/// spacing is unit-testable without a runtime; the caller sleeps this long.
pub(super) fn incremental_debounce_wait(last_started: Option<Instant>, now: Instant, window: Duration) -> Duration {
    match last_started {
        // Leading edge: nothing ran yet this run, so go now.
        None => Duration::ZERO,
        // Trailing edge: wait out whatever remains of the window since the last
        // pass started (zero once the window has fully elapsed).
        Some(started) => window.saturating_sub(now.saturating_duration_since(started)),
    }
}

/// Request a coalesced incremental rescore, accumulating `paths` into the pending
/// set. If this request starts the pass, drive it (plus any coalesced re-run,
/// draining whatever accumulated meanwhile) on a blocking background task.
fn spawn_incremental(scheduler: Arc<ImportanceScheduler>, volume_id: String, available: SignalSet, paths: Vec<String>) {
    let key = incremental_key(&volume_id);
    scheduler.pending_incremental_paths(&volume_id, paths);
    if scheduler.coordinator.request(&key) == BeginOutcome::Coalesced {
        return; // a pass is running; it will drain the accumulated paths on re-run.
    }
    crate::indexing::host::runtime::spawn(async move {
        let key = incremental_key(&volume_id);
        // Debounce across this run's passes: the first runs immediately (leading
        // edge), each further one waits out the window so sustained churn drives at
        // most one index walk per window. Requests arriving during the wait coalesce
        // (the coordinator slot stays running), so the next drain folds them all in.
        let mut last_started: Option<Instant> = None;
        loop {
            let wait = incremental_debounce_wait(last_started, Instant::now(), INCREMENTAL_THROTTLE_WINDOW);
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            let batch = scheduler.take_incremental_paths(&volume_id);
            if !batch.is_empty() {
                last_started = Some(Instant::now());
                let sched = Arc::clone(&scheduler);
                let vid = volume_id.clone();
                let result = crate::indexing::host::runtime::spawn_blocking(move || {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    sched.run_incremental_blocking(&vid, available, &batch, now)
                })
                .await;
                match result {
                    // BOTH numbers: `written` alone would read as "this pass was
                    // free" while the batch still covers most of the volume, which is
                    // the cost that remains once the writes are gone.
                    //
                    // A pass that CONSIDERED nothing is the one exception. Its whole
                    // batch was filtered out before the walk (`sanitize_incremental_batch`
                    // drops floored paths, and a build tree is all floored), so it read
                    // nothing, wrote nothing, and cost nothing. Measured on a machine
                    // running cargo: 908 of 972 rescore lines in half an hour said
                    // `updated 0 folders (of 0 rescored)`. Those roll up to one line a
                    // minute per volume; `considered > 0` always logs, so the too-wide
                    // batch signal this line exists for is untouched.
                    Ok(Ok(report)) if report.considered == 0 => {
                        if let Some(batch) = EMPTY_RESCORES.record(&volume_id) {
                            let rolled_up = if batch.is_rolled_up() {
                                format!(" ×{} in {}s", batch.count, batch.elapsed.as_secs())
                            } else {
                                String::new()
                            };
                            log::debug!(
                                target: "importance",
                                "incremental rescore of '{volume_id}': nothing to rescore{rolled_up}",
                            );
                        }
                    }
                    Ok(Ok(report)) => log::debug!(
                        target: "importance",
                        "incremental rescore of '{volume_id}' updated {} (of {} rescored)",
                        cmdr_fs::pluralize::pluralize(report.written as u64, "folder"),
                        report.considered
                    ),
                    Ok(Err(e)) => log::warn!(target: "importance", "incremental rescore of '{volume_id}' failed: {e}"),
                    Err(e) => log::warn!(target: "importance", "incremental task for '{volume_id}' panicked: {e}"),
                }
            }
            if scheduler.coordinator.finish(&key) == FinishOutcome::Done {
                break;
            }
            // RunAgain: more paths accumulated mid-pass; loop and drain them.
        }
    });
}

/// Request a coalesced recompute for a volume and, if this request starts the
/// pass, drive it (plus any coalesced re-run) on a blocking background task.
fn spawn_recompute(scheduler: Arc<ImportanceScheduler>, volume_id: String, available: SignalSet) {
    if scheduler.coordinator.request(&volume_id) == BeginOutcome::Coalesced {
        // A pass is already running for this volume; it will re-run once when it
        // finishes (the coordinator set the flag). Nothing to spawn.
        return;
    }
    crate::indexing::host::runtime::spawn(async move {
        loop {
            let sched = Arc::clone(&scheduler);
            let vid = volume_id.clone();
            // Recompute is blocking (SQLite + scoring); run it off the async
            // worker so it never parks the runtime, and never on the IPC thread.
            let result = crate::indexing::host::runtime::spawn_blocking(move || {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                sched.run_pass_blocking(&vid, available, now)
            })
            .await;
            match result {
                Ok(Ok(count)) => log::debug!(
                    target: "importance",
                    "recompute of '{volume_id}' scored {}",
                    cmdr_fs::pluralize::pluralize(count as u64, "folder")
                ),
                Ok(Err(e)) => log::warn!(target: "importance", "recompute of '{volume_id}' failed: {e}"),
                Err(e) => log::warn!(target: "importance", "recompute task for '{volume_id}' panicked: {e}"),
            }
            if scheduler.coordinator.finish(&volume_id) == FinishOutcome::Done {
                break;
            }
            // RunAgain: a request arrived mid-pass; loop once more.
        }
    });
}

#[cfg(test)]
mod periodic_refresh_tests {
    use super::{FULL_REFRESH_INTERVAL, INCREMENTAL_THROTTLE_WINDOW};

    /// The full refresh has to stay FAR slower than the incremental throttle.
    ///
    /// Pre-fix, a full walk ran roughly once a minute and burned 17.6% of a 10.5-hour
    /// session's wall clock (`docs/notes/importance-treadmill-2026-08-04.md`). Nothing
    /// in the types stops someone "making importance fresher" by dropping this to the
    /// incremental cadence and rebuilding that treadmill, so the ordering is pinned
    /// here rather than left to a comment.
    #[test]
    fn the_full_refresh_is_far_slower_than_the_incremental_throttle() {
        assert!(
            FULL_REFRESH_INTERVAL >= INCREMENTAL_THROTTLE_WINDOW * 30,
            "a full pass costs ~5.8 s CPU and a ~166 MB transient allocation on a big volume, \
             so it must stay orders of magnitude rarer than an incremental one: \
             full={FULL_REFRESH_INTERVAL:?}, incremental={INCREMENTAL_THROTTLE_WINDOW:?}"
        );
    }
}
