// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
//
// These two can't go with them. `unused_crate_dependencies` is judged per
// compilation unit, so it belongs on the lib target alone. `missing_docs` is
// per-crate: every type here crosses IPC, and its doc comment lands in
// `bindings.ts`.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! "Tell me about this": the facts Cmdr can state about a file or folder, and
//! every rule a person could argue about.
//!
//! Where an item came from, what it really is, who owns it, how it's used, and
//! what removing it costs. This crate owns the vocabulary ([`fact`], [`payload`],
//! [`owner`], [`digest`]), the pure rules ([`provenance`], [`sniff`], [`removal`],
//! [`headline`], [`card::plan`]), the one projection of a card for a model
//! ([`model`]), and the engine that gathers answers off the caller's thread
//! ([`gather`]). The host only reads raw inputs (an xattr's
//! bytes, a file's first kilobytes, a database row) and hands them here.
//!
//! No fact depends on a model, and nothing here reaches for an app: no `tauri`,
//! no filesystem call, no OS `cfg`. Every decoder takes bytes and runs the same
//! on every platform.
//!
//! ❌ **No user-facing words are produced here.** Facts, reasons, the headline,
//! and the removal row are typed parts; the frontend words them.

// The self dev-dependency only turns the `fuzzing` feature on for this crate's own
// test builds.
#[cfg(test)]
use cmdr_about as _;

pub mod actions;
pub mod card;
pub mod digest;
pub mod fact;
#[cfg(feature = "fuzzing")]
pub mod fuzzing;
pub mod gather;
pub mod headline;
pub mod model;
pub mod owner;
pub mod payload;
pub mod provenance;
pub mod removal;
pub mod sniff;
#[cfg(test)]
mod test_support;
pub mod text;
