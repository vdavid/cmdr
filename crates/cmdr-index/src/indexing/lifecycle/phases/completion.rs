//! When a volume is done, and what being done owes everyone.
//!
//! **Completion is derived, never remembered.** The rule is "the frontier under
//! this root is empty", read straight off the database — so it survives a
//! relaunch, needs no in-session bookkeeping, and can't drift from what was
//! actually covered. Ground no walk could read doesn't hold it open: a walk
//! records a directory it gave up on as [`UnreadableCause::Abandoned`], which
//! takes it out of the frontier and into a list of its own, and a persisted
//! per-volume backoff offers it again later.
//!
//! ❌ Don't replace this with "the frontier didn't shrink across two passes". That
//! rule has to compare SETS rather than counts (a pass can legitimately grow the
//! frontier by listing a root and exposing what is inside it), it never terminates
//! on a drive somebody is writing to, and being session-scoped it re-pays a full
//! walk on every launch.
//!
//! Two stamps, same rule, different root: [`HOME_COVERED_AT_KEY`] when home is
//! covered, `scan_completed_at` when the volume is. Each fires once, on the
//! absent→present transition. Re-running the volume's sequence on an already
//! complete index would push the 24-hour sweep window forward every launch, which
//! is the mirror of the bug the sweep ledger exists to prevent.

use std::sync::atomic::Ordering;

use rusqlite::Connection;

use super::Machine;
use crate::indexing::events::IndexEvent;
use crate::indexing::lifecycle::freshness::FreshnessEvent;
use crate::indexing::lifecycle::{lifecycle_bus, state};
use crate::indexing::paths::path_prefix::is_at_or_under;
use crate::indexing::reconcile::reconciler;
use crate::indexing::store::{IndexStore, ScanCalibrationKind};
use crate::indexing::watch::branches;
use crate::indexing::writer::WriteMessage;

/// When home stopped needing a walk, so photo search and folder importance can
/// start here rather than waiting for the rest of the drive.
///
/// It drives that and NOTHING else: not freshness, not the badge, not rescan
/// routing, not the sweep, not `scan_completed_at`. Keeping its blast radius to
/// one subscriber is what makes it cheap.
pub(crate) const HOME_COVERED_AT_KEY: &str = "home_covered_at";

/// Ask the database what is finished now. Called after every drain.
pub(super) fn take_stock(machine: &Machine) {
    if home_is_covered_now(machine) {
        stamp_home(machine);
    }
    if volume_is_covered_now(machine) {
        run_the_completion_sequence(machine);
    }
}

// ── Home, and the early signal ───────────────────────────────────────

/// Whether home has nothing left to walk except the one folder it deliberately
/// waits behind.
///
/// ⚠️ The exception is the whole point. `~/Library` is 27.7% of a real boot index
/// and 82% of home's coverage wall clock, so waiting for it would push the early
/// media kick from ~16 s to ~88 s — past the moment a full scan would have
/// finished the same work. It stays in scope and the phase still walks it; the
/// signal simply doesn't wait for it.
fn home_is_covered_now(machine: &Machine) -> bool {
    let Some(home) = machine.home_on_this_volume() else {
        return false;
    };
    if already_stamped(machine, HOME_COVERED_AT_KEY) {
        return false;
    }
    let deferred = machine
        .deferred_home_folder()
        .map(|path| path.to_string_lossy().into_owned());
    machine.frontier_under(&home).iter().all(|root| {
        deferred
            .as_deref()
            .is_some_and(|deferred| is_at_or_under(root, deferred))
    })
}

/// Stamp it and tell the one subscriber. Nothing else, deliberately.
fn stamp_home(machine: &Machine) {
    let _ = machine.writer.send(WriteMessage::UpdateMeta {
        key: HOME_COVERED_AT_KEY.to_string(),
        value: reconciler::now_unix().to_string(),
    });
    // Committed before the signal goes out: a scheduler that wakes on it reads
    // the marker back to decide whether the volume is admissible at all, and a
    // marker still in the writer's batch would read as absent.
    if let Err(e) = machine.writer.flush_blocking() {
        log::warn!("Phases: the home-coverage marker may not have landed: {e}");
    }
    log::info!("Phases: '{}' has covered home", machine.volume_id);
    lifecycle_bus::publish_home_covered(&machine.volume_id);
}

// ── The volume ───────────────────────────────────────────────────────

fn volume_is_covered_now(machine: &Machine) -> bool {
    !already_stamped(machine, "scan_completed_at") && machine.frontier_under(&machine.volume_root).is_empty()
}

