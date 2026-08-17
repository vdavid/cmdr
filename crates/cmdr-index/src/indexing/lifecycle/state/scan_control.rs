//! Driving the walks a registered volume runs: force a rescan, stop the one in
//! flight, and kick off per-navigation verification.
//!
//! All three reach into a `Running` manager, and the two that can block share the
//! teardown paths' discipline: take the manager OUT under the lock, drop the
//! guard, then do the slow part.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::{INDEX_REGISTRY, IndexInstance, IndexPhase};
use crate::indexing::lifecycle::cover;
use crate::indexing::lifecycle::manager::{IndexManager, PhaseResume};
use crate::indexing::lifecycle::rescan_request::{RescanOutcome, ScanStartError};
use crate::indexing::reconcile::verifier;

/// Flip a Running volume's "a full scan is in flight" flag, so a test can pin
/// what a walk does against one without racing a real scan into place.
#[cfg(test)]
pub(crate) fn set_scanning_for_test(volume_id: &str, scanning: bool) {
    use cmdr_fs::ignore_poison::IgnorePoison;
    let reg = INDEX_REGISTRY.lock_ignore_poison();
    match reg.get(volume_id).map(|i| &i.phase) {
        Some(IndexPhase::Running(mgr)) => mgr.scanning.store(scanning, Ordering::Relaxed),
        _ => panic!("'{volume_id}' has no running manager to mark"),
    }
}

/// Ask a volume for the rescan its KIND routes to, with the phase machine forced
/// to owe this volume work, and report what the scan entry decided.
///
/// The only way to reach a trait scanner's phase guard: `first_index_is_the_machines`
/// requires `uses_local_scanner`, so no `IndexVolumeKind` is both trait-scanned and
/// phase-covered and no public path produces this shape. That is exactly what makes
/// the coupling latent, and why it is worth pinning.
///
/// Takes the manager out the way [`force_scan`] does, minus the restore-side
/// `start_pending_phases`: what this pins is the refusal, not what a machine would
/// then go and do with a share.
#[cfg(test)]
pub(crate) fn rescan_with_phases_owed_for_test(volume_id: &str) -> Result<(), ScanStartError> {
    use crate::indexing::lifecycle::manager::PendingPhases;
    use cmdr_fs::ignore_poison::IgnorePoison;

    let mut held = {
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        let instance = reg.get_mut(volume_id).expect("a registered volume to rescan");
        match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
            IndexPhase::Running(mgr) => mgr,
            other => {
                instance.phase = other;
                panic!("'{volume_id}' has no running manager to rescan");
            }
        }
    };
    held.pending_phases = PendingPhases::Owed;
    let result = held.force_rescan("phases-owed test");
    held.pending_phases = PendingPhases::No;

    let mut reg = INDEX_REGISTRY.lock_ignore_poison();
    let instance = reg.get_mut(volume_id).expect("the volume to still be registered");
    instance.phase = IndexPhase::Running(held);
    result
}

/// Run `f` with the volume's manager held OUT of the registry under a published
/// `ShuttingDown`, which is exactly what [`force_scan`] and
/// `perform_registry_rescan` do for the whole of a scan start.
///
/// That window is where a long walk can end, so it's where anything a walk does
/// on its way out has to keep working.
#[cfg(test)]
pub(crate) fn while_shutting_down_for_test(volume_id: &str, f: impl FnOnce()) {
    use cmdr_fs::ignore_poison::IgnorePoison;
    let held = {
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        let instance = reg.get_mut(volume_id).expect("a registered volume to shut down");
        match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
            IndexPhase::Running(mgr) => mgr,
            other => {
                instance.phase = other;
                panic!("'{volume_id}' has no running manager to take out");
            }
        }
    };
    f();
    let mut reg = INDEX_REGISTRY.lock_ignore_poison();
    let instance = reg.get_mut(volume_id).expect("the volume to still be registered");
    instance.phase = IndexPhase::Running(held);
}

