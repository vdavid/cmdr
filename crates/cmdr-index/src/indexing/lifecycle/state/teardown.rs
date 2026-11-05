//! Taking a volume's index down: stop, forget, reset, and the sweep over every
//! one of them.
//!
//! Every path here shares one ordering, and it is load-bearing: withdraw the read
//! handles FIRST (that IS the read-skip), publish `ShuttingDown` under the lock,
//! DROP the lock, run the blocking drain, then remove the instance.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::{
    INDEX_REGISTRY, IndexPhase, PersistDisable, Registry, StartRequest, TeardownClaim, all_registered_volume_ids,
    resolved_index_db_path, volume_kind,
};
use crate::indexing::hold;
use crate::indexing::lifecycle::manager::IndexManager;
use crate::indexing::read::enrichment::uninstall_read_pool;
use crate::indexing::read::pending_sizes::uninstall_pending_sizes;
use crate::indexing::reconcile::verifier;
use crate::indexing::store::IndexStore;
use crate::indexing::volume::IndexVolumeKind;
use crate::volume_files::{self, Removal, StoreDirs};

/// Take a volume off the read path and drop what a stopped index is no longer
/// owed. **The first thing every teardown does**, whatever ends it.
///
/// Withdrawing the `ReadPool` / `PendingSizes` and invalidating the pool BEFORE
/// anything touches the registry IS the read-skip: in-flight readers stop routing
/// here, so no reader can still open a connection to a database about to be
/// drained (or deleted). ❌ Don't move it below the drain.
///
/// The connections threads already cache to the volume's databases are retired
/// here too, the importance database's included: a stopped share would otherwise
/// keep a connection in every blocking thread that ever read it. Each thread
/// closes its own at its next read (`cmdr_fs::sqlite_util::retire_read_connections`
/// says why it can't be sooner).
///
/// Also called again by each `finish_*` on the deferred path, and it has to be:
/// a teardown that CLAIMED a detached volume runs this at request time, while the
/// scan start still holding the manager can register a rescan of its own before it
/// hands back. Every call is idempotent (a second withdraw finds nothing).
pub(super) fn withdraw_from_the_read_path(volume_id: &str) {
    if let Some(pool) = uninstall_read_pool(volume_id) {
        pool.invalidate();
        volume_files::retire_read_connections(&StoreDirs::around_index_db(pool.db_path()), volume_id);
    }
    drop_what_a_stopped_writer_is_owed(volume_id);
}

/// Everything [`withdraw_from_the_read_path`] does but the read pool: what a
/// volume whose writer stopped is no longer owed, whether or not its reads keep
/// serving. The space pause (`space_pause.rs`) calls this alone, since its
/// database stays readable.
///
/// The pending-sizes tracker goes because nothing will ever drain the queue that
/// would clear it: an hourglass left installed would stick for the whole pause.
pub(super) fn drop_what_a_stopped_writer_is_owed(volume_id: &str) {
    verifier::invalidate();
    uninstall_pending_sizes(volume_id);
    // The branch set goes with the instance that watched it. A cleared index
    // deletes the database, so the persisted copy goes too; a stopped one keeps
    // it, and the next start reads it back.
    crate::indexing::watch::branches::forget(volume_id);
    // And a volume that stopped indexing is owed no walk either, however it
    // stopped (the user, the master switch, the memory watchdog, a dead disk):
    // neither the rescan it remembered nor the coverage pass its machine stopped
    // short of.
    crate::indexing::lifecycle::cover::forget_rescan(volume_id);
    crate::indexing::lifecycle::completion_retry::forget(volume_id);
}

/// Carry out the teardown a volume's detached manager came back to.
///
/// The other half of [`IndexPhase::claim_the_teardown`](super::IndexPhase::claim_the_teardown):
/// the request landed while the manager was out, the caller was told it worked,
/// and this is where it actually happens. Each arm is the same `finish_*` the
/// direct path runs, so a claimed teardown and an immediate one cannot end the
/// volume in different places.
pub(super) fn finish_the_claimed_teardown(
    volume_id: &str,
    claim: TeardownClaim,
    mgr: Box<IndexManager>,
    events: &dyn crate::EventSink,
) {
    match claim {
        TeardownClaim::Failed(reason) => super::supervisor::finish_failing(events, volume_id, mgr, reason),
        TeardownClaim::Paused(why) => super::space_pause::finish_pausing(volume_id, mgr, why),
        TeardownClaim::Stopped(persist) => finish_stopping(volume_id, mgr, persist),
        TeardownClaim::Cleared(why) => {
            if let Err(e) = finish_clearing(volume_id, mgr, why) {
                log::warn!("Drive index clear for '{volume_id}' finished with: {e}");
            }
        }
    }
}