/// Everything a completed volume owes, in the one order that works.
///
/// **The order is enforced by a FLUSH, not by the numbering.** Steps 1–6 are
/// writer MESSAGES; the collapse is in-process state. The read the whole ordering
/// protects (`local_rescan_reconciles` asking `get_index_status()` inside
/// `start_scan`) goes through a read connection, so it sees the stamp only once
/// the writer has committed it — and `PayLedgerIfUnpaid` runs a full
/// `ComputeAllAggregates` over a complete index, minutes of writer-thread work
/// sitting between the stamp being queued and being visible.
///
/// ⚠️ Collapse the branch set before the stamp is visible and there is a window
/// where the volume is neither branch-confined nor marked complete, and one
/// coalesced shallow anchor inside it truncates the index that just finished.
fn run_the_completion_sequence(machine: &Machine) {
    log::info!("Phases: '{}' is covered end to end", machine.volume_id);

    // 1. The completion marker, committed before anything long runs.
    let _ = machine.writer.send(WriteMessage::UpdateMeta {
        key: "scan_completed_at".to_string(),
        value: reconciler::now_unix().to_string(),
    });
    if let Err(e) = machine.writer.flush_blocking() {
        log::warn!("Phases: the completion marker may not have landed: {e}");
    }

    // 2. The calibration a later run's ETA is built from. Nothing else writes
    //    these, so without them the progress tier degrades permanently.
    write_the_calibration(machine);

    // 3. The one-shot `dir_stats` ledger heal. `ArmLedgerHealLatch` is disarmed by
    //    a full `ComputeAllAggregates`, and cover walks only ever send
    //    `ComputeSubtreeAggregates` — so without this the latch stays armed, re-arms
    //    every launch, and the heal never happens. A no-op when it isn't armed.
    let _ = machine.writer.send(WriteMessage::PayLedgerIfUnpaid);

    // 4. Anything the walks left without stats.
    let _ = machine.writer.send(WriteMessage::BackfillMissingDirStats);

    // 5. The shallow-sweep ledger. Without it the in-memory record stays `None`
    //    for the session (it is seeded from meta only at launch), so the very first
    //    shallow anchor after completion triggers a full sweep nobody asked for.
    let sweep = reconciler::record_sweep_completed(&machine.volume_id, reconciler::now_unix());
    if let Some(at) = sweep.last_sweep_unix {
        let _ = machine.writer.send(WriteMessage::UpdateMeta {
            key: reconciler::SHALLOW_SWEEP_AT_KEY.to_string(),
            value: at.to_string(),
        });
    }
    let _ = machine.writer.send(WriteMessage::UpdateMeta {
        key: reconciler::SHALLOW_COALESCED_KEY.to_string(),
        value: "0".to_string(),
    });

    // 6. The volume is Fresh, and the host hears the same three events in the same
    //    order a full scan fires them: the frontend's `resetAggregation()`
    //    handshake depends on `ScanComplete` arriving before the rest.
    let entries = entry_count(machine);
    machine.events.emit(IndexEvent::ScanComplete {
        volume_id: machine.volume_id.clone(),
        total_entries: entries,
        total_dirs: machine.progress.dirs_found.load(Ordering::Relaxed),
        duration_ms: machine.started_at.elapsed().as_millis() as u64,
    });
    machine.events.emit(IndexEvent::AggregationComplete {
        volume_id: machine.volume_id.clone(),
    });
    machine.events.emit(IndexEvent::DirsUpdated {
        paths: vec![machine.space.volume_root_string()],
    });
    state::apply_freshness_event_on(
        &machine.freshness,
        machine.events.as_ref(),
        &machine.volume_id,
        FreshnessEvent::ScanCompleted,
    );

    // The flush the ordering rests on: everything above is committed before the
    // collapse, including the full aggregate step 3 can start.
    if let Err(e) = machine.writer.flush_blocking() {
        log::warn!("Phases: the completion sequence may not have landed: {e}");
    }

    // 7. One branch covering the whole volume. Until now the set held one entry
    //    per frontier root, and every live event pays an O(branches) scan to find
    //    its own. It also restores the shallow sweep: a branch set that covers the
    //    volume root stops being "branch-confined", which is safe at exactly this
    //    moment and no earlier, because the stamp above is what makes a shallow
    //    anchor reconcile in place instead of truncating.
    branches::live_for(&machine.volume_id).collapse_to(
        &machine.space.volume_root_string(),
        &machine.space,
        &machine.writer,
    );
}

/// The totals a later run calibrates its ETA against.
///
/// Read from the DATABASE rather than this run's counters: a machine that resumed
/// a partially covered index walked only the remainder, and `total_entries` is
/// meant to describe the volume. The duration is this run's, which under-reports
/// for a resumed volume — the honest alternative would be to persist walk time
/// across sessions, and an ETA seed doesn't justify it.
fn write_the_calibration(machine: &Machine) {
    let entries = entry_count(machine);
    let bytes = machine.progress.bytes_scanned.load(Ordering::Relaxed);
    let duration = machine.started_at.elapsed().as_millis() as u64;
    for (key, value) in [
        ("scan_duration_ms", duration.to_string()),
        ("total_entries", entries.to_string()),
        ("total_physical_bytes", bytes.to_string()),
    ] {
        let _ = machine.writer.send(WriteMessage::UpdateMeta {
            // A phased first index runs the same walker a full scan does, so its
            // numbers belong in the same bucket.
            key: ScanCalibrationKind::FullWalk.meta_key(&key),
            value: value.clone(),
        });
        let _ = machine.writer.send(WriteMessage::UpdateMeta {
            key: key.to_string(),
            value,
        });
    }
}

// ── Reading the index ────────────────────────────────────────────────

fn already_stamped(machine: &Machine, key: &str) -> bool {
    with_conn(machine, |conn| IndexStore::get_meta(conn, key).ok().flatten().is_some()).unwrap_or(false)
}

fn entry_count(machine: &Machine) -> u64 {
    with_conn(machine, |conn| IndexStore::get_entry_count(conn).unwrap_or(0)).unwrap_or(0)
}

fn with_conn<T>(machine: &Machine, f: impl FnOnce(&Connection) -> T) -> Option<T> {
    IndexStore::open_read_connection(&machine.writer.db_path())
        .ok()
        .map(|conn| f(&conn))
}
