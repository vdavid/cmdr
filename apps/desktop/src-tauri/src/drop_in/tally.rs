//! Pure accounting for one promised drop: AppKit calls the reader block once per
//! promised file, from the operation queue, in any order. The tally says when the
//! last one landed (or the watchdog gave up), exactly once.

use std::path::PathBuf;

use super::events::PromiseDropFailure;

/// How one promised drop ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DropOutcome {
    /// At least one file arrived.
    Ready { paths: Vec<PathBuf>, failed_count: usize },
    /// None did.
    Failed(PromiseDropFailure),
}

/// The running count of one drop's promised files.
#[derive(Debug)]
pub(crate) struct ReceiveTally {
    expected: usize,
    landed: Vec<PathBuf>,
    failed: usize,
    finished: bool,
}

impl ReceiveTally {
    /// A tally waiting for `expected` reader callbacks.
    pub(crate) fn new(expected: usize) -> Self {
        Self {
            expected,
            landed: Vec::new(),
            failed: 0,
            finished: false,
        }
    }

    /// Records one reader callback. Returns the outcome when this was the last one,
    /// and `None` before that or after the drop already finished (a callback that
    /// outlived the watchdog).
    pub(crate) fn record(&mut self, received: Option<PathBuf>) -> Option<DropOutcome> {
        if self.finished {
            return None;
        }
        match received {
            Some(path) => self.landed.push(path),
            None => self.failed += 1,
        }
        if self.landed.len() + self.failed < self.expected {
            return None;
        }
        Some(self.finish(PromiseDropFailure::NothingReceived))
    }

    /// The watchdog's give-up: what landed so far counts, the rest failed. `None`
    /// when the drop already finished.
    pub(crate) fn time_out(&mut self) -> Option<DropOutcome> {
        if self.finished {
            return None;
        }
        self.failed += self.expected.saturating_sub(self.landed.len() + self.failed);
        Some(self.finish(PromiseDropFailure::TimedOut))
    }

    fn finish(&mut self, if_empty: PromiseDropFailure) -> DropOutcome {
        self.finished = true;
        if self.landed.is_empty() {
            return DropOutcome::Failed(if_empty);
        }
        DropOutcome::Ready {
            paths: std::mem::take(&mut self.landed),
            failed_count: self.failed,
        }
    }
}

#[cfg(test)]
#[path = "tally_tests.rs"]
mod tests;