/// Stop all scans and watcher for a volume without deleting its DB.
///
/// Called when the user disables indexing via settings. The index stays on disk
/// but no scanning or watching runs. Directory sizes revert to `<dir>`.
pub fn stop_indexing(volume_id: &str) -> Result<(), String> {
    stop_the_volume(volume_id, PersistDisable::No)
}

/// What stopping a removable volume's index came to, for a host deciding whether
/// the volume is safe to unmount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovableStop {
    /// Nothing this stop is for: the volume isn't a local external drive, or no
    /// index is working on it. A share or a phone keeps its index across an
    /// unmount and tears it down through its own disconnect path.
    NothingToStop,
    /// Its index stopped, and every manager that was working on the volume has
    /// shut down and gone.
    Released,
    /// Its index was still letting go of the volume when the wait ran out: a
    /// drain, a scan start handing its manager back, or a start still standing
    /// its manager up. ❗ Don't unmount it: something may still be watching it.
    StillReleasing,
}

/// Stop a removable volume's index, and answer only once every manager working on
/// the volume has let it go, or once `wait_at_most` has run out.
///
/// ⚠️ **[`stop_the_volume`] returning is NOT the volume being let go**, which is
/// the whole reason this waits. It returns early in three windows, each for a good
/// reason: a stop CLAIMED on a `Detached` volume drains at the handback, a start
/// caught `Initializing` shuts its half-built manager down on its own thread, and a
/// drain somebody else started is still running. All three end with a
/// [`VolumeHold`](hold::VolumeHold) dropping, which is what this subscribes to.
///
/// `wait_at_most` counts from the call, the drain included, so a host's deadline
/// on the call and this wait end together. ❌ Don't move the wait into
/// [`stop_indexing`] or the user's disable: those record their request on a
/// transient phase and return, and nothing that calls them may start blocking.
///
/// ⚠️ **A generation whose drive has already left stops counting BEFORE the wait.**
/// The stop asks the host whether each generation's filesystem is still mounted
/// anywhere, by the mount identity its start captured, and one that reads
/// `Some(false)` is flagged vanished: nothing this stop does can let go of it, and a
/// worker stuck on the dead device must not hold up the drive's next life (a
/// re-plugged drive keeps its UUID, so its id). ❌ Never by root path: a rename moves
/// the root while the drive stays mounted, and its work still reads the drive.
/// Whatever is still held of a vanished generation once the wait ends turns zombie
/// with one `warn` (`../../hold.rs`).
pub(crate) fn stop_removable_volume(volume_id: &str, wait_at_most: Duration) -> RemovableStop {
    let started = Instant::now();
    let kind = volume_kind(volume_id);
    // A share or a phone keeps its index across an unmount.
    if kind.is_some_and(|kind| kind != IndexVolumeKind::LocalExternal) {
        return RemovableStop::NothingToStop;
    }
    let volumes = crate::indexing::host::volumes::current();
    let drive_left = hold::flag_vanished(volume_id, |identity| volumes.is_mounted(identity));
    let stopped = match kind {
        Some(_) => {
            if let Err(e) = stop_indexing(volume_id) {
                log::warn!("stopping the removable volume index '{volume_id}' failed: {e}");
            }
            wait_for_the_release(volume_id, wait_at_most, started)
        }
        // No instance, but a start that some other stop cancelled can still be
        // shutting its half-built manager down, and that start holds the volume.
        None if hold::is_held(volume_id) => wait_for_the_release(volume_id, wait_at_most, started),
        None => RemovableStop::NothingToStop,
    };
    hold::zombify_vanished(volume_id);
    if drive_left {
        mark_a_vanished_drive_for_a_rebuild(volume_id, stopped);
    }
    stopped
}

/// Persist the rebuild marker for a drive that went away while this volume's index
/// had deletes outstanding, so the next start rebuilds instead of trusting rows a
/// leaving drive may have taken.
///
/// ⚠️ **Only once the stop is done**, which is what makes the short-lived write
/// connection legal (`store/connection.rs`). A `StillReleasing` stop may have left a
/// writer thread alive, so it is skipped here: the gates' own in-session write
/// through that writer is what covers it (`indexing/deletes.rs`). A volume with
/// nothing outstanding writes nothing either way.
fn mark_a_vanished_drive_for_a_rebuild(volume_id: &str, stopped: RemovableStop) {
    if stopped == RemovableStop::StillReleasing {
        return;
    }
    match resolved_index_db_path(volume_id) {
        // No database, so there is no index to rebuild.
        Ok(db_path) if !db_path.exists() => {}
        Ok(db_path) => crate::indexing::deletes::note_the_drive_left_after_the_drain(volume_id, &db_path),
        Err(e) => log::debug!("'{volume_id}': can't resolve a db path to mark for a rebuild: {e}"),
    }
}

