//! The per-volume failure supervisor: what happens when a writer reports that its
//! database died.
//!
//! `lifecycle/failure.rs` owns the signal itself; this is the task that waits on
//! it and the transition it runs, which tears the manager down and leaves the
//! instance registered as `Failed` so the badge can be honest about it.

use std::sync::Arc;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::{INDEX_REGISTRY, IndexPhase, TeardownClaim, apply_freshness_event};
use crate::indexing::lifecycle::failure::{IndexFailureSignal, IndexStop};
use crate::indexing::lifecycle::freshness::FreshnessEvent;
use crate::indexing::lifecycle::manager::IndexManager;
use crate::indexing::store::IndexFailure;

/// Spawn the per-volume failure supervisor: a task that awaits the writer's
/// `IndexFailureSignal` and, on the first fatal storage error, transitions the
/// volume to the `Failed` phase via [`fail_index`].
///
/// Spawned once, when the volume becomes `Running` in `start_indexing_for`. The
/// signal is one-shot and its `notified()` resolves even if the trip already
/// happened (a scan can fail in the Initializing→Running window), so a supervisor
/// spawned right after Running never misses an early failure.
pub(crate) fn spawn_failure_supervisor(
    events: Arc<dyn crate::EventSink>,
    volume_id: String,
    signal: Arc<IndexFailureSignal>,
) {
    crate::indexing::host::runtime::spawn(async move {
        signal.notified().await;
        match signal.stop() {
            // Out of disk space is the environment, not a dead database: pause the
            // volume and bring it back once there's room, ❌ never `Failed`, whose
            // recovery throws the index away.
            Some(IndexStop::OutOfSpace(why)) => {
                // `stop_indexing` blocks on the drain; keep it off the async worker.
                let _ = crate::indexing::host::runtime::spawn_blocking(move || {
                    super::space_pause::pause_for_space(&volume_id, why);
                })
                .await;
            }
            Some(IndexStop::Failed(reason)) => fail_index(events.as_ref(), &volume_id, reason),
            // The writer records the stop before notifying; only a poisoned lock
            // loses it, and failing is still the transition worth making.
            None => fail_index(
                events.as_ref(),
                &volume_id,
                IndexFailure {
                    code: 0,
                    extended_code: 0,
                },
            ),
        }
    });
}

/// Transition a volume to the `Failed` phase after its writer detected a fatal
/// storage error: tear the manager down, keep the instance registered as `Failed`
/// (so the badge is honest), uninstall the read-path handles (reads skip cleanly),
/// and fire the phase + freshness transitions ONCE.
///
/// Same drop-the-registry-guard-before-the-blocking-drain discipline as
/// `stop_indexing` / `force_scan`: take the manager out under the lock (publishing
/// a transient `ShuttingDown`), DROP the lock, run the blocking `shutdown()`, then
/// re-lock to install `Failed`. Holding the registry lock across `shutdown()` would
/// freeze every concurrent registry reader (the documented UI-freeze gotcha).
///
/// Three answers, by what the registry says:
///
/// - `Running`: fail it here.
/// - [`Detached`](IndexPhase::Detached), so a scan start has the manager out:
///   CLAIM the volume, and `finish_failing` runs as the manager comes back.
/// - anything else (a concurrent `stop_indexing` / `clear_index` already removed
///   or replaced it): a no-op. The trip just meant "stop", and stopping already
///   happened.
fn fail_index(events: &dyn crate::EventSink, volume_id: &str, reason: IndexFailure) {
    // Withdraw + invalidate the read-path handles BEFORE the phase flips to
    // `Failed`. A `Failed` instance stays registered so the badge can be honest,
    // but its DB is dead, so reads must skip: withdrawing the handles here is what
    // makes them skip, and it's why a `Failed` phase needs no read-path special
    // case anywhere.
    super::teardown::withdraw_from_the_read_path(volume_id);

    // Take the manager out under the lock (transient `ShuttingDown`), so the
    // blocking `shutdown()` drain runs WITHOUT holding the registry lock.
    let owned_mgr = {
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        let Some(instance) = reg.get_mut(volume_id) else {
            return;
        };
        // ⚠️ **The data-safety case.** A scan start has the manager out right now,
        // and the failure signal is ONE-SHOT: bouncing off the transient phase left
        // the volume restored as `Running` over a dead writer, badge normal, every
        // write dropped for the rest of the session, with nothing to retry. So the
        // trip is RECORDED and the `Failed` transition runs as the manager comes
        // back.
        if instance.phase.claim_the_teardown(TeardownClaim::Failed(reason)) {
            log::warn!(
                "fail_index('{volume_id}'): a fatal storage error landed during a scan start; failing the volume as its manager comes back"
            );
            return;
        }
        match std::mem::replace(
            &mut instance.phase,
            IndexPhase::ShuttingDown {
                restart: None,
                then_clear: None,
            },
        ) {
            IndexPhase::Running(mgr) => mgr,
            other => {
                // Not `Running` (already stopped, cleared, or failed elsewhere):
                // restore and bail. The writer already exited; nothing to do.
                instance.phase = other;
                log::debug!("fail_index('{volume_id}'): not Running, skipping the Failed transition");
                return;
            }
        }
    };

    finish_failing(events, volume_id, owned_mgr, reason);
}

