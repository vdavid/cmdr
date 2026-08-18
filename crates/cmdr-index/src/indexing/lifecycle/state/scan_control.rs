//! Driving the walks a registered volume runs: force a rescan, stop the one in
//! flight, and kick off per-navigation verification.
//!
//! All three reach into a `Running` manager, and the two that can block share the
//! teardown paths' discipline: take the manager OUT under the lock, drop the
//! guard, then do the slow part. [`off_the_registry`] is the one place that dance
//! is written, and [`DetachedManager`] is what makes the window it opens
//! impossible to leave behind.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::{INDEX_REGISTRY, IndexInstance, IndexPhase};
use crate::indexing::lifecycle::cover;
use crate::indexing::lifecycle::manager::{IndexManager, PhaseResume};
use crate::indexing::lifecycle::rescan_request::{RescanOutcome, ScanStartError};
use crate::indexing::reconcile::verifier;

/// Flip a Running volume's "a full scan is in flight" flag, so a test can pin
/// what a walk does against one without racing a real scan into place.
#[cfg(test)]
pub(crate) fn set_scanning_for_test(volume_id: &str, scanning: bool) {
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
/// Takes the manager out through the same guard [`force_scan`] uses; it clears the
/// owed start before handing the manager back, so nothing stands a machine up
/// afterwards. What this pins is the refusal, not what a machine would then go and
/// do with a share.
#[cfg(test)]
pub(crate) fn rescan_with_phases_owed_for_test(volume_id: &str) -> Result<(), ScanStartError> {
    use crate::indexing::lifecycle::manager::PendingPhases;

    let mut held = DetachedManager::take(volume_id, |_| {}).expect("a registered volume to rescan");
    held.manager().pending_phases = PendingPhases::Owed;
    let result = held.manager().force_rescan("phases-owed test");
    held.manager().pending_phases = PendingPhases::No;
    let _ = held.hand_back();
    result
}

/// Run `f` with the volume's manager held OUT of the registry under a published
/// [`IndexPhase::Detached`], which is exactly what [`force_scan`] and
/// `perform_registry_rescan` do for the whole of a scan start.
///
/// That window is where a long walk can end and where a teardown can land, so
/// it's where anything either of them does has to keep working. It goes through
/// the same guard production does, so a teardown that claims the volume in here
/// is carried out on the way back, exactly as it would be in the app.
#[cfg(test)]
pub(crate) fn while_detached_for_test(volume_id: &str, f: impl FnOnce()) {
    let held = DetachedManager::take(volume_id, |_| {}).expect("a registered volume to detach");
    f();
    let _ = held.hand_back();
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
/// transient `IndexPhase::Detached`), DROPS the guard, then runs `start_scan` — whose
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
    let detached = off_the_registry(
        volume_id,
        |_| {},
        |mgr| {
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
        },
    )?;
    match detached {
        Handover::Restored(result) => result,
        Handover::TornDownWhileAway(result) => {
            log::info!("force_scan: '{volume_id}' was torn down during scan start; the teardown ran instead");
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
    match off_the_registry(volume_id, |_| {}, IndexManager::cover_again) {
        // A volume that isn't running has no machine to resume; whatever the
        // retry was waiting for is gone.
        Err(e) => {
            log::debug!("Completion retry: '{volume_id}' has nothing to resume: {e}");
            PhaseResume::NothingToCover
        }
        Ok(Handover::Restored(outcome)) => outcome,
        Ok(Handover::TornDownWhileAway(_)) => {
            log::info!("Completion retry: '{volume_id}' was torn down mid-retry; the teardown ran instead");
            PhaseResume::NothingToCover
        }
    }
}

/// What became of the manager while `work` ran with it detached.
pub(in crate::indexing::lifecycle) enum Handover<T> {
    /// `work` ran and the manager is back in the registry as `Running`.
    Restored(T),
    /// `work` ran, but a teardown claimed the volume (or removed it outright)
    /// while the manager was out, so the manager went to that teardown instead of
    /// back into the registry. The teardown is finished by the time this returns.
    TornDownWhileAway(T),
}

/// A volume's manager, held OUT of the registry with [`IndexPhase::Detached`]
/// published in its place.
///
/// **Ending the window is the guard's job, ❌ never the caller's**, and that is
/// the whole point. A `?` or a panic anywhere between the extraction and the
/// restore would otherwise drop the manager on the floor and leave the volume
/// detached for the rest of the session — every scan entry, every teardown, and
/// every status query answering off a phase nothing will ever move again.
/// [`hand_back`](Self::hand_back) is the deliberate ending; `Drop` is the same
/// path minus the phase-machine start, for the unwind that never asked.
struct DetachedManager<'a> {
    volume_id: &'a str,
    /// `Some` until the window is resolved, so `Drop` can tell an unwind from a
    /// caller that already handed the manager back.
    mgr: Option<Box<IndexManager>>,
}

impl<'a> DetachedManager<'a> {
    /// Take a `Running` volume's manager out, publishing [`IndexPhase::Detached`]
    /// with its writer, and run `prepare` against it while the lock is still held.
    ///
    /// ⚠️ `prepare` is for the non-blocking teardown a rescan does to what it is
    /// about to replace (stopping the old watcher and live loop). ❌ Nothing that
    /// blocks or re-enters the registry may go in it; that is what `work` is for.
    fn take(volume_id: &'a str, prepare: impl FnOnce(&mut IndexManager)) -> Result<Self, String> {
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        let instance = reg.get_mut(volume_id).ok_or("Indexing not initialized")?;
        match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
            IndexPhase::Running(mut mgr) => {
                prepare(&mut mgr);
                instance.phase = IndexPhase::Detached {
                    writer: mgr.writer.clone(),
                    teardown: None,
                };
                Ok(Self {
                    volume_id,
                    mgr: Some(mgr),
                })
            }
            other => {
                // Not running (Initializing / ShuttingDown / Detached / Failed):
                // nothing to work with. Put the phase back and report
                // not-initialized.
                instance.phase = other;
                Err("Indexing not initialized".to_string())
            }
        }
    }

    /// The manager, for the work the window exists to run off the lock.
    fn manager(&mut self) -> &mut IndexManager {
        self.mgr.as_mut().expect("the manager is held until hand_back")
    }

    /// End the window: put the manager back as `Running` and start whatever phase
    /// machine the work registered, or hand it to the teardown that claimed the
    /// volume while it was away.
    fn hand_back(mut self) -> Handover<()> {
        let mgr = self.mgr.take().expect("the manager is held until hand_back");
        hand_the_manager_back(self.volume_id, mgr, StartPhases::Yes)
    }
}

impl Drop for DetachedManager<'_> {
    fn drop(&mut self) {
        let Some(mgr) = self.mgr.take() else {
            return;
        };
        // Only an unwind gets here. The volume is still usable, so put it back
        // rather than leaving it detached — but ❌ don't stand a phase machine up
        // on the way out of a panic; whatever registered it is the thing that just
        // failed.
        log::warn!(
            "'{}' unwound with its manager detached; putting it back",
            self.volume_id
        );
        let _ = hand_the_manager_back(self.volume_id, mgr, StartPhases::No);
    }
}

