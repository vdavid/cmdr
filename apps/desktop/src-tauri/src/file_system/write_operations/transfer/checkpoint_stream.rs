//! The between-window cooperative checkpoint for cross-volume streaming copies.
//!
//! `CheckpointStream` is a `VolumeReadStream` decorator that `volume_strategy`'s
//! `stream_pipe_file` wraps the source stream in. Its `next_chunk()` runs a
//! checkpoint once per chunk (park-while-paused, foreground auto-yield, then a
//! cooperative `yield_now`) before delegating — the sync per-chunk `on_progress`
//! callback can't `.await`, so this is what makes a paused copy stop advancing
//! MID-FILE and keeps a long single-file transfer from starving foreground work.
//! Full design (pause, foreground auto-yield, byte exactness, cancel-awareness):
//! [`super::DETAILS.md`] §§ "Pause … chunks", "Foreground auto-yield".

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::super::state::WriteOperationState;
use crate::file_system::volume::{Volume, VolumeError, VolumeReadStream};

/// Wraps a source read stream so a between-chunk cooperative checkpoint runs once
/// per chunk for the cross-volume streaming path, where the per-chunk progress
/// callback (`on_progress`) is sync and so can't `.await` to park or yield.
///
/// Since reads from a session-scarce backend (MTP) are bounded windows that hold
/// nothing between chunks (the device's PTP session is free between windows — see
/// `file_system/volume/backends/mtp.rs`), a pause or a foreground yield is simply
/// **don't start the next window**: park before delegating the next
/// `next_chunk().await`, then resume reading from the current offset. There's no
/// session to release and nothing to reopen. Before each `next_chunk().await`, the
/// wrapper:
///
/// 1. **Parks while paused** via `pause_gate.wait_while_paused_async`. The
///    checkpoint runs at a chunk boundary — the previous chunk is fully written
///    and the next is not yet read — so a paused op holds only its in-flight
///    `.cmdr-tmp-<uuid>`, never a torn target. Resume or cancel unblocks it
///    (`wait_while_paused_async` returns the instant cancel is observed;
///    cancellation wins over pause). This is identical for every backend — pause
///    is always park-in-place now.
/// 2. **Auto-yields the device to foreground work** for sources that opt in via
///    `Volume::supports_foreground_yield()` (MTP only). When a foreground op (a
///    listing / nav on the phone) is pending on the source device, the checkpoint
///    behaves like the index scan's `background_yield_point`: it parks (does NOT
///    start the next window) until foreground drains plus a short debounce window,
///    then lets the next window proceed — all WITHOUT a user pause, and the op
///    stays `Running` (this is a transient device yield, not user intent). Between
///    windows the copy already holds no session, so a foreground listing slips in
///    at its natural cost. A minimum-progress floor (`min_progress_floor`) gates
///    the arm so continuous foreground nav can't starve the copy to zero
///    throughput: after a resume the transfer must move at least
///    `min_progress_floor` bytes before honoring the next yield. The debounce
///    (`foreground_debounce`) collapses a burst of listings into ONE park instead
///    of one per window.
/// 3. **Yields cooperatively** (`tokio::task::yield_now`) so a long transfer
///    doesn't starve foreground tasks (listings, navigation, progress emits) on
///    the runtime.
///
/// **Byte exactness.** The wrapper counts `bytes_yielded` and never drops,
/// double-reads, or reorders bytes — it passes each inner chunk through untouched
/// and forwards `total_size()` unchanged. Parking between windows leaves the
/// current offset alone, so the next window reads `[offset, …)` with no gap or
/// overlap. The destination's `write_from_stream` (and its safe-replace
/// temp+rename) is untouched.
///
/// Cancellation is NOT enforced here: the backend's existing `on_progress`
/// `is_cancelled` check after each write owns the cancel-then-cleanup ordering
/// (drop the handle, remove the partial). A cancel observed while parked unblocks
/// promptly and lets the next chunk flow through to that same `on_progress` check.
pub(super) struct CheckpointStream {
    /// The open source stream. Always present — pause and foreground yield park in
    /// place between bounded windows; nothing is ever released or reopened.
    inner: Box<dyn VolumeReadStream>,
    state: Arc<WriteOperationState>,
    /// Bytes this wrapper has yielded so far == the destination temp's length.
    bytes_yielded: u64,
    /// The source volume, so the foreground auto-yield arm can probe its device
    /// gate (`supports_foreground_yield`, `foreground_pending`,
    /// `wait_until_foreground_idle`). Park-in-place backends (the default
    /// `supports_foreground_yield() == false`) make the arm a no-op.
    source_volume: Arc<dyn Volume>,
    /// `bytes_yielded` at the last resume (initial open = 0, then after each
    /// foreground yield). The min-progress floor is measured from here: the
    /// auto-yield arm only fires once `bytes_yielded - last_resume_offset >=
    /// min_progress_floor`, so continuous foreground nav can't starve the copy.
    last_resume_offset: u64,
    /// Quiet window the auto-yield waits for before starting the next window.
    /// Field (not a bare constant) so tests can set it ≈ 0 for determinism;
    /// defaults to `FOREGROUND_YIELD_DEBOUNCE` (see `volume_strategy`).
    foreground_debounce: Duration,
    /// Bytes the transfer must advance after a resume before honoring the next
    /// foreground yield. Field (not a bare constant) so tests can set a small
    /// floor for determinism; defaults to `MIN_PROGRESS_FLOOR_BYTES` (see
    /// `volume_strategy`).
    min_progress_floor: u64,
}

