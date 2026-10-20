//! Where this process's CPU time goes, per named thread, over a window.
//!
//! Idle CPU in Cmdr has no hotspot: it's dozens of threads at a few hundredths of a
//! percent each, so it's only measurable per thread WITH names, and `ps -M` has no names.
//! This module owns the reading (`docs/notes/performance/idle-census-2026-09-27.md` §
//! "A better CPU instrument" is the recipe it replaces):
//!
//! - [`CpuSnapshot::take`] reads every live thread's name and cumulative user and system
//!   time (`proc_pidinfo` with `PROC_PIDLISTTHREADIDS` + `PROC_PIDTHREADID64INFO`), the
//!   whole process's cumulative CPU (`getrusage`, which keeps exited threads' time), and
//!   the task's wakeup and context-switch counters (`TASK_POWER_INFO`, `PROC_PIDTASKINFO`).
//! - [`CpuCensus::between`] diffs two snapshots. It's pure, so the arithmetic is tested
//!   without a kernel.
//!
//! **Cumulative readings, diffed; never a sampled rate.** `sample` and `top` both misled
//! earlier investigations (a parked tokio worker reads as busy in `sample`, and `top`'s
//! `IDLEW` read static across intervals on macOS 27). Two monotone counters can't lie that
//! way. Same argument as [`crate::thread_cpu`], which reads only the CALLING thread.
//!
//! **Threads match on their kernel thread ID**, which is unique for the life of the
//! system, so a reused `pthread_t` can't splice two threads into one. A thread only in the
//! later snapshot counts in full; one only in the earlier snapshot exited.
//!
//! **Exited threads are the process delta minus the live threads' deltas.** It's the only
//! way the churn threads (walkers, `rescan-subtree`, tokio's blocking pool) show up at
//! all, since they're gone by the second reading.
//!
//! **Privacy**: thread names are set by our code and our dependencies (`tokio-rt-worker`,
//! `index-writer`), never from user data. Everything else is a count or a duration.
//!
//! macOS only: on other platforms [`CpuSnapshot::take`] returns `None`.

mod census;
#[cfg(target_os = "macos")]
mod macos;

pub use census::{
    CpuCensus, CpuShare, ExitedThreads, TaskCounterDeltas, ThreadCensus, ThreadCountChange, ThreadGroup, WakeupDeltas,
};

use std::time::{Duration, Instant};

/// One live thread's cumulative CPU at the moment of a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSample {
    /// The kernel's thread ID: unique for the life of the system, never reused.
    pub id: u64,
    /// The name the thread gave itself, or `None` for an unnamed one.
    pub name: Option<String>,
    /// Cumulative user time since the thread started.
    pub user: Duration,
    /// Cumulative system time since the thread started.
    pub system: Duration,
}

/// The kernel's wakeup counters for the whole task (`TASK_POWER_INFO`), cumulative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WakeupCounters {
    /// Times a thread of this task was woken by an interrupt, timers included.
    pub interrupt: u64,
    /// The subset of those that brought a CPU out of idle: the costly kind, and what
    /// Activity Monitor's energy impact weighs.
    pub platform_idle: u64,
    /// Timer wakeups the kernel files in its first coalescing bin.
    pub timer_bin_1: u64,
    /// Timer wakeups the kernel files in its second coalescing bin.
    pub timer_bin_2: u64,
}

/// The task-wide event counters from `PROC_PIDTASKINFO`, cumulative. 32-bit in the
/// kernel, so they wrap; [`CpuCensus::between`] diffs them modulo 2^32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskCounters {
    /// Context switches across every thread, exited ones included.
    pub context_switches: u64,
    /// BSD system calls.
    pub unix_syscalls: u64,
    /// Mach traps.
    pub mach_syscalls: u64,
    /// Page faults.
    pub faults: u64,
}

/// Everything one reading sees. Diff two with [`CpuCensus::between`].
#[derive(Debug, Clone)]
pub struct CpuSnapshot {
    /// When the reading was taken. The window is the difference of two of these.
    pub taken_at: Instant,
    /// The whole process's cumulative user time, exited threads included.
    pub process_user: Duration,
    /// The whole process's cumulative system time, exited threads included.
    pub process_system: Duration,
    /// Every thread alive at the reading.
    pub threads: Vec<ThreadSample>,
    /// `None` if the kernel refused the query.
    pub wakeups: Option<WakeupCounters>,
    /// `None` if the kernel refused the query.
    pub counters: Option<TaskCounters>,
}

impl CpuSnapshot {
    /// Read this process now, or `None` if the per-thread query failed or the platform
    /// has none. Costs one syscall per thread plus a handful, so it's cheap enough to take
    /// twice a second, and far too costly to take per event.
    pub fn take() -> Option<Self> {
        #[cfg(target_os = "macos")]
        {
            macos::take_snapshot()
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
}
