//! Shared per-source transfer driver for copy and move operations.
//!
//! # What this is
//!
//! After commit `32e6de03` (bulk-skip pre-known conflicts), four functions in
//! `write_operations/` ended up carrying the same scaffolding around different
//! transfer cores:
//!
//! - `volume_copy.rs::copy_volumes_with_progress` (cross-volume copy, async)
//! - `volume_move.rs::move_between_volumes` (cross-volume move, async)
//! - `volume_move.rs::move_within_same_volume` (same-volume rename, async)
//! - `copy.rs::copy_files_with_progress_inner` (local-FS copy, sync inside `spawn_blocking`)
//!
//! This driver owns that scaffolding once:
//!
//! - pre-known-conflicts bulk-skip prelude
//! - per-iter cancellation check (positioned BEFORE any destructive call)
//! - per-iter skip accounting (bumps `files_done` / `bytes_done` and emits throttled progress)
//! - post-loop bookkeeping (final progress event, completion event)
//! - paired `state.emit_progress_via_sink` + `update_operation_status` updates
//!
//! Each operation supplies a per-source `transfer_one` closure. The closure does
//! ONLY the per-source transfer work and reports back via `TransferOutcome`.
//!
//! # Data-safety contract
//!
//! The whole point of this abstraction is to enforce, in one place, that **the
//! `transfer_one` closure is NEVER invoked**:
//!
//! 1. for a source in the pre-known-conflicts bulk-skip set under `ConflictResolution::Skip`, nor
//! 2. after a top-level conflict resolution returned Skip (async driver only — the sync driver
//!    delegates this to the closure), nor
//! 3. after cancellation has been signaled on the operation state.
//!
//! The `transfer_driver_*_tests.rs` suites pin each of these properties so a
//! future refactor that violates the contract gets caught here, not by
//! structural inspection of four different functions.
//!
//! # Module layout
//!
//! - this file: the shared vocabulary (`TransferContext`, `TransferOutcome`, `TransferLoopOutcome`,
//!   `PostLoopIntent`, `DriverConfig`, `ConflictDecisionInput`, `ConflictDecision`) plus the
//!   `build_pre_skip_set` / `emit_progress_and_status` helpers.
//! - [`progress`]: the per-file progress callback builders (`SerialLeafProgress`,
//!   `make_concurrent_per_file_progress`).
//! - [`sync_driver`]: [`drive_transfer_serial_sync`] for local-FS copy/move.
//! - [`async_driver`]: [`drive_transfer_serial_async`] for the volume ops.
//!
//! # Sync vs async: two sibling entry points
//!
//! The four operations split cleanly along sync/async lines:
//!
//! - `copy_files_with_progress_inner` runs inside `tokio::task::spawn_blocking` and uses
//!   synchronous `std::fs` I/O. Its loop is sync, and the closure captures `&mut CopyTransaction` +
//!   `&mut HashSet<PathBuf>` + `&mut SourceItemTracker`.
//! - The three volume ops are async (they `await` on the `Volume` trait's `Pin<Box<dyn
//!   Future>>`-returning methods).
//!
//! Trying to be generic over sync/async via boxed futures would force every
//! sync caller through an unnecessary `Pin<Box<dyn Future>>` allocation per
//! source and lose the closure's `&mut` capture clarity. Two siblings with
//! shared types is cleaner. See [`drive_transfer_serial_sync`] (for local
//! copy/move) and [`drive_transfer_serial_async`] (for volume ops).
//!
//! # Conflict resolution: closure-owned for sync, driver-owned for async
//!
//! The two paths differ in where conflict resolution lives, and that split is
//! load-bearing:
//!
//! - **Sync driver (`drive_transfer_serial_sync`)**: the closure handles conflict resolution.
//!   Local-FS conflicts are discovered mid-flight inside `copy_single_item` (a parent directory
//!   might be a regular file blocking `create_dir_all`; that's a per-file conflict the driver can't
//!   pre-detect via `dest.get_metadata` on the top-level source). The driver only guarantees the
//!   bulk-skip prelude and the data-safety check ordering.
//! - **Async driver (`drive_transfer_serial_async`)**: the driver owns top-level conflict detection
//!   (`dest_volume.get_metadata` per source) and dispatch to `resolve_volume_conflict`. Volume
//!   conflict resolution is uniform (Stop/Skip/Overwrite/Rename, all at the top-level path) which
//!   is why the driver can host it.
//!
//! This asymmetry is the same one the production code has today; the driver
//! just makes it explicit and enforces the data-safety contract for both
//! shapes.
//!
//! # What the driver does NOT do
//!
//! - **Scan phase**: scanning, disk-space check, and source-hint construction are pre-loop concerns
//!   owned by the caller. They produce the inputs the driver needs (`total_files`, `total_bytes`,
//!   `pre_skip_paths`).
//! - **`SourceItemTracker`** (`write-source-item-done` emit): local-FS-only concern. The sync
//!   driver's closure threads `SourceItemTracker` through itself (it's `!Sync` and lives on the
//!   serial path only); volume ops don't emit this event today.
//! - **`CopyTransaction`** (rollback bookkeeping): local-FS-only. The closure captures `&mut
//!   transaction`; the driver never sees it. On a non-Ok `TransferLoopOutcome.intent`, the caller
//!   decides whether to invoke `rollback_with_progress`.
//! - **Concurrent path**: deliberately out of scope. `copy_volumes_with_progress` keeps its
//!   `FuturesUnordered` block inline (only 1 of the 4 ops needs concurrency, so abstracting it
//!   would be a 1-of-4 abstraction, not a shared pattern — see plan § "Concurrent driver scope").

