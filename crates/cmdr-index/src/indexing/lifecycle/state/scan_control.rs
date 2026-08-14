//! Driving the walks a registered volume runs: force a rescan, stop the one in
//! flight, and kick off per-navigation verification.
//!
//! All three reach into a `Running` manager, and the two that can block share the
//! teardown paths' discipline: take the manager OUT under the lock, drop the
//! guard, then do the slow part.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::{INDEX_REGISTRY, IndexInstance, IndexPhase};
use crate::indexing::lifecycle::rescan_request::{self, RescanOutcome, ScanStartError};
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
/// This is where a refused request becomes a REMEMBERED one. A cover walk holds
/// ground for seconds to minutes, and the person who clicked the button can't see
/// when it lets go, so a `GroundBeingWalked` refusal is recorded
/// (`rescan_request`) and reported as [`RescanOutcome::Deferred`]; the walk that
/// blocked it runs it on its way out. ❌ Nothing here decides the coast is clear:
/// the guard below is asked again at that moment, so a second walk still holding
/// ground defers it once more instead of truncating under one.
///
/// A scan that's ALREADY running answers `Started`, because it is: the caller
/// wanted a full walk on this volume and one is in flight.
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
    // Take the manager out under the lock (transient `ShuttingDown`), so the
    // blocking rescan prelude runs WITHOUT holding the registry lock.
    let mut mgr = {
        let mut reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let instance = reg.get_mut(volume_id).ok_or("Indexing not initialized")?;
        match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
            IndexPhase::Running(mgr) => mgr,
            other => {
                // Not running (Initializing / ShuttingDown): nothing to force.
                // Put the phase back and report not-initialized, as before.
                instance.phase = other;
                return Err("Indexing not initialized".to_string());
            }
        }
    };

    // Guard released: run the (blocking-prelude) scan start off the lock.
    // `force_rescan` routes by the volume's TYPED kind: a `Local` volume runs the
    // guarded walker (`start_scan`), an SMB/MTP volume walks the `Volume` trait from its share
    // root (`start_volume_scan`). Calling `start_scan` unconditionally here ran
    // the local guarded walker over a network mount — walking nothing and falsely
    // marking the index complete — so a NAS "Rescan now" indexed zero entries.
    //
    // The request is recorded BEFORE the attempt, and cleared by an attempt that
    // got somewhere. Recording it after a `GroundBeingWalked` refusal reads more
    // naturally and has a hole in it: the walk can end in the window between the
    // guard answering and the request landing, and its `run_if_owed` would carry
    // nothing out — leaving a promise waiting on a walk that already finished.
    rescan_request::remember(volume_id);
    let result = match mgr.force_rescan("manual start") {
        Ok(()) | Err(ScanStartError::AlreadyScanning) => {
            rescan_request::forget(volume_id);
            Ok(RescanOutcome::Started)
        }
        Err(ScanStartError::GroundBeingWalked) => {
            log::info!("force_scan: '{volume_id}' is being walked; its scan runs when that walk ends");
            Ok(RescanOutcome::Deferred)
        }
        Err(ScanStartError::Internal(diagnostic)) => {
            rescan_request::forget(volume_id);
            Err(diagnostic)
        }
    };

    // Re-lock to restore the manager as `Running`. If the instance vanished
    // while we were detached (a concurrent `stop_indexing`/`clear_index` swapped
    // it out), respect that and shut our now-orphaned manager down instead of
    // resurrecting a removed volume.
    let mut reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match reg.get_mut(volume_id) {
        Some(instance) if matches!(instance.phase, IndexPhase::ShuttingDown) => {
            instance.phase = IndexPhase::Running(mgr);
            result
        }
        _ => {
            drop(reg);
            log::info!("force_scan: '{volume_id}' was torn down during scan start; shutting down the manager");
            mgr.shutdown();
            // Whatever we just promised, this volume stopped indexing while we
            // were detached; a request left behind would rescan a drive nobody
            // is indexing any more.
            rescan_request::forget(volume_id);
            result
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
