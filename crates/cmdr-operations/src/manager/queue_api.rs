//! What the queue window, the MCP `queue` tool, and the volume guards ask an
//! engine about its operations: list them, cancel, pause, resume, reorder,
//! dismiss a failure, and whether any still needs a volume. Each is an
//! [`Operations`] method over the engine's manager and registry.

use super::super::Operations;
use super::{OperationSnapshot, PauseAllOutcome, PauseOutcome, QueueMoveDirection, QueueMoveOutcome};
use cmdr_operation_log::types::OperationId;

impl Operations {
    /// The thin registry snapshot (membership + lifecycle status) for the queue
    /// window. Backs the `list_operations` IPC command.
    pub fn list_operations(&self) -> Vec<OperationSnapshot> {
        self.manager().list()
    }

    /// Whether an operation still needs `volume_id`: one the manager holds,
    /// Queued, Running, or Paused, or an external transfer (a drag-out) reading
    /// from it.
    ///
    /// ❗ Wider than the busy set Eject reads (`busy_volume_ids`, Running and
    /// Paused only): a Queued op hasn't touched the device yet, but it names the
    /// volume and will set out for it, so anything that retires the volume's id
    /// for good (a saved server moving to a new address) has to wait for it too.
    pub fn operations_need_volume(&self, volume_id: &str) -> bool {
        // Asked one after the other, ❌ never nested: the busy set takes the status
        // cache lock, and `get_operation_status` orders the two the other way.
        self.manager().names_volume(volume_id) || self.status().busy_volume_ids().iter().any(|id| id == volume_id)
    }

    /// Cancels one operation, keeping already-copied files (the existing
    /// `rollback=false` path). A Queued op is dropped from the registry without
    /// ever spawning; a Running/Paused op routes through the intent state
    /// machine. Backs the `cancel_operation(id)` IPC command.
    pub fn cancel_operation(&self, operation_id: &OperationId) {
        if !self.manager().cancel_if_queued(operation_id) {
            self.cancel_write_operation(operation_id, false);
        }
    }

    /// Cancels several operations (keep-partials each). Backs the
    /// `cancel_operations(ids)` IPC command (the queue window's "Cancel
    /// selected").
    pub fn cancel_operations(&self, operation_ids: &[OperationId]) {
        for id in operation_ids {
            self.cancel_operation(id);
        }
    }

    /// Pauses one Running operation: parks it at its next between-files
    /// boundary and flips its `LifecycleStatus` to `Paused` (re-emitting
    /// `operations-changed`). A paused op keeps its lane slots. Pausing a Queued
    /// op is a v1 no-op (it isn't touching a device yet — it stays Queued and
    /// admits normally when its lanes free); pausing a Done/absent op is a
    /// no-op. Backs `pause_operation(id)`.
    ///
    /// The [`PauseOutcome`] rides all the way out to the IPC command and the MCP
    /// `queue` tool: a surface that reports a pause it didn't get sends its user
    /// (or its agent) off believing the device is free.
    pub fn pause_operation(&self, operation_id: &OperationId) -> PauseOutcome {
        // Flip the live gate (so the driver parks) and the record status (so the
        // UI shows Paused). `set_paused` only flips a Running record, so a Queued
        // op's gate is intentionally left untouched: parking a not-yet-spawned op
        // would do nothing and risk a Paused-but-Queued limbo.
        let outcome = self.manager().set_paused(operation_id, true);
        if outcome == PauseOutcome::Applied {
            self.pause_write_operation(operation_id);
        }
        outcome
    }

    /// Resumes one Paused operation: clears its gate (waking the parked driver)
    /// and flips its `LifecycleStatus` back to `Running`. No admission pass — it
    /// never freed its lanes. Resuming a non-paused op is a no-op. Backs
    /// `resume_operation(id)`, and reports what it did for the same reason
    /// [`Self::pause_operation`] does.
    pub fn resume_operation(&self, operation_id: &OperationId) -> PauseOutcome {
        let outcome = self.manager().set_paused(operation_id, false);
        if outcome == PauseOutcome::Applied {
            self.resume_write_operation(operation_id);
        }
        outcome
    }

    /// Pauses every currently-Running operation. Backs `pause_all` (the queue
    /// window's global Pause all). Snapshots the running set first so the
    /// iteration is stable.
    ///
    /// Reports the whole sweep as a [`PauseAllOutcome`], for the reason
    /// [`Self::pause_operation`] reports its own: an empty running set, a scan
    /// still walking, and three parked copies are three different answers, and a
    /// caller that can't tell them apart says "paused" to all three.
    pub fn pause_all(&self) -> PauseAllOutcome {
        self.manager()
            .running_ids()
            .iter()
            .map(|id| self.pause_operation(id))
            .collect()
    }

    /// Resumes every currently-Paused operation. Backs `resume_all` (Resume
    /// all), and reports the sweep the way [`Self::pause_all`] does.
    pub fn resume_all(&self) -> PauseAllOutcome {
        self.manager()
            .paused_ids()
            .iter()
            .map(|id| self.resume_operation(id))
            .collect()
    }

    /// Moves a waiting operation one place earlier or later in line, so it's
    /// admitted sooner or later than the waiting operations around it. Running
    /// and paused operations keep their place. Backs `move_queued_operation(id,
    /// direction)`, the queue window's ⌥↑ / ⌥↓. Reports what it did, so the
    /// window announces the operation's real place rather than the one it asked
    /// for.
    pub fn move_queued_operation(&self, operation_id: &OperationId, direction: QueueMoveDirection) -> QueueMoveOutcome {
        self.manager().move_queued(operation_id, direction)
    }

    /// Puts a waiting operation at `position` (1-based, counting waiting
    /// operations only), shifting the ones in between. Backs
    /// `place_queued_operation(id, position)`, the queue window's
    /// drag-to-reorder.
    pub fn place_queued_operation(&self, operation_id: &OperationId, position: u32) -> QueueMoveOutcome {
        self.manager().place_queued(operation_id, position)
    }

    /// Drops one retained failure and re-broadcasts the snapshot. Backs
    /// `dismiss_failed_operation(id)`: the queue row's Dismiss button, and the
    /// foreground error dialog's close path for the operation it was showing.
    /// Dismissal is ALWAYS explicit — ❌ never a timer, a window close, or the
    /// next operation starting. Unknown id: a no-op.
    pub fn dismiss_failed_operation(&self, operation_id: &OperationId) {
        self.manager().dismiss_failure(operation_id);
    }

    /// Drops every retained failure and re-broadcasts the snapshot. Backs
    /// `dismiss_all_failed_operations` (the queue toolbar's "Dismiss all").
    pub fn dismiss_all_failed_operations(&self) {
        self.manager().dismiss_all_failures();
    }
}
