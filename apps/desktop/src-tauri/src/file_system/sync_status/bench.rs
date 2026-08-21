//! M4.6 bench: what the sync-status rework costs in threads, wall time, and CPU,
//! measured against a real File Provider folder.
//!
//! Run it against a cloud folder (the incident's was a 764-file Dropbox one):
//!
//! ```sh
//! CMDR_SYNC_STATUS_BENCH_DIR="$HOME/Library/CloudStorage/Dropbox/Apps/SMSBackupRestore" \
//!   cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --release --lib \
//!   sync_status::bench -- --ignored --nocapture
//! ```
//!
//! Without the env var it skips: there is no way to fake File Provider XPC, and a
//! number taken from a local folder would be measuring `stat`, not the thing that
//! wedged.
//!
//! Two scenarios, because they stress different halves of the fix:
//!
//! - **Steady pane**: the same visible range, asked for over and over. This is the
//!   3 s idle poll plus every listing render, and it is where the cache does the work.
//! - **Cold sweep**: every path in the folder, once. This is a first render of a big
//!   cloud folder, where the pool and the batch cap do the work.
//!
//! The baseline is the shape this module had before: a `std::thread::scope` fan-out
//! of `min(paths, available_parallelism())` fresh 8 MB-stack threads per call, with
//! no cache and nothing shared between calls. It's reproduced here rather than
//! described so the comparison is measured, not asserted.

#![cfg(all(test, target_os = "macos"))]
#![allow(
    clippy::print_stderr,
    reason = "Bench harness reports its own numbers via eprintln so `--nocapture` shows them next to test output"
)]

use super::{SERVICE, SyncKnowledge, probe};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Visible rows a pane asks about at a time. The incident's pane showed a range of
/// roughly this size while the copy ran.
const VISIBLE_ROWS: usize = 100;
/// Idle polls to simulate. At the frontend's 3 s interval this is a minute of a
/// user looking at one folder.
const STEADY_ROUNDS: usize = 20;
/// The deadline `commands::sync_status` gives the frontend.
const DEADLINE: Duration = Duration::from_secs(2);

struct Sample {
    threads_spawned: usize,
    wall: Duration,
    cpu: Duration,
    answered: usize,
}

fn bench_dir() -> Option<String> {
    std::env::var("CMDR_SYNC_STATUS_BENCH_DIR").ok()
}

fn paths_in(dir: &str) -> Vec<String> {
    let mut paths: Vec<String> = std::fs::read_dir(dir)
        .expect("bench dir is readable")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path().to_string_lossy().into_owned())
        .collect();
    paths.sort();
    paths
}

/// User + system CPU consumed by the whole process so far.
fn cpu_used() -> Duration {
    // SAFETY: `getrusage` writes a `rusage` through the pointer we hand it, and
    // `usage` is a live, correctly-sized, zeroed `rusage` for the duration of the
    // call. The return value gates whether we read it back.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: as above; `RUSAGE_SELF` is the documented flavor for "this process".
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return Duration::ZERO;
    }
    let secs = |t: libc::timeval| Duration::new(t.tv_sec as u64, (t.tv_usec as u32) * 1000);
    secs(usage.ru_utime) + secs(usage.ru_stime)
}

/// Threads alive in this process right now, so the bench reports the number a
/// `sample` run would have shown.
fn live_thread_count() -> usize {
    let mut info: libc::proc_taskinfo = unsafe {
        // SAFETY: `proc_taskinfo` is a plain `#[repr(C)]` struct of integers, so an
        // all-zero bit pattern is a valid (if meaningless) value; `proc_pidinfo`
        // overwrites it before we read it.
        std::mem::zeroed()
    };
    let size = size_of::<libc::proc_taskinfo>() as i32;
    // SAFETY: `info` is a live, correctly-aligned `proc_taskinfo` and `size` is
    // exactly its size, which is the contract `proc_pidinfo` takes for
    // `PROC_PIDTASKINFO`. We only trust the result when it wrote a full struct.
    let written = unsafe {
        libc::proc_pidinfo(
            std::process::id() as i32,
            libc::PROC_PIDTASKINFO,
            0,
            std::ptr::from_mut(&mut info).cast(),
            size,
        )
    };
    if written == size {
        info.pti_threadnum as usize
    } else {
        0
    }
}

