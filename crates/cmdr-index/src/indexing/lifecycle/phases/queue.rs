//! What to cover next, and what has already had its turn.
//!
//! Deliberately small and in-memory: a launch recomputes the whole thing from the
//! host's answer plus a coverage query per root, so a resumed volume naturally
//! skips what is done. ❌ Persisting it would add a second description of the
//! index's state that can disagree with the index.

use std::path::{Path, PathBuf};

use crate::indexing::events::CoveragePhase;

/// One root, and why it is where it is in the order.
///
/// Its [`CoveragePhase`] is both what the phase IS and how badly it wants to be
/// next: that enum's declaration order is best-first, and the queue sorts by it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Phase {
    pub kind: CoveragePhase,
    pub path: PathBuf,
}

/// The phases still to run, best-first, plus what has already run.
pub(super) struct PhaseQueue {
    pending: Vec<Phase>,
    done: Vec<PathBuf>,
}

impl PhaseQueue {
    pub(super) fn new() -> Self {
        Self {
            pending: Vec::new(),
            done: Vec::new(),
        }
    }

    /// Queue a root, unless it is already queued or already had its turn.
    ///
    /// Exact paths only. A priority root INSIDE a later phase's root is not a
    /// duplicate: `~/Downloads` first and `$HOME` afterwards is the whole point of
    /// the ordering, and the coverage query is what stops the second one re-walking
    /// the first.
    pub(super) fn push(&mut self, kind: CoveragePhase, path: PathBuf) {
        if self.already_done(&path) || self.pending.iter().any(|phase| phase.path == path) {
            return;
        }
        self.pending.push(Phase { kind, path });
        self.pending.sort_by_key(|a| a.kind);
    }

    /// The next phase to run, best first and first-queued within a kind (the
    /// host's order is the schedule, so it has to survive the sort — which is why
    /// the sort is stable).
    pub(super) fn take_next(&mut self) -> Option<Phase> {
        if self.pending.is_empty() {
            return None;
        }
        let phase = self.pending.remove(0);
        self.done.push(phase.path.clone());
        Some(phase)
    }

    pub(super) fn already_done(&self, path: &Path) -> bool {
        self.done.iter().any(|done| done == path)
    }

    pub(super) fn mark_done(&mut self, path: &Path) {
        if !self.already_done(path) {
            self.done.push(path.to_path_buf());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The order IS the schedule: the host's folders first in the order it gave
    /// them, then home, then the drive. A root the user opens mid-run slots in
    /// between the two, never ahead of the host's own answer.
    #[test]
    fn phases_run_in_order() {
        let mut queue = PhaseQueue::new();
        queue.push(CoveragePhase::WholeVolume, PathBuf::from("/"));
        queue.push(CoveragePhase::PriorityRoot, PathBuf::from("/home/me/Downloads"));
        queue.push(CoveragePhase::Home, PathBuf::from("/home/me"));
        queue.push(CoveragePhase::PriorityRoot, PathBuf::from("/home/me/Documents"));
        queue.push(CoveragePhase::VisitedRoot, PathBuf::from("/opt"));

        let order: Vec<PathBuf> = std::iter::from_fn(|| queue.take_next())
            .map(|phase| phase.path)
            .collect();
        assert_eq!(
            order,
            [
                PathBuf::from("/home/me/Downloads"),
                PathBuf::from("/home/me/Documents"),
                PathBuf::from("/opt"),
                PathBuf::from("/home/me"),
                PathBuf::from("/"),
            ]
        );
    }

    /// A root the host already named doesn't get a second turn when the machine
    /// queues `$HOME` itself, and a root that has run doesn't come back.
    #[test]
    fn a_root_that_has_had_its_turn_is_not_queued_again() {
        let mut queue = PhaseQueue::new();
        queue.push(CoveragePhase::PriorityRoot, PathBuf::from("/home/me"));
        queue.push(CoveragePhase::Home, PathBuf::from("/home/me"));
        assert_eq!(
            queue.take_next().map(|phase| phase.kind),
            Some(CoveragePhase::PriorityRoot)
        );
        assert_eq!(queue.take_next(), None, "the duplicate was never queued");

        queue.push(CoveragePhase::VisitedRoot, PathBuf::from("/home/me"));
        assert_eq!(queue.take_next(), None, "and a finished root doesn't come back");
    }
}
