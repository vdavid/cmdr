//! The diff of two [`CpuSnapshot`]s: pure arithmetic, so it's tested without a kernel.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use serde::Serialize;

use super::{CpuSnapshot, TaskCounters, ThreadSample, WakeupCounters};

/// What the process spent over one window, per thread and in total.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuCensus {
    /// How long the window was, in milliseconds.
    pub window_ms: u64,
    /// The whole process, exited threads included.
    pub process: CpuShare,
    /// What threads alive at the end of the window spent inside it.
    pub live_threads: CpuShare,
    /// What threads that exited inside the window spent there.
    pub exited_threads: ExitedThreads,
    /// How the thread population moved.
    pub thread_count: ThreadCountChange,
    /// Every live thread folded by name (`tokio-rt-worker` ×24 is one row), costliest
    /// first. Unnamed threads fold into one row with no name.
    pub threads_by_name: Vec<ThreadGroup>,
    /// The costliest individual live threads, at most the caller's limit.
    pub threads: Vec<ThreadCensus>,
    /// Live threads left out of `threads` by that limit.
    pub threads_omitted: usize,
    /// Wakeup deltas, or `None` when either reading lacked them.
    pub wakeups: Option<WakeupDeltas>,
    /// Task event deltas, or `None` when either reading lacked them.
    pub counters: Option<TaskCounterDeltas>,
}

/// CPU time spent in the window, split, and as a share of one core.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuShare {
    /// User plus system, in microseconds.
    pub cpu_us: u64,
    /// User time, in microseconds.
    pub user_us: u64,
    /// System time, in microseconds.
    pub system_us: u64,
    /// `cpuUs` over the window, as a percent of one core (100 = one core flat out).
    pub percent_of_one_core: f64,
}

/// The CPU threads spent before they exited inside the window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitedThreads {
    /// The process delta minus the live threads' deltas, in microseconds. Includes
    /// threads born AND gone inside the window, which no reading ever saw. Clamped at
    /// zero: the per-thread clock ticks in microseconds and the process one may round
    /// differently, so a near-zero difference can come out a hair negative.
    pub cpu_us: u64,
    /// `cpuUs` as a percent of one core.
    pub percent_of_one_core: f64,
}

/// How the thread population moved across the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadCountChange {
    /// Threads alive at the start.
    pub start: usize,
    /// Threads alive at the end.
    pub end: usize,
    /// Threads alive at the end that weren't at the start.
    pub born: usize,
    /// Threads alive at the start that weren't at the end.
    pub exited: usize,
    /// `end - start`: positive and growing across windows is a thread leak.
    pub growth: i64,
}

/// Live threads sharing a name, folded into one row.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGroup {
    /// The shared name, or `None` for the unnamed threads.
    pub name: Option<String>,
    /// How many live threads carry it.
    pub threads: usize,
    /// What they spent in the window, together.
    #[serde(flatten)]
    pub cpu: CpuShare,
}

/// One live thread's spend in the window.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadCensus {
    /// The kernel's thread ID.
    pub id: u64,
    /// The thread's name, or `None` when it has none.
    pub name: Option<String>,
    /// True when the thread was born inside the window, so its whole life counts.
    pub born_in_window: bool,
    /// What it spent in the window.
    #[serde(flatten)]
    pub cpu: CpuShare,
}

/// Wakeups over the window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeupDeltas {
    /// Interrupt wakeups, timers included.
    pub interrupt: u64,
    /// Wakeups that brought a CPU out of idle.
    pub platform_idle: u64,
    /// Timer wakeups in the kernel's first coalescing bin.
    pub timer_bin_1: u64,
    /// Timer wakeups in the kernel's second coalescing bin.
    pub timer_bin_2: u64,
    /// `interrupt` per second of window.
    pub interrupt_per_second: f64,
    /// `platformIdle` per second of window.
    pub platform_idle_per_second: f64,
}

/// Task events over the window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCounterDeltas {
    /// Context switches, exited threads included.
    pub context_switches: u64,
    /// BSD system calls.
    pub unix_syscalls: u64,
    /// Mach traps.
    pub mach_syscalls: u64,
    /// Page faults.
    pub faults: u64,
    /// `contextSwitches` per second of window: the whole-process wakeup rate `top`
    /// can't give.
    pub context_switches_per_second: f64,
}

