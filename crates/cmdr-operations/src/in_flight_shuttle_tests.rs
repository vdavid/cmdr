//! The in-flight ledger's thread interleavings, explored with Shuttle: copies
//! recording and retiring, a quit flushing, a launch sweeping, and arrivals
//! claiming, racing each other on the ledger's real code.
//!
//! Built only under `--features shuttle` (the `desktop-rust-shuttle` lane),
//! which swaps the ledger's locks, queues, and threads for Shuttle's
//! (`sync_primitives.rs`), so its scheduler picks who runs next at every lock
//! and send. Every test is named `shuttle_*`, which is how the lane and the
//! property-mutant checks find it.
//!
//! A failure prints the schedule that broke it; `shuttle_support.rs` says how
//! to replay it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use shuttle::sync::mpsc;
use shuttle::thread;

use super::super::shuttle_support::explore;
use super::super::state::WriteOperationState;
use super::super::testing::TestOperations;
use super::{ItemKind, Ledger, Record, RecordHome, STORE_FILENAME, TrackedRecord, retire, track};
use cmdr_fs::testing::TestDir;

/// A scratch dir for one schedule, with the ledger's data dir inside it.
fn scratch(name: &str) -> (TestDir, PathBuf) {
    let dir = TestDir::new(name);
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).expect("the data dir");
    (dir, data_dir)
}

/// A state of `engine`'s, recording into `ledger`.
fn state_on(engine: &TestOperations, ledger: &Ledger) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_in_flight_ledger(ledger.clone()),
    )
}

/// What `ledger` holds right now.
fn held(ledger: &Ledger) -> BTreeSet<Record> {
    ledger.store().recorded.clone()
}

/// What a launch would replay from `data_dir` right now.
fn on_disk(data_dir: &Path) -> BTreeSet<Record> {
    Ledger::replay_log(data_dir).into_iter().collect()
}

/// One copy's worth of ledger traffic in `dir`: `files` staged partials, each
/// landed (retired) but every third, which stays in flight. Enough lines that
/// two of these push the log past compaction. Returns what it left recorded.
fn copy_files(state: &WriteOperationState, dir: &Path, name: &str, files: usize) -> Vec<TrackedRecord> {
    let mut in_flight = Vec::new();
    for i in 0..files {
        let temp = dir.join(format!("{name}-{i}.bin.cmdr-tmp-{i:08}"));
        let tracked = track(state, ItemKind::Temp, &temp);
        if i % 3 == 0 {
            in_flight.push(tracked);
        } else {
            retire(state, &tracked);
        }
    }
    in_flight
}

/// Copies, an overwrite parking an original aside, and a quit flushing, all at
/// once. Whatever the order, three things hold: an aside's record is on disk
/// before `track` returns (its caller renames the user's original there next),
/// a flush covers every record made before it started, and the log the last
/// line leaves replays to exactly what the ledger holds, compaction included.
#[test]
fn shuttle_racing_copies_and_a_quit_flush_leave_a_log_that_replays_to_the_ledger() {
    explore(|| {
        let (dir, data_dir) = scratch("shuttle-ledger-copies");
        let ledger = Ledger::recording_in(&data_dir);
        let engine = TestOperations::new();
        let (recorded_before_quit, quit_may_flush) = mpsc::channel::<BTreeSet<Record>>();

        // The first copy tells the quit what it left in flight: partials, whose
        // records only queue, so only the flush puts them on disk.
        let first = {
            let state = state_on(&engine, &ledger);
            let dir = dir.to_path_buf();
            thread::spawn(move || {
                let in_flight = copy_files(&state, &dir, "first", 30);
                let _ = recorded_before_quit.send(in_flight.iter().map(|tracked| tracked.record.clone()).collect());
                in_flight
            })
        };
        let second = {
            let state = state_on(&engine, &ledger);
            let dir = dir.to_path_buf();
            thread::spawn(move || copy_files(&state, &dir, "second", 30))
        };

        let overwrite = {
            let state = state_on(&engine, &ledger);
            let dir = dir.to_path_buf();
            let data_dir = data_dir.clone();
            thread::spawn(move || {
                let aside = dir.join("report.pdf.cmdr-temp-00000001");
                let tracked = track(
                    &state,
                    ItemKind::FileAside {
                        destination: dir.join("report.pdf"),
                        expected_size: 4096,
                    },
                    &aside,
                );
                assert!(
                    on_disk(&data_dir).contains(&tracked.record),
                    "an aside's record is on disk when track returns, before the original is renamed onto it"
                );
            })
        };

        let quit = {
            let ledger = ledger.clone();
            let data_dir = data_dir.clone();
            thread::spawn(move || {
                let must_be_on_disk = quit_may_flush.recv().expect("the first copy finished");
                ledger.flush_log();
                let written = on_disk(&data_dir);
                assert!(
                    must_be_on_disk.is_subset(&written),
                    "a flush covers every record made before it: missing {:?}",
                    must_be_on_disk.difference(&written).collect::<Vec<_>>()
                );
            })
        };

        let mut in_flight: BTreeSet<Record> = BTreeSet::new();
        for copy in [first, second] {
            in_flight.extend(copy.join().expect("a copy").into_iter().map(|tracked| tracked.record));
        }
        overwrite.join().expect("the overwrite");
        quit.join().expect("the quit");

        let held = held(&ledger);
        assert!(in_flight.is_subset(&held), "every partial still in flight is held");
        ledger.die();
        assert_eq!(
            on_disk(&data_dir),
            held,
            "the log replays to exactly what the ledger held, whatever order the lines were queued in"
        );
        assert!(
            std::fs::metadata(data_dir.join(STORE_FILENAME)).expect("the log").len() < 16 * 1024,
            "compaction kept the log small"
        );
    });
}

