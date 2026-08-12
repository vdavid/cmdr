//! A live search folded into one answer, for a caller that can't listen.
//!
//! The dialog subscribes to `search-progress` and watches an answer assemble
//! itself. An MCP tool call is one request and one reply: there is no channel to
//! push a batch down, and nothing on the other end that could render it if there
//! were. So the same run, the same walk, and the same events go through
//! [`CollectingSink`], which folds them into a [`LiveAnswer`] the caller takes
//! when it's ready.
//!
//! **The fold is what streaming becomes over a one-shot transport.** Two things
//! survive the flattening and both matter:
//!
//! - **The rows are whatever had arrived**, not "nothing until it's all done".
//!   A caller that waits 20 seconds on a 4-minute walk gets 20 seconds of
//!   results with the walk's own progress attached, rather than an empty list.
//! - **The walk carries on** ([`AnswerEnding::StillWalking`]). Handing back an
//!   answer is not a cancel, exactly as superseding a dialog run isn't (Decision
//!   11): walking is coverage work, its rows land in the index either way, and
//!   the same search run again picks up from where this one left off. Only the
//!   app quitting stops it.
//!
//! Memory is bounded by the query's own row cap: `ResultStream` emits at most
//! `limit` rows for the whole run, so a fold that outlives its reader can't grow
//! past one result page.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::ignore_poison::IgnorePoison;
use crate::search::types::SearchResultEntry;

use super::events::{
    SearchCancelledEvent, SearchCompleteEvent, SearchErrorEvent, SearchEventSink, SearchProgressEvent,
    SearchRunCoverage, SearchRunError,
};

/// How a folded run ended, from the point of view of whoever was waiting.
#[derive(Debug, Clone)]
pub(crate) enum AnswerEnding {
    /// The run reached its terminal state in time, and said what it couldn't
    /// cover. `SearchRunCoverage::walk` still decides whether the list is
    /// complete.
    Settled(Box<SearchRunCoverage>),
    /// The wait ran out first. The rows are real and the count is honest for
    /// what had been searched; the walk is still going, so both are a lower
    /// bound and the same search again continues from here.
    StillWalking,
    /// The run couldn't run at all: an unusable query, or an index that won't
    /// open.
    Failed {
        /// Typed, word-free classification. Branch on this.
        error: SearchRunError,
        /// The sentence to show, rendered backend-side like the engine's.
        message: String,
    },
}

/// One live search, as a single answer.
#[derive(Debug, Clone)]
pub(crate) struct LiveAnswer {
    /// The ONE volume routing picked.
    pub(crate) target_volume_id: String,
    /// The rows, in arrival order: the index's half, then the walk's. Empty for
    /// a count-only run.
    pub(crate) entries: Vec<SearchResultEntry>,
    /// Every match, including the ones past the row cap.
    pub(crate) match_count: u32,
    /// Directories the walk turned up. Zero when nothing had to be walked, which
    /// is how a caller tells an index-served answer from a walked one.
    pub(crate) dirs_found: u64,
    pub(crate) ending: AnswerEnding,
}

/// What the sink has heard so far.
#[derive(Default)]
struct Fold {
    entries: Vec<SearchResultEntry>,
    match_count: u32,
    dirs_found: u64,
    ending: Option<AnswerEnding>,
}

/// Collects a live run's events into one answer, and wakes whoever is waiting
/// the moment the run reaches a terminal state.
#[derive(Default)]
pub(crate) struct CollectingSink {
    fold: Mutex<Fold>,
    settled: Condvar,
}

impl CollectingSink {
    /// Take the answer, waiting up to `budget` for the run to settle.
    ///
    /// Returns as soon as the run reaches a terminal state, or the moment the
    /// budget runs out with the run still going. Either way the caller gets
    /// every row that had arrived.
    ///
    /// TAKES the rows, so call it once per run: a second call would report the
    /// ones that arrived since, not the answer.
    pub(crate) fn answer_within(&self, budget: Duration, target_volume_id: String) -> LiveAnswer {
        let fold = self.fold.lock_ignore_poison();
        let (mut fold, _) = self
            .settled
            .wait_timeout_while(fold, budget, |fold| fold.ending.is_none())
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        LiveAnswer {
            target_volume_id,
            entries: std::mem::take(&mut fold.entries),
            match_count: fold.match_count,
            dirs_found: fold.dirs_found,
            ending: fold.ending.clone().unwrap_or(AnswerEnding::StillWalking),
        }
    }

    /// Record a terminal state and wake the waiter.
    fn settle(&self, ending: AnswerEnding) {
        self.fold.lock_ignore_poison().ending = Some(ending);
        self.settled.notify_all();
    }
}

impl SearchEventSink for CollectingSink {
    fn emit_progress(&self, event: SearchProgressEvent) {
        let mut fold = self.fold.lock_ignore_poison();
        fold.entries.extend(event.entries);
        fold.match_count = event.match_count;
        fold.dirs_found = event.dirs_found;
    }

    fn emit_complete(&self, event: SearchCompleteEvent) {
        self.fold.lock_ignore_poison().match_count = event.match_count;
        self.settle(AnswerEnding::Settled(Box::new(event.coverage)));
    }

    fn emit_cancelled(&self, event: SearchCancelledEvent) {
        // Settled, not still-walking: a cancelled run has stopped, and its
        // coverage says `Cancelled` so the caller knows the list is short. For an
        // agent's run the only thing that cancels is the app quitting.
        self.fold.lock_ignore_poison().match_count = event.match_count;
        self.settle(AnswerEnding::Settled(Box::new(event.coverage)));
    }

    fn emit_error(&self, event: SearchErrorEvent) {
        self.settle(AnswerEnding::Failed {
            error: event.error,
            message: event.message,
        });
    }
}
