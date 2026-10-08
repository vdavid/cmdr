//! Following a drive whose mount point moved while it stayed mounted: the user
//! renamed it, so `/Volumes/Old` is `/Volumes/New` now and nothing else changed.
//!
//! The answer is a RESTART at the new root, through the same recorded-start door a
//! toggle inside the drain window uses (`IndexPhase::ShuttingDown { restart }`).
//! Rows are mount-relative, so the database carries over untouched, and the start
//! rebuilds every piece that captured the old root (the walker, the watcher, the
//! live loop and its path space, the phase machine) from one value. Moving the
//! root inside a running manager would mean finding each of those, and the one
//! that's missed keeps reading a path that no longer exists.
//!
//! What the restart costs is what a drive's start always costs: a completed index
//! reconciles in place (no truncation, sizes stay visible) and a partial one
//! resumes its phases. That walk is also what makes the restart honest: the old
//! watcher stopped hearing the drive the moment its path moved, and an external
//! drive has no journal to replay the gap from.

use std::path::PathBuf;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::teardown::finish_stopping;
use super::{INDEX_REGISTRY, IndexPhase, PersistDisable, StartRequest, TeardownClaim};
use crate::indexing::lifecycle::manager::IndexManager;

/// What following the move still has to do once the registry lock is dropped.
enum Next {
    /// The volume isn't indexing, already sits at the new root, or is on its way
    /// out with nothing to come back to.
    Nothing,
    /// A running manager to drain. Its phase now carries the restart at the new
    /// root, which the drain carries out as it frees the slot.
    Drain(Box<IndexManager>),
    /// A start caught half way, already taken out of the registry: start it again,
    /// at the new root.
    StartAgain(StartRequest),
    /// A transient phase recorded the restart, and whoever ends that window
    /// carries it out.
    Recorded,
}

/// Restart `volume_id`'s index at `new_root`, where its drive is mounted now.
/// Answers whether the index follows it there.
///
/// Only a local-scanner volume (`IndexVolumeKind::uses_local_scanner`) moves with
/// its mount point: its walker and its watcher read the mount path itself. A share
/// or a phone reads through the host's `Volume`, which the host re-roots on its
/// own.
///
/// ⚠️ **Never through a stop and then a start.** A stop that frees the slot lets a
/// user's disable land in the gap and find nothing to veto, and the start after it
/// would then bring a drive back that the user just turned off. Recording the
/// restart in the same critical section that takes the volume down is what lets a
/// later teardown drop it, exactly like a toggle's.
///
/// **Blocking**: draining a running manager takes up to five seconds.
pub(crate) fn follow_the_move(volume_id: &str, new_root: PathBuf) -> bool {
    let next = {
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        let Some(instance) = reg.get_mut(volume_id) else {
            return false;
        };
        if !instance.kind().uses_local_scanner() || instance.started_as.volume_root() == new_root {
            return false;
        }
        let moved = instance.started_as.clone().moved_to(new_root.clone());
        match std::mem::replace(
            &mut instance.phase,
            IndexPhase::ShuttingDown {
                restart: Some(moved.clone()),
            },
        ) {
            IndexPhase::Running(mgr) => Next::Drain(mgr),
            IndexPhase::Initializing { .. } => {
                // The same arm a stop takes: cancelling this start's stop signal is
                // how it learns at its re-lock that the slot isn't its own any more,
                // and it shuts its half-built manager down itself.
                instance.work.cancel.cancel();
                reg.remove(volume_id);
                Next::StartAgain(moved)
            }
            mut transient @ (IndexPhase::Detached { .. } | IndexPhase::ShuttingDown { .. }) => {
                let next = record_the_move(&mut transient, moved);
                instance.phase = transient;
                next
            }
            failed @ IndexPhase::Failed { .. } => {
                // A failed index has nothing running to move, and the start that
                // rebuilds it asks the host for the root afresh.
                instance.phase = failed;
                Next::Nothing
            }
        }
    };

    match next {
        Next::Nothing => false,
        Next::Drain(mgr) => {
            log::info!(
                "'{volume_id}' moved to {}; restarting its index there",
                new_root.display()
            );
            finish_stopping(volume_id, mgr, PersistDisable::No);
            true
        }
        Next::StartAgain(request) => {
            log::info!(
                "'{volume_id}' moved to {} while its index was starting; starting it there",
                new_root.display()
            );
            if let Err(e) = request.start(volume_id) {
                log::warn!("Starting '{volume_id}' at its new root failed: {e}");
            }
            true
        }
        Next::Recorded => {
            log::info!(
                "'{volume_id}' moved to {} mid-transition; it starts there once that ends",
                new_root.display()
            );
            true
        }
    }
}

/// Point whatever start a transient phase carries at `new_root`, or claim a stop
/// with a restart on a detached volume nothing was tearing down.
///
/// A phase with a teardown and no restart is the user's last word being "off", so
/// it stays off: a move is no reason to bring a drive back.
fn record_the_move(phase: &mut IndexPhase, moved: StartRequest) -> Next {
    let new_root = moved.volume_root().to_path_buf();
    let retarget = |restart: &mut Option<StartRequest>| match restart.take() {
        Some(request) => {
            *restart = Some(request.moved_to(new_root));
            Next::Recorded
        }
        None => Next::Nothing,
    };
    match phase {
        IndexPhase::ShuttingDown { restart } => retarget(restart),
        IndexPhase::Detached {
            teardown: Some(claimed),
            ..
        } => retarget(&mut claimed.restart),
        IndexPhase::Detached { teardown: None, .. } => {
            // A scan start has the manager out and hands it back `Running`, still
            // at the old root. The claim makes that handback drain it instead, and
            // the restart riding the claim brings it up at the new one.
            phase.claim_the_teardown(TeardownClaim::Stopped(PersistDisable::No));
            phase.claim_the_restart(moved);
            Next::Recorded
        }
        IndexPhase::Running(_) | IndexPhase::Initializing { .. } | IndexPhase::Failed { .. } => Next::Nothing,
    }
}
