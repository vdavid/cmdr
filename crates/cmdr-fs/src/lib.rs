//! Filesystem vocabulary and host primitives shared across Cmdr.
//!
//! This crate holds the leaf types every layer of Cmdr speaks in — the `Volume`
//! trait and the data it exchanges, `FileEntry`, the typed error classification —
//! plus the handful of host primitives (thread QoS, process memory, poison-free
//! locking) that a background worker needs and can't sensibly be injected.
//!
//! It knows nothing about Tauri, the index, or any storage backend. Real-storage
//! backends (local, SMB, MTP, archive) live in the app; the only implementation
//! here is [`volume::InMemoryVolume`], which needs no host at all.
//!
//! The lint set this crate is held to lives in the workspace root's
//! `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
//! `unused_crate_dependencies` can't go with them (it's judged per compilation
//! unit, so as a package-wide flag every test target would report unused externs
//! for deps only the lib uses), and `missing_docs` is set here because it's this
//! crate's own contract, not the workspace's.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

pub mod archive_format;
pub mod entry;
pub mod file_provider;
pub mod filesystem_kind;
pub mod firmlinks;
pub mod git_meta;
pub mod icons;
pub mod ignore_poison;
pub mod log_rollup;
pub mod path_locations;
pub mod pluralize;
pub mod process_memory;
pub mod sqlite_util;
pub mod staging;
pub mod tcc_paths;
pub mod thread_cpu;
pub mod thread_qos;
pub mod volume;

#[cfg(any(test, feature = "testing"))]
pub mod testing;