/// Whether a restored manager's registered phase machine should be stood up.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StartPhases {
    Yes,
    No,
}

/// Put a detached manager back where it belongs: the registry, or the teardown
/// that asked for this volume while it was out.
fn hand_the_manager_back(volume_id: &str, mgr: Box<IndexManager>, start_phases: StartPhases) -> Handover<()> {
    /// What the registry says should happen to the manager we are holding.
    enum Next {
        Restored,
        Claimed(super::TeardownClaim, Arc<dyn crate::EventSink>, Box<IndexManager>),
        Orphaned(Box<IndexManager>),
    }

    let next = {
        let mut mgr = Some(mgr);
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        match reg.get_mut(volume_id) {
            Some(instance) => {
                // Two steps rather than one nested match: the claim comes off the
                // phase, then the phase is replaced. Reading and writing it in one
                // expression is what a borrow checker rightly refuses.
                let claimed = match &mut instance.phase {
                    IndexPhase::Detached { teardown, .. } => Some(teardown.take()),
                    _ => None,
                };
                match claimed {
                    Some(None) => {
                        instance.phase = IndexPhase::Running(mgr.take().expect("the manager we are holding"));
                        Next::Restored
                    }
                    Some(Some(claim)) => {
                        // A real teardown owns this volume now. Publish the real
                        // `ShuttingDown` so a second one bails instead of racing
                        // us, and finish the job off the lock.
                        instance.phase = IndexPhase::ShuttingDown;
                        Next::Claimed(
                            claim,
                            Arc::clone(&instance.signals.events),
                            mgr.take().expect("the manager we are holding"),
                        )
                    }
                    // The phase is no longer ours: somebody replaced it wholesale.
                    // Nothing produces this today (a teardown claims rather than
                    // replaces), but resurrecting a volume out of a phase we don't
                    // recognize is not a guess worth making.
                    None => Next::Orphaned(mgr.take().expect("the manager we are holding")),
                }
            }
            // Same, for an instance that went away entirely.
            None => Next::Orphaned(mgr.take().expect("the manager we are holding")),
        }
    };

    match next {
        Next::Restored => {
            // A volume with no completed scan had its PHASES restarted rather than
            // its index truncated, and the machine starts here — on the far side of
            // the registry restore, never inside the window above.
            if start_phases == StartPhases::Yes {
                super::startup::start_pending_phases(volume_id);
            }
            Handover::Restored(())
        }
        Next::Claimed(claim, events, mgr) => {
            super::teardown::finish_the_claimed_teardown(volume_id, claim, mgr, events.as_ref());
            Handover::TornDownWhileAway(())
        }
        Next::Orphaned(mut mgr) => {
            log::info!("'{volume_id}' left the registry while its manager was detached; shutting the manager down");
            mgr.shutdown();
            Handover::TornDownWhileAway(())
        }
    }
}

