//! The test seams on [`Ledger`]: a ledger of a test's own, recording into a log
//! it can read back, and the crash and the next launch it stands in for. Kept
//! apart so the production file reads as production.

use std::fs::File;
use std::path::Path;

use super::super::Operations;
use super::log_writer::LogWriter;
use super::{Ledger, Record, STORE_FILENAME, SweepHandle};

/// A test's own ledger. Each test builds one, hands it to the states it drives
/// (`WriteOperationState::with_in_flight_ledger`), and asserts on it alone, so
/// no other test's records ever land in its log or its sweep.
impl Ledger {
    /// A ledger for a process that hasn't launched yet: no log, nothing
    /// recorded. [`launch_in`](Self::launch_in) is that launch.
    pub(crate) fn for_test() -> Self {
        Self::new()
    }

    /// A ledger recording into a fresh log in `data_dir`, so a test can read
    /// back what [`register`] wrote. Drop it to stand in for the process
    /// dying: the log on disk stays exactly as it was, and a new
    /// [`for_test`](Self::for_test) ledger launching against `data_dir` is the
    /// next run.
    pub(crate) fn recording_in(data_dir: &Path) -> Self {
        let ledger = Self::new();
        let path = data_dir.join(STORE_FILENAME);
        let log = File::options()
            .create(true)
            .append(true)
            .open(&path)
            .expect("open a test in-flight temp ledger");
        log.set_len(0).expect("start the test ledger empty");
        let mut store = ledger.store();
        store.keeps_a_log = true;
        store.log = Some(LogWriter::spawn(log, path).expect("start the test ledger's writer"));
        drop(store);
        ledger
    }

    /// [`recording_in`](Self::recording_in), over a log the writer can't write
    /// to (opened read-only), the way a full or read-only data disk refuses.
    pub(crate) fn recording_into_a_log_that_refuses_writes(data_dir: &Path) -> Self {
        let path = data_dir.join(STORE_FILENAME);
        File::create(&path).expect("create a test in-flight temp ledger");
        let ledger = Self::new();
        let log = File::open(&path).expect("open it read-only");
        let mut store = ledger.store();
        store.keeps_a_log = true;
        store.log = Some(LogWriter::spawn(log, path).expect("start the test ledger's writer"));
        drop(store);
        ledger
    }

    /// The paths this ledger records as landings right now.
    pub(crate) fn recorded_landings(&self) -> Vec<std::path::PathBuf> {
        self.store()
            .recorded
            .iter()
            .filter(|record| {
                matches!(
                    record,
                    Record::Tracked(super::TrackedItem {
                        kind: super::ItemKind::Landing { .. },
                        ..
                    })
                )
            })
            .map(|record| record.path().to_path_buf())
            .collect()
    }

    /// The asides a swap recorded (`ItemKind::FileAside`) and hasn't retired.
    pub(crate) fn recorded_asides(&self) -> Vec<std::path::PathBuf> {
        self.store()
            .recorded
            .iter()
            .filter(|record| {
                matches!(
                    record,
                    Record::Tracked(super::TrackedItem {
                        kind: super::ItemKind::FileAside { .. },
                        ..
                    })
                )
            })
            .map(|record| record.path().to_path_buf())
            .collect()
    }

    /// Parks this ledger's writer thread until the returned sender is dropped,
    /// so a test can see what a caller does while its line is still queued.
    pub(crate) fn hold_writer(&self) -> std::sync::mpsc::Sender<()> {
        let (release, held) = std::sync::mpsc::channel::<()>();
        let (parked, is_parked) = std::sync::mpsc::channel();
        let queued = self.store().log.as_ref().is_some_and(|log| {
            log.barrier(move || {
                let _ = parked.send(());
                // Returns once the test drops `release`.
                let _ = held.recv();
            })
        });
        assert!(queued, "hold_writer needs a ledger with a log (recording_in)");
        is_parked.recv().expect("the writer to park");
        release
    }

    /// The process dies: whatever it queued has reached the log, as a
    /// `write(2)` that returned has before a kill, and nothing more is written.
    /// A state still holding a clone can't write into the log after this, so
    /// the next run replays exactly what this one left.
    pub(crate) fn die(self) {
        self.flush_log();
        self.store().log = None;
    }

    /// A claim's launch against this ledger on `engine`'s host: a launch that
    /// replays `data_dir`'s log. The cells pass an engine over the app's answers
    /// (`operations_host::test_engine()`), so a deferred record waits on the app's
    /// volume registry, where they register their drives. Keep `engine` for as long
    /// as the cell waits on an arrival: the ledger holds its host weakly, as the
    /// app's one engine is held for the whole run.
    pub(crate) fn launch_in(&self, data_dir: &Path, engine: &Operations) -> SweepHandle {
        self.init_and_sweep(data_dir, engine.host_arc())
    }

    /// What a launch would replay from `data_dir`'s log right now: only the
    /// lines the writer thread has already written, with no flush first, so a
    /// cell can ask what was on disk at a given moment.
    pub(crate) fn replay_log(data_dir: &Path) -> Vec<Record> {
        super::read_recorded(&data_dir.join(STORE_FILENAME))
    }

    /// The paths waiting for their volume's next arrival or reconnect.
    pub(crate) fn pending_paths(&self) -> Vec<std::path::PathBuf> {
        self.store()
            .pending
            .iter()
            .map(Record::path)
            .map(Path::to_path_buf)
            .collect()
    }

    /// The set this ledger currently believes is on disk.
    pub(crate) fn live_paths(&self) -> Vec<std::path::PathBuf> {
        self.store()
            .recorded
            .iter()
            .map(Record::path)
            .map(Path::to_path_buf)
            .collect()
    }
}