/// Wait out what's left of a removable stop's bound for the volume's live
/// generations to let go, naming the work that still holds it when they don't.
fn wait_for_the_release(volume_id: &str, wait_at_most: Duration, started: Instant) -> RemovableStop {
    match hold::wait_until_released(volume_id, wait_at_most.saturating_sub(started.elapsed())) {
        hold::Release::Released => RemovableStop::Released,
        hold::Release::StillHeld(holders) => {
            log::warn!(
                "'{volume_id}' was still being let go of when its removable stop ran out of time ({wait_at_most:?}); still held by {holders:?}"
            );
            RemovableStop::StillReleasing
        }
    }
}

/// What a stop still has to do once it knows what it found.
enum StopTarget {
    /// A live manager to drain.
    Drain(Box<IndexManager>),
    /// The instance is gone (or never had a writer); nothing to drain.
    NothingToDrain,
    /// A scan start has the manager out and took the request; it finishes the job,
    /// the sticky veto included.
    Claimed,
    /// A drain is running with a clear recorded on it, so the database is going and
    /// there is nothing to write a veto into.
    BeingCleared,
}

/// Stop a volume, optionally recording the user's sticky "keep this drive off"
/// veto once nothing is writing to its database any more.
///
/// ⚠️ **The veto rides the CLAIM on a detached volume**, ❌ never a write from
/// here. `set_drive_index_intent` opens its own short-lived write connection, and
/// a live writer thread on the same database is what that contract forbids — so a
/// stop that lands mid-scan-start hands `persist` to `finish_stopping`, which
/// writes it on the far side of the drain like every other path does.
fn stop_the_volume(volume_id: &str, persist: PersistDisable) -> Result<(), String> {
    withdraw_from_the_read_path(volume_id);

    // Take the instance out under the lock, publish `ShuttingDown`, then release
    // the lock BEFORE the blocking drain. `mgr.shutdown()` blocks up to 5 s
    // draining the live-event task; holding the registry lock across it would
    // stall every concurrent `get_status`/`is_active`/`trigger_verification`
    // caller (for ANY volume) and park a tokio worker. The live event loop reads
    // via `ReadPool` and never reacquires the registry lock, so dropping the
    // guard while `ShuttingDown` is published is safe: concurrent callers see
    // `ShuttingDown` and proceed.
    let target = {
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        let instance = match reg.get_mut(volume_id) {
            Some(i) => i,
            None => return Ok(()), // not indexed
        };
        // A scan start has the manager out right now. ⚠️ Recording the request is
        // the ONLY correct answer here: bouncing off the transient phase is what
        // made "turn indexing off for this drive" report success and keep indexing
        // for the rest of the session.
        if instance.phase.claim_the_teardown(TeardownClaim::Stopped(persist)) {
            log::info!("Indexing stop for '{volume_id}' lands as its scan start hands the manager back");
            StopTarget::Claimed
        } else {
            // Publishing a FRESH `ShuttingDown` here also drops whatever start a
            // drain already in flight was carrying, which is the point: the user's
            // last word is off.
            match std::mem::replace(
                &mut instance.phase,
                IndexPhase::ShuttingDown {
                    restart: None,
                    then_clear: None,
                    paused_for: None,
                },
            ) {
                IndexPhase::Running(mgr) => StopTarget::Drain(mgr),
                IndexPhase::Initializing { .. } => {
                    // Cancel this start's root stop signal FIRST: it stops the walks
                    // `resume_or_scan` already launched, and it's how that start
                    // learns at its re-lock that the slot it reserved is no longer
                    // its own. Without it, a fresh start that reserves the freed slot
                    // in the window gets its instance overwritten by the old start's
                    // manager — two writer threads on one database, which is exactly
                    // what the reservation exists to prevent.
                    instance.work.cancel.cancel();
                    // An in-flight start observes the removal and shuts its
                    // half-built manager down. Removing the whole instance is
                    // correct: it's disabled now.
                    reg.remove(volume_id);
                    log::info!("Indexing stopped during initialization for '{volume_id}'");
                    StopTarget::NothingToDrain
                }
                IndexPhase::Failed { .. } => {
                    // Its manager/writer are already torn down (nothing to drain).
                    // Disabling a failed index removes the instance, so the badge
                    // goes gray/disabled instead of staying red. The DB file stays
                    // on disk for a future re-enable (or a Forget to reclaim it).
                    reg.remove(volume_id);
                    log::info!("Indexing disabled for a failed volume '{volume_id}'");
                    StopTarget::NothingToDrain
                }
                IndexPhase::Paused { why, .. } => {
                    // Nothing is running (the pause drained it), and the read
                    // handles were withdrawn above: this IS the stop. The waiter
                    // sees the volume isn't paused any more and leaves it be.
                    reg.remove(volume_id);
                    log::info!("Indexing stopped for '{volume_id}' while it was paused for disk space ({why:?})");
                    StopTarget::NothingToDrain
                }
                IndexPhase::ShuttingDown { then_clear, .. } => {
                    // Another teardown is draining this volume and frees the slot
                    // when it's done. ❌ Don't put its phase back: the fresh marker
                    // is what drops the start it was carrying, and restoring the old
                    // one brought the drive back up over this stop. ⚠️ A clear it
                    // carries stays, though: dropping it is what left a forgotten
                    // drive's index on disk.
                    // Drops a pause's `paused_for` too: the stop outranks it.
                    instance.phase = IndexPhase::ShuttingDown {
                        restart: None,
                        then_clear,
                        paused_for: None,
                    };
                    if then_clear.is_some() {
                        StopTarget::BeingCleared
                    } else {
                        StopTarget::NothingToDrain
                    }
                }
                // Unreachable: `claim_the_teardown` above took it.
                IndexPhase::Detached { .. } => StopTarget::NothingToDrain,
            }
        }
    };

    match target {
        StopTarget::Drain(mgr) => finish_stopping(volume_id, mgr, persist),
        StopTarget::NothingToDrain => record_the_disable(volume_id, persist),
        StopTarget::Claimed | StopTarget::BeingCleared => {}
    }
    Ok(())
}

