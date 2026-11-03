//! Measurement harness behind `ReplayBudget`'s ratio and floor (#807): what a
//! journal replay costs per processed path against what the full reconcile it
//! falls back to costs per walked entry, on THIS Mac's real FSEvents journal and
//! boot disk. `#[ignore]`d and env-gated, because it walks `/` and writes to the
//! DB it's pointed at. Findings: `docs/notes/performance/replay-budget-2026-11-03.md`.
//!
//! Point it at a COPY of a real `index-root.db` (an APFS clone of the db and its
//! `-wal` is enough; ❌ never the live file), then run each arm by name:
//!
//!   CMDR_REPLAY_BENCH_DB=/path/copy.db CMDR_REPLAY_BENCH_SINCE=<event id> \
//!     cargo nextest run -p cmdr-index --no-capture --run-ignored all measure_replay_cost
//!   CMDR_REPLAY_BENCH_DB=/path/copy.db \
//!     cargo nextest run -p cmdr-index --no-capture --run-ignored all measure_full_reconcile_cost
//!
//! `CMDR_REPLAY_BENCH_COUNT_ONLY=1` stops the replay arm after its first pass, to
//! size a journal before committing to a processing run.
//!
//! The replay arm reads the journal twice from `SINCE`: once only counting (raw
//! events, distinct paths, and the paths the budget would charge with the
//! production dedup window), then once processing every window through the same
//! `flush_replay_batch` the cold-start replay uses, inside one transaction, with
//! no budget. The walk arm runs `start_local_reconcile` on `/`, which is what a
//! `ReplayOverflow` fallback runs on a populated, completed index. Both report
//! wall clock and whole-process CPU (`getrusage`, so the writer thread counts).

#[allow(
    clippy::print_stderr,
    reason = "ignored measurement harness; the numbers must print on --nocapture, and log::* is level-filtered under the test harness. Never runs in CI."
)]
mod bench {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use super::super::replay::flush_replay_batch;
    use super::super::replay_cost::{REPLAY_DEDUP_WINDOW_PATHS, REPLAY_EVENT_COST, REPLAY_PATH_COST, ReplayWindow};
    use super::super::watcher::{self, DriveWatcher, FsChangeEvent};
    use crate::indexing::IndexPathSpace;
    use crate::indexing::hold::{HoldKind, VolumeWork};
    use crate::indexing::reconcile::local_reconcile::start_local_reconcile;
    use crate::indexing::store::IndexStore;
    use crate::indexing::writer::{IndexWriter, WriteMessage};

