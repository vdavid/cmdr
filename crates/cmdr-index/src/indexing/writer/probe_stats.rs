//! The writer thread's stall-probe heartbeat.
//!
//! One rolling counter set per writer thread, emitted on `stall_probe::writer`.
//! It answers two questions a bundle otherwise can't: is this writer STALLED
//! (queued work, nothing draining), and what is it costing in CPU.

use std::time::{Duration, Instant};

/// How often an IDLE writer still says so. Long enough that a quiet machine
/// costs a line a minute instead of one every five seconds, short enough that a
/// bundle can still tell "idle" from "the thread died".
pub(super) const IDLE_HEARTBEAT: Duration = Duration::from_secs(60);

/// Phase 1 instrumentation: rolling diagnostics for the writer thread.
pub(super) struct ProbeStats {
    /// Which volume's writer this is. Every volume runs its own writer thread and
    /// every one of them beats on the same target, so without this a bundle (and
    /// the churn harness's per-thread CPU counter) cannot tell three interleaved
    /// heartbeats apart.
    volume_id: String,
    last_heartbeat: Instant,
    pub(super) time_in_recv: Duration,
    pub(super) time_in_processing: Duration,
    pub(super) time_in_commit: Duration,
    pub(super) messages_processed: u64,
    pub(super) transaction_commits: u64,
}

impl ProbeStats {
    pub(super) fn new(volume_id: &str) -> Self {
        Self {
            volume_id: volume_id.to_string(),
            last_heartbeat: Instant::now(),
            time_in_recv: Duration::ZERO,
            time_in_processing: Duration::ZERO,
            time_in_commit: Duration::ZERO,
            messages_processed: 0,
            transaction_commits: 0,
        }
    }

    /// This thread's cumulative CPU time in whole milliseconds, for the
    /// heartbeat. `u64::MAX` never happens; a platform without a per-thread clock
    /// reports `0`, which reads as "no counter" against a line whose other
    /// numbers are moving.
    ///
    /// Deliberately NOT reset with the rest of the line's counters: the value is
    /// cumulative since the thread started, so a measurement window is the
    /// difference of two heartbeats. See `cmdr_fs::thread_cpu` for why a rate
    /// would be the wrong instrument.
    fn writer_cpu_ms_total() -> u64 {
        cmdr_fs::thread_cpu::current_thread_cpu_time()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Whether a heartbeat is worth a line right now.
    ///
    /// The probe exists to show a STALL: a writer that has work and isn't
    /// draining it. An idle writer beating every 5 s proves the thread is alive
    /// and says nothing else, and it was ~870 lines an hour. So a beat with
    /// nothing queued and nothing processed since the last one is skipped, unless
    /// [`IDLE_HEARTBEAT`] has passed — the periodic "still alive, still idle"
    /// line a bundle needs to distinguish idle from dead.
    ///
    /// A stall always logs: `queue_depth > 0` with no progress is exactly the
    /// case this keeps at full 5 s resolution.
    pub(super) fn heartbeat_is_worth_logging(&self, queue_depth: usize, since_last: Duration) -> bool {
        let idle = queue_depth == 0 && self.messages_processed == 0 && self.transaction_commits == 0;
        !idle || since_last >= IDLE_HEARTBEAT
    }

    /// The heartbeat's text, built separately from the logging so its shape can
    /// be asserted. `scripts/churn-baseline` parses this line, so the field names
    /// are a contract across two languages: a rename here silently zeroes the
    /// harness's writer-CPU column, with no error on either side.
    ///
    /// The window (`since_last_heartbeat_ms`) is on the line because it is no
    /// longer always 5 s — an idle stretch stretches it to [`IDLE_HEARTBEAT`] —
    /// and every per-window number here is only readable against it.
    ///
    /// `writer_cpu_ms_total` is the one CUMULATIVE number: everything else resets
    /// after the line, so a consumer diffs two heartbeats to get the writer
    /// thread's CPU for a window. That quantity is not observable from outside the
    /// process (macOS `ps -M` reports per-thread CPU but no thread names). Note it
    /// is CPU BURNED, where the `time_in_*` numbers are wall-clock time spent in a
    /// region, waiting included.
    pub(super) fn heartbeat_line(&self, queue_depth: usize, since_last: Duration, writer_cpu_ms_total: u64) -> String {
        format!(
            "heartbeat volume_id={} queue_depth={queue_depth} since_last_heartbeat_ms={} \
             messages_processed_since_last_heartbeat={} transaction_commits_since_last_heartbeat={} \
             time_in_recv_ms={} time_in_processing_ms={} time_in_commit_ms={} \
             writer_cpu_ms_total={writer_cpu_ms_total}",
            self.volume_id,
            since_last.as_millis(),
            self.messages_processed,
            self.transaction_commits,
            self.time_in_recv.as_millis(),
            self.time_in_processing.as_millis(),
            self.time_in_commit.as_millis(),
        )
    }

    pub(super) fn maybe_emit_heartbeat(&mut self, queue_depth: usize) {
        let since_last = self.last_heartbeat.elapsed();
        if since_last < Duration::from_secs(5) {
            return;
        }
        if !self.heartbeat_is_worth_logging(queue_depth, since_last) {
            return;
        }
        log::debug!(
            target: "stall_probe::writer",
            "{}",
            self.heartbeat_line(queue_depth, since_last, Self::writer_cpu_ms_total()),
        );
        self.last_heartbeat = Instant::now();
        self.time_in_recv = Duration::ZERO;
        self.time_in_processing = Duration::ZERO;
        self.time_in_commit = Duration::ZERO;
        self.messages_processed = 0;
        self.transaction_commits = 0;
    }
}