/// The shape this module had before M4: a fresh fan-out per call, no cache, and no
/// deadline that the work itself honours.
fn legacy_fan_out(paths: &[String], threads_spawned: &Arc<AtomicUsize>) -> HashMap<String, SyncKnowledge> {
    const LEGACY_STACK_SIZE: usize = 8 * 1024 * 1024;
    if paths.is_empty() {
        return HashMap::new();
    }
    let workers = paths
        .len()
        .min(std::thread::available_parallelism().map_or(4, |n| n.get()));

    std::thread::scope(|scope| {
        let chunk_size = paths.len().div_ceil(workers);
        let handles: Vec<_> = paths
            .chunks(chunk_size)
            .map(|chunk| {
                threads_spawned.fetch_add(1, Ordering::SeqCst);
                let chunk = chunk.to_vec();
                std::thread::Builder::new()
                    .stack_size(LEGACY_STACK_SIZE)
                    .spawn_scoped(scope, move || {
                        chunk
                            .into_iter()
                            .map(|path| {
                                let status = probe::sync_status_for(Path::new(&path));
                                (path, status)
                            })
                            .collect::<Vec<_>>()
                    })
                    .expect("spawn legacy sync-status thread")
            })
            .collect();

        let mut result = HashMap::with_capacity(paths.len());
        for handle in handles {
            result.extend(handle.join().expect("legacy sync-status thread"));
        }
        result
    })
}

fn measure_legacy(rounds: impl Iterator<Item = Vec<String>>) -> Sample {
    let threads_spawned = Arc::new(AtomicUsize::new(0));
    let cpu_before = cpu_used();
    let started = Instant::now();
    let mut answered = 0;
    for round in rounds {
        answered = legacy_fan_out(&round, &threads_spawned).len();
    }
    Sample {
        threads_spawned: threads_spawned.load(Ordering::SeqCst),
        wall: started.elapsed(),
        cpu: cpu_used().saturating_sub(cpu_before),
        answered,
    }
}

async fn measure_current(rounds: impl Iterator<Item = Vec<String>>) -> Sample {
    let threads_before = SERVICE.pool_worker_count();
    let cpu_before = cpu_used();
    let started = Instant::now();
    let mut answered = 0;
    for round in rounds {
        answered = SERVICE.statuses_within(round, DEADLINE).await.0.len();
    }
    Sample {
        threads_spawned: SERVICE.pool_worker_count() - threads_before,
        wall: started.elapsed(),
        cpu: cpu_used().saturating_sub(cpu_before),
        answered,
    }
}

fn report(scenario: &str, calls: usize, legacy: &Sample, current: &Sample) {
    let ratio = |before: f64, after: f64| {
        if after > 0.0 {
            format!("{:.1}x less", before / after)
        } else {
            "0".to_string()
        }
    };
    // allowed-pluralize-noun: benchmark output; every scenario runs many calls, never one.
    eprintln!("\n  {scenario} ({calls} calls)");
    eprintln!(
        "    threads spawned   before {:>7}   after {:>7}   ({})",
        legacy.threads_spawned,
        current.threads_spawned,
        ratio(legacy.threads_spawned as f64, current.threads_spawned as f64)
    );
    eprintln!(
        "    wall time         before {:>7.0?}   after {:>7.0?}   ({})",
        legacy.wall,
        current.wall,
        ratio(legacy.wall.as_secs_f64(), current.wall.as_secs_f64())
    );
    eprintln!(
        "    cpu (user+sys)    before {:>7.0?}   after {:>7.0?}   ({})",
        legacy.cpu,
        current.cpu,
        ratio(legacy.cpu.as_secs_f64(), current.cpu.as_secs_f64())
    );
    eprintln!(
        "    paths answered    before {:>7}   after {:>7}",
        legacy.answered, current.answered
    );
}

