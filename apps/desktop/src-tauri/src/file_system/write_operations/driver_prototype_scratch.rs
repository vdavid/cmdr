//! M2 step 0: throwaway feasibility prototype for the transfer driver against
//! `copy.rs::copy_files_with_progress_inner`.
//!
//! Goal: prove the three exit criteria from `docs/specs/transfer-driver-refactor-plan.md`
//! § "Milestone 2 step 0":
//!   (a) closure can capture `&mut transaction` and `&mut created_dirs` under `AsyncFnMut`
//!   (b) post-loop rollback runs OUTSIDE the driver (caller invokes it, driver returns intent)
//!   (c) closure can correctly drive `SourceItemTracker` and emit `write-source-item-done`
//!
//! This file is a type-check probe only. Nothing here is called from production code.
//! The whole module is gated `#[cfg(test)]` and is intended to be DELETED at the end of M2.
//!
//! Surprises / load-bearing observations:
//!
//!   - `copy_files_with_progress_inner` is FULLY SYNCHRONOUS today. It runs inside
//!     `spawn_blocking`. The plan describes the driver as async (mirroring
//!     `copy_volumes_with_progress`). Bridging this is a real concern that the plan
//!     doesn't address: either (i) the driver also has a sync entry point for
//!     `copy.rs`, (ii) `copy_files_with_progress_inner` becomes async and the helpers
//!     it calls (`fs::create_dir_all`, `fs::rename`, `copy_file_with_strategy`) stay
//!     sync inside `tokio::task::block_in_place` / are run on a current-thread
//!     runtime, or (iii) the driver is generic over sync/async via a separate trait.
//!     This prototype follows (i) — it models the driver as a sync function with
//!     a `FnMut` closure bound — because the underlying I/O is sync and forcing
//!     async on this path buys nothing. The `AsyncFnMut`-shaped variant is also
//!     sketched at the bottom for the volume-copy entry point, where async is
//!     genuinely needed.
//!
//!   - The plan's M2 step 0 frames (a) as "AsyncFnMut". For `copy.rs` specifically,
//!     `FnMut` (sync) is the correct bound — but the closure-capture question is the
//!     same: can a single closure hold `&mut transaction` and `&mut created_dirs`
//!     simultaneously? Yes, both via `FnMut` (sync) and `AsyncFnMut` (async). The
//!     hard part is the borrow checker, not the trait bound, and that part works.
//!
//!   - GOTCHA discovered during this prototype: for the async driver entry
//!     point, you MUST write the closure with Rust 2024's async-closure syntax
//!     `async |ctx| { … }`, NOT the older `|ctx| async { … }` form. The latter
//!     returns an async block that borrows the closure's `&mut` captures, which
//!     `FnMut`/`AsyncFnMut` rejects ("returns an `async` block that contains a
//!     reference to a captured variable, which then escapes the closure body").
//!     The `async ||` form ties the returned future to `&mut self` of the
//!     closure call, which is what `AsyncFnMut` actually expects. This codebase
//!     is on `edition = "2024"` so the syntax is available. If a future
//!     downgrade ever happens, the fallback is `impl FnMut(TransferContext<'_>)
//!     -> Pin<Box<dyn Future<Output = …> + '_>>` with explicit boxing, but the
//!     boxed-future shape has its own lifetime gymnastics — verify before
//!     locking it in.
//!
//!   - Additional captures the plan's M2-step-0 brief doesn't enumerate but that
//!     `copy_files_with_progress_inner` actually threads through `copy_single_item`:
//!       * `&mut files_done: &mut usize`
//!       * `&mut bytes_done: &mut u64`
//!       * `&mut last_progress_time: &mut Instant`
//!       * `&mut apply_to_all_resolution: &mut Option<ConflictResolution>`
//!       * `&mut tracker: &mut SourceItemTracker`
//!     The driver "owns" `files_done`, `bytes_done`, `last_progress_time` per the
//!     M2 design — those become driver-internal counters and the closure just
//!     returns the bytes it wrote (`TransferOutcome::Done { bytes }`). But
//!     `apply_to_all_resolution` is interesting: today it's threaded into
//!     `copy_single_item` so conflict resolution can latch "apply to all" across
//!     files. The plan moves conflict resolution into the driver, so the driver
//!     would own that latch too. That's fine — the closure for `copy.rs` would
//!     receive a *pre-resolved* destination path from the driver and not call
//!     `resolve_conflict` itself. BUT: `copy_single_item`'s conflict-resolution is
//!     interleaved with the file-vs-directory blocking-file detection (lines
//!     494–572 of copy.rs), where the conflict is on a *parent directory* that
//!     happens to be a file. That's a per-file conflict the driver can't surface
//!     in advance — it's discovered mid-copy. The driver-resolves-conflicts model
//!     doesn't cover this case. Flagged below; this is a partial-fail for the
//!     "driver owns conflict resolution for copy.rs" assumption (NOT for (a/b/c),
//!     which still pass). The driver would need to expose a sync
//!     `resolve_conflict(blocking_path) -> ConflictResolution` callback the
//!     closure can invoke mid-flight, or `copy.rs`'s closure does its own
//!     conflict resolution for blocking-file cases and the driver only handles
//!     the top-level dest conflict. Either is workable; both are uglier than the
//!     volume-copy story.

