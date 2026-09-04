//! Transfer operations: copy and move, both local-FS and volume-aware.
//!
//! All transfer entry points use the shared driver in [`transfer_driver`] and
//! the same `OperationEventSink` plumbing. Public symbols are re-exported up
//! to `super::*` so external callers keep using
//! `crate::file_system::write_operations::<symbol>` paths.
//!
//! See `CLAUDE.md` in this directory for copy + move semantics, conflict
//! resolution, transfer driver design, platform-specific copy backends, and
//! volume-aware copy/move details.

pub(super) mod checkpoint_stream;
pub(super) mod chunked_copy;
pub(super) mod copy;
pub(super) mod copy_strategy;
pub(super) mod dest_name_index;
#[cfg(target_os = "linux")]
pub(super) mod linux_copy;
#[cfg(target_os = "macos")]
pub(crate) mod macos_copy;
pub(super) mod move_op;
pub(super) mod retry;
pub(super) mod staged_write;
pub(super) mod transfer_driver;
pub(super) mod transfer_probe;
/// Cross-volume copy + move. A facade: `volume/mod.rs` re-exports what outside
/// code calls, and every module under it is private to that directory.
pub(super) mod volume;

// Re-export for the nested integration tests below (and to mirror the
// pre-split `write_operations::CopyTransaction` test path).
#[cfg(test)]
#[allow(unused_imports, reason = "used by transaction_integration_test")]
pub(crate) use super::ledger::CopyTransaction;

#[cfg(test)]
pub(crate) mod conflict_responder_test_support;
#[cfg(test)]
mod copy_integration_test;
#[cfg(test)]
mod hardlink_progress_tests;
#[cfg(test)]
pub(crate) mod liveness_test_support;
#[cfg(test)]
mod move_integration_test;
#[cfg(test)]
mod self_collision_tests;
#[cfg(test)]
mod transaction_integration_test;
#[cfg(test)]
mod type_mismatch_rename_tests;
