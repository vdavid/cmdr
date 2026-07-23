//! Image-ML enrichment: makes a volume's images searchable by their content. The OCR slice
//! ships the plumbing + OCR-text search (no model download, no vector math): a
//! per-volume disposable `media.db`, a scheduler driven by the indexing lifecycle
//! bus, an OCR pipeline behind the [`VisionBackend`](backend::VisionBackend) seam
//! (real macOS Vision OCR in production, a fake for tests), deletion-driven GC, and
//! the [`MediaIndex`](read::MediaIndex) read API surfaced over the
//! `media_index_search_ocr` command.
//!
//! A deliberate port of `importance/` (store, scheduler, read API); see
//! [`CLAUDE.md`](CLAUDE.md) for the must-knows and [`DETAILS.md`](DETAILS.md) for the
//! port rationale, the GC safety argument, and the schema. Full plan:
//! `docs/specs/media-ml-index-plan.md`.

pub mod backend;
pub mod clip;
pub mod commands;
pub mod coverage;
pub mod events;
pub mod gate;
pub mod network;
pub mod predicate;
pub(crate) mod progress;
pub mod read;
pub mod scheduler;
pub mod store;
pub mod thermal;
pub mod vector;
pub mod writer;
pub mod writer_registry;

pub use read::{MediaIndex, OcrHit};
