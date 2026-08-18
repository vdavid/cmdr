//! Taking a volume's index down: stop, forget, reset, and the sweep over every
//! one of them.
//!
//! Every path here shares one ordering, and it is load-bearing: withdraw the read
//! handles FIRST (that IS the read-skip), publish `ShuttingDown` under the lock,
//! DROP the lock, run the blocking drain, then remove the instance.

use std::path::{Path, PathBuf};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::{
    INDEX_REGISTRY, IndexPhase, PersistDisable, TeardownClaim, all_registered_volume_ids, resolved_index_db_path,
};
use crate::indexing::lifecycle::manager::IndexManager;
use crate::indexing::read::enrichment::uninstall_read_pool;
use crate::indexing::read::pending_sizes::uninstall_pending_sizes;
use crate::indexing::reconcile::verifier;
use crate::indexing::store::IndexStore;

/// Take a volume off the read path and drop what a stopped index is no longer
/// owed. **The first thing every teardown does**, whatever ends it.
///
/// Withdrawing the `ReadPool` / `PendingSizes` and invalidating the pool BEFORE
/// anything touches the registry IS the read-skip: in-flight readers stop routing
/// here and thread-local connections are discarded, so no reader can still open a
/// connection to a database about to be drained (or deleted). ❌ Don't move it
/// below the drain.
///
/// Also called again by each `finish_*` on the deferred path, and it has to be:
/// a teardown that CLAIMED a detached volume runs this at request time, while the
/// scan start still holding the manager can register a rescan of its own before it
/// hands back. Every call is idempotent (a second withdraw finds nothing).
pub(super) fn withdraw_from_the_read_path(volume_id: &str) {
    verifier::invalidate();
    if let Some(pool) = uninstall_read_pool(volume_id) {
        pool.invalidate();
    }
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
        TeardownClaim::Stopped(persist) => finish_stopping(volume_id, mgr, persist),
        TeardownClaim::Cleared => {
            if let Err(e) = finish_clearing(volume_id, mgr) {
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

/// What a stop still has to do once it knows what it found.
enum StopTarget {
    /// A live manager to drain.
    Drain(Box<IndexManager>),
    /// The instance is gone (or never had a writer); nothing to drain.
    NothingToDrain,
    /// A scan start has the manager out and took the request; it finishes the job,
    /// the sticky veto included.
    Claimed,
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
            match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
                IndexPhase::Running(mgr) => StopTarget::Drain(mgr),
                IndexPhase::Initializing { .. } => {
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
                other => {
                    instance.phase = other; // put it back, wasn't running
                    StopTarget::NothingToDrain
                }
            }
        }
    };

    match target {
        StopTarget::Drain(mgr) => finish_stopping(volume_id, mgr, persist),
        StopTarget::NothingToDrain => record_the_disable(volume_id, persist),
        StopTarget::Claimed => {}
    }
    Ok(())
}

/// Drain a stopped volume's manager, take its instance out of the registry, and
/// record the veto if the stop was the user asking for one. The half of
/// [`stop_indexing`] that runs OFF the lock, shared with the deferred path so both
/// end the volume in exactly the same place.
fn finish_stopping(volume_id: &str, mut mgr: Box<IndexManager>, persist: PersistDisable) {
    withdraw_from_the_read_path(volume_id);
    // Guard released: run the blocking drain without holding the registry lock.
    mgr.shutdown();
    // Re-lock only to remove the now-disabled instance.
    INDEX_REGISTRY.lock_ignore_poison().remove(volume_id);
    log::info!("Indexing stopped for '{volume_id}' (DB preserved on disk)");
    record_the_disable(volume_id, persist);
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
    let pool = {
        let mut reg = INDEX_REGISTRY.lock().expect("INDEX_REGISTRY lock poisoned");
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
    }
}

/// Stop all scans, shut down the writer, delete the DB file, and reset state
/// for a volume.
///
/// Call `start_indexing()` to create a fresh index afterward.
pub fn clear_index(volume_id: &str) -> Result<(), String> {
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
                delete_index_db_files(&db_path)?;
                log::info!("Drive index cleared for '{volume_id}' (no live index; database deleted)");
                return Ok(());
            }
        };
        // A scan start has the manager out right now, so the clear is RECORDED and
        // run as the manager comes back, rather than reporting success and leaving
        // the database on disk.
        if instance.phase.claim_the_teardown(TeardownClaim::Cleared) {
            log::info!("Drive index clear for '{volume_id}' lands as its scan start hands the manager back");
            return Ok(());
        }
        match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
            IndexPhase::Running(mgr) => ClearTarget::Running { mgr },
            IndexPhase::Initializing { store } => {
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
                // a fresh `start_indexing` rebuilds from scratch. The stored
                // `db_path` avoids re-resolving it off an `AppHandle`.
                reg.remove(volume_id);
                ClearTarget::NoWriter { db_path }
            }
            IndexPhase::ShuttingDown | IndexPhase::Detached { .. } => {
                // Another teardown is already draining this volume. It will
                // remove the instance and (for clear) delete the DB; don't race
                // a second delete. Put the marker back and bail. (`Detached` is
                // unreachable: `claim_the_teardown` above took it.)
                instance.phase = IndexPhase::ShuttingDown;
                log::info!("Drive index clear requested but '{volume_id}' is already shutting down");
                return Ok(());
            }
        }
    };

    // Guard released: run the blocking drain (Running only) without the lock.
    match target {
        ClearTarget::Running { mgr } => finish_clearing(volume_id, mgr),
        ClearTarget::NoWriter { db_path } => {
            delete_index_db_files(&db_path)?;
            log::info!("Drive index cleared for '{volume_id}' (DB deleted)");
            Ok(())
        }
    }
}

/// Drain a cleared volume's manager, take its instance out of the registry, and
/// delete the database. The half of [`clear_index`] that runs OFF the lock, shared
/// with the deferred path so both end the volume in exactly the same place.
fn finish_clearing(volume_id: &str, mut mgr: Box<IndexManager>) -> Result<(), String> {
    withdraw_from_the_read_path(volume_id);
    let db_path = mgr.db_path().to_path_buf();
    mgr.shutdown();
    // Re-lock only to remove the now-disabled instance.
    INDEX_REGISTRY.lock_ignore_poison().remove(volume_id);
    delete_index_db_files(&db_path)?;
    log::info!("Drive index cleared for '{volume_id}' (DB deleted)");
    Ok(())
}

/// Delete an index database and its WAL/SHM sidecars. Every caller has already
/// made sure nothing is reading or writing it.
fn delete_index_db_files(db_path: &Path) -> Result<(), String> {
    for path in [
        db_path.to_path_buf(),
        db_path.with_extension("db-wal"),
        db_path.with_extension("db-shm"),
    ] {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Failed to delete {}: {e}", path.display()))?;
        }
    }
    Ok(())
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
        if let Err(e) = clear_index(volume_id) {
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
    for volume_id in all_registered_volume_ids() {
        if let Err(e) = stop_indexing(&volume_id) {
            log::warn!("stop_all_indexing: stop_indexing('{volume_id}') failed: {e}");
        }
    }
    // Tell shared-resident-pool subsystems (media_index enrichment) to yield to the
    // SAME 16 GB ceiling, rather than a second independent budget over one pool.
    crate::indexing::resources::subsystem_stop::run_subsystem_stop_hooks();
}