/// A volume's waiting records, two arrivals of it racing to claim them (the
/// launch sweep finding it registered, its listener firing), and a stopped
/// copy handing over one more record for it meanwhile. Every record ends up
/// claimed by exactly one arrival or still waiting: none settled twice, none
/// forgotten.
#[test]
fn shuttle_each_waiting_record_is_claimed_by_exactly_one_arrival() {
    explore(|| {
        let ledger = Ledger::for_test();
        let home = RecordHome::volume_space("nas");
        let waiting: Vec<Record> = (0..4)
            .map(|i| {
                super::tracked_record(
                    &home,
                    ItemKind::Temp,
                    Path::new(&format!("/share/f{i}.cmdr-tmp-{i:08}")),
                )
            })
            .collect();
        ledger.defer(waiting.clone());
        let late = super::tracked_record(&home, ItemKind::Temp, Path::new("/share/late.cmdr-tmp-00000009"));

        let arrivals: Vec<_> = (0..2)
            .map(|_| {
                let ledger = ledger.clone();
                thread::spawn(move || ledger.claim_pending("nas"))
            })
            .collect();
        let stopped_copy = {
            let ledger = ledger.clone();
            let late = late.clone();
            thread::spawn(move || ledger.defer(vec![late]))
        };

        let mut claimed: Vec<Record> = Vec::new();
        for arrival in arrivals {
            claimed.extend(arrival.join().expect("an arrival"));
        }
        stopped_copy.join().expect("the stopped copy");

        let distinct: BTreeSet<Record> = claimed.iter().cloned().collect();
        assert_eq!(
            distinct.len(),
            claimed.len(),
            "a record was claimed by two arrivals, so two sweeps would settle it: {claimed:?}"
        );
        let still_waiting: BTreeSet<Record> = ledger.store().pending.clone();
        assert!(
            distinct.is_disjoint(&still_waiting),
            "a claimed record is no longer waiting"
        );
        let accounted: BTreeSet<Record> = distinct.union(&still_waiting).cloned().collect();
        let expected: BTreeSet<Record> = waiting.into_iter().chain([late]).collect();
        assert_eq!(accounted, expected, "every record is claimed once or still waiting");
    });
}

/// A launch replaying a crashed run's leftovers while this run's first copy
/// already records (the claim is meant to come first, but its sweep runs on
/// its own thread for as long as a dead mount makes it), and a quit flushing.
/// The sweep removes exactly what the last run left: the new copy's partials
/// stay on disk and recorded, and the log ends up replaying the ledger.
#[test]
fn shuttle_a_launch_sweep_racing_a_new_copy_removes_only_what_the_last_run_left() {
    explore(|| {
        let (dir, data_dir) = scratch("shuttle-ledger-launch");
        let leftovers: Vec<PathBuf> = (0..3)
            .map(|i| dir.join(format!("old-{i}.bin.cmdr-tmp-{i:08}")))
            .collect();
        {
            let crashed_run = Ledger::recording_in(&data_dir);
            let engine = TestOperations::new();
            let state = state_on(&engine, &crashed_run);
            for leftover in &leftovers {
                std::fs::write(leftover, b"half a file").expect("a leftover");
                track(&state, ItemKind::Temp, leftover);
            }
            crashed_run.die();
        }

        let ledger = Ledger::for_test();
        let engine = TestOperations::new();
        let sweep = ledger.launch_in(&data_dir, &engine);

        let copy = {
            let state = state_on(&engine, &ledger);
            let dir = dir.to_path_buf();
            thread::spawn(move || {
                let partials: Vec<TrackedRecord> = (0..3)
                    .map(|i| {
                        let temp = dir.join(format!("new-{i}.bin.cmdr-tmp-{i:08}"));
                        std::fs::write(&temp, b"incoming").expect("a partial");
                        track(&state, ItemKind::Temp, &temp)
                    })
                    .collect();
                partials
            })
        };
        let quit = {
            let ledger = ledger.clone();
            thread::spawn(move || ledger.flush_log())
        };

        let partials = copy.join().expect("the copy");
        quit.join().expect("the quit");
        let tally = sweep.wait();

        for partial in &partials {
            assert!(
                partial.absolute().exists(),
                "the sweep removed a partial this run's copy is still writing: {}",
                partial.absolute().display()
            );
        }
        for leftover in &leftovers {
            assert!(
                !leftover.exists(),
                "the last run's leftover is swept: {}",
                leftover.display()
            );
        }
        assert_eq!(tally.swept, leftovers.len(), "and counted: {tally:?}");
        let held = held(&ledger);
        let expected: BTreeSet<Record> = partials.into_iter().map(|tracked| tracked.record).collect();
        assert_eq!(
            held, expected,
            "the ledger holds this run's partials and nothing the sweep settled"
        );
        ledger.die();
        assert_eq!(on_disk(&data_dir), held, "and the log replays to it");
    });
}
