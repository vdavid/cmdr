//! Folder-importance subsystem: a deterministic, cheap score of "which folders
//! matter" that any expensive feature can consume (the in-app agent, the media-ML
//! enrichment scheduler, future disk-cleanup / prefetch features).
//!
//! M1 ships the pure heart of it: the [`scorer`] (values-in / score-out, no I/O)
//! and its tunable [`Weights`]. Storage, the lifecycle bus, the scheduler, and the
//! read API land in later milestones (see `docs/specs/importance-subsystem-plan.md`).
//!
//! Read [`DETAILS.md`](DETAILS.md) before working here: the signal catalog, the
//! formula shape, and the tunable-weights rationale.

pub(crate) mod classify;
pub mod commands;
pub mod evals;
mod last_used;
pub mod read;
pub mod scheduler;
pub mod scorer;
pub mod signals;
pub mod store;
pub mod writer;
pub mod writer_registry;

#[cfg(test)]
mod fixtures;

pub use read::{FloorReason, ImportanceIndex, ScoredWeight, WeightLookup};
pub(crate) use scheduler::signal_availability;
pub use scorer::{
    Explanation, FolderSignals, PathClass, Score, SignalContribution, SignalKind, SignalSet, Weights, explain, score,
};
