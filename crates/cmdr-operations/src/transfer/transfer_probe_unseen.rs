//! Time the stall watchdog couldn't see: when a tick counts as late
//! ([`gap`]), and what the process was doing meanwhile, the evidence a late
//! tick logs so the next bundle can name a starved runtime's cause.
//!
//! A late tick ([`LATE_TICK_AFTER`]) proves only that the watchdog went
//! unscheduled. ERR-P4GWB's bundle couldn't say WHY: its workers were silent
//! for ten minutes while plain threads logged on, which fits a pool blocked
//! inside sync calls and a CPU-bound task that never yields equally well. The
//! process's per-thread CPU over the gap tells those apart:
//!
//! - **`tokio-runtime-worker` threads near 0% across the gap**: the workers
//!   were parked inside blocking calls (a lock, a syscall, a SQLite busy wait),
//!   so a blocking call on an async worker is the culprit.
//! - **The workers near 100% each**: a task spinning without an `.await`.
//! - **Another thread group dominating** (indexing, a walker): the machine, or
//!   our own background work, outbid the runtime for CPU.
//!
//! Tokio's blocking pool carries the same thread name as its workers, so the
//! row counts both; the busy-blocking-pool case shows as a thread count well
//! above the core count.
//!
//! **Cost.** One snapshot (a syscall per thread) every [`BASELINE_EVERY`], and
//! only while a transfer's watchdog runs; one more on a late tick. Nothing at
//! idle, and nothing per tick.

use std::time::Duration;

use cmdr_fs::process_cpu::{CpuCensus, CpuSnapshot};

use super::STALL_TICK;

/// How far past its due time a watchdog tick may land before the gap reads as
/// "the watchdog wasn't running" rather than "the runtime is busy".
///
/// A tick that lands this late says the async runtime (or the whole process)
/// went unscheduled: a starved worker pool, a system sleep, App Nap. Nothing
/// the watchdog could see happened in that gap, so it is forgiven
/// (`OperationProbe::forgive_unseen_time`) and logged ([`log`]), which also
/// makes the watchdog the runtime-stall witness for as long as a transfer runs.
///
/// 5 s mirrors `smb2`'s own cadence rule (`forgive_scheduling_stall`: a gap
/// of `KEEPALIVE_AFTER` on top of its tick), so the two layers agree on what a
/// stall is. ❌ Don't lower it toward the tick: forgiving a busy runtime's
/// second-or-two jitter would let a real wedge on a loaded Mac dodge
/// `STALL_ABORT_AFTER` indefinitely.
pub(super) const LATE_TICK_AFTER: Duration = Duration::from_secs(5);

/// How long the watchdog went unscheduled between the tick at `previous` and
/// the one at `now`, when that's at least [`LATE_TICK_AFTER`] past the tick's
/// due time. Measured past the due time, so a healthy cadence reads zero.
pub(super) fn gap(previous: Duration, now: Duration) -> Option<Duration> {
    let unseen = now.saturating_sub(previous).saturating_sub(STALL_TICK);
    (unseen >= LATE_TICK_AFTER).then_some(unseen)
}

/// The runtime-stall probe's line. INFO, so it never auto-sends a report, and
/// it rides the tick a transfer already pays for, so it costs nothing at idle.
/// Read it beside `smb2`'s "this process was not scheduled" lines: theirs say
/// a connection was starved, this one says what the process did meanwhile
/// (the module docs say how to read the CPU split).
pub(super) fn log(operation_id: &str, unseen: Duration, evidence: String) {
    log::info!(
        "transfer watchdog: op={operation_id} was not scheduled for {:.1}s (a starved async runtime, App Nap, or a \
         system sleep). Nothing could be observed in that time, so it doesn't count toward ending a still task's \
         wait{evidence}",
        unseen.as_secs_f64()
    );
}

/// How often the witness re-reads its baseline. The census it logs covers
/// the gap plus at most this much before it, so a long stall (the case worth
/// explaining) is barely diluted while the reading stays rare.
const BASELINE_EVERY: Duration = Duration::from_secs(20);

/// How many thread groups the late-tick line names, costliest first.
const GROUPS_NAMED: usize = 4;

/// The watchdog's CPU baseline, refreshed on its own cadence.
// DEFAULT-OK: an empty witness holds no reading and claims nothing; its first
// `refresh_if_due` takes the baseline.
#[derive(Default)]
pub(super) struct CpuWitness {
    baseline: Option<(Duration, CpuSnapshot)>,
}

impl CpuWitness {
    /// Re-read the baseline when it's older than [`BASELINE_EVERY`] (or missing).
    pub(super) fn refresh_if_due(&mut self, now: Duration) {
        let due = self
            .baseline
            .as_ref()
            .is_none_or(|(taken, _)| now.saturating_sub(*taken) >= BASELINE_EVERY);
        if due {
            self.baseline = CpuSnapshot::take().map(|snapshot| (now, snapshot));
        }
    }

    /// The process's CPU since the baseline, as one log fragment, and a fresh
    /// baseline from the reading it took. Empty when the platform has no
    /// reading (anything but macOS) or the kernel refused one.
    pub(super) fn describe_since_baseline(&mut self, now: Duration) -> String {
        let Some(end) = CpuSnapshot::take() else {
            return String::new();
        };
        let line = self
            .baseline
            .as_ref()
            .map(|(_, start)| render(&CpuCensus::between(start, &end, 0)))
            .unwrap_or_default();
        self.baseline = Some((now, end));
        line
    }
}

/// `"; over the last 612s the process used 41.0% of one core (threads 152→150): tokio-runtime-worker ×14 0.3%, …"`.
/// Thread names are set by our code and our dependencies, never from user data
/// (`cmdr_fs::process_cpu`'s privacy note), so the line needs no marking.
fn render(census: &CpuCensus) -> String {
    let groups: Vec<String> = census
        .threads_by_name
        .iter()
        .take(GROUPS_NAMED)
        .map(|group| {
            format!(
                "{name} ×{count} {share:.1}%",
                name = group.name.as_deref().unwrap_or("unnamed"),
                count = group.threads,
                share = group.cpu.percent_of_one_core
            )
        })
        .collect();
    format!(
        "; over the last {window}s the process used {share:.1}% of one core (threads {start}→{end}): {groups}",
        window = census.window_ms / 1_000,
        share = census.process.percent_of_one_core,
        start = census.thread_count.start,
        end = census.thread_count.end,
        groups = groups.join(", "),
    )
}

#[cfg(test)]
#[path = "transfer_probe_unseen_tests.rs"]
mod tests;
