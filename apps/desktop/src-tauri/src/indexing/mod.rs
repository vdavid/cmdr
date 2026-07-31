//! Drive indexing module.
//!
//! Background-indexes local volumes into a per-volume SQLite database,
//! tracking every file and directory with recursive size aggregates.
//! Design history is in git (former `docs/specs/drive-indexing/`).
//!
//! [`Index`] is the public API: the app builds one, holds it, and calls methods on
//! it. This file re-exports that handle plus the vocabulary its signatures are
//! written in, and nothing else; `handle/DETAILS.md` records the item-by-item
//! audit that decided what a `pub` here means.
//!
//! The state machine (the global `INDEX_REGISTRY` mutex, `IndexPhase` enum, phase
//! transitions, and the `IndexManager` + `ReadPool` bootstrap) lives in
//! [`lifecycle::state`].

// Area modules. Cross-area references use each module's real path
// (`indexing::lifecycle::state::…`, `indexing::paths::routing::…`); `mod.rs` re-exports only
// the curated public item surface below, never a module alias that would hide where code lives.
pub(crate) mod aggregator;
mod events;
/// The public API: the [`Index`] handle and everything you can ask it.
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
pub(crate) mod watch;
pub(crate) mod writer;

/// The index's test-only surface. ❌ Not part of the API; see the module docs.
#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub mod testing;

/// The allocation-counting harness behind the memory-shape guards. `cfg(test)` because it
/// installs a `#[global_allocator]`, which is per binary.
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
pub(crate) use tests::stress_test_helpers;

// ── The public API ───────────────────────────────────────────────
//
// Everything below is a promise. A `pub` here says "the host may rely on this
// forever"; anything the index merely happens to have is `pub(crate)`, in the
// second block. The audit that decided each one, item by item, is in
// `DETAILS.md` § "The public surface".

/// The handle, what you can ask it, and what it fails with.
pub use handle::{
    Index, IndexBuildError, IndexBuilder, IndexError, IngestError, ListingAgreement, ListingObservation, ObservedEntry,
    SizeError, SizeFreshness, SizeProgress, SizeRequest, SizeStream, SizeVerdict, StartOutcome, WatchGap, WatchScope,
};

/// What the index reports while it works, and the sink it reports through. The
/// host maps these to its own wire format; the index produces no user-facing
/// words. Named rather than globbed, so the surface is readable from this file.
pub use events::{
    ActivityPhase, Diagnostic, EventSink, IndexDebugStatusResponse, IndexErrorReport, IndexEvent, IndexEventKind,
    IndexStatusResponse, MemoryWatchdogAction, NoopEventSink, PhaseRecord, RescanReason, ScanRunKind,
    VolumeIndexStatus,
};

/// The vocabulary the handle's own signatures are written in.
pub use aggregator::AggregationPhase;
pub use lifecycle::freshness::Freshness;
pub use lifecycle::state::{IndexVolumeKind, ROOT_VOLUME_ID};
pub use read::enrichment::ReadPool;
pub use read::expected_totals::ExpectedTotals;
pub use scanner::SYSTEM_DIR_EXCLUDES;
pub use store::IndexFailure;
pub use transports::smb::index::SmbIndexGateReason;

// ── Internal convenience ─────────────────────────────────────────
//
// Short paths for the index's own areas. Not API: none of this survives the
// crate boundary, and a new one here is a maintenance choice, not a promise.

pub(crate) use events::DEBUG_STATS;
#[cfg(test)]
pub(crate) use events::one_of_every_kind;
pub(crate) use lifecycle::failure::IndexFailureSignal;
pub(crate) use paths::routing::IndexPathSpace;
pub(crate) use read::enrichment::get_read_pool_for;
// `pub` under `testing` so `benches/index_benchmarks.rs` can install a synthetic
// index DB; see the items' docs in `read/enrichment.rs`.
#[cfg(any(test, feature = "testing"))]
pub use read::enrichment::{test_install_root_read_pool, test_read_pool_lock, test_uninstall_root_read_pool};
