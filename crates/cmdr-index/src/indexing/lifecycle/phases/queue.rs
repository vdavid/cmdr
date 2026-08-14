//! What to cover next, and what has already had its turn.
//!
//! Deliberately small and in-memory: a launch recomputes the whole thing from the
//! host's answer plus a coverage query per root, so a resumed volume naturally
//! skips what is done. ❌ Persisting it would add a second description of the
//! index's state that can disagree with the index.

use std::path::{Path, PathBuf};

/// How badly a root wants to be next. Ordered best-first, which is what
/// `#[derive(Ord)]` gives from the declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Rank {
    /// A folder the host says this user cares about. The best signal there is —
    /// last session's tabs, their favorites, the folders they keep things in — so
    /// nothing displaces it, including a folder they open mid-run: that IS the
    /// same question, answered less well.
    PriorityRoot,
    /// A folder the user opened while the machine was running.
    VisitedRoot,
    /// The rest of `$HOME`.
    Home,
    /// Everything else on the drive.
    WholeVolume,
}

/// One root, and why it is where it is in the order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Phase {
    pub rank: Rank,
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
    pub(super) fn push(&mut self, rank: Rank, path: PathBuf) {
        if self.already_done(&path) || self.pending.iter().any(|phase| phase.path == path) {
            return;
        }
        self.pending.push(Phase { rank, path });
        self.pending.sort_by(|a, b| a.rank.cmp(&b.rank));
    }

    /// The next phase to run, best-ranked first and first-queued within a rank
    /// (the host's order is the schedule, so it has to survive the sort — which is
    /// why the sort is stable).
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
        queue.push(Rank::WholeVolume, PathBuf::from("/"));
        queue.push(Rank::PriorityRoot, PathBuf::from("/home/me/Downloads"));
        queue.push(Rank::Home, PathBuf::from("/home/me"));
        queue.push(Rank::PriorityRoot, PathBuf::from("/home/me/Documents"));
        queue.push(Rank::VisitedRoot, PathBuf::from("/opt"));

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
        queue.push(Rank::PriorityRoot, PathBuf::from("/home/me"));
        queue.push(Rank::Home, PathBuf::from("/home/me"));
        assert_eq!(queue.take_next().map(|phase| phase.rank), Some(Rank::PriorityRoot));
        assert_eq!(queue.take_next(), None, "the duplicate was never queued");

        queue.push(Rank::VisitedRoot, PathBuf::from("/home/me"));
        assert_eq!(queue.take_next(), None, "and a finished root doesn't come back");
    }
}