/// A pane sitting on one cloud folder: the same visible range, re-asked on every
/// render and every idle poll.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a real File Provider folder in CMDR_SYNC_STATUS_BENCH_DIR"]
async fn bench_steady_pane() {
    let Some(dir) = bench_dir() else {
        eprintln!("skipped: set CMDR_SYNC_STATUS_BENCH_DIR to a cloud folder");
        return;
    };
    let all = paths_in(&dir);
    let visible: Vec<String> = all.iter().take(VISIBLE_ROWS).cloned().collect();
    eprintln!(
        "\nsync-status bench: {} files in {dir}, {} visible rows, {} threads live",
        all.len(),
        visible.len(),
        live_thread_count()
    );

    let legacy = measure_legacy((0..STEADY_ROUNDS).map(|_| visible.clone()));
    let current = measure_current((0..STEADY_ROUNDS).map(|_| visible.clone())).await;
    report("steady pane, same visible range", STEADY_ROUNDS, &legacy, &current);
    eprintln!("    threads live now  {}", live_thread_count());
}

/// What the structural shortcut is worth on an ORDINARY folder, which is nearly
/// every folder: the cost of one probe with the domain resolver saying "no domain
/// above this" against the same probe forced down the full `stat` + `NSURL` path.
///
/// Needs no File Provider, so it runs against any directory (`/usr/bin` by default,
/// the same 884-file folder `docs/notes/sync-status-pool-bench-2026-07-31.md`
/// measured the ~22 µs-per-path figure on).
#[test]
#[ignore = "measurement, not an assertion: run it with --nocapture when re-tuning"]
fn bench_outside_a_domain() {
    let dir = bench_dir().unwrap_or_else(|| "/usr/bin".to_string());
    let all = paths_in(&dir);
    if all.is_empty() {
        eprintln!("skipped: no files in {dir}");
        return;
    }

    let per_path = |domains: &cmdr_fs::file_provider::FileProviderDomains| {
        let started = Instant::now();
        for path in &all {
            let _ = probe::knowledge_for(Path::new(path), domains);
        }
        started.elapsed() / u32::try_from(all.len()).unwrap_or(1)
    };

    let root = Path::new(&dir).canonicalize().expect("canonical bench dir");
    // The "before" side: a scripted domain root above the folder forces every path
    // down the full `stat` + `NSURL` path, which is what every path used to cost.
    let forced = cmdr_fs::file_provider::FileProviderDomains::with_domain_roots(vec![root], Duration::from_secs(600));
    // The "after" side is the REAL resolver: its per-path cost is a live `getxattr`
    // on the leaf plus a memo hit for the directory, so a scripted one would flatter
    // the number by leaving the syscall out.
    let real = cmdr_fs::file_provider::FileProviderDomains::new(Duration::from_secs(600));
    assert_eq!(
        real.membership_of_dir(Path::new(&dir)),
        cmdr_fs::file_provider::DomainMembership::Outside,
        "this bench needs an ordinary folder; {dir} is in a domain or the marker isn't vouched for here"
    );

    // Full path first, so the page cache can't favour the number we're advertising.
    let full = per_path(&forced);
    let skipped = per_path(&real);
    eprintln!("\n  outside a domain, {} files in {dir}", all.len());
    eprintln!("    full probe (stat + NSURL)   {full:>9.2?} per path");
    eprintln!("    structural shortcut         {skipped:>9.2?} per path");
}

/// A first render of the whole folder: every path, cold.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a real File Provider folder in CMDR_SYNC_STATUS_BENCH_DIR"]
async fn bench_cold_sweep() {
    let Some(dir) = bench_dir() else {
        eprintln!("skipped: set CMDR_SYNC_STATUS_BENCH_DIR to a cloud folder");
        return;
    };
    let all = paths_in(&dir);
    eprintln!(
        "\nsync-status bench: {} files in {dir}, {} threads live",
        all.len(),
        live_thread_count()
    );

    let legacy = measure_legacy(std::iter::once(all.clone()));
    let current = measure_current(std::iter::once(all.clone())).await;
    report("cold sweep, whole folder", 1, &legacy, &current);
    eprintln!("    threads live now  {}", live_thread_count());
}
