//! The macOS reads behind [`CpuSnapshot::take`].
//!
//! Per-thread times come from `proc_pidinfo`, which reports them in nanoseconds (it
//! converts `thread_basic_info`'s microseconds). The process total comes from
//! `getrusage`, ❌ never `proc_taskinfo.pti_total_user`: that one is in Mach absolute-time
//! ticks, which are nanoseconds on Intel and 125/3 ns on Apple silicon, so reading it as
//! nanoseconds undercounts the process 24-fold there.

use std::ffi::CStr;
use std::time::{Duration, Instant};

use super::{CpuSnapshot, TaskCounters, ThreadSample, WakeupCounters};

// `proc_pidinfo` flavors libc doesn't export (from <sys/proc_info.h>).
/// The task's thread IDs, as `u64`s.
const PROC_PIDLISTTHREADIDS: libc::c_int = 28;
/// One thread's `proc_threadinfo`, addressed by thread ID.
const PROC_PIDTHREADID64INFO: libc::c_int = 15;

/// Headroom over the task's own thread count when sizing the ID buffer, so a thread
/// born between the two calls doesn't force a retry.
const THREAD_ID_HEADROOM: usize = 32;

pub(super) fn take_snapshot() -> Option<CpuSnapshot> {
    let taken_at = Instant::now();
    let pid = std::process::id() as libc::c_int;
    let task = query_task_info(pid);
    let ids = list_thread_ids(pid, task.map_or(0, |t| t.pti_threadnum.max(0) as usize))?;
    let threads = ids.into_iter().filter_map(|id| query_thread(pid, id)).collect();
    let (process_user, process_system) = query_process_times()?;
    Some(CpuSnapshot {
        taken_at,
        process_user,
        process_system,
        threads,
        wakeups: query_power_info(),
        counters: task.map(|t| TaskCounters {
            // The kernel keeps these as 32-bit counters; `census` diffs them modulo 2^32.
            context_switches: u64::from(t.pti_csw as u32),
            unix_syscalls: u64::from(t.pti_syscalls_unix as u32),
            mach_syscalls: u64::from(t.pti_syscalls_mach as u32),
            faults: u64::from(t.pti_faults as u32),
        }),
    })
}

/// The process's cumulative user and system time, exited threads included.
fn query_process_times() -> Option<(Duration, Duration)> {
    // SAFETY: an all-zero `rusage` is a valid value of that plain C struct.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: `getrusage` writes one `rusage` through the pointer; `usage` is a live,
    // writable local of exactly that type. On failure it writes nothing we read.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return None;
    }
    Some((timeval(usage.ru_utime), timeval(usage.ru_stime)))
}

fn timeval(tv: libc::timeval) -> Duration {
    Duration::new(tv.tv_sec.max(0) as u64, 0) + Duration::from_micros(tv.tv_usec.max(0) as u64)
}

fn query_task_info(pid: libc::c_int) -> Option<libc::proc_taskinfo> {
    // SAFETY: an all-zero `proc_taskinfo` is a valid value of that plain C struct.
    let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
    let size = size_of::<libc::proc_taskinfo>() as libc::c_int;
    // SAFETY: `proc_pidinfo` writes at most `size` bytes into `info`, which is exactly
    // `size` bytes of writable `#[repr(C)]` memory. We read it only on a full write.
    let written = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTASKINFO, 0, (&raw mut info).cast(), size) };
    (written == size).then_some(info)
}

fn list_thread_ids(pid: libc::c_int, expected: usize) -> Option<Vec<u64>> {
    let mut capacity = expected + THREAD_ID_HEADROOM;
    // A buffer filled to the brim may have been too small, so grow and ask again, a few
    // times at most: a process spawning threads faster than this can't be read anyway.
    for _ in 0..4 {
        let mut ids = vec![0u64; capacity];
        let bytes = (capacity * size_of::<u64>()) as libc::c_int;
        // SAFETY: `ids` owns `bytes` bytes of writable, `u64`-aligned memory, and the
        // kernel writes at most `bytes` of thread IDs into it, returning how many bytes
        // it wrote (or ≤ 0 on failure).
        let written = unsafe { libc::proc_pidinfo(pid, PROC_PIDLISTTHREADIDS, 0, ids.as_mut_ptr().cast(), bytes) };
        if written <= 0 {
            log::debug!("process_cpu: proc_pidinfo(LISTTHREADIDS) returned {written}");
            return None;
        }
        if written < bytes {
            ids.truncate(written as usize / size_of::<u64>());
            return Some(ids);
        }
        capacity *= 2;
    }
    None
}

