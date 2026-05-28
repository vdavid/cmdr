//! Downloads-watcher primitives.
//!
//! Pure-Rust data structures and filters that back the `~/Downloads` watcher.
//! No `notify`, no Tauri, no IPC — just the deciders.
//!
//! Three pieces:
//!
//! - [`is_eligible`] decides whether a path looks like a "real" download we
//!   should surface (not hidden, not a partial download, not a directory).
//! - [`IgnoreSet`] holds paths that Cmdr itself just wrote, so the watcher can
//!   suppress its own events for a short TTL. Bounded with FIFO eviction so
//!   it can't grow unbounded if events never arrive.
//! - [`LatestRing`] keeps the last ~10 observed downloads in insertion order
//!   so the "reveal latest" action has an O(1) answer without a directory
//!   scan.
//!
//! The watcher loop (M2b) glues these together with `notify`. The shape of
//! the feature and the rationale for each primitive lives in
//! `docs/specs/downloads-watcher-plan.md`.

mod filter;
mod ignore_set;
mod latest_ring;

// M2a has no callers yet; M2b's watcher consumes these. Suppress the
// unused-import lint until then so the crate's `#![deny(unused)]` doesn't
// fire on a milestone-of-one.
#[allow(unused_imports, reason = "M2a has no callers; M2b's watcher consumes these")]
pub use filter::is_eligible;
#[allow(unused_imports, reason = "M2a has no callers; M2b's watcher consumes these")]
pub use ignore_set::IgnoreSet;
#[allow(unused_imports, reason = "M2a has no callers; M2b's watcher consumes these")]
pub use latest_ring::LatestRing;
