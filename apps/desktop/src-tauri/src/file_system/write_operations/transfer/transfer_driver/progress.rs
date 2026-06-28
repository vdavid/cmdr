//! Per-file progress callback builders shared by the volume transfer paths.

use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::file_system::write_operations::state::{WriteOperationState, is_cancelled};
use crate::file_system::write_operations::types::{OperationEventSink, WriteOperationPhase, WriteOperationType};
use crate::ignore_poison::IgnorePoison;

use super::emit_progress_and_status;

/// Leaf-granular progress accounting for the **serial** transfer paths
/// (`volume_copy::copy_volumes_with_progress` serial path and
/// `volume_move::move_volumes_with_progress`, one source in flight at a time).
///
/// A single top-level source can expand to many leaf files (a directory copies
/// its whole subtree through ONE `copy_single_path` call, reusing ONE
/// `on_file_progress` / `on_file_complete` pair across every inner file). The
/// progress bars are leaf-granular: `bytes_total` and `files_total` are the
/// preflight LEAF counts, so the emitted `bytes_done` / `files_done` must climb
/// across leaves too. This type owns that running tally so both bars advance
/// smoothly through a directory instead of resetting at every inner file:
///
/// - `byte_base` seeds from the driver's per-iteration `bytes_done_so_far`
///   (cumulative bytes of all PRIOR top-level sources, including bulk-skipped
///   ones) and `on_leaf_complete` adds each finished leaf's exact byte count.
///   `on_chunk` then emits `byte_base + file_bytes_done` for the in-flight leaf,
///   so the Size bar never sees a per-leaf reset.
/// - `files_done` is the OPERATION-WIDE leaf counter (shared across every
///   source via `Arc`), bumped once per completed leaf. The File bar climbs
///   0 → N across the whole op, not 0 → (top-level source count).
///
/// Both closures built from it (see the call sites) are `'static + Send + Sync`
/// — safe to pass through `copy_single_path`'s `&dyn Fn(...)` parameters from
/// inside an async move-block executed across `tokio::spawn` boundaries.
pub(in crate::file_system::write_operations::transfer) struct SerialLeafProgress {
    events: Arc<dyn OperationEventSink>,
    state: Arc<WriteOperationState>,
    operation_id: String,
    operation_type: WriteOperationType,
    file_name: Option<String>,
    /// Cumulative bytes already committed: prior top-level sources (seed) plus
    /// every leaf of THIS source that `on_leaf_complete` has finished.
    byte_base: AtomicU64,
    /// Operation-wide completed-leaf counter, shared across all sources.
    files_done: Arc<AtomicUsize>,
    total_files: usize,
    total_bytes: u64,
    last_emit: Arc<Mutex<Instant>>,
    progress_interval: Duration,
}

impl SerialLeafProgress {
    #[allow(
        clippy::too_many_arguments,
        reason = "matches WriteProgressEvent shape; bundling into a context struct adds ceremony without cleaning anything up"
    )]
    pub(in crate::file_system::write_operations::transfer) fn new(
        events: Arc<dyn OperationEventSink>,
        state: Arc<WriteOperationState>,
        operation_id: String,
        operation_type: WriteOperationType,
        file_name: Option<String>,
        bytes_done_so_far: u64,
        files_done: Arc<AtomicUsize>,
        total_files: usize,
        total_bytes: u64,
        last_emit: Arc<Mutex<Instant>>,
        progress_interval: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            events,
            state,
            operation_id,
            operation_type,
            file_name,
            byte_base: AtomicU64::new(bytes_done_so_far),
            files_done,
            total_files,
            total_bytes,
            last_emit,
            progress_interval,
        })
    }

    /// Per-chunk `on_file_progress` callback. `file_bytes_done` is the in-flight
    /// leaf's running byte count (0 → leaf size). Throttled; returns `Break` to
    /// abort the write on cancel.
    pub(in crate::file_system::write_operations::transfer) fn on_chunk(&self, file_bytes_done: u64) -> ControlFlow<()> {
        if is_cancelled(&self.state.intent) {
            return ControlFlow::Break(());
        }
        let current_total = self.byte_base.load(Ordering::Relaxed) + file_bytes_done;
        try_emit_throttled_progress(
            &*self.events,
            &self.state,
            &self.operation_id,
            self.operation_type,
            self.file_name.clone(),
            self.files_done.load(Ordering::Relaxed),
            self.total_files,
            current_total,
            self.total_bytes,
            &self.last_emit,
            self.progress_interval,
        );
        ControlFlow::Continue(())
    }

    /// Per-leaf `on_file_complete` callback: roll the finished leaf's exact byte
    /// count into `byte_base` and bump the operation-wide leaf counter, then emit
    /// a milestone that BYPASSES the throttle so the bumped `files_done` always
    /// reaches the FE — chunked emits inside the file carry the pre-completion
    /// counter, so without this a single large leaf would never cross `N/N`.
    pub(in crate::file_system::write_operations::transfer) fn on_leaf_complete(&self, leaf_bytes: u64) {
        let new_total = self.byte_base.fetch_add(leaf_bytes, Ordering::Relaxed) + leaf_bytes;
        let new_files = self.files_done.fetch_add(1, Ordering::Relaxed) + 1;
        *self.last_emit.lock_ignore_poison() = Instant::now();
        emit_progress_and_status(
            &*self.events,
            &self.state,
            &self.operation_id,
            self.operation_type,
            WriteOperationPhase::Copying,
            self.file_name.clone(),
            new_files,
            self.total_files,
            new_total,
            self.total_bytes,
        );
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
pub(in crate::file_system::write_operations::transfer) fn make_concurrent_per_file_progress(
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
