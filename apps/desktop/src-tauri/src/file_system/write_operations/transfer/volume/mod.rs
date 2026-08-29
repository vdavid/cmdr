//! Cross-volume transfer: copy and move across Local ↔ MTP ↔ SMB ↔ archive
//! backends, with the shared merge / conflict / staging engine underneath.
//!
//! This module is a **facade**. Every submodule below is private to `volume`,
//! and the `use` block re-exports the handful of items outside code actually
//! calls, so a caller writes `transfer::volume::move_between_volumes` and never
//! names a submodule. That keeps the merge engine, the conflict resolver, and
//! the staging plumbing free to move around inside this directory, and it is
//! why `move.rs` can be `r#move` without the keyword escape leaking anywhere.
//!
//! Semantics, flows, and decisions: `../DETAILS.md` § "Volume copy + move".
//! Must-know invariants: `../CLAUDE.md`.

mod cleanup;
mod conflict;
mod copy;
mod copy_concurrent;
mod copy_concurrent_source;
mod copy_concurrent_task;
mod copy_serial;
mod merge;
/// `move` is a Rust keyword, so the module is `r#move`. Nothing outside this
/// facade names it: the move entry points are re-exported below.
mod r#move;
mod move_file;
mod move_same;
mod naming;
mod preflight;
mod rename_merge;
mod sequential_extract;
mod strategy;
mod transfer_error;

// The public surface. Everything else in here is an implementation detail of
// `volume/`; add a re-export rather than widening a submodule's visibility.
pub use copy::{copy_between_volumes, scan_for_volume_copy};
pub use r#move::move_between_volumes;

/// The recursive source sweep a zip copy-into needs after pulling a subtree,
/// plus the enum every caller names its authorization with.
pub(in crate::file_system::write_operations) use cleanup::{TreeRemoval, remove_tree};
/// The cross-volume copy body, reused as the extract phase of an out-of-zip
/// move (`archive_edit`).
pub(crate) use conflict::is_the_same_item;
pub(crate) use copy::copy_volumes_with_progress;
/// Move ONE file across two volumes, staged and mid-file cancelable, with no
/// driver above it (the operation-log rollback's cross-volume restore).
pub(in crate::file_system::write_operations) use move_file::move_file_across_volumes;
/// Pull a remote path down to a local scratch copy (remote zip edits).
pub(in crate::file_system::write_operations) use strategy::pull_path_to_local;
/// The one place a `VolumeError` becomes a typed `WriteOperationError`; the
/// delete walker maps its own volume failures through it too.
pub(in crate::file_system::write_operations) use transfer_error::{PathRole, map_volume_error};

// Driven directly by the SMB/MTP integration suites and the volume-journal
// capture tests, which bypass the Tauri command layer.
#[cfg(test)]
#[allow(unused_imports, reason = "used by integration suites outside write_operations")]
pub(crate) use r#move::move_volumes_with_progress;
#[cfg(test)]
#[allow(unused_imports, reason = "used by integration suites outside write_operations")]
pub(crate) use move_same::move_within_same_volume_with_progress;

/// The one statement of what a finished operation must have left behind, shared
/// by the copy matrix, the move matrix, and the coverage grid.
#[cfg(test)]
mod safety_oracle;

/// The shared fault injector (`FaultyVolume`) and the forwarding macro every
/// `Volume` double in this directory builds on.
#[cfg(test)]
#[path = "faulty_volume_test_support.rs"]
mod faulty_volume;

/// The coverage grid: op × cache state × outcome, and the shape axis.
#[cfg(test)]
mod safety_grid_tests;

// The SMB integration suite lives outside `write_operations`, so it reaches the
// fault injector through the facade rather than by widening the submodule.
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "used by SMB integration tests in file_system::volume::backends"
)]
pub(crate) use faulty_volume::{FaultyOp, FaultyVolume};

/// Duplicating in place at the volume seam: both engines, plus the folded-leaf
/// identity rule that stands in for `dev+ino` out here.
#[cfg(test)]
mod self_collision_tests;

/// Fixtures every same-volume rename-merge suite starts from.
#[cfg(test)]
mod rename_merge_test_support;

#[cfg(test)]
mod rename_merge_cancel_tests;
#[cfg(test)]
mod rename_merge_case_fold_tests;
#[cfg(test)]
mod rename_merge_mtp_tests;
#[cfg(test)]
mod rename_merge_stat_tests;
#[cfg(test)]
mod rename_merge_tests;
#[cfg(test)]
mod rename_merge_walk_tests;
