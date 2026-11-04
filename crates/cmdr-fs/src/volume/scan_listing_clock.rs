//! `ScanListingClock`: how long one background-scan listing has spent actually
//! talking to its volume, as opposed to waiting its turn.
//!
//! The index walk gives up on a listing that runs too long, because a wedged
//! mount never answers. But a backend that yields to foreground work (MTP parks
//! a scan listing while the user browses or copies on the phone, and queues it
//! behind the walk's other listings on the one USB pipe) can hold a perfectly
//! healthy listing for minutes without sending a byte. Counting that wait as
//! "the device didn't answer" turned our own pacing into holes in the index.
//!
//! So the walk hands each listing a clock, and the backend pauses it around
//! every wait that's ITS OWN choice (a yield point, a lock the scan's siblings
//! hold). The walk's timeout reads [`active`](ScanListingClock::active), which
//! leaves those pauses out. A round trip that's genuinely stuck on the wire is
//! never inside a pause, so it still times out.
//!
//! ❗ Pause only around waits the backend controls. Wrapping a device round trip
//! in a pause would hide exactly the wedge the timeout exists to catch.

use crate::rt::time::Instant;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One listing's running-time stopwatch, pausable from inside the backend.
/// Cheap to clone; clones share the same clock.
#[derive(Clone, Debug)]
pub struct ScanListingClock {
    inner: Arc<Mutex<ClockState>>,
}

#[derive(Debug)]
struct ClockState {
    started: Instant,
    /// Total time spent fully paused, not counting a pause still open.
    paused_total: Duration,
    /// How many pause guards are alive. Pauses nest: the clock runs again only
    /// once the last one ends.
    open_pauses: usize,
    /// When the outermost open pause began.
    paused_since: Option<Instant>,
}

impl Default for ScanListingClock {
    fn default() -> Self {
        Self::start()
    }
}

impl ScanListingClock {
    /// A clock that starts running now.
    pub fn start() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ClockState {
                started: crate::rt::time::now(),
                paused_total: Duration::ZERO,
                open_pauses: 0,
                paused_since: None,
            })),
        }
    }

    /// Time since [`start`](Self::start), minus every pause (including one
    /// still open).
    pub fn active(&self) -> Duration {
        let state = self.lock();
        let now = crate::rt::time::now();
        let open = state.paused_since.map_or(Duration::ZERO, |since| now - since);
        (now - state.started).saturating_sub(state.paused_total + open)
    }

    /// Stop the clock until the returned guard drops. Nestable.
    pub fn pause(&self) -> ScanListingClockPause {
        let mut state = self.lock();
        if state.open_pauses == 0 {
            state.paused_since = Some(crate::rt::time::now());
        }
        state.open_pauses += 1;
        ScanListingClockPause { clock: self.clone() }
    }

    /// Await `fut` with the clock stopped. For a wait the backend chose (a yield
    /// point, a lock its own scan holds), never for a device round trip.
    pub async fn paused<F: Future>(&self, fut: F) -> F::Output {
        let _pause = self.pause();
        fut.await
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ClockState> {
        // A poisoned clock is still a valid clock: every critical section here
        // is a few arithmetic steps that can't leave it half-updated.
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Keeps a [`ScanListingClock`] stopped while alive.
#[derive(Debug)]
#[must_use = "the clock runs again as soon as the pause guard drops"]
pub struct ScanListingClockPause {
    clock: ScanListingClock,
}

impl Drop for ScanListingClockPause {
    fn drop(&mut self) {
        let mut state = self.clock.lock();
        state.open_pauses -= 1;
        if state.open_pauses == 0
            && let Some(since) = state.paused_since.take()
        {
            state.paused_total += crate::rt::time::now() - since;
        }
    }
}

#[cfg(test)]
#[path = "scan_listing_clock_test.rs"]
mod tests;
