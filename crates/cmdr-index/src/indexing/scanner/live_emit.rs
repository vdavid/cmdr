//! When a partial batch of discovered entries goes out on its own.
//!
//! A walk hands its entries to a live consumer in batches, and a batch fills at
//! 2 000 entries. That is the right size for the crossing and the wrong one for
//! the wait: a search over a sparse tree (one matching file per directory) finds
//! rows the whole time and shows none of them until the walk is nearly done.
//!
//! So a batch that has been waiting [`EMIT_INTERVAL`] goes out part-full. The
//! clock runs from the FIRST row in the batch, not the last, and only while a
//! batch is actually waiting — a full scan (no consumer) and a walk fast enough to
//! fill 2 000 entries both pay nothing for it.
//!
//! ❌ Don't make this an unconditional tick, and ❌ don't shrink the batch to the
//! interval's worth of rows: the channel behind it is bounded on purpose
//! (Decision 3), and a per-entry crossing is what that bound exists to prevent.
//! The 100 ms matches the rate the search's own `ResultStream` emits at
//! (`apps/desktop/src-tauri/src/search/live.rs`: 100 rows or 100 ms), so the whole
//! pipe has one cadence rather than two that beat against each other.

use std::time::{Duration, Instant};

/// The longest a found row waits for company before its batch is handed over.
pub(in crate::indexing) const EMIT_INTERVAL: Duration = Duration::from_millis(100);

/// The clock on one walk's pending batch.
///
/// Held wherever the pending batch is held, under the same lock, so asking it
/// costs nothing extra. Not `Copy`: there is exactly one per walk, and two would
/// each time only half the rows.
#[derive(Debug)]
pub(in crate::indexing) struct EmitPacer {
    interval: Duration,
    /// When the batch that is currently waiting may go out, or `None` when
    /// nothing is waiting.
    due: Option<Instant>,
}

impl Default for EmitPacer {
    fn default() -> Self {
        Self::new()
    }
}

impl EmitPacer {
    /// A pacer at the production interval.
    pub(in crate::indexing) fn new() -> Self {
        Self::with_interval(EMIT_INTERVAL)
    }

    /// The same at a chosen interval. Production takes [`EMIT_INTERVAL`] through
    /// [`new`](Self::new); a test picks its own so it doesn't have to wait.
    pub(in crate::indexing) fn with_interval(interval: Duration) -> Self {
        Self { interval, due: None }
    }

    /// A row just joined the pending batch. Starts the clock if this is the row
    /// the batch is waiting on; leaves it alone otherwise, so the deadline belongs
    /// to the OLDEST row rather than the newest.
    pub(in crate::indexing) fn waiting(&mut self) {
        if self.due.is_none() {
            self.due = Some(Instant::now() + self.interval);
        }
    }

    /// Whether the waiting batch has waited long enough. `false` costs no clock
    /// read at all when nothing is waiting.
    pub(in crate::indexing) fn is_due(&self) -> bool {
        self.due.is_some_and(|due| Instant::now() >= due)
    }

    /// The batch went out, by whatever route (full, due, or the walk ending).
    pub(in crate::indexing) fn sent(&mut self) {
        self.due = None;
    }
}
