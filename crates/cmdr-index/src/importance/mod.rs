//! Folder-importance subsystem: a deterministic, cheap score of "which folders
//! matter" that any expensive feature can consume (the in-app agent, the media-ML
//! enrichment scheduler, future disk-cleanup / prefetch features).
//!
//! At its heart is the [`scorer`] (values-in / score-out, no I/O) and its tunable
//! [`Weights`]; around it sit the per-volume store, the scheduler that rides
//! `indexing/`'s lifecycle bus, and the read API every consumer goes through.
//!
//! Read [`DETAILS.md`](DETAILS.md) before working here: the subsystem-wide rules
//! and the top-level files. Each area subdir documents itself.

pub(crate) mod classify;
/// The evaluation corpus and scoring harness. Compiled only for the developer
/// tools that drive it; reach it through [`tooling`], which documents why.
#[cfg(any(test, feature = "tooling"))]
#[doc(hidden)]
pub mod evals;
mod last_used;
pub mod read;
pub mod scheduler;
pub(crate) mod scorer;
pub(crate) mod signals;
pub(crate) mod store;
pub(crate) mod writer;
pub(crate) mod writer_registry;

/// The subsystem's test-only surface. ❌ Not part of the API.
#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub mod testing;

/// The subsystem's developer-tooling surface. ❌ Not part of the API.
#[cfg(any(test, feature = "tooling"))]
#[doc(hidden)]
pub mod tooling;

#[cfg(test)]
mod fixtures;

pub use read::{FloorReason, ImportanceIndex, ScoredWeight, WeightLookup};
pub use scheduler::{is_background_scored, signal_availability};
pub use scorer::{
    Explanation, FolderSignals, PathClass, Score, SignalContribution, SignalKind, SignalSet, Weights, explain, score,
};
pub use store::ImportanceStoreError;
