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
//! - [`policy`]: whether the user is busy, so background work can stand aside.
//! - [`volumes`]: which volumes are mounted, where, and what kind of storage they are.
//! - [`config`]: what the product tells the index to do, settings resolved by the app.
//! - [`events`]: where the index's typed events go.
//!
//! Cmdr answers all five from one place, `apps/desktop/src-tauri/src/index_host.rs`.
//!
//! Rationale, including why the runtime is injected rather than crate-owned:
//! `DETAILS.md`.

pub mod config;
pub mod events;
pub mod policy;
pub(crate) mod runtime;
pub mod volumes;
