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
mod extract_quarantine;
// It carries its own `//!` header. ❌ No outer `///` here: rustdoc
// concatenates it with the child's header and resolves the merged doc in THIS
// scope, so the child's links to its own items break.
mod folder_metadata;
mod item_identity;
mod landed_flags;
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
mod source_reads;
mod source_sweep;
mod strategy;
mod stream_pipe;
mod transfer_error;

// The public surface. Everything else in here is an implementation detail of
// `volume/`; add a re-export rather than widening a submodule's visibility.
// The cross-volume starters are `Operations` methods.
pub use copy::copy_volumes_with_progress;
pub(crate) use displaced_destination::{DisplacedDestination, displace_destination};
/// The cross-volume copy body, reused as the extract phase of an out-of-zip
/// move (`archive_edit`).
pub(crate) use item_identity::is_the_same_item;
/// Move ONE file across two volumes, staged and mid-file cancelable, with no
/// driver above it (the operation-log rollback's cross-volume restore).
pub(crate) use move_file::move_file_across_volumes;
/// A move's source sweep, for an into-zip move: stamp what it carries before
/// reading it, then remove exactly that once the archive commits.
pub(crate) use source_sweep::{CarriedSource, stamp_source, sweep_carried_source};
/// Pull a remote path down to a local scratch copy (remote zip edits).
pub(crate) use strategy::pull_path_to_local;
/// The refusal for a volume id the registry had nothing for, shared by the
/// transfer routing and the volume delete so both name an unconnected phone or
/// server the same way.
pub(crate) use transfer_error::unregistered_volume_error;
/// The one place a `VolumeError` becomes a typed `WriteOperationError`; the
/// delete walker maps its own volume failures through it too.
pub(crate) use transfer_error::{PathRole, map_failure_after_wait, map_finalize_failure, map_volume_error};

// Driven directly by the SMB/MTP integration suites and the volume-journal
// capture tests, which bypass the Tauri command layer.
#[cfg(any(test, feature = "testing"))]
#[allow(unused_imports, reason = "used by integration suites outside the crate")]
pub use r#move::move_volumes_with_progress;
#[cfg(any(test, feature = "testing"))]
#[allow(unused_imports, reason = "used by integration suites outside the crate")]
pub use move_same::move_within_same_volume_with_progress;

/// The one statement of what a finished operation must have left behind, shared
/// by the copy matrix, the move matrix, and the coverage grid.
#[cfg(any(test, feature = "testing"))]
mod safety_oracle;
// The network-server scenarios (`crates/cmdr-operations/src/network_*_test_support.rs`)
// assert through the same oracle, so a live backend is held to the words the
// in-memory grid is.
#[cfg(any(test, feature = "testing"))]
#[allow(unused_imports, reason = "used by the network scenario suites outside this facade")]
pub use safety_oracle::{SafetySpec, assert_operation_was_safe};

/// The shared fault injector (`FaultyVolume`) and the forwarding macro every
/// `Volume` double in this directory builds on.
#[cfg(any(test, feature = "testing"))]
#[path = "faulty_volume_test_support.rs"]
mod faulty_volume;

/// The coverage grid: op × cache state × outcome, and the shape axis.
#[cfg(test)]
mod safety_grid_tests;

/// Every engine walk deletes a folder it walked as a folder (issue #510).
#[cfg(test)]
mod folder_delete_kind_tests;

// The SMB integration suite lives outside `cmdr-operations`, so it reaches the
// fault injector through the facade rather than by widening the submodule.
#[cfg(any(test, feature = "testing"))]
#[allow(
    unused_imports,
    reason = "used by SMB integration tests in file_system::volume::backends"
)]
pub use faulty_volume::{FaultyOp, FaultyVolume};
// The forwarding macro, for a double outside this facade that lies about one
// method of a LIVE volume (`network_gated_source_test_support.rs`'s gated reads).
#[cfg(any(test, feature = "testing"))]
#[allow(unused_imports, reason = "used by the network scenario suites outside this facade")]
pub(crate) use faulty_volume::forward_volume_methods;

/// Duplicating in place at the volume seam: both engines, plus the folded-leaf
/// identity rule that stands in for `dev+ino` out here.
#[cfg(test)]
mod self_collision_tests;

/// Fixtures every same-volume rename-merge suite starts from.
#[cfg(test)]
mod rename_merge_test_support;

/// What a safe-replace's swap does when the original won't go, or the new bytes
/// can't take its name.
#[cfg(test)]
mod swap_landing_tests;

/// A file→file Overwrite at each of the five sites a safe-replace lands, across
/// success and each refusal.
#[cfg(test)]
mod safe_replace_grid_tests;

/// The same Overwrite at the same five sites, with the share dropping inside
/// the swap: the landing waits, settles, and finishes or keeps the new bytes.
#[cfg(test)]
mod swap_outage_tests;

/// What each engine does when the destination won't say whether a name is taken.
#[cfg(test)]
mod dest_precheck_failure_tests;

/// A refusal deep in a folder transfer is named against the side that refused.
#[cfg(test)]
mod failure_side_tests;

/// A stopped transfer names the new files its stop kept at the destination.
#[cfg(test)]
mod kept_new_data_tests;

/// A source that doesn't deliver its declared length fails the file (#538).
#[cfg(test)]
mod source_length_tests;

/// A copied folder keeps its source folder's dates, stamped after its contents.
#[cfg(test)]
mod folder_metadata_tests;

/// A copy keeps Hidden and Locked, Locked only once each item has its final name.
#[cfg(test)]
mod copy_flags_tests;
/// A copy onto local disk keeps creation dates and a routed source's mode.
#[cfg(test)]
mod copy_metadata_tests;
/// A copy out of a local FAT folder leaves its AppleDouble sidecars behind.
#[cfg(test)]
mod copy_sidecar_tests;

/// Every backend pair keeps or reports each class: the loss note's matrix.
#[cfg(test)]
mod metadata_loss_tests;

/// A selected local link copied to another volume lands as what it points at.
#[cfg(test)]
mod copy_selected_link_tests;
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
