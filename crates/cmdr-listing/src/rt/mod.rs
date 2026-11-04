//! Where the engine's background work runs and what clock it reads: the ONE
//! seam between the engine and its executor.
//!
//! On every native target this module is a set of re-exports and nothing else:
//! [`Runtime`] IS `tokio::runtime::Handle`, [`time`] IS `tokio::time`, and
//! [`Instant`] IS `std::time::Instant`, so the desktop build compiles the exact
//! code it did before the seam existed (start-paused test clocks included).
//!
//! The browser build (`wasm32`, for `crates/cmdr-listing-wasm`) swaps all three
//! (`wasm.rs`): a browser has no threads to put a tokio runtime on, and
//! `std::time::Instant::now` panics on `wasm32-unknown-unknown`. Why this shape
//! and not another: `DETAILS.md` § "The browser build".
//!
//! ❌ Engine code never names `tokio::runtime::Handle`, `tokio::time`, or
//! `std::time::Instant` directly: it goes through here, or the browser build
//! panics at runtime.

#[cfg(not(target_family = "wasm"))]
pub(crate) use std::time::Instant;
#[cfg(not(target_family = "wasm"))]
pub(crate) use tokio::runtime::Handle as Runtime;
#[cfg(not(target_family = "wasm"))]
pub(crate) use tokio::time;

#[cfg(target_family = "wasm")]
mod wasm;
#[cfg(target_family = "wasm")]
pub(crate) use wasm::Runtime;
#[cfg(target_family = "wasm")]
pub(crate) use wasm::{Instant, time};

/// Where an engine nobody handed a runtime spawns: `cmdr-fs`'s shared fallback.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn default_runtime() -> Runtime {
    cmdr_fs::volume::host::VolumeHost::detached().runtime()
}

/// The browser build's only executor, the page's event loop.
#[cfg(target_family = "wasm")]
pub(crate) fn default_runtime() -> Runtime {
    Runtime::default()
}