    /// Whole-process CPU (user + system) and peak RSS so far.
    fn cpu_and_rss() -> (Duration, u64) {
        // SAFETY: `getrusage` only writes the zeroed struct we hand it.
        let usage = unsafe {
            let mut usage: libc::rusage = std::mem::zeroed();
            libc::getrusage(libc::RUSAGE_SELF, &mut usage);
            usage
        };
        let tv = |t: libc::timeval| Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64);
        // macOS reports `ru_maxrss` in bytes.
        (tv(usage.ru_utime) + tv(usage.ru_stime), usage.ru_maxrss as u64)
    }

    fn env_db() -> Option<PathBuf> {
        std::env::var("CMDR_REPLAY_BENCH_DB").ok().map(PathBuf::from)
    }

    fn per(total: Duration, n: u64) -> f64 {
        total.as_secs_f64() * 1e6 / n.max(1) as f64
    }

    /// Receive one journal replay from `since` until `HistoryDone`, or until
    /// `CMDR_REPLAY_BENCH_MAX_EVENTS` raw events, handing each event to `each`.
    /// Prints progress every million events. Returns the number of raw events.
    async fn drain_journal(since: u64, mut each: impl FnMut(FsChangeEvent)) -> u64 {
        let max: u64 = std::env::var("CMDR_REPLAY_BENCH_MAX_EVENTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(u64::MAX);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = DriveWatcher::start(Path::new("/"), since, tx).expect("the watcher starts");
        let started = Instant::now();
        let mut raw = 0u64;
        while let Some(event) = rx.recv().await {
            raw += 1;
            let done = event.flags.history_done;
            if raw.is_multiple_of(1_000_000) {
                eprintln!(
                    "  … {raw} raw events, id {}, {:.1}s, {} queued",
                    event.event_id,
                    started.elapsed().as_secs_f64(),
                    rx.len()
                );
            }
            each(event);
            if done || raw >= max {
                break;
            }
        }
        watcher.stop();
        raw
    }

    #[test]
    #[ignore = "walks the real journal and writes the DB it's given; run by hand (see module docs)"]
    fn measure_replay_cost() {
        let Some(db) = env_db() else {
            eprintln!("set CMDR_REPLAY_BENCH_DB (a COPY) and CMDR_REPLAY_BENCH_SINCE");
            return;
        };
        let since: u64 = std::env::var("CMDR_REPLAY_BENCH_SINCE")
            .expect("CMDR_REPLAY_BENCH_SINCE")
            .parse()
            .expect("an event id");
        let space = IndexPathSpace::root();
        let rt = crate::indexing::host::runtime::handle();
        let current = watcher::current_event_id(Path::new("/"));
        eprintln!(
            "\n=== REPLAY COST === since={since} current={current} id span={}",
            current - since
        );

        // ── Pass 1: count only ──
        let (cpu0, _) = cpu_and_rss();
        let t0 = Instant::now();
        let mut distinct = HashSet::<String>::new();
        let mut window = HashSet::<String>::new();
        let mut budget_paths = 0u64;
        let mut must_scan = 0u64;
        let mut first_id = None;
        let mut last_id = 0u64;
        let raw = rt.block_on(drain_journal(since, |event| {
            first_id.get_or_insert(event.event_id);
            last_id = event.event_id;
            if event.flags.history_done {
                return;
            }
            if event.flags.must_scan_sub_dirs {
                must_scan += 1;
                return;
            }
            let path = space.absolute(&event.path);
            distinct.insert(path.clone());
            window.insert(path);
            if window.len() >= REPLAY_DEDUP_WINDOW_PATHS {
                budget_paths += window.len() as u64;
                window.clear();
            }
        }));
        budget_paths += window.len() as u64;
        let read_wall = t0.elapsed();
        let (cpu1, rss1) = cpu_and_rss();
        let excluded = distinct
            .iter()
            .filter(|p| crate::indexing::scanner::should_exclude(p, space.exclusion_scope()))
            .count();
        eprintln!(
            "pass 1 (journal read only): raw={raw} first_id={first_id:?} last_id={last_id} distinct_paths={} \
             excluded_distinct={excluded} must_scan_sub_dirs={must_scan} budget_paths(window {REPLAY_DEDUP_WINDOW_PATHS})={budget_paths}",
            distinct.len()
        );
        eprintln!(
            "  wall={:.1}s cpu={:.1}s ({:.2} µs wall/raw event) peak_rss={} MB",
            read_wall.as_secs_f64(),
            (cpu1 - cpu0).as_secs_f64(),
            per(read_wall, raw),
            rss1 >> 20
        );
        drop(distinct);
        eprintln!(
            "  ReplayBudget would charge {} walked entries' worth ({raw} × {REPLAY_EVENT_COST} + {budget_paths} × {REPLAY_PATH_COST})",
            raw * REPLAY_EVENT_COST + budget_paths * REPLAY_PATH_COST
        );
        if std::env::var_os("CMDR_REPLAY_BENCH_COUNT_ONLY").is_some() {
            return;
        }

        // ── Pass 2: process every window the way cold-start replay does ──
        let _store = IndexStore::open(&db).expect("open the copy");
        let writer = IndexWriter::spawn(&db, crate::NoopEventSink::shared()).expect("writer");
        let conn = IndexStore::open_read_connection(&db).expect("read conn");
        let entries = IndexStore::get_entry_count(&conn).ok();
        eprintln!("index entries={entries:?}");
        let work = VolumeWork::for_test("root");
        let mut origin_dirs = HashSet::new();
        let mut origins_overflow = false;
        let mut pending_rescans = HashSet::new();
        let mut window = ReplayWindow::new(REPLAY_DEDUP_WINDOW_PATHS);
        let mut processed = 0u64;
        let mut processing = Duration::ZERO;
        let mut drains: Vec<(u64, Duration)> = Vec::new();

        let (cpu2, _) = cpu_and_rss();
        let t2 = Instant::now();
        writer.send(WriteMessage::BeginTransaction).expect("begin");
        let mut drain = |window: &mut ReplayWindow| {
            let td = Instant::now();
            let n = flush_replay_batch(
                window.pending_mut(),
                &space,
                &conn,
                &writer,
                &work,
                &mut origin_dirs,
                &mut origins_overflow,
                &mut pending_rescans,
            ) as u64;
            let took = td.elapsed();
            processing += took;
            processed += n;
            drains.push((n, took));
        };
        let raw2 = rt.block_on(drain_journal(since, |event| {
            if event.flags.history_done || event.flags.must_scan_sub_dirs {
                return;
            }
            let full = window.add(FsChangeEvent {
                path: space.absolute(&event.path),
                event_id: event.event_id,
                flags: event.flags,
            });
            if full {
                drain(&mut window);
            }
        }));
        drain(&mut window);
        writer.send(WriteMessage::CommitTransaction).expect("commit");
        let tc = Instant::now();
        writer.flush_blocking().expect("flush");
        let commit = tc.elapsed();
        let replay_wall = t2.elapsed();
        let (cpu3, rss3) = cpu_and_rss();
        let replay_cpu = cpu3 - cpu2;
        eprintln!(
            "pass 2 (replay, no budget): raw={raw2} processed_paths={processed} windows={} \
             wall={:.1}s (of which in drains {:.1}s, commit+flush {:.1}s) cpu={:.1}s peak_rss={} MB",
            drains.len(),
            replay_wall.as_secs_f64(),
            processing.as_secs_f64(),
            commit.as_secs_f64(),
            replay_cpu.as_secs_f64(),
            rss3 >> 20
        );
        eprintln!(
            "  per processed path: {:.1} µs wall, {:.1} µs CPU; deferred rescans={} origin_dirs_overflowed={origins_overflow}",
            per(replay_wall, processed),
            per(replay_cpu, processed),
            pending_rescans.len()
        );
        for (i, (n, took)) in drains.iter().enumerate() {
            // allowed-pluralize-noun: a measurement printout; a window holds thousands of paths.
            eprintln!(
                "  drain {i}: {n} paths in {:.2}s ({:.1} µs/path)",
                took.as_secs_f64(),
                per(*took, *n)
            );
        }
        writer.shutdown();
        eprintln!("=== END REPLAY COST ===\n");
    }

    #[test]
    #[ignore = "walks the whole boot disk and writes the DB it's given; run by hand (see module docs)"]
    fn measure_full_reconcile_cost() {
        let Some(db) = env_db() else {
            eprintln!("set CMDR_REPLAY_BENCH_DB (a COPY)");
            return;
        };
        let _store = IndexStore::open(&db).expect("open the copy");
        {
            // A rescan runs under a fresh epoch, as `start_scan` gives it.
            let wconn = IndexStore::open_write_connection(&db).expect("write conn");
            IndexStore::bump_current_epoch(&wconn).expect("bump epoch");
        }
        let writer = IndexWriter::spawn(&db, crate::NoopEventSink::shared()).expect("writer");
        let conn = IndexStore::open_read_connection(&db).expect("read conn");
        let entries = IndexStore::get_entry_count(&conn).ok();
        let volume = VolumeWork::for_test("root");

        let (cpu0, _) = cpu_and_rss();
        let t0 = Instant::now();
        let (_handle, join) = start_local_reconcile(
            PathBuf::from("/"),
            IndexPathSpace::root(),
            &writer,
            volume.child(HoldKind::LocalReconcile),
        )
        .expect("the reconcile starts");
        let summary = join.join().expect("the walk thread").expect("the walk");
        writer.flush_blocking().expect("flush");
        let wall = t0.elapsed();
        let (cpu1, rss1) = cpu_and_rss();
        let cpu = cpu1 - cpu0;
        eprintln!(
            "\n=== FULL RECONCILE COST ===\nindex entries before={entries:?} walked entries={} dirs={} \
             wall={:.1}s cpu={:.1}s peak_rss={} MB",
            summary.total_entries,
            summary.total_dirs,
            wall.as_secs_f64(),
            cpu.as_secs_f64(),
            rss1 >> 20
        );
        eprintln!(
            "  per walked entry: {:.1} µs wall, {:.1} µs CPU\n=== END FULL RECONCILE COST ===\n",
            per(wall, summary.total_entries),
            per(cpu, summary.total_entries)
        );
        writer.shutdown();
    }
}
