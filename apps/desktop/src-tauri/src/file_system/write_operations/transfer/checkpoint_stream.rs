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
use std::time::{Duration, Instant};

use super::super::state::WriteOperationState;
use crate::file_system::volume::{Volume, VolumeError, VolumeReadStream};

/// How often the destination-side bounded park re-checks the share's foreground
/// signal (and cancellation). Short so that when the user stops browsing, the
/// upload resumes within roughly one slice of the share's idle threshold, not a
/// full hard-cap window.
const DEST_PARK_POLL_SLICE: Duration = Duration::from_millis(50);

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
    /// The DESTINATION volume, so the bounded destination-yield arm can probe its
    /// per-share `foreground_pending`. This is the UPLOAD counterpart to the
    /// source arm: for a local → SMB copy the source doesn't opt in, but the SMB
    /// destination does (`supports_foreground_yield_as_destination`), so a running
    /// upload stands aside for the user browsing the same share. Backends that
    /// don't opt in (the default `false`) make the arm a no-op.
    dest_volume: Arc<dyn Volume>,
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
    /// Hard cap on a SINGLE destination-side park. Load-bearing data-safety bound:
    /// the upload holds an open SMB write handle across the park, so it must
    /// resume (and write, keeping the handle warm) at least this often even under
    /// continuous browsing. Field (not a bare constant) so tests can set it small
    /// for determinism; defaults to `DEST_FOREGROUND_YIELD_HARD_CAP` (see
    /// `volume_strategy`). ❌ Don't turn this into an unbounded wait; see
    /// `dest_park_continues` and `Volume::supports_foreground_yield_as_destination`.
    dest_yield_hard_cap: Duration,
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
    /// Wrap `inner` with the between-window checkpoint. `foreground_debounce`,
    /// `min_progress_floor`, and `dest_yield_hard_cap` come from
    /// `volume_strategy::auto_yield_tuning()` (the production constants, or a test
    /// override); `bytes_yielded` and `last_resume_offset` start at 0 (a fresh
    /// open at offset 0).
    pub(super) fn new(
        inner: Box<dyn VolumeReadStream>,
        state: Arc<WriteOperationState>,
        source_volume: Arc<dyn Volume>,
        dest_volume: Arc<dyn Volume>,
        foreground_debounce: Duration,
        min_progress_floor: u64,
        dest_yield_hard_cap: Duration,
    ) -> Self {
        Self {
            inner,
            state,
            bytes_yielded: 0,
            source_volume,
            dest_volume,
            last_resume_offset: 0,
            foreground_debounce,
            min_progress_floor,
            dest_yield_hard_cap,
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

        // Destination-side counterpart, for an UPLOAD to a share the user is
        // browsing (local → SMB). The source arm above is a no-op there (the local
        // source doesn't opt in); this arm stands aside for the destination share
        // instead. It is BOUNDED because the write holds an open handle across the
        // pause (see the method).
        self.bounded_yield_to_dest_foreground().await;

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

    /// The DESTINATION-side bounded foreground yield: the UPLOAD counterpart to
    /// `auto_yield_to_foreground`. No-op unless the DESTINATION opts in
    /// (`supports_foreground_yield_as_destination()`, SMB only) and a foreground
    /// op is pending on the destination share. Stands aside for the user browsing
    /// that share, in short slices, but HARD-CAPPED at `dest_yield_hard_cap`.
    ///
    /// The cap is the data-safety bound: an upload holds an OPEN SMB write handle
    /// across the park (the source read wrapped here sits between two
    /// `writer.write_chunk` calls in the destination's `write_from_stream`). An
    /// unbounded park would let that handle sit idle long enough for the server to
    /// reap it, breaking the transfer. Resuming at the cap writes the next chunk,
    /// keeping the handle warm; the offset is untouched, so no desync. ❌ Don't
    /// convert this to `wait_until_foreground_idle` (the unbounded source path).
    async fn bounded_yield_to_dest_foreground(&mut self) {
        // Enable-switch: the DESTINATION's own opt-in. Non-opting targets (local
        // FS, in-memory, and MTP, whose one `SendObject` transaction can't pause
        // mid-write) default to false and never park here.
        if !self.dest_volume.supports_foreground_yield_as_destination() {
            return;
        }
        if super::super::state::is_cancelled(&self.state.intent) {
            return; // cancel owns teardown; never start a yield while cancelled
        }
        // At EOF there's nothing left to write; let the copy finalize (the
        // safe-replace rename must not wait behind a park).
        if self.bytes_yielded >= self.inner.total_size() {
            return;
        }
        // Min-progress floor: after a resume, write at least `min_progress_floor`
        // bytes before honoring the next yield, so continuous browsing can't
        // starve the upload to zero throughput. Shared with the source arm; in
        // practice only one arm is active per transfer (source XOR destination is
        // the SMB side).
        if self.bytes_yielded.saturating_sub(self.last_resume_offset) < self.min_progress_floor {
            return;
        }
        // Cheap probe (a per-share timestamp read); skip the park when the share
        // is quiet.
        if !self.dest_volume.foreground_pending().await {
            return;
        }

        // Bounded park: stand aside in short slices while the share stays busy,
        // but never past the hard cap. Cancel-aware throughout, so a cancel while
        // parked unblocks promptly and the next chunk flows to the backend's
        // `on_progress` cleanup.
        let park_start = Instant::now();
        loop {
            if super::super::state::is_cancelled(&self.state.intent) {
                break;
            }
            let pending = self.dest_volume.foreground_pending().await;
            if !dest_park_continues(pending, park_start.elapsed(), self.dest_yield_hard_cap) {
                break;
            }
            if sleep_cancel_aware(&self.state.intent, DEST_PARK_POLL_SLICE).await {
                break; // cancelled during the slice
            }
        }
        // Resuming: restart the min-progress floor so the next yield can only fire
        // after another `min_progress_floor` bytes.
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

/// The pure decision for the DESTINATION-side bounded park: whether to KEEP
/// standing aside for foreground work on the write destination.
///
/// Unlike the source arm's `wait_until_foreground_idle` (which parks until the
/// share is quiet, however long that takes), this park is HARD-CAPPED. An upload
/// holds an OPEN SMB write handle across the pause, so it must resume (and write
/// a chunk, keeping the handle warm) at least every `hard_cap`, even if the user
/// keeps browsing. So: keep parking only while the share is STILL busy AND we're
/// still UNDER the cap; once either fails, resume the next write. Pure over a
/// `Duration` clock, like [`crate::priority::foreground::is_idle`], so it's
/// unit-testable against a fake clock without a real timer.
fn dest_park_continues(foreground_pending: bool, parked_for: Duration, hard_cap: Duration) -> bool {
    foreground_pending && parked_for < hard_cap
}

#[cfg(test)]
mod tests {
    use super::*;

    /// While the share is busy and we're under the cap, keep standing aside.
    #[test]
    fn keeps_parking_while_busy_and_under_cap() {
        assert!(dest_park_continues(
            true,
            Duration::from_millis(200),
            Duration::from_secs(1)
        ));
    }

    /// The moment the share goes quiet, resume the write, cap or no cap.
    #[test]
    fn resumes_the_instant_the_share_goes_quiet() {
        assert!(!dest_park_continues(
            false,
            Duration::from_millis(10),
            Duration::from_secs(1)
        ));
    }

    /// THE data-safety bound: even with the user still browsing, the park must
    /// end at the hard cap so the open SMB write handle can't sit idle forever
    /// (a long idle risks the server reaping the handle/session). At or past the
    /// cap, resume regardless of foreground.
    #[test]
    fn stops_parking_at_the_hard_cap_even_if_still_busy() {
        let cap = Duration::from_secs(1);
        assert!(
            !dest_park_continues(true, cap, cap),
            "at the cap, resume even though foreground is still pending"
        );
        assert!(
            !dest_park_continues(true, cap + Duration::from_millis(1), cap),
            "past the cap, resume even though foreground is still pending"
        );
    }
}