#![cfg(test)]
#![allow(dead_code, reason = "Throwaway prototype, deleted at end of M2")]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use super::scan::SourceItemTracker;
use super::state::{CopyTransaction, FileInfo, WriteOperationState};
use super::types::{
    ConflictResolution, OperationEventSink, WriteOperationError, WriteOperationType,
    WriteSourceItemDoneEvent,
};

// ---------------------------------------------------------------------------
// Proposed driver API surface (sketch, not the final shape)
// ---------------------------------------------------------------------------

/// What the closure returns for a single per-source iteration. The driver uses
/// this for its progress accounting.
#[derive(Debug)]
pub(super) enum TransferOutcome {
    /// Source transferred successfully; `bytes` were written.
    Done { bytes: u64 },
    /// Source was skipped (e.g., same-inode self-copy, or symlink edge case the
    /// closure handles itself). The driver bumps `files_done` but not `bytes_done`.
    Skipped { bytes_accounted: u64 },
}

/// What the driver returns to the caller when the loop ends.
#[derive(Debug)]
pub(super) struct TransferLoopOutcome {
    pub files_done: usize,
    pub bytes_done: u64,
    /// "Why did the loop end?" Lets the caller (e.g., `copy.rs`) decide whether
    /// to rollback, commit, or emit cancelled/error.
    pub intent: TransferLoopIntent,
}

#[derive(Debug)]
pub(super) enum TransferLoopIntent {
    /// All sources processed successfully.
    Completed,
    /// Cancelled mid-loop. The caller inspects `state.intent` to distinguish
    /// `Stopped` vs `RollingBack` (matches today's `copy.rs` logic).
    Cancelled,
    /// A non-cancellation error. The caller decides whether to rollback.
    Failed(WriteOperationError),
}

/// What the closure gets per iteration. Per-iter fields only; transaction +
/// tracker + created_dirs are CAPTURED by the closure (not in the context),
/// which is the whole point of (a).
pub(super) struct TransferContext<'a> {
    pub events: &'a dyn OperationEventSink,
    pub state: &'a Arc<WriteOperationState>,
    pub operation_id: &'a str,
    pub operation_type: WriteOperationType,
    pub destination: &'a Path,
    /// For local-FS: the FileInfo for this iteration (driver iterates per-file
    /// for local copy; for volume copy it would iterate per-top-level-source).
    pub file_info: &'a FileInfo,
}

// ---------------------------------------------------------------------------
// (a) Serial driver entry point — SYNC version for copy.rs
// ---------------------------------------------------------------------------
//
// `copy_files_with_progress_inner` is sync. So the driver-against-copy.rs needs
// a sync entry point. The closure bound is `FnMut(TransferContext<'_>) ->
// Result<TransferOutcome, WriteOperationError>`. This is a function pointer
// type; the bound permits capturing `&mut` refs and is the natural translation
// of `AsyncFnMut` for sync code.

