// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// `unused_crate_dependencies` can't go with them: it's judged per compilation
// unit, so it belongs on the lib alone. `missing_docs` and `unreachable_pub` are
// this crate's own contract, as in every crate whose surface is capped.
#![warn(unused_crate_dependencies)]
#![warn(unreachable_pub)]
#![deny(missing_docs)]
// `clippy.toml` refuses direct clock and timer reads (the browser build traps on
// them); a test drives its own clocks and threads, as a host does.
#![cfg_attr(
    test,
    allow(
        clippy::disallowed_methods,
        reason = "tests drive their own clocks, timers, and threads, as a host would"
    )
)]

//! The operation log: a durable, cross-volume journal of every file mutation,
//! the foundation for search, rollback, and a future undo.
//!
//! The DURABLE STORE (the schema, the forward-migration ladder, the single
//! writer thread, and dir interning), the capture layer the write engine
//! journals through, the rollback PLANNER, and the read API, behind one
//! [`OperationLog`] handle the host builds once. It never names its host: the
//! data dir, the settings it reads, the runtime it prunes on, and the volume
//! registry the planner looks volumes up in all arrive as values or traits.
//!
//! Unlike every other on-disk store in the app (the drive index and
//! `importance.db` are disposable per-volume caches that delete-and-recreate on
//! a schema bump), this DB lives for years, so it introduces the codebase's
//! first forward-migration ladder (`store`'s migration ladder) and retention
//! discipline. Design rationale, the migration-ladder template, and the capture/
//! rollback contracts: `CLAUDE.md` + `DETAILS.md`.

//noinspection RsUnusedImport
// We dev-depend on ourselves so the `testing` feature is on for dev targets and
// off for the lib (see `Cargo.toml`). That makes `cmdr_operation_log` an extern
// crate of its own test target, which `unused_crate_dependencies` reports.
#[cfg(test)]
use cmdr_operation_log as _;

pub mod capture;
pub mod query;
pub mod retention;
pub mod rollback;
pub mod store;
pub mod types;
pub mod writer;

mod handle;
mod recording;
mod rt;

pub use handle::{OpJournal, OpenError, OperationLog};

use std::time::UNIX_EPOCH;

/// Seconds since the Unix epoch — the journal's opaque clock (the store owns no
/// clock, so callers supply the time). Shared by the write pipeline's capture glue
/// and the rollback engine.
pub(crate) fn now_secs() -> i64 {
    rt::wall_now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Mint an operation id.
///
/// **UUIDv7, never v4, and that is load-bearing.** This clock is whole seconds,
/// so two operations started in the same second tie on `started_at` and every
/// ordered read falls back to `op_id` — the history feed, the retention
/// watermark, and the startup reconcile's "which inverse op is the latest one".
/// A v7's leading 48 bits are a big-endian millisecond timestamp, so its hex
/// rendering sorts chronologically and that fallback becomes the right answer
/// instead of a coin flip. A v4 id makes the same-second order random: history
/// showing a reversal above the operation it reversed, and a reconcile that can
/// read the WRONG inverse op's outcomes.
///
/// Every managed operation, and every inverse operation, takes its id from here.
pub fn new_operation_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
