//! Where Cmdr's engines run their background work and what clock they read: the
//! ONE seam between an engine and its executor, shared by the listing engine,
//! the write engine, and the backends under them.
//!
//! On every native target this module is re-exports plus one-line wrappers:
//! [`Runtime`] IS `tokio::runtime::Handle`, [`Instant`] IS `std::time::Instant`,
//! [`time::Instant`] IS `tokio::time::Instant`, and every function forwards to the
//! `std` / tokio call of the same name, so the desktop build runs the code it ran
//! before the seam existed (start-paused test clocks included).
//!
//! The browser build (`wasm32`, for `crates/cmdr-listing-wasm`) swaps all of it
//! (`wasm.rs`): a browser has no threads to put a tokio runtime on, and
//! `std::time::Instant::now` panics on `wasm32-unknown-unknown`. Why this shape
//! and not another: `DETAILS.md` § "The browser build".
//!
//! ❌ Engine code never reads a clock, sleeps, or times out except through here;
//! each engine crate's `clippy.toml` refuses the direct calls.
#![allow(
    clippy::disallowed_methods,
    reason = "this module is the one place the engines' clock and timer calls are allowed to live"
)]

#[cfg(not(target_family = "wasm"))]
pub use std::time::Instant;
#[cfg(not(target_family = "wasm"))]
pub use tokio::runtime::Handle as Runtime;
#[cfg(not(target_family = "wasm"))]
pub use tokio::task::{JoinError, JoinHandle};

#[cfg(target_family = "wasm")]
mod wasm;
#[cfg(target_family = "wasm")]
pub use wasm::{Instant, JoinError, JoinHandle, Runtime, spawn_ambient, task, time, wall_now};

/// The current instant on the wall-clock-backed [`Instant`].
#[inline]
pub fn now() -> Instant {
    Instant::now()
}

/// How long ago `earlier` was.
#[inline]
pub fn since(earlier: Instant) -> std::time::Duration {
    earlier.elapsed()
}

std::thread_local! {
    /// How deep this thread is in [`in_one_poll`] scopes.
    static ONE_POLL_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Runs `f` as a caller that polls each future ONCE and can't wait for a second
/// (the browser viewer's bridge, `cmdr-viewer`'s `bridge.rs`). Pacing that only
/// hands the thread back (a device's breath, a modeled bandwidth) skips its wait
/// inside, so the answer is there on that one poll. Nests; thread-local, so it
/// never reaches another caller's work.
pub fn in_one_poll<R>(f: impl FnOnce() -> R) -> R {
    struct Exit;
    impl Drop for Exit {
        fn drop(&mut self) {
            ONE_POLL_DEPTH.with(|depth| depth.set(depth.get() - 1));
        }
    }
    ONE_POLL_DEPTH.with(|depth| depth.set(depth.get() + 1));
    let _exit = Exit;
    f()
}

/// Whether the caller can wait for a second poll: `false` inside [`in_one_poll`].
pub fn caller_can_wait() -> bool {
    ONE_POLL_DEPTH.with(|depth| depth.get() == 0)
}

/// The wall clock, as `std`'s `SystemTime` (what file dates and journal stamps are
/// made of). `std`'s own `SystemTime::now` panics in the browser build.
#[cfg(not(target_family = "wasm"))]
#[inline]
pub fn wall_now() -> std::time::SystemTime {
    std::time::SystemTime::now()
}

/// Spawns `future` on the CALLER's runtime: `tokio::spawn` natively (it panics
/// outside one, as a bare `tokio::spawn` does), the page's event loop in the
/// browser. Only for helpers whose design is ambient (`crate::deadline`); an
/// engine spawns on its host's [`Runtime`].
#[cfg(not(target_family = "wasm"))]
pub fn spawn_ambient<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future)
}

/// Task sets and cooperative yields: tokio's natively, the page's in the browser.
#[cfg(not(target_family = "wasm"))]
pub mod task {
    pub use tokio::task::{AbortHandle, Id, JoinSet};

    /// `tokio::task::yield_now`, wrapped rather than re-exported so an engine's
    /// clippy ban on the direct call can tell the two apart.
    #[inline]
    pub async fn yield_now() {
        tokio::task::yield_now().await;
    }
}

/// The engines' timers, on tokio's clock natively (so `start_paused` tests
/// control them) and on the page's in the browser.
#[cfg(not(target_family = "wasm"))]
pub mod time {
    use std::future::IntoFuture;
    use std::time::Duration;

    pub use tokio::time::Instant;

    /// The current instant on tokio's clock.
    #[inline]
    pub fn now() -> Instant {
        Instant::now()
    }

    /// `tokio::time::sleep`.
    #[inline]
    pub fn sleep(duration: Duration) -> tokio::time::Sleep {
        tokio::time::sleep(duration)
    }

    /// `tokio::time::sleep_until`.
    #[inline]
    pub fn sleep_until(deadline: Instant) -> tokio::time::Sleep {
        tokio::time::sleep_until(deadline)
    }

    /// `tokio::time::timeout`.
    #[inline]
    pub fn timeout<F: IntoFuture>(duration: Duration, future: F) -> tokio::time::Timeout<F::IntoFuture> {
        tokio::time::timeout(duration, future)
    }

    /// `tokio::time::timeout_at`.
    #[inline]
    pub fn timeout_at<F: IntoFuture>(deadline: Instant, future: F) -> tokio::time::Timeout<F::IntoFuture> {
        tokio::time::timeout_at(deadline, future)
    }

    /// The error a [`timeout`] or [`timeout_at`] answers when time ran out.
    pub use tokio::time::error::Elapsed;

    /// A ticker whose late ticks shift the schedule rather than burst
    /// (`tokio::time::interval` with `MissedTickBehavior::Delay`). The first tick
    /// is immediate, as tokio's is.
    pub fn interval_delayed(period: Duration) -> Interval {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval
    }

    /// What [`interval_delayed`] answers.
    pub use tokio::time::Interval;
}
