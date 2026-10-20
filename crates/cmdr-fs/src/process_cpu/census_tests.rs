use super::*;
use std::time::Instant;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn thread(id: u64, name: Option<&str>, user_ms: u64, system_ms: u64) -> ThreadSample {
    ThreadSample {
        id,
        name: name.map(str::to_owned),
        user: ms(user_ms),
        system: ms(system_ms),
    }
}

fn snapshot(at: Instant, user_ms: u64, system_ms: u64, threads: Vec<ThreadSample>) -> CpuSnapshot {
    CpuSnapshot {
        taken_at: at,
        process_user: ms(user_ms),
        process_system: ms(system_ms),
        threads,
        wakeups: None,
        counters: None,
    }
}

/// A 10 s window: thread 1 spends 300 ms, thread 2 nothing, thread 3 is born and spends
/// 50 ms, thread 4 exits. The process spent 500 ms, so 150 ms went to threads gone by the
/// end.
fn window() -> (CpuSnapshot, CpuSnapshot) {
    let t0 = Instant::now();
    let start = snapshot(
        t0,
        1_000,
        500,
        vec![
            thread(1, Some("tokio-rt-worker"), 400, 100),
            thread(2, Some("tokio-rt-worker"), 50, 50),
            thread(4, Some("rescan-subtree"), 20, 10),
        ],
    );
    let end = snapshot(
        t0 + Duration::from_secs(10),
        1_300,
        700,
        vec![
            thread(1, Some("tokio-rt-worker"), 600, 200),
            thread(2, Some("tokio-rt-worker"), 50, 50),
            thread(3, None, 30, 20),
        ],
    );
    (start, end)
}

#[test]
fn process_and_live_deltas_are_measured_over_the_window() {
    let (start, end) = window();
    let census = CpuCensus::between(&start, &end, 10);
    assert_eq!(census.window_ms, 10_000);
    assert_eq!(census.process.cpu_us, 500_000);
    assert_eq!(census.process.user_us, 300_000);
    assert_eq!(census.process.system_us, 200_000);
    assert_eq!(census.process.percent_of_one_core, 5.0);
    assert_eq!(census.live_threads.cpu_us, 350_000);
}

/// The churn threads are gone by the second reading, so the remainder is the only place
/// their CPU appears.
#[test]
fn exited_threads_get_the_process_delta_the_live_threads_do_not_explain() {
    let (start, end) = window();
    let census = CpuCensus::between(&start, &end, 10);
    assert_eq!(census.exited_threads.cpu_us, 150_000);
    assert_eq!(census.exited_threads.percent_of_one_core, 1.5);
}

#[test]
fn a_thread_born_in_the_window_counts_in_full() {
    let (start, end) = window();
    let census = CpuCensus::between(&start, &end, 10);
    let born = census.threads.iter().find(|t| t.id == 3).expect("thread 3 is live");
    assert!(born.born_in_window);
    assert_eq!(born.cpu.cpu_us, 50_000);
    let old = census.threads.iter().find(|t| t.id == 1).expect("thread 1 is live");
    assert!(!old.born_in_window);
    assert_eq!(old.cpu.cpu_us, 300_000);
}

#[test]
fn thread_count_reports_births_exits_and_growth() {
    let (start, end) = window();
    let census = CpuCensus::between(&start, &end, 10);
    assert_eq!(
        census.thread_count,
        ThreadCountChange {
            start: 3,
            end: 3,
            born: 1,
            exited: 1,
            growth: 0
        }
    );
}

#[test]
fn threads_fold_by_name_costliest_first_with_unnamed_as_one_row() {
    let (start, end) = window();
    let census = CpuCensus::between(&start, &end, 10);
    let rows: Vec<_> = census
        .threads_by_name
        .iter()
        .map(|g| (g.name.as_deref(), g.threads, g.cpu.cpu_us))
        .collect();
    assert_eq!(rows, vec![(Some("tokio-rt-worker"), 2, 300_000), (None, 1, 50_000)]);
}

#[test]
fn individual_threads_are_capped_costliest_first_and_the_rest_counted() {
    let (start, end) = window();
    let census = CpuCensus::between(&start, &end, 2);
    let ids: Vec<_> = census.threads.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![1, 3]);
    assert_eq!(census.threads_omitted, 1);
}

/// Clocks of different granularity can put the live sum a hair over the process delta;
/// that must read as "nothing exited", never as a wrapped u64.
#[test]
fn exited_cpu_clamps_at_zero_when_the_live_sum_overshoots() {
    let t0 = Instant::now();
    let start = snapshot(t0, 100, 0, vec![thread(1, Some("main"), 100, 0)]);
    let end = snapshot(t0 + ms(1_000), 200, 0, vec![thread(1, Some("main"), 201, 0)]);
    let census = CpuCensus::between(&start, &end, 10);
    assert_eq!(census.exited_threads.cpu_us, 0);
    assert_eq!(census.live_threads.cpu_us, 101_000);
}

#[test]
fn wakeups_and_counters_diff_into_rates_when_both_readings_have_them() {
    let (mut start, mut end) = window();
    start.wakeups = Some(WakeupCounters {
        interrupt: 1_000,
        platform_idle: 100,
        timer_bin_1: 10,
        timer_bin_2: 1,
    });
    end.wakeups = Some(WakeupCounters {
        interrupt: 1_500,
        platform_idle: 150,
        timer_bin_1: 30,
        timer_bin_2: 2,
    });
    start.counters = Some(TaskCounters {
        context_switches: 5_000,
        unix_syscalls: 100,
        mach_syscalls: 200,
        faults: 7,
    });
    end.counters = Some(TaskCounters {
        context_switches: 6_000,
        unix_syscalls: 150,
        mach_syscalls: 260,
        faults: 9,
    });
    let census = CpuCensus::between(&start, &end, 10);
    let wakeups = census.wakeups.expect("both readings had wakeups");
    assert_eq!((wakeups.interrupt, wakeups.platform_idle), (500, 50));
    assert_eq!((wakeups.timer_bin_1, wakeups.timer_bin_2), (20, 1));
    assert_eq!(wakeups.interrupt_per_second, 50.0);
    assert_eq!(wakeups.platform_idle_per_second, 5.0);
    let counters = census.counters.expect("both readings had counters");
    assert_eq!(counters.context_switches, 1_000);
    assert_eq!(counters.context_switches_per_second, 100.0);
    assert_eq!(
        (counters.unix_syscalls, counters.mach_syscalls, counters.faults),
        (50, 60, 2)
    );

    end.wakeups = None;
    assert_eq!(CpuCensus::between(&start, &end, 10).wakeups, None);
}
