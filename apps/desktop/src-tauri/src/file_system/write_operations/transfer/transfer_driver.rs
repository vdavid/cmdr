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
//! `transfer_driver_tests.rs` pins each of these properties so a future refactor
//! that violates the contract gets caught here, not by structural inspection of
//! four different functions.
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
// bearing for the driver's contract and exercised by `transfer_driver_tests.rs`
// (`TransferOutcome::Skipped` is constructed in test closures; the config
// fields feed `build_pre_skip_set` audits). Keep them as part of the public-
// to-the-module surface so adding a future caller doesn't require widening the
// driver in a separate commit.
#![allow(
    dead_code,
    reason = "Driver surface kept stable for future callers; exercised by transfer_driver_tests.rs"
)]

use std::collections::HashSet;
use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::ignore_poison::IgnorePoison;

use super::super::state::{OperationIntent, WriteOperationState, is_cancelled, load_intent, update_operation_status};
use super::super::types::{
    ConflictResolution, OperationEventSink, WriteOperationError, WriteOperationPhase, WriteOperationType,
    WriteProgressEvent,
};

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
// Per-file progress callback builders
// ============================================================================

/// Builds a per-file `on_progress` callback for `copy_single_path` for
/// **serial** transfer paths (one source in flight at a time).
///
/// The returned closure is invoked per chunk by the destination volume's
/// `write_from_stream`. On each call it:
/// - Checks `state.intent`; returns `Break` to abort the write on cancel.
/// - Throttles via `last_emit` (skip if the previous emit was less than
///   `progress_interval` ago).
/// - Emits a `WriteProgressEvent { phase: Copying, ... }` carrying the
///   driver's per-iteration snapshots (`files_done_so_far`,
///   `bytes_done_so_far`) plus the in-flight file's `file_bytes_done` as
///   `bytes_done_so_far + file_bytes_done`.
///
/// Used by:
/// - `volume_copy::copy_volumes_with_progress` serial path
/// - `volume_move::move_volumes_with_progress` (cross-volume copy phase)
///
/// Captures everything by value (`Arc`-cloned), so the returned closure
/// is `'static + Send + Sync` — safe to pass through `copy_single_path`'s
/// `&dyn Fn(...)` parameter from inside an async move-block executed
/// across `tokio::spawn` boundaries.
#[allow(
    clippy::too_many_arguments,
    reason = "matches WriteProgressEvent shape; bundling into a context struct adds ceremony without cleaning anything up"
)]
pub(super) fn make_serial_per_file_progress(
    events: Arc<dyn OperationEventSink>,
    state: Arc<WriteOperationState>,
    operation_id: String,
    operation_type: WriteOperationType,
    file_name: Option<String>,
    files_done_so_far: usize,
    bytes_done_so_far: u64,
    total_files: usize,
    total_bytes: u64,
    last_emit: Arc<Mutex<Instant>>,
    progress_interval: Duration,
) -> impl Fn(u64, u64) -> ControlFlow<()> + Send + Sync + 'static {
    move |file_bytes_done: u64, _file_bytes_total: u64| -> ControlFlow<()> {
        if is_cancelled(&state.intent) {
            return ControlFlow::Break(());
        }
        let current_total = bytes_done_so_far + file_bytes_done;
        try_emit_throttled_progress(
            &*events,
            &state,
            &operation_id,
            operation_type,
            file_name.clone(),
            files_done_so_far,
            total_files,
            current_total,
            total_bytes,
            &last_emit,
            progress_interval,
        );
        ControlFlow::Continue(())
    }
}

