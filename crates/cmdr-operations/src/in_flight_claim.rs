//! What a launch does with a ledger file: the engine's one claim, which opens
//! the data dir's log, replays what an earlier run left, truncates it, and
//! hands the leftovers to the sweep. The only code that reads a ledger file.

use std::collections::BTreeSet;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use super::super::host::{EngineHost, OperationErrorReport};
use super::log_writer::LogWriter;
use super::primitives::thread;
use super::{Ledger, Record, RecordedTemp, STORE_FILENAME, SweepHandle, TrackedItem, append, sweep};
use cmdr_fs::log_private::LogPrivate;

/// Why [`Operations::claim_ledger`](super::super::Operations::claim_ledger) refused:
/// this engine already claimed its ledger. A second claim would replay the
/// first one's LIVE records and delete its temps mid-copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlreadyClaimed;

impl super::super::Operations {
    /// Points this engine's in-flight ledger at `data_dir` and settles whatever
    /// an earlier run left behind: the one path that reads, truncates, or
    /// sweeps a ledger file, and it takes this engine's one claim.
    ///
    /// Call it once, at startup, on the engine the host installed, after the
    /// data dir is locked to this process (`instance_lock.rs`) and before any
    /// copy can start. ❌ Never on any other engine (a test's, a detached one,
    /// a duplicate that lost the install), and ❌ never inside `build()`.
    ///
    /// `None` (no data dir resolved) leaves the ledger unpersisted, recording
    /// into memory as every unclaimed engine's does, and raises an error
    /// report: the only unpersisted production path.
    ///
    /// Only the data-dir work happens inline. **The disk work goes to its own
    /// thread** (see [`SweepHandle`] for why nothing on the
    /// launch path waits on it): the records it acts on were already retired
    /// from the log by the truncate, so a new copy can start underneath it
    /// safely. A record naming
    /// a volume that isn't in the registry yet is re-recorded and held pending
    /// instead, which is why the truncate can't simply throw the log away.
    pub fn claim_ledger(&self, data_dir: Option<&Path>) -> Result<SweepHandle, AlreadyClaimed> {
        if !self.stores().take_ledger_claim() {
            return Err(AlreadyClaimed);
        }
        let Some(data_dir) = data_dir else {
            self.host()
                .events()
                .error_report(OperationErrorReport::LedgerUnpersisted);
            return Ok(SweepHandle(None));
        };
        Ok(self.stores().ledger().init_and_sweep(data_dir, self.host_arc()))
    }

    /// Waits for every record this engine's ledger queued to reach the kernel,
    /// so the next launch's sweep can see it. The quit teardown's last act
    /// before the process ends.
    ///
    /// Records are written by the ledger's own thread a moment after they're
    /// queued, so without this a quit could end the process with the last few
    /// still in the queue. Bounded by [`FLUSH_WAIT`](super::FLUSH_WAIT): a wedged
    /// disk costs those records, never the quit. Deliberately NOT an `fsync`;
    /// see the `in_flight_temps` module docs on why a power loss is the
    /// directory scan's problem, not this ledger's.
    pub fn flush_in_flight_temps(&self) {
        self.stores().ledger().flush_log();
    }
}

