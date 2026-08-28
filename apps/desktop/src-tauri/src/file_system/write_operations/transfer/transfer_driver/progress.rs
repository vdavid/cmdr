//! Per-file progress callback builders shared by the volume transfer paths.

use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::file_system::write_operations::event_sinks::OperationEventSink;
use crate::file_system::write_operations::state::{WriteOperationState, is_cancelled};
use crate::file_system::write_operations::types::{WriteOperationPhase, WriteOperationType};
use crate::ignore_poison::IgnorePoison;

use super::emit_progress_and_status;

/// Leaf-granular progress accounting for the **serial** transfer paths
/// (`volume::copy::copy_volumes_with_progress` serial path and
/// `volume::r#move::move_volumes_with_progress`, one source in flight at a time).
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
    /// The furthest the IN-FLIGHT leaf has reported, so a leaf that restarts at
    /// byte zero can't walk the Size bar backwards.
    ///
    /// A file that hits a transport blip is run again from its first byte
    /// (`retry.rs`), so `file_bytes_done` legitimately drops to 0 mid-leaf. The
    /// bar must not: dropping from 4 MiB back to 0 reads as data being lost, and
    /// the ETA estimator would take the reversal as negative throughput. Reporting
    /// the high-water mark keeps the number monotonic AND keeps it honest at the
    /// end, because `on_leaf_complete` adds the leaf's exact size once, whatever
    /// the attempt count. Reset per leaf, so the next (possibly much smaller) one
    /// isn't held up by this one's mark.
    leaf_high_water: AtomicU64,
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
            leaf_high_water: AtomicU64::new(0),
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
        // The high-water mark, not the raw count: see `leaf_high_water`.
        let leaf_done = self
            .leaf_high_water
            .fetch_max(file_bytes_done, Ordering::Relaxed)
            .max(file_bytes_done);
        let current_total = self.byte_base.load(Ordering::Relaxed) + leaf_done;
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
        // The next leaf starts from its own first byte.
        self.leaf_high_water.store(0, Ordering::Relaxed);
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

    /// Per-leaf `on_file_skipped` callback: a child the conflict policy declined.
    ///
    /// Credits the same two counters a completed leaf does, because a skipped
    /// child IS done and both bars have to reach their totals. `note_skipped`
    /// is what keeps it out of the rate: see
    /// [`WriteOperationState::note_skipped`].
    ///
    /// ❗ Throttled, unlike [`on_leaf_complete`](Self::on_leaf_complete). That
    /// one bypasses the throttle so a single large leaf's `N/N` always lands;
    /// skips arrive in the opposite shape — a merge into a folder the user
    /// already has can decline tens of thousands of children back to back, and
    /// an unthrottled emit apiece is a flood of IPC nobody reads. The
    /// operation's completion event carries the final tally regardless.
    pub(in crate::file_system::write_operations::transfer) fn on_leaf_skipped(&self, leaf_bytes: u64) {
        let new_total = self.byte_base.fetch_add(leaf_bytes, Ordering::Relaxed) + leaf_bytes;
        self.leaf_high_water.store(0, Ordering::Relaxed);
        let new_files = self.files_done.fetch_add(1, Ordering::Relaxed) + 1;
        self.state.note_skipped(1, leaf_bytes);
        // The bool says whether the throttle let this tick through, and a skip has
        // nothing to do about either answer: the counters are credited above, and
        // the completion event carries the final tally whatever the throttle ate.
        // allowed-discarded-outcome: whether this tick was throttled changes nothing here.
        try_emit_throttled_progress(
            &*self.events,
            &self.state,
            &self.operation_id,
            self.operation_type,
            self.file_name.clone(),
            new_files,
            self.total_files,
            new_total,
            self.total_bytes,
            &self.last_emit,
            self.progress_interval,
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
/// before rolling into the shared `bytes_done_atomic`. It holds the
/// file's HIGH-WATER mark rather than its latest report, so a file that
/// restarts at byte zero on a retry credits its bytes exactly once.
/// Callers must allocate a fresh `AtomicU64` per spawned task; the caller
/// can also inspect `last_file_bytes.load() == 0` after the task finishes
/// to detect volumes that never invoked `on_progress` and credit the
/// file's bytes to the aggregate as a compensation.
///
/// Used by: `volume::copy::copy_volumes_with_progress` concurrent path.
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
        // `fetch_max`, NOT `swap`: a file that is run again after a transport
        // blip (`retry.rs`) restarts at byte zero, and a `swap` would lower the
        // watermark and then credit the whole re-streamed prefix a second time —
        // a silent over-count of the operation's byte total, and a Size bar that
        // reaches 100% before the copy does.
        let prev = last_file_bytes.fetch_max(file_bytes_done, Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::write_operations::event_sinks::CollectorEventSink;
    use crate::file_system::write_operations::test_support::TestOperationGuard;

    /// The bytes the FE was told about, in order.
    fn totals(sink: &CollectorEventSink) -> Vec<u64> {
        sink.progress
            .lock_ignore_poison()
            .iter()
            .map(|e| e.bytes_done)
            .collect()
    }

    fn leaf_progress(guard: &TestOperationGuard, sink: &Arc<CollectorEventSink>, base: u64) -> Arc<SerialLeafProgress> {
        SerialLeafProgress::new(
            Arc::clone(sink) as Arc<dyn OperationEventSink>,
            Arc::clone(guard.state()),
            guard.id().to_owned(),
            WriteOperationType::Copy,
            None,
            base,
            Arc::new(AtomicUsize::new(0)),
            10,
            1_000_000,
            Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1))),
            // No throttle: every call must be observable.
            Duration::ZERO,
        )
    }

    /// A file that is run again after a transport blip restarts at byte zero
    /// (`retry.rs`), so the leaf's own counter goes backwards. What the user sees
    /// must not: the Size bar dropping from 4 MiB back to 0 and climbing again
    /// reads as data being lost, and the ETA estimator would take the reversal as
    /// negative throughput.
    #[test]
    fn a_retried_leaf_never_walks_the_size_bar_backwards() {
        let guard = TestOperationGuard::register("leaf-retry-progress");
        let sink = Arc::new(CollectorEventSink::new());
        let progress = leaf_progress(&guard, &sink, 1_000);

        // First attempt gets a third of the way in, then the blip.
        let _ = progress.on_chunk(2_000);
        let _ = progress.on_chunk(4_000);
        // The retry starts over from zero.
        let _ = progress.on_chunk(1_000);
        let _ = progress.on_chunk(4_000);
        let _ = progress.on_chunk(6_000);
        progress.on_leaf_complete(6_000);

        let seen = totals(&sink);
        assert!(
            seen.windows(2).all(|w| w[1] >= w[0]),
            "the reported byte total must never go backwards across a retry: {seen:?}"
        );
        assert_eq!(
            *seen.last().expect("the leaf milestone emits"),
            7_000,
            "the finished leaf must be counted exactly once: base 1000 + 6000 bytes"
        );
    }

    /// The concurrent path's callback rolls a per-file DELTA into a shared
    /// operation total, so a retried file must not credit the same bytes twice.
    /// `saturating_sub` already gives that for free — this pins it, because the
    /// obvious "simplify" (a plain subtraction, or storing then adding) would
    /// double-count a restart in a way nothing else would catch.
    #[test]
    fn a_retried_file_credits_its_bytes_to_the_operation_total_exactly_once() {
        let guard = TestOperationGuard::register("concurrent-retry-progress");
        let sink = Arc::new(CollectorEventSink::new());
        let op_total = Arc::new(AtomicU64::new(0));
        let callback = make_concurrent_per_file_progress(
            Arc::clone(&sink) as Arc<dyn OperationEventSink>,
            Arc::clone(guard.state()),
            guard.id().to_owned(),
            WriteOperationType::Copy,
            None,
            Arc::new(AtomicU64::new(0)),
            Arc::clone(&op_total),
            Arc::new(AtomicUsize::new(0)),
            10,
            1_000_000,
            Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1))),
            Duration::ZERO,
        );

        // First attempt reaches 6 KB, then the blip; the retry starts over and
        // runs the file to its full 9 KB.
        for done in [3_000, 6_000] {
            let _ = callback(done, 9_000);
        }
        for done in [3_000, 6_000, 9_000] {
            let _ = callback(done, 9_000);
        }

        assert_eq!(
            op_total.load(Ordering::Relaxed),
            9_000,
            "one 9 KB file must add 9 KB to the operation total, however many attempts it took"
        );
        let seen = totals(&sink);
        assert!(
            seen.windows(2).all(|w| w[1] >= w[0]),
            "the operation total must never go backwards: {seen:?}"
        );
    }

    /// And the next leaf starts from its own zero: the previous leaf's high-water
    /// mark must not hold this one's bar up while it climbs.
    #[test]
    fn the_next_leaf_measures_from_its_own_first_byte() {
        let guard = TestOperationGuard::register("leaf-retry-next");
        let sink = Arc::new(CollectorEventSink::new());
        let progress = leaf_progress(&guard, &sink, 0);

        let _ = progress.on_chunk(5_000);
        progress.on_leaf_complete(5_000);
        // A second, much smaller leaf.
        let _ = progress.on_chunk(100);

        assert_eq!(
            totals(&sink).last().copied(),
            Some(5_100),
            "the second leaf's bytes must add to the first leaf's total, not be swallowed by its high-water mark"
        );
    }
}
