//! How importance work hears its volume's stop signal, and what a stopped pass is.
//!
//! The signal is the crate's one cancellation primitive, a
//! `tokio_util::sync::CancellationToken` that is a child of the volume's root token
//! (`indexing/host/DETAILS.md` § Cancellation). Nothing here is a second primitive:
//! [`StopPoll`] only decides how OFTEN a tight loop looks at that token.

use tokio_util::sync::CancellationToken;

/// How many items a loop handles between two looks at the stop signal.
///
/// A look takes the token's mutex, so a per-item look over the 7.4 M file rows of a
/// real root index would be the most-executed lock in the pass. Every 1,024 items it
/// is ~7,200 looks for that whole stream, and the slowest loop that polls (the store
/// write, a few microseconds a row) still hears a stop within a handful of
/// milliseconds. Sizing and the measured cost: `scheduler/DETAILS.md` § "How a pass
/// stops".
pub(crate) const STOP_CHECK_INTERVAL: u32 = 1024;

/// Why a pass ended without finishing.
///
/// `Cancelled` is a variant of the error rather than a flag on a success value, for
/// the reason `indexing/host/DETAILS.md` § "Cancellation is observable, as a typed
/// error" gives: everything that records a finished pass (the generation stamp, the
/// scoring-policy stamp, the recompute notice) sits behind an `Ok`, so a caller can't
/// reach it by forgetting to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PassError {
    /// The volume's stop signal fired. The store holds exactly what it held before
    /// the pass started.
    Cancelled,
    /// Reading the index or writing the store went wrong.
    Failed(String),
}

impl From<String> for PassError {
    fn from(reason: String) -> Self {
        Self::Failed(reason)
    }
}

impl From<super::store::ImportanceStoreError> for PassError {
    fn from(error: super::store::ImportanceStoreError) -> Self {
        Self::Failed(error.to_string())
    }
}

impl std::fmt::Display for PassError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("stopped before it finished"),
            Self::Failed(reason) => f.write_str(reason),
        }
    }
}

/// A loop's view of the stop signal: looks at the token on the first item and then
/// once every [`STOP_CHECK_INTERVAL`] items.
pub(crate) struct StopPoll<'a> {
    stop: &'a CancellationToken,
    until_next_look: u32,
}

impl<'a> StopPoll<'a> {
    /// A poll that looks at `stop` on its first [`tick`](Self::tick).
    pub(crate) fn new(stop: &'a CancellationToken) -> Self {
        Self {
            stop,
            until_next_look: 0,
        }
    }

    /// Count one item, and say whether the work should stop.
    pub(crate) fn tick(&mut self) -> Result<(), PassError> {
        if self.until_next_look == 0 {
            self.until_next_look = STOP_CHECK_INTERVAL;
            check(self.stop)?;
        }
        self.until_next_look -= 1;
        Ok(())
    }
}

/// Look at the stop signal right now: for the boundary between two phases of a pass,
/// where there's no loop to count.
pub(crate) fn check(stop: &CancellationToken) -> Result<(), PassError> {
    if stop.is_cancelled() {
        Err(PassError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The interval is the whole point of the type: a token cancelled between two
    /// looks is heard at the next look, and never later.
    #[test]
    fn a_stop_is_heard_at_the_next_look() {
        let stop = CancellationToken::new();
        let mut poll = StopPoll::new(&stop);
        assert_eq!(poll.tick(), Ok(()), "the first item looks, and the signal hasn't fired");

        stop.cancel();
        let heard_after = (1..=STOP_CHECK_INTERVAL)
            .find(|_| poll.tick().is_err())
            .expect("a fired signal is heard within one interval");
        assert_eq!(heard_after, STOP_CHECK_INTERVAL, "and no look happens in between");
    }

    #[test]
    fn a_signal_that_already_fired_stops_the_first_item() {
        let stop = CancellationToken::new();
        stop.cancel();
        assert_eq!(StopPoll::new(&stop).tick(), Err(PassError::Cancelled));
        assert_eq!(check(&stop), Err(PassError::Cancelled));
    }
}
