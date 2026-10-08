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
mod displaced_destination;
// Both carry their own `//!` headers. ❌ No outer `///` here: rustdoc
// concatenates it with the child's header and resolves the merged doc in THIS
// scope, so the child's links to its own items break.
mod finalize;
mod folder_dates;
mod item_identity;
/// What mode a file lands with on a LOCAL destination. The volumes report a
/// mode; this is the layer that applies it.
mod landed_mode;
mod landing;
mod merge;
mod merge_ctx;
/// `move` is a Rust keyword, so the module is `r#move`. Nothing outside this
/// facade names it: the move entry points are re-exported below.
mod r#move;
/// The cross-volume copy-then-delete engine `r#move` dispatches to. Split out of
/// it so the dispatcher and the engine read as the two decisions they are.
mod move_cross;
mod move_file;
mod move_same;
mod naming;
mod preflight;
mod rename_merge;
mod sequential_extract;
mod server_side_copy;
mod source_sweep;
mod strategy;
mod transfer_error;

// The public surface. Everything else in here is an implementation detail of
// `volume/`; add a re-export rather than widening a submodule's visibility.
pub use copy::copy_between_volumes;
pub use r#move::move_between_volumes;

pub(crate) use copy::copy_volumes_with_progress;
pub(in crate::file_system::write_operations) use displaced_destination::displace_destination;
/// The cross-volume copy body, reused as the extract phase of an out-of-zip
/// move (`archive_edit`).
pub(crate) use item_identity::is_the_same_item;
/// Move ONE file across two volumes, staged and mid-file cancelable, with no
/// driver above it (the operation-log rollback's cross-volume restore).
pub(in crate::file_system::write_operations) use move_file::move_file_across_volumes;
/// A move's source sweep, for an into-zip move: stamp what it carries before
/// reading it, then remove exactly that once the archive commits.
pub(in crate::file_system::write_operations) use source_sweep::{CarriedSource, stamp_source, sweep_carried_source};
/// Pull a remote path down to a local scratch copy (remote zip edits).
pub(in crate::file_system::write_operations) use strategy::pull_path_to_local;
/// The same refusal for a source, reachable from outside the engine.
pub(crate) use transfer_error::unregistered_source_error;
/// The refusal for a volume id the registry had nothing for, shared by the
/// transfer routing and the volume delete so both name an unconnected phone or
/// server the same way.
pub(in crate::file_system::write_operations) use transfer_error::unregistered_volume_error;
/// The one place a `VolumeError` becomes a typed `WriteOperationError`; the
/// delete walker maps its own volume failures through it too.
pub(in crate::file_system::write_operations) use transfer_error::{PathRole, map_finalize_failure, map_volume_error};

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
// The network-server scenarios (`write_operations/network_*_test_support.rs`)
// assert through the same oracle, so a live backend is held to the words the
// in-memory grid is.
#[cfg(test)]
#[allow(unused_imports, reason = "used by the network scenario suites outside this facade")]
pub(crate) use safety_oracle::{SafetySpec, assert_operation_was_safe};

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
// The forwarding macro, for a double outside this facade that lies about one
// method of a LIVE volume (`network_gated_source_test_support.rs`'s gated reads).
#[cfg(test)]
#[allow(unused_imports, reason = "used by the network scenario suites outside this facade")]
pub(crate) use faulty_volume::forward_volume_methods;

/// Duplicating in place at the volume seam: both engines, plus the folded-leaf
/// identity rule that stands in for `dev+ino` out here.
#[cfg(test)]
mod self_collision_tests;

/// Fixtures every same-volume rename-merge suite starts from.
#[cfg(test)]
mod rename_merge_test_support;

/// Where the new bytes go when a safe-replace finalize can't land them.
#[cfg(test)]
mod finalize_recovery_tests;

/// What each engine does when the destination won't say whether a name is taken.
#[cfg(test)]
mod dest_precheck_failure_tests;

/// A copied folder keeps its source folder's date, dated after its contents.
#[cfg(test)]
mod folder_dates_tests;

/// A listed name that isn't one plain path component (`../x`, `/x`) never
/// lands outside the destination.
#[cfg(test)]
mod hostile_names_tests;
/// The same rule on a backend whose `is_directory` follows a link (ADB, SFTP).
#[cfg(test)]
mod link_following_backend_tests;
#[cfg(test)]
mod preflight_stop_tests;
#[cfg(test)]
mod rename_merge_cancel_tests;
#[cfg(test)]
mod rename_merge_case_fold_tests;
#[cfg(test)]
mod rename_merge_mtp_tests;
#[cfg(test)]
mod rename_merge_pause_tests;
#[cfg(test)]
mod rename_merge_stat_tests;
/// A symlink is an opaque entry to the merge, never a directory to descend.
#[cfg(test)]
mod rename_merge_symlink_tests;
#[cfg(test)]
mod rename_merge_tests;
#[cfg(test)]
mod rename_merge_walk_tests;
