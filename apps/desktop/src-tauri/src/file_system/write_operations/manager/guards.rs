//! The two RAII guards every manager-spawned or manager-run op holds.
//!
//! Split out of `manager.rs` so that file stays the admission story; the guards are a
//! self-contained contract about what happens when a task ends, panic included. Same
//! module tree, so they still reach the manager's privates.

use super::manager;

/// RAII safety net held by each manager-spawned task. On `Drop` (including a
/// panic that the runtime catches), it frees the op's lane slots and cleans
/// the caches — but NEVER spawns (no admission pass), so a panicking op can't
/// re-enter the manager mid-unwind. The happy path disarms it by calling
/// `on_settled` first (which removes the record, making the Drop a no-op).
///
/// This subsumes the old `OperationStateGuard`'s cache-cleanup-on-panic role
/// for managed ops, and adds lane release. The op's `WriteSettledGuard` (the FE
/// `write-settled` event) is separate and still lives inside each op's body.
pub(crate) struct ManagedTaskGuard {
    operation_id: String,
    armed: bool,
}

impl ManagedTaskGuard {
    pub(crate) fn new(operation_id: impl Into<String>) -> Self {
        Self {
            operation_id: operation_id.into(),
            armed: true,
        }
    }

    /// Call on the happy path right BEFORE `on_settled` so the Drop doesn't
    /// re-run the (now redundant) cleanup. `on_settled` already removed the
    /// record, so even an armed Drop would be a no-op; disarming just makes
    /// that explicit and skips the lock.
    pub(crate) fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for ManagedTaskGuard {
    fn drop(&mut self) {
        if self.armed {
            log::warn!(target: "op_manager", "op={} task ended without on_settled (panic?); freeing lanes", self.operation_id);
            manager().free_and_remove(&self.operation_id);
        }
    }
}

/// RAII net for [`OperationManager::run_instant`]. On `Drop` (the command's
/// IPC-timeout dropping the `run_instant` future mid-`op.await`, or a panic in
/// the awaited op) it frees the op's record and unregisters its busy status via
/// `free_and_remove`, then re-emits `operations-changed` so the queue snapshot
/// drops the now-gone row too. The busy-set release is the load-bearing part:
/// without it the eject guard would stick ON forever for the op's volume.
/// Instant ops reserve no lanes, so unlike `ManagedTaskGuard` there's nothing to
/// release there. The happy path disarms it after an explicit `free_and_remove`
/// + `emit_changed`, making the Drop a no-op.
pub(super) struct InstantTaskGuard {
    operation_id: String,
    armed: bool,
}

impl InstantTaskGuard {
    pub(super) fn new(operation_id: impl Into<String>) -> Self {
        Self {
            operation_id: operation_id.into(),
            armed: true,
        }
    }

    /// Call on the happy path right after the explicit `free_and_remove` so the
    /// Drop doesn't re-run the (now redundant) cleanup.
    pub(super) fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for InstantTaskGuard {
    fn drop(&mut self) {
        if self.armed {
            log::warn!(target: "op_manager", "instant op={} dropped/panicked before completion; freeing record + busy status", self.operation_id);
            let mgr = manager();
            mgr.free_and_remove(&self.operation_id);
            mgr.emit_changed();
        }
    }
}