/// Builds a per-file `on_progress` callback for `copy_single_path` for
/// **concurrent** transfer paths (multiple sources in flight at once).
///
/// Unlike the serial variant, each task fires its own callback against
/// shared op-wide counters: `bytes_done_atomic` accumulates deltas across
/// all in-flight files; `files_done_atomic` is read (not written) per
/// chunk so the emitted event reflects the latest cross-task tally.
///
/// `last_file_bytes` is a per-task atomic that the callback uses to
/// convert the volume's cumulative-for-this-file count into a delta
/// before rolling into the shared `bytes_done_atomic`. Callers must
/// allocate a fresh `AtomicU64` per spawned task; the caller can also
/// inspect `last_file_bytes.load() == 0` after the task finishes to
/// detect volumes that never invoked `on_progress` and credit the file's
/// bytes to the aggregate as a compensation.
///
/// Used by: `volume_copy::copy_volumes_with_progress` concurrent path.
#[allow(
    clippy::too_many_arguments,
    reason = "matches WriteProgressEvent shape + per-task cross-file delta tracking"
)]
pub(super) fn make_concurrent_per_file_progress(
    events: Arc<dyn OperationEventSink>,
    state: Arc<WriteOperationState>,
    operation_id: String,
    operation_type: WriteOperationType,
    file_name: Option<String>,
    last_file_bytes: Arc<AtomicU64>,
    bytes_done_atomic: Arc<AtomicU64>,
    files_done_atomic: Arc<AtomicUsize>,
    total_files: usize,
    total_bytes: u64,
    last_emit: Arc<Mutex<Instant>>,
    progress_interval: Duration,
) -> impl Fn(u64, u64) -> ControlFlow<()> + Send + Sync + 'static {
    move |file_bytes_done: u64, _file_bytes_total: u64| -> ControlFlow<()> {
        if is_cancelled(&state.intent) {
            return ControlFlow::Break(());
        }
        let prev = last_file_bytes.swap(file_bytes_done, Ordering::Relaxed);
        let delta = file_bytes_done.saturating_sub(prev);
        let current_total = bytes_done_atomic.fetch_add(delta, Ordering::Relaxed) + delta;
        let current_files_done = files_done_atomic.load(Ordering::Relaxed);
        try_emit_throttled_progress(
            &*events,
            &state,
            &operation_id,
            operation_type,
            file_name.clone(),
            current_files_done,
            total_files,
            current_total,
            total_bytes,
            &last_emit,
            progress_interval,
        );
        ControlFlow::Continue(())
    }
}

/// Throttle gate + paired emit. Returns `true` if it emitted, `false` if
/// the call was suppressed by the throttle.
///
/// Two callers racing on the gate can both succeed; over-emission is
/// fine — the throttle protects the *floor* event rate, not a strict
/// ceiling. The Mutex is released before `emit_progress_and_status`
/// (which may take its own internal locks for the ETA estimator and
/// status cache), so the gate never serializes downstream emits.
#[allow(
    clippy::too_many_arguments,
    reason = "matches WriteProgressEvent shape; bundling into a context struct adds ceremony without cleaning anything up"
)]
fn try_emit_throttled_progress(
    events: &dyn OperationEventSink,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
    operation_type: WriteOperationType,
    file_name: Option<String>,
    files_done: usize,
    total_files: usize,
    bytes_done: u64,
    total_bytes: u64,
    last_emit: &Mutex<Instant>,
    progress_interval: Duration,
) -> bool {
    let mut last = last_emit.lock_ignore_poison();
    if last.elapsed() < progress_interval {
        return false;
    }
    *last = Instant::now();
    drop(last);
    emit_progress_and_status(
        events,
        state,
        operation_id,
        operation_type,
        WriteOperationPhase::Copying,
        file_name,
        files_done,
        total_files,
        bytes_done,
        total_bytes,
    );
    true
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
}

// ============================================================================
// Sync driver entry point (for local-FS copy/move, ran inside spawn_blocking)
// ============================================================================