/// Drain a stopped volume's manager, take its instance out of the registry, and
/// record the veto if the stop was the user asking for one. The half of
/// [`stop_indexing`] that runs OFF the lock, shared with the deferred path so both
/// end the volume in exactly the same place.
pub(super) fn finish_stopping(volume_id: &str, mut mgr: Box<IndexManager>, persist: PersistDisable) {
    withdraw_from_the_read_path(volume_id);
    // Guard released: run the blocking drain without holding the registry lock.
    let db_path = mgr.db_path().to_path_buf();
    mgr.shutdown();
    // The manager carries a share of the volume's hold: dropped ahead of a recorded
    // clear's wait, or the clear would wait on itself.
    drop(mgr);
    retire_after_the_drain(volume_id, &db_path, persist);
}

/// The far side of a stop's drain, once the manager is shut down and dropped:
/// free the slot, then carry out the clear and the start that met the drain, or
/// record the veto. Shared with a space pause whose drain somebody else's stop
/// or clear landed in (`space_pause::finish_pausing`), so that pause ends the
/// volume exactly where the stop would have.
pub(super) fn retire_after_the_drain(volume_id: &str, db_path: &std::path::Path, persist: PersistDisable) {
    let Retired { restart, then_clear } = retire_the_instance(volume_id);
    if let Some(why) = then_clear {
        // A forget met this drain. It deletes now, and a start that came after it
        // still runs (`finish_clearing` says why it runs even if the files stay).
        match remove_once_let_go(volume_id, db_path, why) {
            Ok(()) => log::info!("Indexing stopped for '{volume_id}' and its index cleared, as asked mid-stop"),
            Err(e) => log::warn!("Drive index clear for '{volume_id}' after its stop finished with: {e}"),
        }
        if let Some(request) = restart {
            start_again(volume_id, request);
        }
        return;
    }
    log::info!("Indexing stopped for '{volume_id}' (DB preserved on disk)");
    match restart {
        // ⚠️ The veto is SKIPPED, and that is not a tidy-up. `record_drive_index_enabled`
        // already wrote the enable when this start was asked for, so a veto written
        // here would be the last word on the drive's own database: the drive would
        // come back off at the next launch, having been turned on a moment ago.
        Some(request) => start_again(volume_id, request),
        None => record_the_disable(volume_id, persist),
    }
}

/// What a drain's far side still owes: the requests that met the drain.
#[derive(Default)]
struct Retired {
    restart: Option<StartRequest>,
    then_clear: Option<Removal>,
}

/// Take a drained volume's instance out of the registry and hand back the start
/// and the clear that landed while the drain ran, if any did.
///
/// ⚠️ **Freeing the slot and reading the requests are ONE step, and ❌ never a
/// bare `remove`.** A start or clear that met the drain was told the request was
/// recorded, and this is the only moment anything looks at it: the slot has to be
/// free before it can be reserved, and nothing else is watching for the moment it
/// becomes free.
fn retire_the_instance(volume_id: &str) -> Retired {
    INDEX_REGISTRY
        .lock_ignore_poison()
        .remove(volume_id)
        .map_or_else(Retired::default, |instance| match instance.phase {
            IndexPhase::ShuttingDown {
                restart, then_clear, ..
            } => Retired { restart, then_clear },
            _ => Retired::default(),
        })
}

