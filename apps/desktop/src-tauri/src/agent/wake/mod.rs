//! The wake pipeline's pure core: what the agent gets told about, and how much of it.
//!
//! ```text
//! already-corrected change events
//!    → coalesce   (per-folder counters in a window)
//!    → interest   (deterministic, from values)
//!    → compact    (hierarchical, to a hard token budget)
//!    → digest
//! ```
//!
//! Three pure functions, values in and values out, no I/O and no clock. That is the whole
//! module: the agent never receives raw events, and everything about WHICH events matter and
//! HOW MUCH of them fits is decided here, deterministically, before any model is involved.
//!
//! ## What this module deliberately does not do
//!
//! - ❌ **No event subscription.** [`FolderEvent`] is this module's own vocabulary, not the
//!   indexer's type. Where the agent taps the indexer's corrected stream is an open design
//!   question (agent-spec §18.14), and taking a value keeps it open rather than answering it by
//!   accident.
//! - **No inbox, no deadlines held anywhere, no wake job, no LLM.** [`wake_delay`] turns an
//!   interest score into a delay and hands it back; whoever owns a clock does the waiting.
//!
//! ## The properties worth defending
//!
//! - **The digest never exceeds its budget.** Not with the rollups, not with the "and more"
//!   line, not at a budget too small to hold anything.
//! - **The budget is spent in interest order.** Noise cannot eat the budget before the
//!   interesting tail is described, whatever order it arrived in.
//! - **Nothing is silently dropped.** What doesn't fit is rolled up and counted, so the agent
//!   is told how much it isn't seeing.
//!
//! Depth (the windowing rule, the interest formula, the compaction fold, the token estimate):
//! `DETAILS.md`.

mod coalesce;
mod compact;
mod inbox;
mod interest;
mod job;
mod persist;
mod readiness;

#[cfg(test)]
mod tests;

pub use coalesce::{coalesce, merge_bundles};
pub use compact::{Digest, DigestLine, Rollup, ScoredBundle, compact};
pub use inbox::{Inbox, InboxRow, ReconcileReport, SETTLE_AFTER_LAUNCH, STALE_AFTER};
pub use interest::{
    FolderImportance, HOT_DELAY, HOT_THRESHOLD, Interest, WARM_DELAY, WARM_THRESHOLD, interest, wake_delay,
};
pub use job::{WakeOutcome, WakeParams, run_wake, thread_title};
pub use persist::{clear, load, save_all, save_row};
pub use readiness::{AgentGates, WakeReadiness, readiness};

/// What kind of change happened. Intent-bearing kinds (a file appearing, a file being renamed)
/// mean more to the agent than churn (a file written to again), which is what [`interest()`]
/// weighs them by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    /// A file or folder appeared.
    Created,
    /// Existing content changed.
    Modified,
    /// A file or folder went away.
    Removed,
    /// A file or folder was renamed in place.
    Renamed,
}

/// One already-corrected change, in this module's own vocabulary.
///
/// Deliberately NOT the indexer's event type: the tap point is undesigned (agent-spec §18.14),
/// and a value here means the answer stays open. Whoever wires the stream later maps into this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEvent {
    /// The folder the change happened IN, not the changed path. The pipeline counts per folder.
    pub folder: String,
    pub kind: ChangeKind,
    /// When it happened, unix seconds. A value, never a clock read — the same purity contract
    /// the importance scorer keeps.
    pub at: u64,
}

/// How many of each kind of change a bundle saw. The whole of what a bundle carries: no file
/// names.
///
/// Names would grow memory with the event count on exactly the path that has to survive five
/// million of them, and they'd spend digest budget on detail the agent can pull for itself with
/// a `list_dir` once it's awake. The digest says WHERE something happened and HOW MUCH; the
/// agent looks up WHAT.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChangeCounters {
    pub created: u32,
    pub modified: u32,
    pub removed: u32,
    pub renamed: u32,
}

impl ChangeCounters {
    /// Count one more change of `kind`. Saturating, because a counter that wrapped would turn
    /// the noisiest folder on the disk into the quietest.
    pub fn record(&mut self, kind: ChangeKind) {
        let slot = match kind {
            ChangeKind::Created => &mut self.created,
            ChangeKind::Modified => &mut self.modified,
            ChangeKind::Removed => &mut self.removed,
            ChangeKind::Renamed => &mut self.renamed,
        };
        *slot = slot.saturating_add(1);
    }

    /// Every change this bundle saw.
    pub fn total(&self) -> u64 {
        u64::from(self.created) + u64::from(self.modified) + u64::from(self.removed) + u64::from(self.renamed)
    }

    /// Fold another set of counters in — what a rollup line does to its members.
    pub fn merge(&mut self, other: &ChangeCounters) {
        self.created = self.created.saturating_add(other.created);
        self.modified = self.modified.saturating_add(other.modified);
        self.removed = self.removed.saturating_add(other.removed);
        self.renamed = self.renamed.saturating_add(other.renamed);
    }

    /// The share of changes carrying INTENT (something appeared or was renamed) rather than
    /// churn (something written to again). `0.0` when nothing happened.
    pub fn intent_share(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        let intent = u64::from(self.created) + u64::from(self.renamed);
        intent as f64 / total as f64
    }
}

/// One folder's changes within one window: what the coalescer produces, and what an inbox row
/// would hold (agent-spec §4.2's `agent_inbox`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBundle {
    pub folder: String,
    pub counters: ChangeCounters,
    /// The start of the window this bundle covers, unix seconds.
    pub window_start: u64,
    /// The most recent change in it — what a deadline would be measured from.
    pub last_event_at: u64,
}