// A handful of driver surface items aren't wired up by today's three callers
// (`TransferOutcome::Skipped`, `DriverConfig::{conflict_resolution,
// pre_known_conflicts}`, `ConflictDecisionInput::{source_is_directory_hint,
// source_size_hint}`, and the unused `TransferContext` fields). They're load-
// bearing for the driver's contract and exercised by the
// `transfer_driver_*_tests.rs` suites
// (`TransferOutcome::Skipped` is constructed in test closures; the config
// fields feed `build_pre_skip_set` audits). Keep them as part of the public-
// to-the-module surface so adding a future caller doesn't require widening the
// driver in a separate commit.
#![allow(
    dead_code,
    reason = "Driver surface kept stable for future callers; exercised by the transfer_driver_*_tests.rs suites"
)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::state::{WriteOperationState, update_operation_status};
use super::super::types::{
    ConflictResolution, OperationEventSink, WriteOperationError, WriteOperationPhase, WriteOperationType,
    WriteProgressEvent,
};

mod async_driver;
mod progress;
mod sync_driver;

pub(in crate::file_system::write_operations::transfer) use async_driver::drive_transfer_serial_async;
pub(in crate::file_system::write_operations::transfer) use progress::{
    SerialLeafProgress, make_concurrent_per_file_progress,
};
pub(in crate::file_system::write_operations::transfer) use sync_driver::drive_transfer_serial_sync;

// ============================================================================
// Core types
// ============================================================================

/// Per-iteration context passed to the `transfer_one` closure.
///
/// Carries everything the closure needs that the driver shouldn't care about.
/// Operation-specific mutable state (transaction, created_dirs, tracker, partials
/// list) is CAPTURED by the closure itself, not passed through this struct.
/// That way, the driver doesn't grow new fields every time a new operation
/// joins.
pub(super) struct TransferContext<'a> {
    /// Event sink for emitting progress / conflict / source-item-done events
    /// from within the closure (used by sync local-copy for mid-flight conflict
    /// events and for `write-source-item-done`).
    pub events: &'a dyn OperationEventSink,
    /// Operation state (cancellation flag, ETA estimator, conflict channel).
    pub state: &'a Arc<WriteOperationState>,
    pub operation_id: &'a str,
    pub operation_type: WriteOperationType,
    /// The source path the driver is asking the closure to transfer. For the
    /// async driver this is the **top-level** source path (post conflict
    /// resolution); for the sync driver this is whatever path the caller
    /// passed in `sources` (typically also a top-level path, sometimes a
    /// per-file `FileInfo.path` if the caller pre-flattened).
    pub source_path: &'a Path,
    /// The destination path the driver is asking the closure to write to. For
    /// the async driver this is the path that came out of conflict resolution
    /// (so it may differ from `dest_root.join(source_path.file_name())` under
    /// Rename). For the sync driver this is `None` (the closure derives the
    /// destination from its own destination root + `source_path`).
    pub dest_path: Option<&'a Path>,
    /// File→file safe-replace target. When `Some(orig)`, `dest_path` is a temp
    /// sibling: after a successful streaming write, the closure must finalize by
    /// deleting `orig` and renaming `dest_path` → `orig` (see
    /// `volume_conflict::finalize_safe_replace`). `None` ⇒ write `dest_path`
    /// directly. Only set by the async driver from
    /// `ConflictDecision::Proceed`; always `None` for the sync driver and for
    /// no-conflict paths.
    pub replace_after_write: Option<&'a Path>,
    /// Cumulative files processed BEFORE this iteration. Lets the closure
    /// compute `effective_bytes_done` for intra-file progress callbacks
    /// without having to thread the counter through itself. Snapshotted by the
    /// driver at the start of each iteration.
    pub files_done_so_far: usize,
    /// Cumulative bytes processed BEFORE this iteration. See `files_done_so_far`.
    pub bytes_done_so_far: u64,
    /// Total file count for the operation (post bulk-skip subtraction is NOT
    /// applied — this is the original total so progress fractions stay
    /// consistent with what the user saw at scan time).
    pub total_files: usize,
    /// Total bytes for the operation. See `total_files`.
    pub total_bytes: u64,
}