/// Trigger background verification of a directory against the volume's index DB.
/// Called after enrichment on each navigation. No-op if the volume's index is
/// not running. Fully fire-and-forget: the registry lock is acquired on a
/// spawned task, so it never blocks the caller (navigation thread).
pub fn trigger_verification(volume_id: &str, dir_path: &str) {
    let volume_id = volume_id.to_string();
    let dir_path = dir_path.to_string();
    crate::indexing::host::runtime::spawn(async move {
        let reg = match INDEX_REGISTRY.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if let Some(IndexInstance {
            phase: IndexPhase::Running(mgr),
            signals,
            ..
        }) = reg.get(&volume_id)
        {
            let writer = mgr.writer.clone();
            let events = Arc::clone(&mgr.events);
            // A phase walk suppresses the verifier exactly as a full scan does. The
            // durable half of that protection is the verifier's own `listed_epoch`
            // bail; this is the concurrency half, and it asks "is a walk reading the
            // disk right now" rather than "does the machine have work" — between
            // roots there is nothing to race.
            let scanning = mgr.scanning.load(Ordering::Relaxed) || mgr.phases_are_walking();
            // The volume's path space, taken off the SAME instance the writer came
            // from. The verifier reads `volume_id`'s index and writes `mgr.writer`,
            // so the two must name one volume: routing the read through root's pool
            // (or resolving a mount-absolute path against root's index) made this a
            // silent no-op on every SMB, MTP, and external volume.
            let space = mgr.path_space();
            // Hand the walk a child of THIS volume's stop signal, taken here where
            // we already hold the instance. The verifier feeds this volume's writer,
            // so tearing the volume down must stop the walk rather than let it write
            // into a draining writer — and a token resolved here can't come back
            // `None` (and silently never fire) the way a later lookup could, once the
            // volume is gone.
            let cancel = signals.cancel.child_token();
            drop(reg);
            verifier::maybe_verify(volume_id, dir_path, space, writer, events, scanning, cancel);
        }
    });
}

/// Force a fresh full scan for a volume: the manual entry point behind "Rescan
/// now" and behind an enable that finds a volume still awaiting its first scan.
///
/// This is where a refused request becomes a REMEMBERED one. Ground on a volume
/// is held for seconds to minutes, and the person who clicked the button can't see
/// when it lets go, so a refusal by either kind of holder is recorded in the claim
/// table and reported as one of the two deferred outcomes; the holder that blocked
/// it runs it on its way out. ❌ Nothing here decides the coast is clear: the
/// guards below are asked again at that moment, so a holder still on the volume
/// defers it once more instead of truncating under one.
///
/// A volume whose FIRST INDEX is still the phase machine's answers `Started` and
/// remembers nothing: the machine is walking the drive whole, in pieces, which is
/// the walk the caller asked for, and it composes with everything else on the
/// drive rather than blocking it. So there is nothing to wait for.
///
/// The other automatic rescan door, `manager::perform_registry_rescan`, remembers
/// nothing on purpose. Its triggers (a journal gap, a coalesced shallow anchor)
/// recur on their own, and nobody is watching a button for them.
///
/// Takes the `Running` manager OUT of the registry under the lock (publishing a
/// transient `ShuttingDown`), DROPS the guard, then runs `start_scan` — whose
/// prelude does blocking I/O (`block_in_place(flush_blocking)`, a space-info
/// query) — off the lock, and finally re-locks only to put the manager back as
/// `Running`. Same drop-the-guard-before-blocking discipline as
/// `stop_indexing`/`clear_index` (DETAILS § "Drop the registry guard before the
/// shutdown drain"): a blocking flush under the global registry lock would
/// freeze every concurrent registry user (the QA-observed UI freeze), on top of
/// the self-deadlock from the freshness firing (now fixed via the manager's own
/// freshness `Arc`). `start_scan`'s spawned tasks capture their own clones and
/// never re-resolve the manager in the registry, so it's safe to run detached.
pub fn force_scan(volume_id: &str) -> Result<RescanOutcome, String> {
    // The request is recorded BEFORE the attempt, and cleared by an attempt that
    // got somewhere. Recording it after a `GroundBeingWalked` refusal reads more
    // naturally and has a hole in it: the walk can end in the window between the
    // guard answering and the request landing, and its `run_if_owed` would carry
    // nothing out — leaving a promise waiting on a walk that already finished.
    //
    // `force_rescan` routes by the volume's TYPED kind: a `Local` volume runs the
    // guarded walker (`start_scan`), an SMB/MTP volume walks the `Volume` trait from
    // its share root (`start_volume_scan`). Calling `start_scan` unconditionally
    // here ran the local guarded walker over a network mount — walking nothing and
    // falsely marking the index complete — so a NAS "Rescan now" indexed zero
    // entries.
    let detached = off_the_registry(volume_id, |mgr| {
        cover::remember_rescan(volume_id);
        match mgr.force_rescan("manual start") {
            Ok(()) | Err(ScanStartError::AlreadyScanning) => {
                cover::forget_rescan(volume_id);
                Ok(RescanOutcome::Started)
            }
            Err(ScanStartError::GroundBeingWalked) => {
                log::info!("force_scan: '{volume_id}' is being walked; its scan runs when that walk ends");
                Ok(RescanOutcome::DeferredUntilSearchEnds)
            }
            Err(ScanStartError::GroundBeingRewritten) => {
                log::info!("force_scan: '{volume_id}' is being rebuilt; its scan runs when that run ends");
                Ok(RescanOutcome::DeferredUntilScanEnds)
            }
            Err(ScanStartError::Internal(diagnostic)) => {
                cover::forget_rescan(volume_id);
                Err(diagnostic)
            }
        }
    })?;
    match detached {
        Detached::Done(result) => result,
        Detached::TornDownWhileAway(result) => {
            log::info!("force_scan: '{volume_id}' was torn down during scan start; shutting down the manager");
            // Whatever we just promised, this volume stopped indexing while we
            // were detached; a request left behind would rescan a drive nobody
            // is indexing any more.
            cover::forget_rescan(volume_id);
            result
        }
    }
}