/// Carry out a start that was recorded while the volume was being torn down.
///
/// Off the registry lock, on the thread that freed the slot. It goes back through
/// the full `start_indexing_for`, so the master switch and every transport gate get
/// their say again.
fn start_again(volume_id: &str, request: StartRequest) {
    log::info!("Indexing for '{volume_id}' was asked for again while it stopped; starting it now");
    if let Err(e) = request.start(volume_id) {
        log::warn!("Starting '{volume_id}' again after its teardown failed: {e}");
    }
}

/// Run `f` with the volume held in the `ShuttingDown` window a user's disable
/// opens, then finish that stop exactly as [`stop_indexing`] does.
///
/// The window is the blocking drain, up to five seconds wide (`mgr.shutdown()`
/// waits on the live-event task), and it is where a second toggle lands when
/// somebody flips a drive off and straight back on. A test can't get inside a
/// real one without racing it, so this opens the same one deliberately: the same
/// phase, the same `finish_stopping` on the far side, the same
/// [`PersistDisable::Yes`] the disable command carries.
#[cfg(test)]
pub(crate) fn while_stopping_for_test(volume_id: &str, f: impl FnOnce()) {
    withdraw_from_the_read_path(volume_id);
    let mgr = {
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        let instance = reg.get_mut(volume_id).expect("a registered volume to stop");
        match std::mem::replace(
            &mut instance.phase,
            IndexPhase::ShuttingDown {
                restart: None,
                then_clear: None,
                paused_for: None,
            },
        ) {
            IndexPhase::Running(mgr) => mgr,
            other => {
                instance.phase = other;
                panic!("'{volume_id}' has no running manager to stop");
            }
        }
    };
    f();
    finish_stopping(volume_id, mgr, PersistDisable::Yes);
}

/// Write the sticky veto, if this stop was the user asking for one.
///
/// ⚠️ Guarded on the DB existing, so a drive nobody ever indexed isn't given one
/// just to record that it stays off: absent markers already read as "off" for every
/// drive but the boot disk. Best-effort (a failure only means a future reconnect
/// might re-resume; logged).
fn record_the_disable(volume_id: &str, persist: PersistDisable) {
    if persist == PersistDisable::No {
        return;
    }
    if let Ok(db_path) = resolved_index_db_path(volume_id)
        && db_path.exists()
        && let Err(e) = IndexStore::set_drive_index_intent(&db_path, false)
    {
        log::warn!("disable_drive_index_persist_intent('{volume_id}'): recording the disable failed: {e}");
    }
}

/// Discard a volume's partial index and reset it to gray / not-indexed
/// (D-interrupted): an interrupted/disconnected network scan leaves data that's
/// worthless once the volume is gone, so we don't keep a half-snapshot live.
///
/// Removes the registry instance (so reads skip → gray), draining/shutting down
/// the writer first. The DB file stays on disk but carries no `scan_completed_at`
/// (the scan path cleared it at start), so a future enable does a clean fresh
/// scan. Equivalent to `stop_indexing` for this purpose, named for intent.
pub(crate) fn reset_to_not_indexed(volume_id: &str) {
    if let Err(e) = stop_indexing(volume_id) {
        log::warn!("reset_to_not_indexed('{volume_id}') failed: {e}");
    }
}

/// Turn indexing OFF for a drive at the user's explicit request: stop it (DB kept
/// on disk for a fast re-enable), then persist the sticky veto so nothing resumes
/// what the user turned off.
///
/// This is the ONLY door that writes the veto — deliberately NOT `stop_indexing`
/// itself, which also runs on eject, unmount, an interrupted network scan
/// (`reset_to_not_indexed`), and the memory watchdog; marking there would suppress
/// auto-resume after a transient teardown, not a real user disable. The write also
/// withdraws the enable marker `record_drive_index_enabled` left, so the two halves
/// of per-drive intent stay one fact. It travels as [`PersistDisable::Yes`] so it
/// lands AFTER the drain on every path, deferred stops included, and no writer
/// thread contends for the database.
pub fn disable_drive_index_persist_intent(volume_id: &str) -> Result<(), String> {
    stop_the_volume(volume_id, PersistDisable::Yes)
}