impl CpuCensus {
    /// Diff `start` against `end`, keeping at most `max_threads` individual rows.
    pub fn between(start: &CpuSnapshot, end: &CpuSnapshot, max_threads: usize) -> Self {
        let window = end.taken_at.saturating_duration_since(start.taken_at);
        let share = |user: Duration, system: Duration| CpuShare::over(user, system, window);

        let process = share(
            end.process_user.saturating_sub(start.process_user),
            end.process_system.saturating_sub(start.process_system),
        );

        let before: HashMap<u64, &ThreadSample> = start.threads.iter().map(|t| (t.id, t)).collect();
        let mut threads: Vec<ThreadCensus> = end
            .threads
            .iter()
            .map(|t| {
                let earlier = before.get(&t.id);
                let (user, system) = match earlier {
                    Some(e) => (t.user.saturating_sub(e.user), t.system.saturating_sub(e.system)),
                    None => (t.user, t.system),
                };
                ThreadCensus {
                    id: t.id,
                    name: t.name.clone(),
                    born_in_window: earlier.is_none(),
                    cpu: share(user, system),
                }
            })
            .collect();

        let live_user: u64 = threads.iter().map(|t| t.cpu.user_us).sum();
        let live_system: u64 = threads.iter().map(|t| t.cpu.system_us).sum();
        let live_threads = CpuShare::from_micros(live_user, live_system, window);

        let exited_us = process.cpu_us.saturating_sub(live_threads.cpu_us);
        let exited_threads = ExitedThreads {
            cpu_us: exited_us,
            percent_of_one_core: percent(exited_us, window),
        };

        let born = threads.iter().filter(|t| t.born_in_window).count();
        let survivors = threads.len() - born;
        let thread_count = ThreadCountChange {
            start: start.threads.len(),
            end: end.threads.len(),
            born,
            exited: start.threads.len().saturating_sub(survivors),
            growth: end.threads.len() as i64 - start.threads.len() as i64,
        };

        let threads_by_name = fold_by_name(&threads, window);

        threads.sort_by(|a, b| b.cpu.cpu_us.cmp(&a.cpu.cpu_us).then(a.id.cmp(&b.id)));
        let threads_omitted = threads.len().saturating_sub(max_threads);
        threads.truncate(max_threads);

        let seconds = window.as_secs_f64();
        let wakeups = start
            .wakeups
            .zip(end.wakeups)
            .map(|(a, b)| wakeup_deltas(a, b, seconds));
        let counters = start
            .counters
            .zip(end.counters)
            .map(|(a, b)| counter_deltas(a, b, seconds));

        Self {
            window_ms: u64::try_from(window.as_millis()).unwrap_or(u64::MAX),
            process,
            live_threads,
            exited_threads,
            thread_count,
            threads_by_name,
            threads,
            threads_omitted,
            wakeups,
            counters,
        }
    }
}

impl CpuShare {
    fn over(user: Duration, system: Duration, window: Duration) -> Self {
        Self::from_micros(micros(user), micros(system), window)
    }

    fn from_micros(user_us: u64, system_us: u64, window: Duration) -> Self {
        let cpu_us = user_us.saturating_add(system_us);
        Self {
            cpu_us,
            user_us,
            system_us,
            percent_of_one_core: percent(cpu_us, window),
        }
    }
}

fn micros(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

/// A share of one core, rounded to thousandths of a percent: idle threads live in the
/// hundredths, so coarser rounding would zero out exactly the rows a census is for.
fn percent(cpu_us: u64, window: Duration) -> f64 {
    let window_us = window.as_micros() as f64;
    if window_us == 0.0 {
        return 0.0;
    }
    (cpu_us as f64 / window_us * 100_000.0).round() / 1_000.0
}

fn per_second(count: u64, seconds: f64) -> f64 {
    if seconds == 0.0 {
        return 0.0;
    }
    (count as f64 / seconds * 1_000.0).round() / 1_000.0
}

fn fold_by_name(threads: &[ThreadCensus], window: Duration) -> Vec<ThreadGroup> {
    // `BTreeMap` so equal-cost groups come out in a stable (name) order.
    let mut groups: BTreeMap<Option<&str>, (usize, u64, u64)> = BTreeMap::new();
    for t in threads {
        let entry = groups.entry(t.name.as_deref()).or_default();
        entry.0 += 1;
        entry.1 += t.cpu.user_us;
        entry.2 += t.cpu.system_us;
    }
    let mut rows: Vec<ThreadGroup> = groups
        .into_iter()
        .map(|(name, (count, user, system))| ThreadGroup {
            name: name.map(str::to_owned),
            threads: count,
            cpu: CpuShare::from_micros(user, system, window),
        })
        .collect();
    rows.sort_by_key(|group| std::cmp::Reverse(group.cpu.cpu_us));
    rows
}

fn wakeup_deltas(a: WakeupCounters, b: WakeupCounters, seconds: f64) -> WakeupDeltas {
    let interrupt = b.interrupt.saturating_sub(a.interrupt);
    let platform_idle = b.platform_idle.saturating_sub(a.platform_idle);
    WakeupDeltas {
        interrupt,
        platform_idle,
        timer_bin_1: b.timer_bin_1.saturating_sub(a.timer_bin_1),
        timer_bin_2: b.timer_bin_2.saturating_sub(a.timer_bin_2),
        interrupt_per_second: per_second(interrupt, seconds),
        platform_idle_per_second: per_second(platform_idle, seconds),
    }
}

/// The kernel keeps these as 32-bit counters (a busy process wraps one in weeks), so
/// the difference is taken modulo 2^32: one wrap inside a window still diffs right.
fn counter_delta(a: u64, b: u64) -> u64 {
    u64::from((b as u32).wrapping_sub(a as u32))
}

fn counter_deltas(a: TaskCounters, b: TaskCounters, seconds: f64) -> TaskCounterDeltas {
    let context_switches = counter_delta(a.context_switches, b.context_switches);
    TaskCounterDeltas {
        context_switches,
        unix_syscalls: counter_delta(a.unix_syscalls, b.unix_syscalls),
        mach_syscalls: counter_delta(a.mach_syscalls, b.mach_syscalls),
        faults: counter_delta(a.faults, b.faults),
        context_switches_per_second: per_second(context_switches, seconds),
    }
}

#[cfg(test)]
#[path = "census_tests.rs"]
mod tests;