impl Ledger {
    /// Opens `data_dir`'s log, truncating it, and settles what it held, the
    /// sweep running on `host`. Only a claim calls this (and the tests'
    /// `launch_in`, on a ledger of their own).
    pub(super) fn init_and_sweep(&self, data_dir: &Path, host: &Arc<EngineHost>) -> SweepHandle {
        let path = data_dir.join(STORE_FILENAME);
        let recorded = read_recorded(&path);
        // Before the open: a log that won't open is a ledger that couldn't
        // keep its records, ❌ never one that didn't mean to.
        self.store().keeps_a_log = true;

        // Truncating as we open is what retires the records we're about to act
        // on: sweeping twice would be harmless, but a log that only ever grew
        // wouldn't.
        match File::options().create(true).append(true).open(&path) {
            Ok(log) => {
                // `truncate` isn't legal alongside `append`; retire the replayed
                // records with an explicit `ftruncate` instead.
                let _ = log.set_len(0);
                match LogWriter::spawn(log) {
                    Ok(writer) => {
                        let mut store = self.store();
                        store.log = Some(writer);
                        store.logged_bytes = 0;
                    }
                    Err(e) => log::warn!(
                        target: "copy",
                        "couldn't start the in-flight temp ledger's writer: {e}. A copy interrupted this session \
                         will leave its leftovers for the next transfer into that directory to reap."
                    ),
                }
            }
            Err(e) => log::warn!(
                target: "copy",
                "couldn't open the in-flight temp ledger at {}: {e}. A copy interrupted this session will \
                 leave its leftovers for the next transfer into that directory to reap.",
                LogPrivate::file_path(&path)
            ),
        }

        if recorded.is_empty() {
            return SweepHandle(None);
        }

        // Split by path space. The local ones this thread can act on directly; a
        // volume's can only be reached once that volume is registered, which at
        // this point in the launch it usually isn't.
        let (on_volumes, locals): (Vec<Record>, Vec<Record>) =
            recorded.into_iter().partition(|record| record.volume_id().is_some());
        if !on_volumes.is_empty() {
            self.defer(on_volumes);
            self.ensure_arrival_listener(host);
        }

        let ledger = self.clone();
        let host = Arc::clone(host);
        match thread::Builder::new()
            .name("cmdr-temp-sweep".to_string())
            .spawn(move || sweep::persisted_orphans(&ledger, &host, &locals))
        {
            Ok(sweep) => SweepHandle(Some(sweep)),
            Err(e) => {
                log::warn!(target: "copy", "couldn't start the orphaned-leftover sweep: {e}");
                SweepHandle(None)
            }
        }
    }

    /// Re-records `records` and holds the ones with a volume to wait for.
    ///
    /// The re-record is what carries them past the truncate in
    /// [`init_and_sweep`](Self::init_and_sweep): a record the sweep couldn't
    /// act on has to outlive the launch that replayed it, or a NAS orphan is
    /// forgotten by the one ledger that knew about it. A LOCAL record is re-recorded but not held pending:
    /// nothing is going to arrive for it, so the next launch is when it gets
    /// another look.
    pub(super) fn defer(&self, records: Vec<Record>) {
        let mut store = self.store();
        for record in records {
            store.recorded.insert(record.clone());
            append(&mut store, record.add_op(), &record);
            if record.volume_id().is_some() {
                store.pending.insert(record);
            }
        }
    }
}

/// Replays the log and returns what an earlier session left behind.
///
/// Every failure means "nothing recorded": a missing file is the normal
/// clean-exit case, and an unreadable one is not worth failing a launch over. A
/// line that doesn't parse is skipped rather than aborting the replay — the
/// last one can be a torn `write` from the process dying, which is exactly the
/// case this whole ledger exists for. An op byte from a NEWER build is skipped
/// the same way, which is what lets a rollback forget those records instead of
/// acting on them with the wrong rules.
pub(super) fn read_recorded(path: &Path) -> Vec<Record> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut live: BTreeSet<Record> = BTreeSet::new();
    for line in contents.lines() {
        let Some((op, encoded)) = line.split_at_checked(1) else {
            continue;
        };
        let record = match op {
            "+" | "-" => serde_json::from_str::<RecordedTemp>(encoded).ok().map(Record::Legacy),
            "A" | "a" => serde_json::from_str::<TrackedItem>(encoded).ok().map(Record::Tracked),
            _ => None,
        };
        let Some(record) = record else {
            continue;
        };
        if op == "+" || op == "A" {
            live.insert(record);
        } else {
            live.remove(&record);
        }
    }
    live.into_iter().collect()
}