/// Remove a volume's instance from the registry and withdraw its read-path
/// handles from the tables in `read/handles.rs`. Used on start-up failure paths.
///
/// ⚠️ Freeing the slot and withdrawing the handles is ONE critical section, and
/// must stay one. Freeing the slot first and releasing the guard before the
/// withdrawal lets a competing `start_indexing_for` reserve the now-empty slot and
/// install FRESH handles in between — which this call then withdraws, leaving a
/// live, registered index that routes no read pool, so its listings show `<dir>`
/// until the next stop/start. ❌ Don't "tidy" the withdrawal out of the guard.
///
/// Holding the registry across it is safe by the leaf-lock property in
/// `read/handles.rs` (a hash removal, nothing called under the guard), and it's the
/// same registry → table nesting `try_reserve_initializing_phase` already uses, so
/// it adds no ordering hazard. `invalidate()` stays OUTSIDE: it's the one part no
/// successor can observe, and the guard has no reason to cover it.
///
/// The other teardown paths (`stop_indexing`, `clear_index`, `fail_index`) withdraw
/// BEFORE they touch the registry and are safe for the mirror-image reason: the key
/// still exists while they withdraw, so no competing start can reserve yet.
pub(super) fn remove_instance_and_handles(volume_id: &str) {
    remove_instance_and_handles_on(&INDEX_REGISTRY, volume_id);
}

/// [`remove_instance_and_handles`] against a registry passed in, so a test can
/// drive the removal through a poisoned lock of its own.
pub(super) fn remove_instance_and_handles_on(registry: &Registry, volume_id: &str) {
    let pool = {
        let mut reg = registry.lock_ignore_poison();
        reg.remove(volume_id);
        let pool = uninstall_read_pool(volume_id);
        uninstall_pending_sizes(volume_id);
        // The branch set goes with the instance that watched it. A cleared index
        // deletes the database, so the persisted copy goes too; a stopped one keeps
        // it, and the next start reads it back.
        crate::indexing::watch::branches::forget(volume_id);
        // And a volume that stopped indexing is owed no walk either, however it
        // stopped (the user, the master switch, the memory watchdog): neither the
        // rescan it remembered nor the coverage pass its machine stopped short of.
        crate::indexing::lifecycle::cover::forget_rescan(volume_id);
        crate::indexing::lifecycle::completion_retry::forget(volume_id);
        pool
    };
    if let Some(pool) = pool {
        pool.invalidate();
        volume_files::retire_read_connections(&StoreDirs::around_index_db(pool.db_path()), volume_id);
    }
}