/// One thread's reading, or `None` if it exited between the listing and this call.
fn query_thread(pid: libc::c_int, id: u64) -> Option<ThreadSample> {
    // SAFETY: an all-zero `proc_threadinfo` is a valid value of that plain C struct.
    let mut info: libc::proc_threadinfo = unsafe { std::mem::zeroed() };
    let size = size_of::<libc::proc_threadinfo>() as libc::c_int;
    // SAFETY: `proc_pidinfo` writes at most `size` bytes into `info`, which is exactly
    // `size` bytes of writable `#[repr(C)]` memory. We read it only on a full write.
    let written = unsafe { libc::proc_pidinfo(pid, PROC_PIDTHREADID64INFO, id, (&raw mut info).cast(), size) };
    if written != size {
        return None;
    }
    // The last byte is forced to NUL so the name is terminated even if the kernel filled
    // the whole array.
    info.pth_name[libc::MAXTHREADNAMESIZE - 1] = 0;
    // SAFETY: `pth_name` is a NUL-terminated C string inside `info` (terminated above),
    // and `info` outlives the borrow.
    let name = unsafe { CStr::from_ptr(info.pth_name.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    Some(ThreadSample {
        id,
        name: (!name.is_empty()).then_some(name),
        user: Duration::from_nanos(info.pth_user_time),
        system: Duration::from_nanos(info.pth_system_time),
    })
}

fn query_power_info() -> Option<WakeupCounters> {
    // Mach task info flavor (from <mach/task_info.h>).
    const TASK_POWER_INFO: u32 = 21;

    // DEFAULT-OK: an out-buffer the kernel overwrites; its zero value is never read as a fact.
    #[repr(C)]
    #[derive(Default)]
    struct TaskPowerInfo {
        total_user: u64,
        total_system: u64,
        task_interrupt_wakeups: u64,
        task_platform_idle_wakeups: u64,
        task_timer_wakeups_bin_1: u64,
        task_timer_wakeups_bin_2: u64,
    }

    let mut info = TaskPowerInfo::default();
    // `count` is in `natural_t` (u32) words, per the `task_info` ABI.
    let mut count = (size_of::<TaskPowerInfo>() / size_of::<u32>()) as u32;
    #[allow(deprecated, reason = "mach_task_self is deprecated in libc but works fine")]
    // SAFETY: `info` is a `#[repr(C)]` match of `task_power_info` and `count` is its size
    // in `natural_t` words, so the kernel writes only within `info`. We read it only when
    // the call succeeds.
    let result = unsafe {
        libc::task_info(
            libc::mach_task_self(),
            TASK_POWER_INFO,
            (&raw mut info).cast(),
            &mut count,
        )
    };
    if result != 0 {
        log::debug!("process_cpu: task_info(POWER_INFO) failed with code {result}");
        return None;
    }
    Some(WakeupCounters {
        interrupt: info.task_interrupt_wakeups,
        platform_idle: info.task_platform_idle_wakeups,
        timer_bin_1: info.task_timer_wakeups_bin_1,
        timer_bin_2: info.task_timer_wakeups_bin_2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn burn(iterations: u64) {
        let mut sink = 0u64;
        for i in 0..iterations {
            sink = sink.wrapping_add(i).wrapping_mul(2_654_435_761);
        }
        std::hint::black_box(sink);
    }

    /// The whole point: a named thread's work lands on THAT name, and nowhere else.
    #[test]
    fn a_named_busy_thread_shows_up_with_its_name_and_its_cpu() {
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let worker = std::thread::Builder::new()
            .name("cpu-census-probe".into())
            .spawn(move || {
                go_rx.recv().expect("go");
                burn(300_000_000);
                done_tx.send(()).expect("done");
                stop_rx.recv().expect("stop");
            })
            .expect("spawn");

        // `std::thread::Builder::name` names the pthread from inside the new thread, so
        // wait until the kernel sees the name before the first reading.
        let mut start = None;
        crate::testing::wait_until(Duration::from_secs(2), "the probe thread appears with its name", || {
            start = take_snapshot().filter(|snap| {
                snap.threads
                    .iter()
                    .any(|t| t.name.as_deref() == Some("cpu-census-probe"))
            });
            start.is_some()
        });
        let start = start.expect("wait_until returned, so the named reading is there");

        go_tx.send(()).expect("go");
        done_rx.recv().expect("done");
        let end = take_snapshot().expect("second reading");
        stop_tx.send(()).expect("stop");
        worker.join().expect("worker");

        let census = super::super::CpuCensus::between(&start, &end, 1_000);
        let probe = census
            .threads
            .iter()
            .find(|t| t.name.as_deref() == Some("cpu-census-probe"))
            .expect("the probe is live at the second reading");
        assert!(
            probe.cpu.cpu_us > 20_000,
            "300 M iterations must cost the probe real CPU: {probe:?}"
        );
        assert!(
            census.process.cpu_us >= probe.cpu.cpu_us,
            "the process delta includes every thread: {census:?}"
        );
        assert!(census.wakeups.is_some(), "TASK_POWER_INFO must be readable");
        assert!(census.counters.is_some(), "PROC_PIDTASKINFO must be readable");
    }

    /// A thread that exits between the readings still shows up, as the process delta
    /// the live threads don't explain.
    #[test]
    fn a_thread_that_exits_inside_the_window_lands_in_exited_cpu() {
        let start = take_snapshot().expect("first reading");
        std::thread::Builder::new()
            .name("cpu-census-churn".into())
            .spawn(|| burn(300_000_000))
            .expect("spawn")
            .join()
            .expect("churn thread");
        let end = take_snapshot().expect("second reading");

        let census = super::super::CpuCensus::between(&start, &end, 1_000);
        assert!(
            census
                .threads
                .iter()
                .all(|t| t.name.as_deref() != Some("cpu-census-churn")),
            "the churn thread is gone by the second reading"
        );
        assert!(
            census.exited_threads.cpu_us > 20_000,
            "the exited thread's work must land in exitedThreads: {census:?}"
        );
    }
}