/// What the closure returns for a single per-source iteration.
///
/// Outcomes drive the driver's progress accounting and post-loop intent.
#[derive(Debug)]
pub(super) enum TransferOutcome {
    /// Source transferred successfully; `bytes` were written.
    Transferred { bytes: u64 },
    /// Source was skipped by closure-side logic (mid-flight conflict resolved
    /// as Skip, same-inode self-copy detected, etc.). The driver bumps
    /// `files_done` and adds `bytes_accounted` to `bytes_done` so the progress
    /// bar reflects the skip immediately.
    Skipped { bytes_accounted: u64 },
}

/// Driver's return value to the caller.
///
/// Carries the final counters plus an `intent` so the caller can decide
/// whether to commit, roll back, or surface an error. `files_skipped` and
/// `bytes_skipped` are a subset of `files_done` / `bytes_done`: they cover
/// the pre-known-conflict bulk-skip prelude plus per-iter Skip decisions
/// (conflict resolver returning `Skip` and closure returning `Skipped`).
/// The caller uses them to annotate the completion log.
#[derive(Debug)]
pub(super) struct TransferLoopOutcome {
    pub files_done: usize,
    pub bytes_done: u64,
    pub files_skipped: usize,
    pub bytes_skipped: u64,
    pub intent: PostLoopIntent,
}

/// Why the loop ended. The caller branches on this to decide
/// rollback/commit/cancel emission.
#[derive(Debug)]
pub(super) enum PostLoopIntent {
    /// All sources processed (closure returned `Ok` for every non-skipped one).
    Completed,
    /// Cancellation was observed mid-loop. The caller inspects
    /// `load_intent(&state.intent)` to distinguish `Stopped` vs `RollingBack`.
    Cancelled,
    /// The closure returned an error for a source. The driver short-circuits;
    /// the caller decides whether to rollback.
    Failed(WriteOperationError),
}

// ============================================================================
// Shared helpers
// ============================================================================

/// Builds the pre-known-conflicts bulk-skip set.
///
/// Returns the subset of `sources` whose **file names** appear in
/// `config_pre_known_conflicts`, when `config_resolution == Skip`. Empty for
/// any other resolution. Exposed for the driver tests so they can audit it
/// independently of the loop (per plan's "Open questions" section).
///
/// **Top-level directories are excluded from the bulk-skip set** even when
/// their filenames match. A bulk-skip drops the whole subtree, which is
/// correct only when the top-level source is a single file; for a top-level
/// directory, only some children may actually conflict at dest, and the
/// non-conflicting ones still need to copy. The caller is responsible for
/// passing the set of top-level paths it has identified as directories via
/// `known_directory_paths`; the helper falls through to the per-iter
/// resolution path for those (which handles individual child conflicts).
pub(super) fn build_pre_skip_set(
    sources: &[PathBuf],
    config_resolution: ConflictResolution,
    config_pre_known_conflicts: &[String],
    known_directory_paths: &HashSet<PathBuf>,
) -> HashSet<PathBuf> {
    if config_resolution != ConflictResolution::Skip || config_pre_known_conflicts.is_empty() {
        return HashSet::new();
    }
    let names: HashSet<&str> = config_pre_known_conflicts.iter().map(String::as_str).collect();
    sources
        .iter()
        .filter(|p| {
            if known_directory_paths.contains(*p) {
                return false;
            }
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| names.contains(n))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// Emits a paired `write-progress` event AND `update_operation_status` call,
/// so callers can't forget one or the other. Always go through this from the
/// driver.
#[allow(
    clippy::too_many_arguments,
    reason = "These are the natural fields of a progress event; bundling adds ceremony without cleaning anything up"
)]
pub(super) fn emit_progress_and_status(
    events: &dyn OperationEventSink,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
    operation_type: WriteOperationType,
    phase: WriteOperationPhase,
    current_file: Option<String>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
) {
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            operation_type,
            phase,
            current_file.clone(),
            files_done,
            files_total,
            bytes_done,
            bytes_total,
        ),
    );
    update_operation_status(
        operation_id,
        phase,
        current_file,
        files_done,
        files_total,
        bytes_done,
        bytes_total,
    );
}