/// Stop all scans, shut down the writer, delete the volume's files, and reset
/// state for a volume.
///
/// `why` decides WHICH files: a forgotten volume loses every store that scores or
/// describes its index, a rebuild loses the index database and the weights scored from it
/// (`volume_files::Removal`). ❌ Never unlink a volume's database anywhere else:
/// `volume_files::remove` is the one door, which is what keeps a store from being
/// left behind by a path written before it existed.
///
/// Call `start_indexing()` to create a fresh index afterward.
pub(crate) fn clear_index(volume_id: &str, why: Removal) -> Result<(), String> {
    withdraw_from_the_read_path(volume_id);

    // Take the instance out under the lock, publish `ShuttingDown`, then release
    // the lock BEFORE the blocking drain (same reasoning as `stop_indexing`: the
    // up-to-5 s `mgr.shutdown()` drain must not stall concurrent registry
    // readers or park a tokio worker). The live event loop reads via `ReadPool`
    // and never reacquires the registry lock, so dropping the guard while
    // `ShuttingDown` is published is safe.
    // Take ownership of whatever the instance carries. `Running` hands back the
    // manager (needs a blocking drain before the files go); `Initializing` /
    // `ShuttingDown` carry no live writer to drain but MUST still be removed so
    // the badge goes gray (not a dangling Stale) and the DB is reclaimed —
    // forgetting a re-enabled-but-still-scanning Stale index has to work. Either
    // way we resolve the DB path before dropping the guard.
    enum ClearTarget {
        Running { mgr: Box<IndexManager> },
        NoWriter { db_path: PathBuf },
    }
    let target = {
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        let instance = match reg.get_mut(volume_id) {
            Some(i) => i,
            None => {
                // No instance, but very possibly a database: one a search's walk
                // built and nothing re-registered after a restart, or one a drive
                // kept when the user turned its indexing off. Clearing has to
                // reclaim that disk — a no-op here is what made "clear" and
                // "forget this drive" silently do nothing for exactly the people
                // with an index nobody is maintaining. Nothing is running, so the
                // files can go straight away.
                drop(reg);
                let db_path = resolved_index_db_path(volume_id)?;
                remove_once_let_go(volume_id, &db_path, why)?;
                log::info!("Drive index cleared for '{volume_id}' (no live index; database deleted)");
                return Ok(());
            }
        };
        // A scan start has the manager out right now, so the clear is RECORDED and
        // run as the manager comes back, rather than reporting success and leaving
        // the database on disk.
        if instance.phase.claim_the_teardown(TeardownClaim::Cleared(why)) {
            log::info!("Drive index clear for '{volume_id}' lands as its scan start hands the manager back");
            return Ok(());
        }
        match std::mem::replace(
            &mut instance.phase,
            IndexPhase::ShuttingDown {
                restart: None,
                then_clear: None,
                paused_for: None,
            },
        ) {
            IndexPhase::Running(mgr) => ClearTarget::Running { mgr },
            IndexPhase::Initializing { store } => {
                // Same as the stop: cancel this start's stop signal so it can tell
                // the freed slot from a successor's reservation.
                instance.work.cancel.cancel();
                // No live writer thread to drain (still in resume_or_scan), but
                // an in-flight start may be mid-`resume_or_scan`: publishing
                // `ShuttingDown` makes it observe the change and shut its
                // half-built manager down (same contract as `stop_indexing`).
                let db_path = store.db_path().to_path_buf();
                reg.remove(volume_id);
                ClearTarget::NoWriter { db_path }
            }
            IndexPhase::Failed { db_path, .. } => {
                // The failed manager/writer are already torn down. This is the
                // recovery reclaim: remove the instance and delete the (dead) DB so
                // a fresh `start_indexing` rebuilds from scratch.
                reg.remove(volume_id);
                ClearTarget::NoWriter { db_path }
            }
            IndexPhase::Paused { db_path, .. } => {
                // Drained already; its read handles were withdrawn above, so
                // nothing reads the file the clear is about to delete.
                reg.remove(volume_id);
                ClearTarget::NoWriter { db_path }
            }
            IndexPhase::ShuttingDown { then_clear, .. } => {
                // Another teardown is already draining this volume, and that may be
                // a STOP, which keeps the database. So the clear is RECORDED on the
                // drain and whoever ends it deletes, with the same wait for the
                // volume to be let go; ❌ bailing here left a forgotten drive's index
                // on disk. The start that drain was carrying stays dropped:
                // forgetting a drive is the user's last word.
                instance.phase = IndexPhase::ShuttingDown {
                    restart: None,
                    then_clear: Some(then_clear.map_or(why, |recorded| recorded.wider(why))),
                    paused_for: None,
                };
                log::info!("Drive index clear for '{volume_id}' lands as the drain already running ends");
                return Ok(());
            }
            IndexPhase::Detached { .. } => {
                // Unreachable: `claim_the_teardown` above took it.
                return Ok(());
            }
        }
    };

    // Guard released: run the blocking drain (Running only) without the lock.
    match target {
        ClearTarget::Running { mgr } => finish_clearing(volume_id, mgr, why),
        ClearTarget::NoWriter { db_path } => {
            remove_once_let_go(volume_id, &db_path, why)?;
            log::info!("Drive index cleared for '{volume_id}' (DB deleted)");
            Ok(())
        }
    }
}

/// How long a clear waits for the volume's last worker to let go before it gives
/// up on deleting the database. The drain it follows is bounded at five seconds
/// too, so a clear blocks no longer than it already could.
const CLEAR_WAITS_FOR_RELEASE: Duration = Duration::from_secs(5);

/// Delete the files `why` takes for `volume_id`, but only once no worker holds the
/// volume any more (#459, #620). EVERY clear waits, the one claimed on a `Detached`
/// volume and carried out at the handback included: that path deleted at once, so
/// the old generation's phase machine could still open the database after the
/// start riding the claim built a new one.
///
/// ⚠️ **So the thread that runs a clear must hold no share of the volume**, or the
/// wait can only time out on itself (and keep the files). The manager's share is
/// dropped before the wait (`finish_clearing`), and no caller of the detached
/// window (`off_the_registry`: an IPC rescan, the spawned `run_owed_now` and
/// `perform_registry_rescan` tasks, the maintenance tick's phase resume) carries
/// a `VolumeWork`. ❌ Don't call `off_the_registry` from a worker that does.
///
/// ❗ **A worker that outlives its manager's drain may still open a connection**
/// (the phase machine reading its frontier, a cancelled walk finishing a
/// directory). SQLite shares a database's memory by INODE but opens its `-shm` by
/// NAME, and POSIX locks never conflict inside one process: a connection to the
/// unlinked database that opens its `-shm` after the rebuild created a new one at
/// the same path takes the new file for its own, truncates it as the first user it
/// believes it is, and the rebuild's writer dies on SIGBUS. Waiting on the hold,
/// which every drive-reading worker carries, is what closes that window.
///
/// Still held when the wait runs out ⇒ the files stay and this is an error: a
/// dead index left on disk is a rebuild later, a truncated `-shm` under a live
/// writer is a crash now.
fn remove_once_let_go(volume_id: &str, db_path: &std::path::Path, why: Removal) -> Result<(), String> {
    if let hold::Release::StillHeld(holders) = hold::wait_until_released(volume_id, CLEAR_WAITS_FOR_RELEASE) {
        return Err(format!(
            "'{volume_id}' is still held by {holders:?} after {CLEAR_WAITS_FOR_RELEASE:?}; keeping its database rather than deleting it under a live reader"
        ));
    }
    volume_files::remove(&StoreDirs::around_index_db(db_path), volume_id, why)
}

