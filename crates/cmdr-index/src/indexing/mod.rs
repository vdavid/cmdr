//! Drive indexing module.
//!
//! Background-indexes local volumes into a per-volume SQLite database,
//! tracking every file and directory with recursive size aggregates.
//! Design history is in git (former `docs/specs/drive-indexing/`).
//!
//! [`handle::Index`] is the public API: a host builds one, holds it, and calls
//! methods on it. The crate root re-exports that handle plus the vocabulary its
//! signatures are written in, and nothing else; `handle/DETAILS.md` records the
//! item-by-item audit that decided what a `pub` here means.
//!
//! The state machine (the global `INDEX_REGISTRY` mutex, `IndexPhase` enum, phase
//! transitions, and the `IndexManager` + `ReadPool` bootstrap) lives in
//! [`lifecycle::state`].

// Area modules. Cross-area references use each module's real path
// (`indexing::lifecycle::state::…`, `indexing::paths::routing::…`); `mod.rs` re-exports only
// the curated public item surface below, never a module alias that would hide where code lives.
pub(crate) mod aggregator;
pub(crate) mod events;
pub mod handle;
/// What the index asks its host for, as traits and values a host implements.
pub mod host;
pub(crate) mod lifecycle;
mod metadata;
pub(crate) mod network_scanner;
pub(crate) mod paths;
pub(crate) mod read;
pub(crate) mod reconcile;
pub(crate) mod resources;
pub(crate) mod scanner;
/// The index database: its schema vocabulary, for the query layers that read one
/// directly. See `handle/DETAILS.md` § "The two exceptions" for why this is wide.
pub mod store;
pub(crate) mod transports;
pub(crate) mod volume;
pub(crate) mod watch;
pub(crate) mod writer;

/// The index's test-only surface. ❌ Not part of the API; see the module docs.
#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub mod testing;

/// The shared source walk behind the guards that enumerate call sites (see the module doc for
/// when one of those is the right answer and when it isn't).
#[cfg(test)]
pub(crate) mod source_guard;
/// The allocation-counting harness behind the memory-shape guards. `cfg(test)` because it
/// installs a `#[global_allocator]`, which is per binary.
#[cfg(test)]
pub(crate) mod test_support;
// Wider than `cfg(test)` because of ONE item: the disk-image fixture, which a
// host-side test drives too (see `tests/mod.rs`). Everything else inside stays
// `cfg(test)`.
#[cfg(any(test, feature = "testing"))]
pub(crate) mod tests;
#[cfg(test)]
pub(crate) use tests::stress_test_helpers;

// ── Internal convenience ─────────────────────────────────────────
//
// Short paths for the index's own areas. Not API: the crate root decides what a
// host may rely on, and a new line here is a maintenance choice, not a promise.

pub(crate) use events::DEBUG_STATS;
pub(crate) use lifecycle::failure::IndexFailureSignal;
pub(crate) use paths::routing::IndexPathSpace;
pub(crate) use read::enrichment::get_read_pool_for;