impl VolumeReadStream for CheckpointStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            self.checkpoint().await;
            match self.inner.next_chunk().await {
                Some(Ok(chunk)) => {
                    self.bytes_yielded += chunk.len() as u64;
                    Some(Ok(chunk))
                }
                other => other,
            }
        })
    }

    fn total_size(&self) -> u64 {
        self.inner.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_yielded
    }
}

impl CheckpointStream {
    /// Wrap `inner` with the between-window checkpoint. `foreground_debounce` and
    /// `min_progress_floor` come from `volume_strategy::auto_yield_tuning()` (the
    /// production constants, or a test override); `bytes_yielded` and
    /// `last_resume_offset` start at 0 (a fresh open at offset 0).
    pub(super) fn new(
        inner: Box<dyn VolumeReadStream>,
        state: Arc<WriteOperationState>,
        source_volume: Arc<dyn Volume>,
        foreground_debounce: Duration,
        min_progress_floor: u64,
    ) -> Self {
        Self {
            inner,
            state,
            bytes_yielded: 0,
            source_volume,
            last_resume_offset: 0,
            foreground_debounce,
            min_progress_floor,
        }
    }

    /// Run the between-window pause checkpoint and the foreground auto-yield, then
    /// yield cooperatively.
    async fn checkpoint(&mut self) {
        // Park between windows while paused (returns immediately on cancel). Pause
        // is park-in-place for every backend: a bounded-window read holds nothing
        // between windows, so there's no scarce resource to release.
        self.state.pause_gate.wait_while_paused_async(&self.state.intent).await;

        // Foreground auto-yield: when a foreground op (listing / nav on the phone)
        // is pending on the source device, don't start the next window until it
        // drains — WITHOUT a user pause, op stays Running. The copy already holds
        // no session between windows, so the foreground op slips in at its natural
        // cost; this arm just keeps the copy from immediately re-grabbing the lock
        // and starving foreground.
        self.auto_yield_to_foreground().await;

        // Yield so foreground tasks get scheduled during a long copy.
        tokio::task::yield_now().await;
    }