/// Run `work` against a volume's manager with the manager held OUT of the
/// registry under a published [`IndexPhase::Detached`], then put it back and start
/// whatever phase machine `work` registered.
///
/// **The discipline every blocking scan start follows**, and the reason it isn't
/// written twice: `work` does blocking I/O (`block_in_place(flush_blocking)`, a
/// space-info query), and a blocking flush under the global registry lock freezes
/// every concurrent registry user for ANY volume (the QA-observed UI freeze) on
/// top of the self-deadlock a freshness event used to cause. Concurrent callers
/// see `Detached` and proceed. `start_scan`'s spawned tasks capture their own
/// clones and never re-resolve the manager, so running detached is safe.
///
/// ⚠️ **Holding the manager out is also the mutual exclusion** a caller asking
/// "does this volume already have a machine?" depends on: `start_pending_phases`
/// finds nothing to start while we are away, so an answer read in here can't go
/// stale before it is acted on.
///
/// ⚠️ **The window can't strand the volume**, and ❌ that isn't up to `work`: the
/// [`DetachedManager`] guard ends it on every path out, panic included.
pub(in crate::indexing::lifecycle) fn off_the_registry<T>(
    volume_id: &str,
    prepare: impl FnOnce(&mut IndexManager),
    work: impl FnOnce(&mut IndexManager) -> T,
) -> Result<Handover<T>, String> {
    let mut held = DetachedManager::take(volume_id, prepare)?;
    let result = work(held.manager());
    Ok(match held.hand_back() {
        Handover::Restored(()) => Handover::Restored(result),
        Handover::TornDownWhileAway(()) => Handover::TornDownWhileAway(result),
    })
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// Every place that takes a volume's manager out of the registry says what a
    /// teardown landing in that window should do about it.
    ///
    /// **What this pins.** One transient `ShuttingDown` used to serve both a real
    /// teardown and the millisecond a scan start holds the manager out, so
    /// `stop_indexing`, `clear_index`, and `fail_index` all read the window as
    /// "somebody is already tearing this volume down", reported success, and did
    /// nothing. The `fail_index` case left a volume registered as `Running` over a
    /// DEAD writer, badge normal, dropping every write for the rest of the session,
    /// with a one-shot signal that never fires twice.
    ///
    /// **Why a source scan.** The type system can't hold it: any new caller is free
    /// to write its own `mem::replace` over `instance.phase`, and the compiler has no
    /// opinion about what the phase it publishes means to anyone else. The extraction
    /// sites are few and deliberate, so pin the set and what each one promises.
    #[test]
    fn every_manager_extraction_says_what_a_teardown_in_the_window_does() {
        fn collect(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) {
            for entry in std::fs::read_dir(dir).expect("an indexing dir") {
                let path = entry.expect("dir entry").path();
                let name = path.file_name().expect("file name").to_string_lossy().to_string();
                let rel = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                if path.is_dir() {
                    if name != "tests" {
                        collect(&path, &rel, out);
                    }
                } else if path.extension().is_some_and(|e| e == "rs") && !name.ends_with("tests.rs") {
                    out.push((rel, path));
                }
            }
        }

        let indexing = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/indexing");
        let mut sources: Vec<(String, PathBuf)> = Vec::new();
        collect(&indexing, "", &mut sources);

        // Assembled rather than written out, so a marker can't match its own mention.
        let takes_the_manager_out = concat!("mem::replace(&mut ", "instance.phase,");
        let records_the_request = concat!("claim_the_", "teardown(");
        let publishes_the_window = concat!("IndexPhase::Detached ", "{\n");

        let mut extractors: Vec<String> = Vec::new();
        for (name, path) in sources {
            let src = std::fs::read_to_string(&path).expect("read source");
            if !src.contains(takes_the_manager_out) {
                continue;
            }
            assert!(
                src.contains(records_the_request) || src.contains(publishes_the_window),
                // allowed-pluralize-noun: `{name}` is a file name, and `takes` is its verb.
                "{name} takes a volume's manager out of the registry without either publishing the \
                 detached window or recording what a teardown that lands in it asked for, so that \
                 request would be reported as done and then lost"
            );
            extractors.push(name);
        }
        extractors.sort();
        assert_eq!(
            extractors,
            vec![
                "lifecycle/state/scan_control.rs".to_string(),
                "lifecycle/state/supervisor.rs".to_string(),
                "lifecycle/state/teardown.rs".to_string(),
            ],
            "a new site takes a volume's manager out of the registry. If it's a teardown, claim the \
             detached window instead of bouncing off it; if it's blocking work on a live volume, go \
             through `state::off_the_registry` rather than writing the dance again, then update this list"
        );
    }
}