/// The `Failed` transition the supervisor runs, driven directly so a test can
/// place it in a chosen window rather than racing a real writer death.
#[cfg(test)]
pub(crate) fn fail_index_for_test(events: &dyn crate::EventSink, volume_id: &str, reason: IndexFailure) {
    fail_index(events, volume_id, reason);
}

/// Drain a failed volume's manager and register it as `Failed`. The half of
/// `fail_index` that runs OFF the lock, shared with the deferred path so a trip
/// that landed mid-scan-start ends the volume in exactly the same place as one
/// that didn't.
pub(super) fn finish_failing(
    events: &dyn crate::EventSink,
    volume_id: &str,
    mgr: Box<IndexManager>,
    reason: IndexFailure,
) {
    super::teardown::withdraw_from_the_read_path(volume_id);

    // Guard released: blocking drain. The writer thread already exited on the trip,
    // so `writer.shutdown()`'s join returns fast; this mainly stops the watcher and
    // drains the live event loop's final batch.
    let mut mgr = mgr;
    let db_path = mgr.db_path().to_path_buf();
    mgr.shutdown();

    // Re-lock and install `Failed` — but only if the instance is still the
    // `ShuttingDown` marker we published (a concurrent stop/clear may have removed
    // it while we drained; respect that).
    let (restart, then_clear) = {
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        match reg.get_mut(volume_id) {
            Some(instance) if matches!(instance.phase, IndexPhase::ShuttingDown { .. }) => {
                match std::mem::replace(&mut instance.phase, IndexPhase::Failed { reason, db_path }) {
                    IndexPhase::ShuttingDown { restart, then_clear } => (restart, then_clear),
                    _ => (None, None),
                }
            }
            _ => {
                log::info!("fail_index('{volume_id}'): instance changed during drain, not marking Failed");
                return;
            }
        }
    };

    // Fire the phase + freshness transitions through the canonical paths (never
    // raw), so the debug timeline, the per-volume phase event, and the badge all
    // learn the volume stopped. Freshness `Failed` is terminal, so a late
    // scan-completion handler can't downgrade it.
    crate::indexing::events::set_phase_for(
        events,
        volume_id,
        crate::indexing::events::ActivityPhase::Failed,
        &format!("fatal storage error (SQLite {}/{})", reason.code, reason.extended_code),
    );
    apply_freshness_event(volume_id, FreshnessEvent::StorageFailed);

    log::warn!(
        "Indexing stopped for '{volume_id}' after a fatal storage error (SQLite {}/{}); retry rebuilds the index",
        reason.code,
        reason.extended_code,
    );

    // A clear that met the drain completes now. `clear_index` meets the `Failed`
    // just installed, removes it, and deletes with the usual wait for the volume to
    // be let go, so the manager's share has to be gone first.
    if let Some(why) = then_clear {
        drop(mgr);
        if let Err(e) = super::teardown::clear_index(volume_id, why) {
            log::warn!("fail_index('{volume_id}'): the clear that landed while it failed finished with: {e}");
        }
    }

    // Somebody asked for this volume back while its database was dying. The start
    // does the documented recovery for a `Failed` index — clear the dead one out of
    // the way, then rebuild — so a person who flipped the switch on gets an answer
    // rather than a badge that never changes.
    if let Some(request) = restart {
        log::info!("fail_index('{volume_id}'): a start landed while it failed; rebuilding the index for it");
        if let Err(e) = request.start(volume_id) {
            log::warn!("fail_index('{volume_id}'): rebuilding after the failure didn't take: {e}");
        }
    }
}
