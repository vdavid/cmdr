//! Reordering the line: a waiting operation moves among the waiting ones (one
//! place by key, or straight to a place by a drag), so the next lane to free
//! admits the one the user put in front. `../../DETAILS.md` § "Reordering the line".

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_operation_log::types::OperationId;

use super::super::types::LifecycleStatus;
use super::{OperationManager, QueueMoveDirection, QueueMoveOutcome};

/// Where a reorder asks the op to go, in 0-based places among WAITING ops.
enum Target {
    /// One place earlier or later: past the front or back is `AtEdge`.
    Step(QueueMoveDirection),
    /// A place, clamped to the line (a drop past the end lands last).
    Place(usize),
}

impl OperationManager {
    /// Moves a waiting op one place earlier or later in line, past every running
    /// or paused op between it and the nearest WAITING one on that side.
    pub(crate) fn move_queued(&self, operation_id: &OperationId, direction: QueueMoveDirection) -> QueueMoveOutcome {
        self.reorder(operation_id, Target::Step(direction))
    }

    /// Puts a waiting op at `position` (1-based, among waiting ops), shifting the
    /// waiting ops in between by one: the drop of a drag in the queue window.
    pub(crate) fn place_queued(&self, operation_id: &OperationId, position: u32) -> QueueMoveOutcome {
        let place = usize::try_from(position.saturating_sub(1)).unwrap_or(usize::MAX);
        self.reorder(operation_id, Target::Place(place))
    }

    /// The waiting ops are rewritten into the `order` slots the waiting ops
    /// already hold, so every running or paused op keeps its slot (their place in
    /// `order` decides nothing; they already hold their lanes).
    ///
    /// No admission pass. Admission takes any waiting op whose lanes are free,
    /// and the pass that ran when the line last changed left none, so a reorder
    /// can't make one admittable; it only decides who goes FIRST when a lane
    /// frees. Broadcasts only when something moved.
    fn reorder(&self, operation_id: &OperationId, target: Target) -> QueueMoveOutcome {
        let outcome = {
            let mut inner = self.inner.lock_ignore_poison();
            let waiting_slots: Vec<usize> = inner
                .order
                .iter()
                .enumerate()
                .filter(|(_, id)| {
                    inner
                        .records
                        .get(*id)
                        .is_some_and(|rec| rec.status == LifecycleStatus::Queued)
                })
                .map(|(slot, _)| slot)
                .collect();
            let Some(index) = waiting_slots
                .iter()
                .position(|&slot| inner.order[slot] == *operation_id)
            else {
                return QueueMoveOutcome::NotQueued;
            };
            // The line is the waiting ops, and it's never anywhere near u32::MAX long.
            let waiting = u32::try_from(waiting_slots.len()).unwrap_or(u32::MAX);
            let place = |i: usize| u32::try_from(i + 1).unwrap_or(u32::MAX);
            let to = match target {
                Target::Step(QueueMoveDirection::Earlier) => index.checked_sub(1),
                Target::Step(QueueMoveDirection::Later) => Some(index + 1).filter(|&next| next < waiting_slots.len()),
                Target::Place(to) => Some(to.min(waiting_slots.len() - 1)),
            };
            match to {
                None => QueueMoveOutcome::AtEdge {
                    position: place(index),
                    waiting,
                },
                Some(to) if to == index => QueueMoveOutcome::Unchanged {
                    position: place(index),
                    waiting,
                },
                Some(to) => {
                    let mut line: Vec<OperationId> =
                        waiting_slots.iter().map(|&slot| inner.order[slot].clone()).collect();
                    let moved = line.remove(index);
                    line.insert(to, moved);
                    for (&slot, id) in waiting_slots.iter().zip(line) {
                        inner.order[slot] = id;
                    }
                    QueueMoveOutcome::Moved {
                        position: place(to),
                        waiting,
                    }
                }
            }
        };
        if let QueueMoveOutcome::Moved { position, waiting } = outcome {
            log::info!(target: "op_manager", "move queued op={operation_id} to {position} of {waiting}");
            self.emit_changed();
        }
        outcome
    }
}