fn drive_transfer_serial_sync<F>(
    _events: &dyn OperationEventSink,
    _state: &Arc<WriteOperationState>,
    _operation_id: &str,
    files: &[FileInfo],
    _destination: &Path,
    mut transfer_one: F,
) -> Result<TransferLoopOutcome, WriteOperationError>
where
    F: FnMut(TransferContext<'_>) -> Result<TransferOutcome, WriteOperationError>,
{
    let mut files_done = 0usize;
    let mut bytes_done = 0u64;

    // The driver owns the per-iter loop, bulk-skip prelude, cancellation check,
    // conflict resolution (where it applies), and progress emit. The closure
    // ONLY does the per-source transfer work. None of that is fleshed out here
    // — we're proving the type signature compiles, not the runtime behavior.
    for fi in files {
        let ctx = TransferContext {
            events: _events,
            state: _state,
            operation_id: _operation_id,
            operation_type: WriteOperationType::Copy,
            destination: _destination,
            file_info: fi,
        };
        match transfer_one(ctx)? {
            TransferOutcome::Done { bytes } => {
                files_done += 1;
                bytes_done += bytes;
            }
            TransferOutcome::Skipped { bytes_accounted } => {
                files_done += 1;
                bytes_done += bytes_accounted;
            }
        }
    }

    Ok(TransferLoopOutcome {
        files_done,
        bytes_done,
        intent: TransferLoopIntent::Completed,
    })
}

// ---------------------------------------------------------------------------
// (a) + (c): closure captures `&mut transaction`, `&mut created_dirs`,
// `&mut tracker`. This is THE proof for (a) and (c) — if this compiles, the
// closure can hold all three simultaneously.
// ---------------------------------------------------------------------------

/// Pseudo-helper standing in for the post-M1.5 `copy_single_item(events, ...)`.
/// The real signature would take `&dyn OperationEventSink` instead of `&AppHandle`.
/// For prototype purposes we just need a signature the closure can call.
fn copy_single_item_pseudo(
    _events: &dyn OperationEventSink,
    _state: &Arc<WriteOperationState>,
    _operation_id: &str,
    source: &Path,
    _dest: PathBuf,
    transaction: &mut CopyTransaction,
    created_dirs: &mut HashSet<PathBuf>,
    apply_to_all: &mut Option<ConflictResolution>,
) -> Result<u64, WriteOperationError> {
    // Pretend to record a created file and a created dir to exercise the
    // `&mut` captures. Returns "bytes written".
    transaction.record_file(source.to_path_buf());
    created_dirs.insert(source.to_path_buf());
    let _ = apply_to_all; // touched to prove the capture
    Ok(0)
}

/// Pretend caller — this is the body that would replace the loop in
/// `copy_files_with_progress_inner` post-migration.
fn pretend_copy_outer(
    events: &dyn OperationEventSink,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
    files: &[FileInfo],
    destination: &Path,
) -> Result<(), WriteOperationError> {
    // ALL of these live in the outer scope (today: `copy_files_with_progress_inner`),
    // captured by the closure passed to the driver.
    let mut transaction = CopyTransaction::new();
    let mut created_dirs: HashSet<PathBuf> = HashSet::new();
    let mut apply_to_all: Option<ConflictResolution> = None;
    let mut tracker = SourceItemTracker::new(files);

    // (a) PROOF: closure captures `&mut transaction` AND `&mut created_dirs`
    //           AND `&mut apply_to_all` AND `&mut tracker` simultaneously,
    //           under the `FnMut` bound of the sync driver.
    // (c) PROOF: closure invokes `tracker.record(file_info)`; when it returns
    //           `Some(source)`, the closure (NOT the driver) emits
    //           `write-source-item-done`. The driver never sees the tracker.
    let outcome = drive_transfer_serial_sync(
        events,
        state,
        operation_id,
        files,
        destination,
        |ctx: TransferContext<'_>| -> Result<TransferOutcome, WriteOperationError> {
            // Capture &mut transaction, &mut created_dirs, &mut apply_to_all
            let bytes = copy_single_item_pseudo(
                ctx.events,
                ctx.state,
                ctx.operation_id,
                &ctx.file_info.path,
                ctx.file_info.dest_path(ctx.destination),
                &mut transaction,
                &mut created_dirs,
                &mut apply_to_all,
            )?;

            // (c) PROOF: tracker.record + emit happens HERE in the closure, not in the driver.
            if let Some(source_path) = tracker.record(ctx.file_info) {
                ctx.events.emit_source_item_done(WriteSourceItemDoneEvent {
                    operation_id: ctx.operation_id.to_string(),
                    source_path: source_path.display().to_string(),
                });
            }

            Ok(TransferOutcome::Done { bytes })
        },
    )?;

    // (b) PROOF: post-loop rollback decision runs HERE, in the caller — NOT inside
    //           the driver. The driver returned `outcome` carrying intent + counters;
    //           we match on it and decide whether to call `rollback_with_progress`
    //           with `transaction.created_files` (accumulated during the loop).
    match outcome.intent {
        TransferLoopIntent::Completed => {
            transaction.commit();
            Ok(())
        }
        TransferLoopIntent::Cancelled => {
            // Today's copy.rs checks `load_intent(&state.intent)` here to decide
            // between `Stopped` (keep partial files) and `RollingBack` (delete them
            // with progress events). The driver returned control; we have full
            // access to the transaction and can call the existing
            // `rollback_with_progress(&transaction, events, …)` exactly as today.
            // No driver-side callback needed.
            let _files_to_potentially_rollback: usize = transaction.created_files.len();
            // pretend_rollback_with_progress(&transaction, events, operation_id, ...);
            transaction.commit();
            Ok(())
        }
        TransferLoopIntent::Failed(e) => {
            transaction.rollback();
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Bonus: the async/`AsyncFnMut`-shaped variant for the volume-copy serial path.
// Sanity check — does the same closure shape work for the async entry point?
// ---------------------------------------------------------------------------

/// The async sister of `drive_transfer_serial_sync`. The plan calls this out
/// for `copy_volumes_with_progress`'s serial fallback path. We only sketch the
/// signature here to confirm the bound type-checks; we don't migrate
/// `copy_volumes_with_progress` itself in this prototype.
async fn drive_transfer_serial_async<F>(
    _events: &dyn OperationEventSink,
    _state: &Arc<WriteOperationState>,
    _operation_id: &str,
    files: &[FileInfo],
    _destination: &Path,
    mut transfer_one: F,
) -> Result<TransferLoopOutcome, WriteOperationError>
where
    F: AsyncFnMut(TransferContext<'_>) -> Result<TransferOutcome, WriteOperationError>,
{
    let mut files_done = 0usize;
    let mut bytes_done = 0u64;

    for fi in files {
        let ctx = TransferContext {
            events: _events,
            state: _state,
            operation_id: _operation_id,
            operation_type: WriteOperationType::Copy,
            destination: _destination,
            file_info: fi,
        };
        match transfer_one(ctx).await? {
            TransferOutcome::Done { bytes } => {
                files_done += 1;
                bytes_done += bytes;
            }
            TransferOutcome::Skipped { bytes_accounted } => {
                files_done += 1;
                bytes_done += bytes_accounted;
            }
        }
    }

    Ok(TransferLoopOutcome {
        files_done,
        bytes_done,
        intent: TransferLoopIntent::Completed,
    })
}

/// Pretend caller using the async entry point, with the SAME captures pattern.
/// This stands in for what `copy_volumes_with_progress`'s serial fallback would
/// look like — but importantly it does NOT capture `&mut transaction` (volume
/// copy doesn't use `CopyTransaction`). Captured: a hypothetical mutable
/// "partial cleanup list" that volume copy maintains.
async fn pretend_volume_copy_serial_outer(
    events: &dyn OperationEventSink,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
    files: &[FileInfo],
    destination: &Path,
) -> Result<(), WriteOperationError> {
    let mut partials_to_cleanup: Vec<PathBuf> = Vec::new();

    let _outcome = drive_transfer_serial_async(
        events,
        state,
        operation_id,
        files,
        destination,
        // `async |…|` is the Rust 2024 async-closure syntax. This is REQUIRED
        // for `&mut` captures to work with `AsyncFnMut`: the `|x| async { }`
        // form returns an async block that borrows the closure's captures,
        // which `FnMut` won't permit to escape the closure body. `async |x|`
        // produces a proper async closure whose returned future is tied to
        // `&mut self` of the closure call, which is exactly what `AsyncFnMut`
        // wants. This is the key practical finding for any async driver.
        async |ctx: TransferContext<'_>| -> Result<TransferOutcome, WriteOperationError> {
            partials_to_cleanup.push(ctx.file_info.path.clone());
            Ok(TransferOutcome::Done { bytes: 0 })
        },
    )
    .await?;

    // Use the capture post-loop to silence "unused" and prove it survives the driver call.
    let _n = partials_to_cleanup.len();
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests — these don't run real logic; they just force the types to be
// instantiated so any bound regression shows up at `cargo check` time.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::write_operations::types::CollectorEventSink;

    fn make_state() -> Arc<WriteOperationState> {
        Arc::new(WriteOperationState::new(std::time::Duration::from_millis(200)))
    }

    #[test]
    fn proves_a_and_c_typecheck_for_sync_copy_path() {
        let sink = CollectorEventSink::new();
        let state = make_state();
        let files: Vec<FileInfo> = Vec::new();
        let dest = PathBuf::from("/tmp/proto");
        // We don't actually call the closure (files is empty); we only force
        // the types through.
        let _ = pretend_copy_outer(&sink, &state, "op-1", &files, &dest);
    }

    #[tokio::test]
    async fn proves_async_variant_typechecks_for_volume_path() {
        let sink = CollectorEventSink::new();
        let state = make_state();
        let files: Vec<FileInfo> = Vec::new();
        let dest = PathBuf::from("/tmp/proto");
        let _ = pretend_volume_copy_serial_outer(&sink, &state, "op-2", &files, &dest).await;
    }
}