// ============================================================================
// Configuration passed in by the caller
// ============================================================================

/// Driver-relevant slice of the operation config. The driver doesn't care
/// about all of `WriteOperationConfig` / `VolumeCopyConfig`; this struct
/// picks out exactly what it needs so callers can construct one from either.
#[derive(Debug, Clone)]
pub(super) struct DriverConfig {
    pub operation_type: WriteOperationType,
    pub phase: WriteOperationPhase,
    /// Used by [`build_pre_skip_set`]. The list of filenames that the FE
    /// already discovered as conflicts (only consulted when `resolution`
    /// is `Skip`).
    pub conflict_resolution: ConflictResolution,
    pub pre_known_conflicts: Vec<String>,
    /// Whether the ASYNC driver emits a per-source milestone in its
    /// `Transferred` arm (top-level-source granular). `true` for paths whose
    /// `transfer_one` closure does NOT emit its own progress — the same-volume
    /// rename-merge, which moves a whole source with one `rename` and no
    /// streaming, so the driver milestone is its ONLY Copying event. `false`
    /// for the streaming paths (cross-volume copy/move): their
    /// `SerialLeafProgress::on_leaf_complete` already emits LEAF-granular
    /// milestones, and a top-level-granular driver milestone would visibly
    /// regress the File bar at the end of a directory source (e.g. 9/9 → 1/9).
    /// Ignored by the sync driver, whose closure always owns its emits.
    pub emit_per_source_milestone: bool,
}

// ============================================================================
// Async-driver conflict-resolution vocabulary
// ============================================================================

/// What the async driver hands the caller's resolver callback when it detects
/// a top-level dest conflict.
pub(super) struct ConflictDecisionInput<'a> {
    pub source_path: &'a Path,
    pub initial_dest_path: &'a Path,
    /// Pre-fetched dest metadata so the resolver doesn't re-stat (the driver
    /// already paid the `get_metadata` cost to discover the conflict).
    pub dest_size_hint: Option<u64>,
    /// Hint for the resolver: is the top-level source a directory? Saves the
    /// resolver from re-probing.
    pub source_is_directory_hint: Option<bool>,
    /// Hint for the resolver: top-level source size for files. Saves an MTP
    /// parent-listing on the conflict dialog path (see `resolve_volume_conflict`).
    pub source_size_hint: Option<u64>,
}

/// Conflict-resolution outcome the resolver hands back to the driver.
#[derive(Debug)]
pub(super) enum ConflictDecision {
    /// Skip this source. Driver bumps counters via skip accounting (both
    /// `files_done += 1` and `bytes_done += bytes_accounted`) and continues.
    /// The `transfer_one` closure is NOT invoked. `bytes_accounted` is the
    /// source's byte size from the caller's pre-flight scan (volume copy
    /// looks it up in `source_hints`; volume move has no scan and passes 0).
    /// Without it, the size progress bar would stay at 0 % while the file
    /// counter moved forward on Skip-All operations.
    Skip { bytes_accounted: u64 },
    /// Proceed with the given (possibly rewritten) destination path. Driver
    /// calls `transfer_one` with `dest_path = Some(this)`.
    ///
    /// `replace_after_write` carries the file→file safe-replace contract: when
    /// `Some(orig)`, `dest_path` is a temp sibling the closure streams into,
    /// and after a successful write the closure must finalize by deleting
    /// `orig` and renaming the temp into place (see
    /// `volume_conflict::finalize_safe_replace`). `None` ⇒ write `dest_path`
    /// directly. The driver passes `dest_path` through `TransferContext`
    /// unchanged; only the closure acts on `replace_after_write`, so the driver
    /// stays agnostic to the safe-replace mechanism.
    Proceed {
        dest_path: PathBuf,
        replace_after_write: Option<PathBuf>,
    },
}

// The test files stay at the `transfer/` level (one is large and tracked in the
// file-length allowlist by that path); `../` keeps the path stable. They're
// still submodules of `transfer_driver` via these declarations, so their
// `super::super::super::…` paths reach `write_operations` as before.
#[cfg(test)]
#[path = "../transfer_driver_async_tests.rs"]
mod async_tests;
#[cfg(test)]
#[path = "../transfer_driver_concurrent_tests.rs"]
mod concurrent_tests;
#[cfg(test)]
#[path = "../transfer_driver_pre_skip_tests.rs"]
mod pre_skip_tests;
#[cfg(test)]
#[path = "../transfer_driver_sync_tests.rs"]
mod sync_tests;
#[cfg(test)]
#[path = "../transfer_driver_test_support.rs"]
mod test_support;
