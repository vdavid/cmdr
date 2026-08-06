//! The per-volume failure supervisor: what happens when a writer reports that its
//! database died.
//!
//! `lifecycle/failure.rs` owns the signal itself; this is the task that waits on
//! it and the transition it runs, which tears the manager down and leaves the
//! instance registered as `Failed` so the badge can be honest about it.

use std::sync::Arc;

use super::{INDEX_REGISTRY, IndexPhase, apply_freshness_event};
use crate::indexing::lifecycle::failure::IndexFailureSignal;
use crate::indexing::lifecycle::freshness::FreshnessEvent;
use crate::indexing::read::enrichment::uninstall_read_pool;
use crate::indexing::read::pending_sizes::uninstall_pending_sizes;
use crate::indexing::reconcile::verifier;
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
        // The writer records the reason before notifying; default only if a
        // poisoned lock lost it (the transition is still worth making).
        let reason = signal.reason().unwrap_or(IndexFailure {
            code: 0,
            extended_code: 0,
        });
        fail_index(events.as_ref(), &volume_id, reason);
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
/// A no-op if the volume isn't `Running` (a concurrent `stop_indexing` /
/// `clear_index` already removed or replaced it): the trip just meant "stop", and
/// stopping already happened.
fn fail_index(events: &dyn crate::EventSink, volume_id: &str, reason: IndexFailure) {
    verifier::invalidate();

    // Withdraw + invalidate the read-path handles BEFORE the phase flips to
    // `Failed`. A `Failed` instance stays registered so the badge can be honest,
    // but its DB is dead, so reads must skip: withdrawing the handles here is what
    // makes them skip, and it's why a `Failed` phase needs no read-path special
    // case anywhere.
    if let Some(pool) = uninstall_read_pool(volume_id) {
        pool.invalidate();
    }
    uninstall_pending_sizes(volume_id);
    // The branch set goes with the instance that watched it. A cleared index
    // deletes the database, so the persisted copy goes too; a stopped one keeps
    // it, and the next start reads it back.
    crate::indexing::watch::branches::forget(volume_id);

    // Take the manager out under the lock (transient `ShuttingDown`), so the
    // blocking `shutdown()` drain runs WITHOUT holding the registry lock.
    let owned_mgr = {
        let mut reg = match INDEX_REGISTRY.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("fail_index('{volume_id}'): registry lock poisoned: {e}");
                return;
            }
        };
        let Some(instance) = reg.get_mut(volume_id) else {
            return;
        };
        match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
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

    // Guard released: blocking drain. The writer thread already exited on the trip,
    // so `writer.shutdown()`'s join returns fast; this mainly stops the watcher and
    // drains the live event loop's final batch.
    let mut mgr = owned_mgr;
    let db_path = mgr.db_path().to_path_buf();
    mgr.shutdown();

    // Re-lock and install `Failed` — but only if the instance is still the
    // `ShuttingDown` marker we published (a concurrent stop/clear may have removed
    // it while we drained; respect that).
    {
        let mut reg = match INDEX_REGISTRY.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("fail_index('{volume_id}'): registry lock poisoned on re-lock: {e}");
                return;
            }
        };
        match reg.get_mut(volume_id) {
            Some(instance) if matches!(instance.phase, IndexPhase::ShuttingDown) => {
                instance.phase = IndexPhase::Failed { reason, db_path };
            }
            _ => {
                log::info!("fail_index('{volume_id}'): instance changed during drain, not marking Failed");
                return;
            }
        }
    }

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
}
