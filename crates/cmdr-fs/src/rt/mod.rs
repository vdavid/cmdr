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

#[cfg(target_family = "wasm")]
mod wasm;
#[cfg(target_family = "wasm")]
pub use wasm::{Instant, JoinError, JoinHandle, Runtime, time};

/// The current instant on the wall-clock-backed [`Instant`].
pub fn now() -> Instant {
    Instant::now()
}

/// How long ago `earlier` was.
pub fn since(earlier: Instant) -> std::time::Duration {
    earlier.elapsed()
}

/// The engines' timers, on tokio's clock natively (so `start_paused` tests
/// control them) and on the page's in the browser.
#[cfg(not(target_family = "wasm"))]
pub mod time {
    use std::future::IntoFuture;
    use std::time::Duration;

    pub use tokio::time::Instant;

    /// The current instant on tokio's clock.
    pub fn now() -> Instant {
        Instant::now()
    }

    /// `tokio::time::sleep`.
    pub fn sleep(duration: Duration) -> tokio::time::Sleep {
        tokio::time::sleep(duration)
    }

    /// `tokio::time::sleep_until`.
    pub fn sleep_until(deadline: Instant) -> tokio::time::Sleep {
        tokio::time::sleep_until(deadline)
    }

    /// `tokio::time::timeout`.
    pub fn timeout<F: IntoFuture>(duration: Duration, future: F) -> tokio::time::Timeout<F::IntoFuture> {
        tokio::time::timeout(duration, future)
    }
}
