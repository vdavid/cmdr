//! Image-ML enrichment: makes a volume's images searchable by their content. M1
//! ships the plumbing + OCR-text search (no model download, no vector math): a
//! per-volume disposable `media.db`, a scheduler driven by the indexing lifecycle
//! bus, a fake-backed OCR pipeline behind the [`VisionBackend`](backend::VisionBackend)
//! seam, deletion-driven GC, and the [`MediaIndex`](read::MediaIndex) read API.
//!
//! A deliberate port of `importance/` (store, scheduler, read API); see
//! [`CLAUDE.md`](CLAUDE.md) for the must-knows and [`DETAILS.md`](DETAILS.md) for the
//! port rationale, the GC safety argument, and the schema. Full plan:
//! `docs/specs/media-ml-index-plan.md`.

pub mod backend;
pub mod gate;
pub mod predicate;
pub mod read;
pub mod scheduler;
pub mod store;
pub mod writer;
pub mod writer_registry;

pub use read::{MediaIndex, OcrHit};
