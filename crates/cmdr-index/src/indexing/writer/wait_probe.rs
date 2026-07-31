//! Per-thread accounting of time spent waiting on the writer queue.
//!
//! The writer channel is a bounded `sync_channel`, so a sender parks once it's
//! full — that's the backpressure that keeps a fast producer from outrunning the
//! single writer thread. The wait is invisible from the producer's own timings,
//! though: it lands inside whatever the producer is measuring, with nothing to
//! attribute it to. A subtree reconcile that spent 19 of its 21 seconds parked
//! reported itself as a slow walk, sending readers hunting in the reconciler.
//!
//! So `IndexWriter::send` and `flush_blocking` add each wait here, and a producer
//! that reports its own duration brackets its work with [`take`] and says how much
//! of it was the queue. Thread-local because each producer runs on its own thread
//! (the rescan walk on `rescan-subtree`, the scanner on its own) and only ever
//! wants its own wait.

use std::cell::Cell;
use std::time::Duration;

thread_local! {
    static WRITER_WAIT: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

/// Add a wait THIS thread just finished. Called from the two blocking paths.
pub(crate) fn note(waited: Duration) {
    WRITER_WAIT.with(|w| w.set(w.get().saturating_add(waited)));
}

/// This thread's accumulated wait, resetting the counter. Call it once to arm
/// (discarding whatever an earlier producer left) and again to read the span.
pub(crate) fn take() -> Duration {
    WRITER_WAIT.with(|w| w.replace(Duration::ZERO))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Waits accumulate across calls, and a `take` both reads and rearms — the
    /// bracket a producer relies on to measure one span and not the one before it.
    #[test]
    fn waits_accumulate_and_take_resets() {
        take();
        assert_eq!(take(), Duration::ZERO, "nothing recorded yet");

        note(Duration::from_millis(30));
        note(Duration::from_millis(12));
        assert_eq!(take(), Duration::from_millis(42));
        assert_eq!(take(), Duration::ZERO, "take rearms for the next span");
    }
}
