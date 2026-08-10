// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// It's there rather than here so every crate we wrote shares one definition.
//
// These two can't go with them. `unused_crate_dependencies` is judged per
// compilation unit, so as a package-wide flag every test and bench would report
// "unused extern crate" for deps only the lib uses; it belongs on the lib target
// alone. `missing_docs` is deliberately per-crate: the app doesn't hold itself to
// it, and this crate's API is a deliverable rather than a side effect.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Cmdr's index: what's on the user's volumes, what's inside their images, and
//! which of their folders matter.
//!
//! Three subsystems behind one handle. [`Index`] is the whole API: build one,
//! hold it, call methods on it. Nothing here reaches for an application around
//! it; everything it needs from a host arrives through the traits in [`host`],
//! and everything it reports leaves through an [`EventSink`]. That's what lets
//! the same code run under Cmdr, under a test with an `InMemoryVolume`, and
//! under a bench with no host at all.
//!
//! - **The file index** (`indexing/`): scans volumes into per-volume SQLite
//!   databases, keeps them fresh against filesystem events, and answers recursive
//!   size and freshness questions. The handle's own module docs and
//!   `indexing/handle/DETAILS.md` record the item-by-item audit behind every
//!   `pub` below.
//! - **[`media_index`]**: OCR, image classification, and CLIP embeddings over the
//!   images the file index found, with ANN search on top.
//! - **[`importance`]**: a deterministic, cheap "which folders matter" score that
//!   expensive features (the agent, media enrichment) consult before spending.
//!
//! ❌ **No user-facing words are produced here.** The index emits typed values;
//! the host renders every string a human reads. Diagnostic text for `log::` is
//! fine and stays English.

// The three subsystems. `indexing` is private: everything it promises is
// re-exported below, so this file is the one place that answers "what may a host
// rely on?". The other two carry their own curated surfaces (see their `mod.rs`),
// so they're public under their own names rather than flattened into this one.
mod indexing;

pub mod importance;
pub mod media_index;

//noinspection RsUnusedImport
// We dev-depend on ourselves so the `testing` and `tooling` features are on for
// dev targets and off for the lib (see `Cargo.toml`). That makes `cmdr_index` an
// extern crate of its own test target, which `unused_crate_dependencies` reports.
#[cfg(test)]
use cmdr_index as _;
//noinspection RsUnusedImport
// Used from module-local `mod proptests` blocks, which a partial test build may
// not compile; the marker keeps the lint quiet either way.
#[cfg(test)]
use proptest as _;
//noinspection RsUnusedImport
// Used by `benches/`, a separate compilation unit the lint can't see from here.
#[cfg(test)]
use criterion as _;
//noinspection RsUnusedImport
// Used by a concurrency stress test when `RUST_LOG` wire traces are switched on.
#[cfg(test)]
use env_logger as _;

// ── The public API ───────────────────────────────────────────────
//
// Everything below is a promise. A `pub` here says "a host may rely on this
// forever"; anything the index merely happens to have stays `pub(crate)` behind
// the module wall. The audit that decided each one, item by item, is in
// `indexing/handle/DETAILS.md` § "The public surface".

// Two whole modules, each documented where it's declared: `host` (what the index
// asks a host for) and `store` (the index database's schema vocabulary, wide on
// purpose for the query layers that read one directly).
pub use indexing::{host, store};

/// The handle, what you can ask it, and what it fails with.
pub use indexing::handle::{
    Index, IndexBuildError, IndexBuilder, IndexError, IngestError, ListingAgreement, ListingObservation, ObservedEntry,
    SizeError, SizeFreshness, SizeProgress, SizeRequest, SizeStream, SizeVerdict, StartOutcome, WatchGap, WatchScope,
};

/// What the index reports while it works, and the sink it reports through. The
/// host maps these to its own wire format; the index produces no user-facing
/// words. Named rather than globbed, so the surface is readable from this file.
pub use indexing::events::{
    ActivityPhase, Diagnostic, EventSink, IndexDebugStatusResponse, IndexErrorReport, IndexEvent, IndexEventKind,
    IndexStatusResponse, MemoryWatchdogAction, NoopEventSink, PhaseRecord, RescanReason, ScanRunKind,
    VolumeIndexStatus,
};

/// The vocabulary the handle's own signatures are written in.
pub use indexing::aggregator::AggregationPhase;
pub use indexing::lifecycle::cover::{CoverOutcome, CoverWalk};
pub use indexing::lifecycle::freshness::Freshness;
pub use indexing::read::coverage::{CoverageDimension, CoverageMap, CoverageToken};
pub use indexing::read::enrichment::ReadPool;
pub use indexing::read::expected_totals::ExpectedTotals;
pub use indexing::resources::retention::sweep_legacy_scheme_dbs;
pub use indexing::scanner::CoveredEntry;
pub use indexing::scanner::SYSTEM_DIR_EXCLUDES;
pub use indexing::store::IndexFailure;
pub use indexing::transports::smb::index::SmbIndexGateReason;
pub use indexing::volume::{IndexVolumeKind, ROOT_VOLUME_ID};

/// The file index's test-only surface. ❌ Not part of the API; see the module docs.
#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub use indexing::testing;

// `pub` under `testing` so `benches/index_benchmarks.rs` — an external crate —
// can install a synthetic index DB; see the items' docs in `read/enrichment.rs`.
#[cfg(any(test, feature = "testing"))]
pub use indexing::read::enrichment::{test_install_root_read_pool, test_read_pool_lock, test_uninstall_root_read_pool};
