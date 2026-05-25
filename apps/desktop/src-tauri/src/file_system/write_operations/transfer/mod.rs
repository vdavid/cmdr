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

pub(super) mod chunked_copy;
pub(super) mod copy;
pub(super) mod copy_strategy;
#[cfg(target_os = "linux")]
pub(super) mod linux_copy;
#[cfg(target_os = "macos")]
pub(crate) mod macos_copy;
pub(super) mod move_op;
pub(super) mod transfer_driver;
pub(super) mod volume_conflict;
pub(super) mod volume_copy;
pub(super) mod volume_move;
pub(super) mod volume_preflight;
pub(super) mod volume_strategy;

// Re-export for the nested integration tests below (and to mirror the
// pre-split `write_operations::CopyTransaction` test path).
#[cfg(test)]
#[allow(unused_imports, reason = "used by transaction_integration_test")]
pub(crate) use super::state::CopyTransaction;

#[cfg(test)]
mod copy_integration_test;
#[cfg(test)]
mod move_integration_test;
#[cfg(test)]
mod transaction_integration_test;