/// Sync serial driver for local-FS operations.
///
/// # Data-safety contract
///
/// The `transfer_one` closure is invoked for a source iff **all** of the
/// following hold:
///
/// - the source is NOT in the pre-known-conflicts bulk-skip set, AND
/// - cancellation has NOT been observed on `state.intent` at the start of this iteration.
///
/// Top-level conflict resolution is the **closure's** responsibility for the
/// sync path (local-FS conflicts happen mid-flight inside `copy_single_item`
/// at parent-directory level, not at the top-level dest). The closure can
/// still return `TransferOutcome::Skipped` to participate in the driver's
/// skip accounting.
///
/// # What the driver does
///
/// 1. Pre-known-conflicts bulk-skip prelude. Sums `bulk_skip_bytes` from the caller, increments
///    counters, and emits one bulk progress event.
/// 2. Per-source loop: a. Check cancellation. If cancelled, return with
///    `PostLoopIntent::Cancelled`. b. Skip pre-known-conflict sources (no closure invocation). c.
///    Call `transfer_one`. On `Ok`, update counters. On `Err` with
///    `WriteOperationError::Cancelled`, return `PostLoopIntent::Cancelled`. On any other `Err`,
///    return `PostLoopIntent::Failed`.
/// 3. Return `PostLoopIntent::Completed` if the loop drained without incident.
///
/// # Why no async / no boxed-future variant
///
/// The local-FS copy/move closures capture `&mut CopyTransaction` and
/// `&mut HashSet<PathBuf>`. Forcing those captures through a boxed-future
/// shape buys nothing — the underlying `std::fs::*` calls are sync.
#[allow(
    clippy::too_many_arguments,
    reason = "Driver wraps the full per-iter loop context; bundling adds ceremony without removing parameters"
)]
pub(super) fn drive_transfer_serial_sync<F>(
    events: &dyn OperationEventSink,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
    sources: &[PathBuf],
    total_files: usize,
    total_bytes: u64,
    bulk_skip_files: usize,
    bulk_skip_bytes: u64,
    pre_skip_paths: &HashSet<PathBuf>,
    config: &DriverConfig,
    mut transfer_one: F,
) -> TransferLoopOutcome
where
    F: FnMut(TransferContext<'_>) -> Result<TransferOutcome, WriteOperationError>,
{
    let mut files_done = 0usize;
    let mut bytes_done = 0u64;
    let mut files_skipped = 0usize;
    let mut bytes_skipped = 0u64;

    // ---- Pre-known-conflicts bulk-skip prelude. ----
    // The caller has already filtered the source set (sync ops are per-file
    // and have to filter `FileInfo`s, not top-level paths). The driver just
    // emits the one progress event reflecting the bulk skip so the bar jumps
    // in one go, instead of advancing one tick per skipped file interleaved
    // with the real transfers.
    if bulk_skip_files > 0 {
        files_done += bulk_skip_files;
        bytes_done += bulk_skip_bytes;
        files_skipped += bulk_skip_files;
        bytes_skipped += bulk_skip_bytes;
        log::info!(
            "drive_transfer_serial_sync: bulk-skipping {} files ({} bytes) before main iteration",
            bulk_skip_files,
            bulk_skip_bytes
        );
        // Re-anchor the rate estimator BEFORE emitting. The bulk-skip jump
        // is past work credited instantly, not throughput; without this
        // reseed the first real per-file emit's delta is computed against
        // `(0, 0)` and pins `bytes_per_second` at GB/s level. See
        // `eta::EtaEstimator::reseed_baseline` for the full rationale.
        if let Ok(mut est) = state.estimator.lock() {
            est.reseed_baseline(Instant::now(), bytes_done, files_done);
        }
        emit_progress_and_status(
            events,
            state,
            operation_id,
            config.operation_type,
            config.phase,
            None,
            files_done,
            total_files,
            bytes_done,
            total_bytes,
        );
    }

    // ---- Main loop. ----
    for source_path in sources {
        // CRITICAL: cancellation check is BEFORE any destructive call.
        if is_cancelled(&state.intent) {
            log::debug!(
                "drive_transfer_serial_sync: cancellation observed at {} (of {}) for op={}",
                files_done,
                total_files,
                operation_id
            );
            return TransferLoopOutcome {
                files_done,
                bytes_done,
                files_skipped,
                bytes_skipped,
                intent: PostLoopIntent::Cancelled,
            };
        }

        // CRITICAL: pre-skip check is BEFORE any closure invocation. The
        // closure must NEVER see a pre-known-conflict source.
        if pre_skip_paths.contains(source_path) {
            continue;
        }

        let ctx = TransferContext {
            events,
            state,
            operation_id,
            operation_type: config.operation_type,
            source_path,
            dest_path: None,
            replace_after_write: None,
            files_done_so_far: files_done,
            bytes_done_so_far: bytes_done,
            total_files,
            total_bytes,
        };

        match transfer_one(ctx) {
            Ok(TransferOutcome::Transferred { bytes }) => {
                // Sync per-file closures (`copy_single_item`) own their own
                // per-file milestone emit and intra-file byte progress, so
                // the driver just updates its counters here. Mirrors the
                // existing conflict-resolution split (sync driver delegates
                // to the closure; async driver dispatches itself).
                files_done += 1;
                bytes_done += bytes;
            }
            Ok(TransferOutcome::Skipped { bytes_accounted }) => {
                files_done += 1;
                bytes_done += bytes_accounted;
                files_skipped += 1;
                bytes_skipped += bytes_accounted;
                // Skip-arm bump emits a throttled progress event so the bar
                // reflects the user's Skip choice immediately.
                emit_progress_and_status(
                    events,
                    state,
                    operation_id,
                    config.operation_type,
                    config.phase,
                    source_path.file_name().map(|n| n.to_string_lossy().to_string()),
                    files_done,
                    total_files,
                    bytes_done,
                    total_bytes,
                );
            }
            Err(WriteOperationError::Cancelled { .. }) => {
                return TransferLoopOutcome {
                    files_done,
                    bytes_done,
                    files_skipped,
                    bytes_skipped,
                    intent: PostLoopIntent::Cancelled,
                };
            }
            Err(e) => {
                return TransferLoopOutcome {
                    files_done,
                    bytes_done,
                    files_skipped,
                    bytes_skipped,
                    intent: PostLoopIntent::Failed(e),
                };
            }
        }
    }

    TransferLoopOutcome {
        files_done,
        bytes_done,
        files_skipped,
        bytes_skipped,
        intent: PostLoopIntent::Completed,
    }
}

// ============================================================================
// Async driver entry point (for volume copy/move)
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

/// Async serial driver for volume operations.
///
/// # Data-safety contract
///
/// The `transfer_one` closure is invoked for a source iff **all** of the
/// following hold:
///
/// - the source is NOT in the pre-known-conflicts bulk-skip set, AND
/// - cancellation has NOT been observed on `state.intent` at the start of this iteration, AND
/// - the conflict resolver returned `ConflictDecision::Proceed` (not Skip) when a top-level
///   conflict was detected.
///
/// # What the driver does
///
/// 1. Pre-known-conflicts bulk-skip prelude (emit one bulk progress event).
/// 2. Per-source loop: a. Cancellation check. If cancelled, return `PostLoopIntent::Cancelled`. b.
///    Skip pre-known-conflict sources (no closure invocation). c. Conflict detection via
///    `dest_meta_fetcher`. If conflict, invoke `conflict_resolver` and respect its decision. d. If
///    Skip, do skip accounting + emit throttled progress; continue. e. If Proceed, invoke
///    `transfer_one` with the resolved dest path.
/// 3. Return `PostLoopIntent::Completed` if the loop drained without incident.
///
/// # Closure-bound shape: boxed future, not `AsyncFnMut`
///
/// Each closure returns `Pin<Box<dyn Future<...> + Send + 'a>>` rather than
/// being bound as `AsyncFnMut(...) -> T`. The explicit boxed-future shape is
/// load-bearing for production callers: the driver's returned future must be
/// `Send` so callers can `tokio::spawn` it, and `AsyncFnMut`'s HRTB-bound
/// `CallRefFuture<'a>` is not provably `Send` for all `'a` when the closure
/// body captures `&Arc<...>` or similar refs (rust-lang/rust#100013-class).
/// The `+ Send` on the boxed future moves the Send obligation inside the
/// per-call return type, where it's discharged at each call site.
///
/// `transfer_driver_tests.rs::driver_future_is_send_across_spawn` pins this:
/// the driver call must compile inside a `tokio::spawn(async move { ... })`,
/// which fails under `AsyncFnMut` and passes under the boxed-future shape.
///
/// The driver loop is still single-threaded (one `.await` per closure call
/// in sequence); only the FUTURE returned by each call needs `Send`. The
/// closure itself doesn't need to be `Send`.
#[allow(
    clippy::too_many_arguments,
    reason = "Driver wraps the full per-iter context; bundling adds ceremony without removing parameters"
)]
pub(super) async fn drive_transfer_serial_async<DestMetaFetcher, ConflictResolver, TransferOne>(
    events: &dyn OperationEventSink,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
    sources: &[PathBuf],
    dest_root: &Path,
    total_files: usize,
    total_bytes: u64,
    bulk_skip_files: usize,
    bulk_skip_bytes: u64,
    pre_skip_paths: &HashSet<PathBuf>,
    config: &DriverConfig,
    mut dest_meta_fetcher: DestMetaFetcher,
    mut conflict_resolver: ConflictResolver,
    mut transfer_one: TransferOne,
) -> TransferLoopOutcome
where
    DestMetaFetcher: for<'a> FnMut(&'a Path) -> Pin<Box<dyn Future<Output = Option<u64>> + Send + 'a>>,
    ConflictResolver: for<'a> FnMut(
        ConflictDecisionInput<'a>,
    ) -> Pin<
        Box<dyn Future<Output = Result<ConflictDecision, WriteOperationError>> + Send + 'a>,
    >,
    TransferOne: for<'a> FnMut(
        TransferContext<'a>,
    ) -> Pin<
        Box<dyn Future<Output = Result<TransferOutcome, WriteOperationError>> + Send + 'a>,
    >,
{
    let mut files_done = 0usize;
    let mut bytes_done = 0u64;
    let mut files_skipped = 0usize;
    let mut bytes_skipped = 0u64;
    let mut last_progress_time = Instant::now();

    // ---- Pre-known-conflicts bulk-skip prelude. ----
    if bulk_skip_files > 0 {
        files_done += bulk_skip_files;
        bytes_done += bulk_skip_bytes;
        files_skipped += bulk_skip_files;
        bytes_skipped += bulk_skip_bytes;
        log::info!(
            "drive_transfer_serial_async: bulk-skipping {} files ({} bytes) before main iteration",
            bulk_skip_files,
            bulk_skip_bytes
        );
        // Re-anchor the rate estimator: bulk-skip credit is past work, not
        // throughput. See `drive_transfer_serial_sync` for the rationale.
        if let Ok(mut est) = state.estimator.lock() {
            est.reseed_baseline(Instant::now(), bytes_done, files_done);
        }
        emit_progress_and_status(
            events,
            state,
            operation_id,
            config.operation_type,
            config.phase,
            None,
            files_done,
            total_files,
            bytes_done,
            total_bytes,
        );
    }

    let progress_interval = state.progress_interval;

    // ---- Main loop. ----
    for source_path in sources {
        // CRITICAL: cancellation check is BEFORE pre-skip, which is BEFORE
        // any closure / conflict-resolver / destructive call.
        if is_cancelled(&state.intent) {
            log::debug!(
                "drive_transfer_serial_async: cancellation observed at {} (of {}) for op={}",
                files_done,
                total_files,
                operation_id
            );
            return TransferLoopOutcome {
                files_done,
                bytes_done,
                files_skipped,
                bytes_skipped,
                intent: PostLoopIntent::Cancelled,
            };
        }

        if pre_skip_paths.contains(source_path) {
            continue;
        }

        let initial_dest_path = if let Some(name) = source_path.file_name() {
            dest_root.join(name)
        } else {
            dest_root.to_path_buf()
        };

        // Conflict detection via caller-supplied dest meta fetcher.
        // `Some(size)` => conflict; `None` => no conflict (or stat failed,
        // treated identically to no-conflict at the top-level — same shape as
        // today's `dest_volume.get_metadata(...).await.ok()` check in
        // `copy_volumes_with_progress`).
        let dest_size_hint = dest_meta_fetcher(&initial_dest_path).await;

        let (resolved_dest, replace_after_write) = if dest_size_hint.is_some() {
            log::debug!(
                "drive_transfer_serial_async: conflict detected at {}",
                initial_dest_path.display()
            );
            let decision = conflict_resolver(ConflictDecisionInput {
                source_path,
                initial_dest_path: &initial_dest_path,
                dest_size_hint,
                source_is_directory_hint: None,
                source_size_hint: None,
            })
            .await;

            match decision {
                Ok(ConflictDecision::Skip { bytes_accounted }) => {
                    // Per-iter skip accounting: bump counters and emit
                    // throttled progress so the bar reflects the skip
                    // immediately.
                    files_done += 1;
                    bytes_done += bytes_accounted;
                    files_skipped += 1;
                    bytes_skipped += bytes_accounted;
                    if last_progress_time.elapsed() >= progress_interval {
                        last_progress_time = Instant::now();
                        emit_progress_and_status(
                            events,
                            state,
                            operation_id,
                            config.operation_type,
                            config.phase,
                            source_path.file_name().map(|n| n.to_string_lossy().to_string()),
                            files_done,
                            total_files,
                            bytes_done,
                            total_bytes,
                        );
                    }
                    continue;
                }
                Ok(ConflictDecision::Proceed {
                    dest_path,
                    replace_after_write,
                }) => (dest_path, replace_after_write),
                Err(e) => {
                    return TransferLoopOutcome {
                        files_done,
                        bytes_done,
                        files_skipped,
                        bytes_skipped,
                        intent: PostLoopIntent::Failed(e),
                    };
                }
            }
        } else {
            (initial_dest_path, None)
        };

        let ctx = TransferContext {
            events,
            state,
            operation_id,
            operation_type: config.operation_type,
            source_path,
            dest_path: Some(&resolved_dest),
            replace_after_write: replace_after_write.as_deref(),
            files_done_so_far: files_done,
            bytes_done_so_far: bytes_done,
            total_files,
            total_bytes,
        };

        match transfer_one(ctx).await {
            Ok(TransferOutcome::Transferred { bytes }) => {
                files_done += 1;
                bytes_done += bytes;
                // Per-file milestone emit. Bypasses the throttle because
                // it's a per-file event (bounded by file count, not noisy)
                // and the FE's files-done axis needs at least one Copying
                // event with the bumped `files_done` — chunked emits inside
                // `transfer_one` carry the pre-iteration snapshot, so for
                // single-file ops the chunked path never crosses `N/N`.
                last_progress_time = Instant::now();
                emit_progress_and_status(
                    events,
                    state,
                    operation_id,
                    config.operation_type,
                    config.phase,
                    source_path.file_name().map(|n| n.to_string_lossy().to_string()),
                    files_done,
                    total_files,
                    bytes_done,
                    total_bytes,
                );
            }
            Ok(TransferOutcome::Skipped { bytes_accounted }) => {
                files_done += 1;
                bytes_done += bytes_accounted;
                files_skipped += 1;
                bytes_skipped += bytes_accounted;
                if last_progress_time.elapsed() >= progress_interval {
                    last_progress_time = Instant::now();
                    emit_progress_and_status(
                        events,
                        state,
                        operation_id,
                        config.operation_type,
                        config.phase,
                        source_path.file_name().map(|n| n.to_string_lossy().to_string()),
                        files_done,
                        total_files,
                        bytes_done,
                        total_bytes,
                    );
                }
            }
            Err(WriteOperationError::Cancelled { .. }) => {
                return TransferLoopOutcome {
                    files_done,
                    bytes_done,
                    files_skipped,
                    bytes_skipped,
                    intent: PostLoopIntent::Cancelled,
                };
            }
            Err(e) => {
                return TransferLoopOutcome {
                    files_done,
                    bytes_done,
                    files_skipped,
                    bytes_skipped,
                    intent: PostLoopIntent::Failed(e),
                };
            }
        }
    }

    // One final post-loop check: a `RollingBack` or `Stopped` transition could
    // land after the last iteration's check and before we return Completed.
    // Today's `copy_files_with_progress_inner` post-loop intent check exists
    // exactly for this race (commit `1de4255d`). We mirror it here so a
    // future caller doesn't have to remember.
    if load_intent(&state.intent) != OperationIntent::Running {
        return TransferLoopOutcome {
            files_done,
            bytes_done,
            files_skipped,
            bytes_skipped,
            intent: PostLoopIntent::Cancelled,
        };
    }

    TransferLoopOutcome {
        files_done,
        bytes_done,
        files_skipped,
        bytes_skipped,
        intent: PostLoopIntent::Completed,
    }
}

#[cfg(test)]
#[path = "transfer_driver_tests.rs"]
mod tests;
