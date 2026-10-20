//! The registry of every live operation's [`WriteOperationState`], keyed by
//! operation id: the lookup every by-id control goes through, and the walks
//! that touch all of them at once (cancel and abort at quit, the newest tick
//! for a window that opens late).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::{WriteOperationState, WriteProgressEvent};
use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_operation_log::types::OperationId;

/// The live [`WriteOperationState`] of every registered operation, keyed by
/// operation id.
///
/// A struct with methods rather than a bare `static` map, because
/// [`cancel_all`](Self::cancel_all) is a WALK: it touches every entry at once.
/// A walk is only honestly testable against a registry the caller OWNS — driving
/// the process-global one from a test cancels whatever operations OTHER tests
/// have in flight at that moment. DETAILS § "Test isolation".
pub(crate) struct WriteOperationRegistry {
    entries: RwLock<HashMap<OperationId, Arc<WriteOperationState>>>,
}

impl WriteOperationRegistry {
    pub(crate) fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Registers `state` under `operation_id`, so cancel / pause / conflict
    /// resolution can reach it by id.
    pub(crate) fn insert(&self, operation_id: OperationId, state: Arc<WriteOperationState>) {
        self.entries.write_ignore_poison().insert(operation_id, state);
    }

    /// The live state for `operation_id`, if it's still registered. Returns an
    /// `Arc` rather than a guard so callers don't hold the registry lock while
    /// they touch the operation.
    pub(crate) fn get(&self, operation_id: &OperationId) -> Option<Arc<WriteOperationState>> {
        Some(Arc::clone(self.entries.read_ignore_poison().get(operation_id)?))
    }

    /// Whether `operation_id` still has a state entry.
    ///
    /// ❌ Test-only, and it must stay that way: this is presence in the state
    /// map, ❌ never "is it running". A paused operation keeps its entry and a
    /// queued one has none yet, so answering a lifecycle question with this got
    /// both backwards. `manager().lifecycle_status()` is that answer; the tests
    /// here use this to pin the difference.
    #[cfg(test)]
    pub(crate) fn contains(&self, operation_id: &OperationId) -> bool {
        self.entries
            .read()
            .is_ok_and(|entries| entries.contains_key(operation_id))
    }

    /// Drops `operation_id`'s state, ending its liveness first.
    ///
    /// ❌ Every removal goes through here. Removing the entry alone would leave a
    /// wedged operation's staged temps hidden forever (a task the driver
    /// abandoned still holds an `Arc` to the state, so the map entry going away
    /// doesn't drop it): `WriteOperationState::end_liveness`.
    pub(crate) fn forget(&self, operation_id: &OperationId) {
        let removed = self.entries.write_ignore_poison().remove(operation_id);
        if let Some(state) = removed {
            state.end_liveness();
        }
    }

    /// Every registered operation, copied out, so a walk takes no operation's
    /// own locks (and writes no log line) under the registry's.
    fn snapshot(&self) -> Vec<(OperationId, Arc<WriteOperationState>)> {
        self.entries
            .read_ignore_poison()
            .iter()
            .map(|(id, state)| (id.clone(), Arc::clone(state)))
            .collect()
    }

    /// The newest progress tick of every registered operation that has emitted
    /// one, its wait classified now: the stored tick was measured while the
    /// operation was still moving.
    pub(crate) fn latest_progress(&self) -> Vec<WriteProgressEvent> {
        self.snapshot()
            .into_iter()
            .filter_map(|(id, state)| {
                let mut event = state.last_progress()?;
                event.activity = state.activity(id.as_str(), Some(&event));
                Some(event)
            })
            .collect()
    }

    /// Stops every registered operation, keeping partials.
    ///
    /// Transitions each to `Stopped` (never `RollingBack`: teardown must not
    /// silently delete files with no visual feedback), flips `backend_cancel` so
    /// in-flight backend I/O bails, drops any pending conflict sender to unblock
    /// its waiter, and wakes a paused op so it observes the cancel. Already-
    /// `Stopped` operations are left alone.
    pub(crate) fn cancel_all(&self) {
        for (id, state) in self.snapshot() {
            // Atomic, like every intent write: an already-`Stopped` op refuses
            // and is left alone.
            if state.stop_for_teardown().is_ok() {
                log::info!("cancel_all_write_operations: stopping op={id}");
                state.backend_cancel.cancel();
                // Drop the conflict resolution sender to unblock any waiting receiver
                state.conflict_slot.abandon();
                state.sources_gone.abandon();
                state.locked_sources.abandon();
                // Wake a paused, parked op so teardown's cancel is observed.
                state.pause_gate.wake();
            }
        }
    }

    /// Stops every registered operation and stops WAITING for the ones that
    /// don't answer. See
    /// [`Operations::abort_all_write_operations`](super::super::Operations::abort_all_write_operations).
    pub(crate) fn abort_all(&self) {
        // Tier 1 first, and unconditionally: an abort is a cancel that ran out
        // of patience, so an operation must never observe tier 2 without the
        // cooperative signal that gives its backends the chance to wind down
        // cleanly in the moments before the process goes.
        self.cancel_all();
        for (id, state) in self.snapshot() {
            if !state.backend_abort.is_cancelled() {
                log::info!("abort_all_write_operations: no longer waiting for op={id}");
                state.backend_abort.cancel();
            }
        }
    }
}

impl super::super::Operations {
    /// The newest progress tick of every live operation that has emitted one. See
    /// [`WriteOperationRegistry::latest_progress`].
    pub fn latest_operation_progress(&self) -> Vec<WriteProgressEvent> {
        self.registry().latest_progress()
    }
}