/// Drain a cleared volume's manager, take its instance out of the registry, and
/// delete the files `why` takes. The half of [`clear_index`] that runs OFF the
/// lock, shared with the deferred path so both end the volume in exactly the same
/// place.
///
/// ⚠️ The files go AFTER the drain, and that order carries the other stores too:
/// `mgr.shutdown()` fires the volume's stop signal, which is what stops an
/// importance pass, so by the time `volume_files::remove` asks the importance
/// writer to let go nothing is feeding it.
fn finish_clearing(volume_id: &str, mut mgr: Box<IndexManager>, why: Removal) -> Result<(), String> {
    withdraw_from_the_read_path(volume_id);
    let db_path = mgr.db_path().to_path_buf();
    mgr.shutdown();
    // The manager carries a share of the volume's hold: dropped here, ahead of the
    // wait, or the clear would wait on itself.
    drop(mgr);
    let Retired { restart, then_clear } = retire_the_instance(volume_id);
    // A second clear that met this drain may take more (a forget over a rebuild).
    let why = then_clear.map_or(why, |recorded| recorded.wider(why));
    let deleted = remove_once_let_go(volume_id, &db_path, why);
    if deleted.is_ok() {
        log::info!("Drive index cleared for '{volume_id}' (DB deleted)");
    }
    // A start that landed mid-clear runs even if the files wouldn't go: it asked for
    // this drive to be indexed, and a locked database file is not an answer to that.
    if let Some(request) = restart {
        start_again(volume_id, request);
    }
    deleted
}

/// Clear EVERY volume's index: the ones running right now and the databases on
/// disk nothing has registered.
///
/// The two halves are both needed and neither implies the other. The registry
/// knows the boot disk mid-scan; the data dir knows the share whose index a
/// search walked into existence three launches ago, which is precisely the disk
/// use nobody could see before. Each volume goes through [`clear_index`], so a
/// live one still drains its writer and withdraws its read handles first.
///
/// The data dir's half is read from every store a forgotten volume loses, so it
/// also reclaims an importance database whose index is already gone.
///
/// Reports the first failure but always finishes the sweep: leaving half the
/// databases behind because one file was locked would be a worse answer than
/// reclaiming what it could.
pub fn clear_every_index() -> Result<(), String> {
    let mut volume_ids = all_registered_volume_ids();
    for volume_id in crate::indexing::resources::retention::volume_ids_on_disk() {
        if !volume_ids.contains(&volume_id) {
            volume_ids.push(volume_id);
        }
    }
    let mut first_error = None;
    for volume_id in &volume_ids {
        if let Err(e) = clear_index(volume_id, Removal::Forgotten) {
            log::warn!("clear_every_index: clearing '{volume_id}' failed: {e}");
            first_error.get_or_insert(e);
        }
    }
    log::info!("Every drive index cleared ({} volume(s))", volume_ids.len());
    first_error.map_or(Ok(()), Err)
}

/// Stop indexing for every registered volume. Each `stop_indexing` drains and
/// removes one instance; we snapshot the ids first so we're not iterating the map
/// while `stop_indexing` mutates it.
///
/// Two callers: the memory watchdog (the global memory-budget action) and the
/// master switch going off. Neither writes the sticky `user_disabled` marker, so
/// per-drive intent survives and the master switch can restore it (see
/// `master::drives_to_resume`).
pub(crate) fn stop_all_indexing() {
    // Tell shared-resident-pool subsystems (media_index enrichment, the importance
    // scheduler) to yield to the SAME 16 GB ceiling, rather than a second independent
    // budget over one pool. FIRST, because the hooks only fire signals and return,
    // while each volume stop below drains for up to seconds: run last, a subsystem
    // working on the fifth volume would keep allocating through four drains.
    crate::indexing::resources::subsystem_stop::run_subsystem_stop_hooks();
    for volume_id in all_registered_volume_ids() {
        if let Err(e) = stop_indexing(&volume_id) {
            log::warn!("stop_all_indexing: stop_indexing('{volume_id}') failed: {e}");
        }
    }
}