/// Ask a volume whose first index stopped short to carry on covering it, for the
/// retry ladder in `../completion_retry.rs`.
///
/// A sibling of [`force_scan`] rather than a call into it, and the difference is
/// the whole point: this can only ever restart the PHASES. `force_scan` on a
/// volume that completed in the meantime is a full truncating rescan, which is a
/// fine thing for a button and an unacceptable thing for a background timer.
///
/// Everything else is the same discipline: the manager comes out from under the
/// registry lock for the blocking start, and the machine is stood up on the far
/// side of the restore.
pub(in crate::indexing::lifecycle) fn resume_the_phases(volume_id: &str) -> PhaseResume {
    match off_the_registry(volume_id, IndexManager::cover_again) {
        // A volume that isn't running has no machine to resume; whatever the
        // retry was waiting for is gone.
        Err(e) => {
            log::debug!("Completion retry: '{volume_id}' has nothing to resume: {e}");
            PhaseResume::NothingToCover
        }
        Ok(Detached::Done(outcome)) => outcome,
        Ok(Detached::TornDownWhileAway(_)) => {
            log::info!("Completion retry: '{volume_id}' was torn down mid-retry; shutting down the manager");
            PhaseResume::NothingToCover
        }
    }
}

/// What became of the manager while `work` ran with it detached.
enum Detached<T> {
    /// `work` ran and the manager is back in the registry as `Running`.
    Done(T),
    /// `work` ran, but the volume was torn down while it was out, so the manager
    /// was shut down instead of resurrecting a removed volume.
    TornDownWhileAway(T),
}

/// Run `work` against a volume's manager with the manager held OUT of the
/// registry under a published `ShuttingDown`, then put it back and start whatever
/// phase machine `work` registered.
///
/// **The discipline every blocking scan start follows**, and the reason it isn't
/// written twice: `work` does blocking I/O (`block_in_place(flush_blocking)`, a
/// space-info query), and a blocking flush under the global registry lock freezes
/// every concurrent registry user for ANY volume (the QA-observed UI freeze) on
/// top of the self-deadlock a freshness event used to cause. Concurrent callers
/// see `ShuttingDown` and proceed. `start_scan`'s spawned tasks capture their own
/// clones and never re-resolve the manager, so running detached is safe.
///
/// ⚠️ **Holding the manager out is also the mutual exclusion** a caller asking
/// "does this volume already have a machine?" depends on: `start_pending_phases`
/// finds nothing to start while we are away, so an answer read in here can't go
/// stale before it is acted on.
fn off_the_registry<T>(volume_id: &str, work: impl FnOnce(&mut IndexManager) -> T) -> Result<Detached<T>, String> {
    let mut mgr = {
        let mut reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let instance = reg.get_mut(volume_id).ok_or("Indexing not initialized")?;
        match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
            IndexPhase::Running(mgr) => mgr,
            other => {
                // Not running (Initializing / ShuttingDown): nothing to work with.
                // Put the phase back and report not-initialized.
                instance.phase = other;
                return Err("Indexing not initialized".to_string());
            }
        }
    };

    let result = work(&mut mgr);

    // Re-lock to restore the manager as `Running`. If the instance vanished while
    // we were detached (a concurrent `stop_indexing`/`clear_index` swapped it
    // out), respect that and shut our now-orphaned manager down.
    let mut reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match reg.get_mut(volume_id) {
        Some(instance) if matches!(instance.phase, IndexPhase::ShuttingDown) => {
            instance.phase = IndexPhase::Running(mgr);
            drop(reg);
            // A volume with no completed scan had its PHASES restarted rather than
            // its index truncated, and the machine starts here — on the far side of
            // the registry restore, never inside the window above.
            super::startup::start_pending_phases(volume_id);
            Ok(Detached::Done(result))
        }
        _ => {
            drop(reg);
            mgr.shutdown();
            Ok(Detached::TornDownWhileAway(result))
        }
    }
}

/// Stop the active scan for a volume without shutting down the manager.
pub fn stop_scan(volume_id: &str) -> Result<(), String> {
    let mut reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match reg.get_mut(volume_id).map(|i| &mut i.phase) {
        Some(IndexPhase::Running(mgr)) => {
            mgr.stop_scan();
            Ok(())
        }
        _ => Err("Indexing not initialized".to_string()),
    }
}
