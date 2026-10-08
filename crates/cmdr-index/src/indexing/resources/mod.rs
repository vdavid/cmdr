//! Process-wide resource governance for indexing: bounded memory and bounded
//! disk. These are a different concern from per-volume lifecycle (that's
//! `lifecycle/`): they cap the WHOLE indexing pool, not one volume.
//!
//! - [`memory_watchdog`]: the single global `phys_footprint` budget (warn at
//!   8 GB, stop ALL indexing at 16 GB). `memory_snapshot.rs` is the breakdown
//!   it logs when a threshold trips (macOS only).
//! - [`subsystem_stop`]: the stop-hook registry the watchdog runs alongside
//!   `stop_all_indexing`, so a second resident-pool subsystem yields to the SAME
//!   ceiling instead of standing up a second 16 GB budget.
//! - [`retention`]: the external-index-DB count cap with LRU eviction of
//!   offline drives.

#[cfg(target_os = "macos")]
mod memory_snapshot;
pub(crate) mod memory_watchdog;
pub(crate) mod retention;
pub(crate) mod subsystem_stop;