    /// The foreground auto-yield arm (step 2 in the wrapper's doc). No-op unless
    /// the source opts in (`supports_foreground_yield()`, MTP only) and a
    /// foreground op is actually pending on its device. Parks ("don't start the
    /// next window") until foreground drains plus a debounce window, then returns
    /// so the next `next_chunk` reads the next window from the current offset.
    async fn auto_yield_to_foreground(&mut self) {
        // The enable-switch is the source's own opt-in, NOT a release/reopen
        // proxy: park-in-place backends (local FS, SMB, in-memory) default to
        // `false` and never auto-yield.
        if !self.source_volume.supports_foreground_yield() {
            return;
        }
        if super::super::state::is_cancelled(&self.state.intent) {
            return; // cancel owns teardown; never start a yield while cancelled
        }
        // At EOF there's nothing left to yield for; let the copy finish.
        if self.bytes_yielded >= self.inner.total_size() {
            return;
        }
        // Min-progress floor: after a resume, transfer at least `min_progress_floor`
        // bytes before honoring the next yield, so continuous foreground nav can't
        // starve the copy to zero throughput.
        if self.bytes_yielded.saturating_sub(self.last_resume_offset) < self.min_progress_floor {
            return;
        }
        // Cheap probe (an atomic load behind the device gate); skip the park
        // entirely when nothing foreground is waiting.
        if !self.source_volume.foreground_pending().await {
            return;
        }

        // Clone the source handle so the park loop borrows it, not `self` (we
        // mutate `self.last_resume_offset` after the loop). Arc clone is cheap.
        let source_volume = Arc::clone(&self.source_volume);

        // Debounce: wait for foreground to drain, then a quiet window; if a new
        // foreground op arrives during the window, re-park. This collapses a BURST
        // of listings into ONE suspension instead of re-checking every window. The
        // whole loop is cancel-aware (a cancel breaks out promptly and lets the
        // backend's `on_progress` own cleanup).
        loop {
            if super::super::state::is_cancelled(&self.state.intent) {
                break;
            }
            // Park until the device is clear of foreground work, but RACE it
            // against cancellation: `wait_until_foreground_idle` only returns once
            // foreground drains, and a cancel doesn't clear the foreground signal,
            // so without this race a cancel-while-yielding would hang until the
            // (unrelated) foreground op happens to finish.
            tokio::select! {
                () = source_volume.wait_until_foreground_idle() => {}
                () = poll_until_cancelled(&self.state.intent) => break,
            }
            if super::super::state::is_cancelled(&self.state.intent) {
                break;
            }
            // Stay parked for the quiet window. If foreground becomes pending
            // again before it elapses (a burst), loop and re-drain.
            if sleep_cancel_aware(&self.state.intent, self.foreground_debounce).await {
                break; // cancelled during the wait
            }
            if !source_volume.foreground_pending().await {
                break; // quiet for the full window ⇒ resume
            }
        }
        // We're resuming: restart the min-progress floor from here so the next
        // auto-yield can only fire after another `min_progress_floor` bytes.
        self.last_resume_offset = self.bytes_yielded;
    }
}

/// Sleep for `dur`, returning early if the operation is cancelled. Returns
/// `true` if it bailed on a cancel, `false` if the full window elapsed.
///
/// Cancel-awareness matters: a cancel during the auto-yield debounce wait must
/// not be slept through. We slice the sleep and re-check `is_cancelled` between
/// slices, so a cancel is observed within at most one slice. (A zero/near-zero
/// debounce — what tests inject — returns at the first check, before any sleep.)
///
/// Free function (not a method) so it borrows only the cancel atomic, not
/// `&CheckpointStream`: a `&self` held across an `.await` would force the
/// wrapper's `next_chunk` future to require `CheckpointStream: Sync`, which it
/// isn't (it holds a non-`Sync` `dyn VolumeReadStream`).
async fn sleep_cancel_aware(intent: &std::sync::atomic::AtomicU8, dur: Duration) -> bool {
    const SLICE: Duration = Duration::from_millis(20);
    let mut remaining = dur;
    loop {
        if super::super::state::is_cancelled(intent) {
            return true;
        }
        if remaining.is_zero() {
            return false;
        }
        let step = remaining.min(SLICE);
        tokio::time::sleep(step).await;
        remaining = remaining.saturating_sub(step);
    }
}

/// Resolves once the operation is cancelled; otherwise never. Used to RACE the
/// auto-yield's `wait_until_foreground_idle` against a cancel via `select!`: the
/// device gate only releases when foreground drains, and a cancel doesn't clear
/// the foreground signal, so a cancel-while-parked needs a separate waker. We
/// poll the intent (a cheap relaxed atomic load) on a short tick rather than
/// reach into the gate's internals; the latency is bounded by the tick.
async fn poll_until_cancelled(intent: &std::sync::atomic::AtomicU8) {
    const TICK: Duration = Duration::from_millis(20);
    while !super::super::state::is_cancelled(intent) {
        tokio::time::sleep(TICK).await;
    }
}
