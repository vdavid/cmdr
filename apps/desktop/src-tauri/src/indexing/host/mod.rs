//! The seams through which the index subsystems reach their host.
//!
//! `indexing/`, `media_index/`, and `importance/` are being extracted into a
//! Tauri-free crate, so anything they need from the surrounding application has to
//! arrive through a named seam rather than a `crate::`-qualified reach upward. This
//! module is the whole list of them, in one place, so "what does the index need from
//! its host?" has a readable answer.
//!
//! Each seam is injected once at startup and read through a `pub(crate)` accessor.
//! The accessors resolve to process-wide statics for now; when the public `Index`
//! handle lands, the handle owns them and the statics disappear without touching a
//! single call site.
//!
//! - [`runtime`]: the tokio runtime background work spawns onto.
//!
//! Rationale, including why the runtime is injected rather than crate-owned:
//! `DETAILS.md`.

pub(crate) mod runtime;
