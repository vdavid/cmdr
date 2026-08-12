//! Image-ML enrichment: makes a volume's images searchable by their content. The OCR slice
//! ships the plumbing + OCR-text search (no model download, no vector math): a
//! per-volume disposable `media.db`, a scheduler driven by the indexing lifecycle
//! bus, an OCR pipeline behind the [`VisionBackend`](backend::VisionBackend) seam
//! (real macOS Vision OCR in production, a fake for tests), deletion-driven GC, and
//! the [`MediaIndex`] read API surfaced over the
//! `media_index_search_ocr` command.
//!
//! A deliberate port of `importance/` (store, scheduler, read API); see
//! [`CLAUDE.md`](CLAUDE.md) for the must-knows and [`DETAILS.md`](DETAILS.md) for the
//! port rationale, the GC safety argument, and the schema. Full plan:
//! `docs/specs/later/media-ml-index-plan.md`.

pub(crate) mod ann;
pub(crate) mod backend;
pub mod clip;
pub mod coverage;
pub mod events;
pub mod gate;
pub mod network;
pub(crate) mod paths;
pub mod predicate;
pub(crate) mod progress;
pub mod read;
pub mod scheduler;
pub mod store;
pub(crate) mod thermal;
pub mod vector;
pub(crate) mod writer;
pub(crate) mod writer_registry;

/// The subsystem's test-only surface. ❌ Not part of the API.
#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub mod testing;

pub use clip::ClipError;
pub use read::{MediaIndex, OcrHit};
pub use store::MediaStoreError;
