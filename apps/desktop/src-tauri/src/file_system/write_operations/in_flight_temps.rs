//! The `.cmdr-tmp-*` partials Cmdr is writing RIGHT NOW, kept in two ledgers.
//!
//! - **The operation's own**, `WriteOperationState::in_flight_temps`, in memory.
//!   It answers "what did this operation's abandoned tasks leave behind?" while
//!   the operation is still alive, and its sweep is
//!   `transfer::volume::cleanup::clean_abandoned_staged_writes`.
//! - **A process-wide file in the app data dir**, so the answer survives the
//!   process. An in-memory list dies with a force-quit or a crash — exactly the
//!   two endings that leave partials — and the directory scan that would
//!   otherwise find them (`reap_stale_transfer_temps`)
//!   only runs when something copies into that same directory, and only for
//!   leftovers over an hour old. So a quit-orphaned temp could sit there for
//!   days. [`sweep_persisted_orphans`] clears the recorded ones at the next
//!   launch instead.
//!
//! **No age gate on the persisted sweep, and that's safe.** The hour the
//! directory scan waits exists to protect a temp a CONCURRENT Cmdr is streaming
//! into. These paths aren't guesses from a name pattern: each one is a path this
//! app recorded when it minted the UUID in it, so no other instance can own it,
//! and the instance lock already keeps two processes off one data dir. What is
//! recorded and still on disk at startup is ours and is garbage.
//!
//! ## Granularity: one append per change, on an open handle, never fsynced
//!
//! This sits on the per-file hot path — a 2 000-file local copy hits it 4 000
//! times — so the write has to be about as cheap as a write can be:
//!
//! - **One `write(2)` per change**, appending a single line to a handle held
//!   open for the session. ❌ No rewrite-the-whole-file (a create + write +
//!   rename per change measured at **+0.4 ms per file**, tripling a
//!   many-small-files copy), ❌ no per-change path resolution.
//! - **❌ No `fsync`.** An fsync here would cost milliseconds per file and turn
//!   a copy into a flush-per-file crawl. An unsynced write is already in the
//!   page cache, so the record survives the process dying — a quit, a panic, a
//!   `SIGKILL`, which is every ending this exists for. A power loss can lose
//!   it, and then the hour-gated directory scan is the backstop; that's the
//!   right trade, since a power loss can equally lose the temp's own directory
//!   entry and leave nothing to sweep.
//! - **Compaction is cheap and unconditional**: past [`COMPACT_ABOVE_BYTES`] the
//!   log is rewritten down to just what's in flight (a handful of paths), so
//!   nothing accumulates across a long copy or a long session.
//!
//! The format is one line per record: `+` or `-`, then the path as a JSON
//! string (so a newline in a filename can't forge a record). A trailing torn
//! line — the process died mid-`write` — is ignored on read. A path that isn't
//! valid UTF-8 can't be written as JSON and goes unrecorded; the hour-gated
//! directory scan remains its backstop.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use super::state::WriteOperationState;
use crate::ignore_poison::IgnorePoison;

/// The persisted log's file name inside the app data dir.
const STORE_FILENAME: &str = "in-flight-temps.log";

/// Rewrite the log down to just what's in flight once it has grown past this.
/// Small enough that a session never carries a big file, large enough that a
/// serial copy doesn't rewrite after every single file (~50 files' worth).
const COMPACT_ABOVE_BYTES: u64 = 8 * 1024;

/// The process-wide half: the open log, and what's currently in flight.
///
// DEFAULT-OK: the zero value is the truthful pre-startup state — no log open
// yet, nothing written to it, and nothing in flight, which is exactly where a
// process begins. It claims nothing about the disk.
#[derive(Default)]
struct Store {
    /// `None` until [`init_and_sweep`] runs, which is also what keeps unit
    /// tests from touching disk unless they ask to.
    log: Option<File>,
    /// Bytes appended since the last truncation, tracked here so the compaction
    /// check costs no syscall.
    logged_bytes: u64,
    live: BTreeSet<PathBuf>,
}

static STORE: LazyLock<Mutex<Store>> = LazyLock::new(|| Mutex::new(Store::default()));

/// Records `temp` as a partial this operation is writing, in both ledgers.
///
/// Call before the first byte can land there, and pair with [`deregister`] the
/// moment the file stops being a partial.
pub(super) fn register(state: &WriteOperationState, temp: &Path) {
    state.in_flight_temps.lock_ignore_poison().push(temp.to_path_buf());
    let mut store = STORE.lock_ignore_poison();
    store.live.insert(temp.to_path_buf());
    append(&mut store, b'+', temp);
}

/// Stops tracking `temp`: it landed under its real name, or it's gone.
pub(super) fn deregister(state: &WriteOperationState, temp: &Path) {
    state.in_flight_temps.lock_ignore_poison().retain(|p| p != temp);
    let mut store = STORE.lock_ignore_poison();
    store.live.remove(temp);
    append(&mut store, b'-', temp);
    compact_if_large(&mut store);
}

/// Pushes whatever the ledger is holding out to the kernel, so the next launch's
/// sweep can see it. The quit teardown's last act before the process ends.
///
/// Today every record already reaches the kernel inside [`register`] — the log is
/// a bare `File`, so there is no user-space buffer to lose — and this is the
/// explicit fence that keeps it that way: if the handle ever gains a `BufWriter`,
/// the quit path won't silently start dropping the last partials it recorded.
/// Deliberately NOT an `fsync`; see the module docs on why a power loss is the
/// directory scan's problem, not this ledger's.
pub fn flush() {
    let mut store = STORE.lock_ignore_poison();
    let Some(log) = &mut store.log else {
        return;
    };
    if let Err(e) = log.flush() {
        log::warn!(target: "copy", "couldn't flush the in-flight temp ledger before exit: {e}");
    }
}

/// The background sweep [`init_and_sweep`] started, and the only way to learn
/// that it has finished.
///
/// **The launch path drops it.** Waiting there is the one thing this must never
/// do: a recorded partial can sit on a Finder-mounted NAS that is no longer
/// answering, and `unlink` on a dead mount blocks for a minute or two, which
/// reads to the user as an app that won't launch. Whoever can afford to wait —
/// a test asserting on what the sweep removed — calls [`SweepHandle::wait`]
/// instead of racing a wall-clock deadline it can only lose under load.
///
/// The handle stays in the signature on every build rather than behind a
/// `cfg(test)`: a function whose shape changes between the app and its tests
/// is a function the tests no longer describe.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the launch path drops the handle; only a caller that can wait joins it"
    )
)]
pub struct SweepHandle(Option<std::thread::JoinHandle<()>>);

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the launch path drops the handle; only a caller that can wait joins it"
    )
)]
impl SweepHandle {
    /// Blocks until the sweep has visited every path the ledger recorded.
    ///
    /// ❌ Never call this from a launch path; see the type docs for why.
    pub fn wait(self) {
        if let Some(handle) = self.0
            && handle.join().is_err()
        {
            log::warn!(target: "copy", "the orphaned-partial sweep panicked; some partials may remain");
        }
    }
}

/// Points the persisted ledger at the app data dir and clears whatever an
/// earlier run left behind. Call once at startup, before any copy can start.
///
/// Only the app-data-dir work happens inline. **The deletes go to their own
/// thread** (see [`SweepHandle`] for why nothing on the launch path waits on
/// it): the records it acts on were already retired from the log by the
/// truncate below, so a new copy can start underneath it safely.
pub fn init_and_sweep(data_dir: &Path) -> SweepHandle {
    let path = data_dir.join(STORE_FILENAME);
    let recorded = read_recorded(&path);

    // Truncating as we open is what retires the records we're about to act on:
    // sweeping twice would be harmless, but a log that only ever grew wouldn't.
    match File::options().create(true).append(true).open(&path) {
        Ok(log) => {
            // `truncate` isn't legal alongside `append`; retire the replayed
            // records with an explicit `ftruncate` instead.
            let _ = log.set_len(0);
            let mut store = STORE.lock_ignore_poison();
            store.log = Some(log);
            store.logged_bytes = 0;
        }
        Err(e) => log::warn!(
            target: "copy",
            "couldn't open the in-flight temp ledger at {}: {e}. A copy interrupted this session will \
             leave its partial for the next transfer into that directory to reap.",
            path.display()
        ),
    }

    if recorded.is_empty() {
        return SweepHandle(None);
    }
    match std::thread::Builder::new()
        .name("cmdr-temp-sweep".to_string())
        .spawn(move || sweep_persisted_orphans(&recorded))
    {
        Ok(sweep) => SweepHandle(Some(sweep)),
        Err(e) => {
            log::warn!(target: "copy", "couldn't start the orphaned-partial sweep: {e}");
            SweepHandle(None)
        }
    }
}

/// Removes the partials an earlier run recorded and never finished.
///
/// Skips anything that isn't one of ours by name: the ledger should only ever
/// hold `.cmdr-tmp-*` paths, and a delete driven by a file is worth one cheap
/// check that the file wasn't tampered with. A path that's already gone (the
/// normal case — it landed under its real name) costs nothing.
fn sweep_persisted_orphans(recorded: &[PathBuf]) {
    let mut swept = 0usize;
    for temp in recorded {
        let is_ours = temp
            .file_name()
            .is_some_and(|n| cmdr_fs::staging::is_staging_temp_name(&n.to_string_lossy()));
        if !is_ours {
            log::warn!(
                target: "copy",
                "in-flight temp ledger holds a path that isn't one of our scratch files, leaving it: {}",
                temp.display()
            );
            continue;
        }
        match std::fs::remove_file(temp) {
            Ok(()) => swept += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::debug!(
                target: "copy",
                "couldn't sweep the orphaned transfer partial {}: {e}",
                temp.display()
            ),
        }
    }
    if swept > 0 {
        log::info!(
            target: "copy",
            "swept {swept} transfer partial(s) an earlier run left behind"
        );
    }
}

/// Replays the log and returns what an earlier session left in flight.
///
/// Every failure means "nothing recorded": a missing file is the normal
/// clean-exit case, and an unreadable one is not worth failing a launch over. A
/// line that doesn't parse is skipped rather than aborting the replay — the
/// last one can be a torn `write` from the process dying, which is exactly the
/// case this whole ledger exists for.
fn read_recorded(path: &Path) -> Vec<PathBuf> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut live: BTreeSet<PathBuf> = BTreeSet::new();
    for line in contents.lines() {
        let Some((op, encoded)) = line.split_at_checked(1) else {
            continue;
        };
        let Ok(temp) = serde_json::from_str::<PathBuf>(encoded) else {
            continue;
        };
        match op {
            "+" => {
                live.insert(temp);
            }
            "-" => {
                live.remove(&temp);
            }
            _ => {}
        }
    }
    live.into_iter().collect()
}

/// Appends one record. See the module docs for the format and why there's no
/// fsync.
///
/// Best-effort: a record we couldn't write costs a leftover the hour-gated
/// directory scan still catches, so it must never fail a copy.
fn append(store: &mut Store, op: u8, temp: &Path) {
    let Some(log) = &mut store.log else {
        return;
    };
    let Ok(encoded) = serde_json::to_string(temp) else {
        // Not valid UTF-8, so it can't be a JSON string. Rare enough to accept:
        // the directory scan is this path's backstop.
        log::debug!(target: "copy", "not recording the in-flight temp {}: its name isn't UTF-8", temp.display());
        return;
    };
    let mut line = Vec::with_capacity(encoded.len() + 2);
    line.push(op);
    line.extend_from_slice(encoded.as_bytes());
    line.push(b'\n');
    match log.write_all(&line) {
        Ok(()) => store.logged_bytes += line.len() as u64,
        Err(e) => log::debug!(target: "copy", "couldn't record the in-flight temp {}: {e}", temp.display()),
    }
}

/// Rewrites the log down to just what's in flight once it has grown past
/// [`COMPACT_ABOVE_BYTES`], so a long copy can't grow an unbounded file.
///
/// ❌ Don't gate this on "nothing is in flight": the concurrent cross-volume
/// driver keeps a window open for the whole transfer, so an idle-only rule would
/// let a 100k-file copy append megabytes before it ever got a chance to run.
/// Rewriting the live set (a handful of paths) costs one truncate and one write
/// every ~50 files.
fn compact_if_large(store: &mut Store) {
    if store.logged_bytes < COMPACT_ABOVE_BYTES {
        return;
    }
    let live: Vec<PathBuf> = store.live.iter().cloned().collect();
    let Some(log) = &mut store.log else {
        return;
    };
    // The handle is in append mode, so writes go to the new end without a seek.
    if let Err(e) = log.set_len(0) {
        log::debug!(target: "copy", "couldn't compact the in-flight temp ledger: {e}");
        return;
    }
    store.logged_bytes = 0;
    for temp in &live {
        append(store, b'+', temp);
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use super::{File, PathBuf, STORE};
    use crate::ignore_poison::IgnorePoison;
    use std::sync::{Mutex, MutexGuard};

    /// Serializes every test that installs its own ledger into [`STORE`].
    ///
    /// [`STORE`] is ONE singleton for the whole test binary, and installing a
    /// log into it redirects every `register` in the process, from any thread,
    /// into that file. Two tests doing it at once is how one test's records
    /// land in another's log, and how a startup-sweep fixture ends up replaying
    /// an empty log and never sweeping at all. Both shapes reproduced at
    /// `--test-threads=2` (47 failures in 60 runs) before this lock existed.
    static SINGLE_FILE: Mutex<()> = Mutex::new(());

    /// Exclusive use of the process-wide ledger for the length of the guard.
    ///
    /// Hold it across the WHOLE test body, ❌ never just the part that writes:
    /// the moment it drops, another test may install its log and take over
    /// every `register` this one still had coming.
    pub(in crate::file_system::write_operations) struct StoreGuard {
        previous: Option<File>,
        previous_bytes: u64,
        // Declared last so it's released after `Drop` has put the singleton
        // back: the next test in line must never see this one's log.
        _single_file: MutexGuard<'static, ()>,
    }

    /// Takes the ledger for the length of the guard and leaves it EMPTY: no log
    /// open, nothing in flight, which is exactly where a process begins.
    pub(in crate::file_system::write_operations) fn take_store() -> StoreGuard {
        // Poison here just means an earlier test panicked while holding it. The
        // singleton was restored by that guard's `Drop` on the way out, so the
        // state is sound and the next test deserves a real verdict, not an
        // unwrap on someone else's failure.
        let single_file = SINGLE_FILE.lock_ignore_poison();
        let mut store = STORE.lock_ignore_poison();
        let previous = store.log.take();
        let previous_bytes = std::mem::take(&mut store.logged_bytes);
        store.live.clear();
        StoreGuard {
            previous,
            previous_bytes,
            _single_file: single_file,
        }
    }

    /// [`take_store`], then points the ledger at `data_dir` so [`super::register`]
    /// records into a file this test can read back.
    pub(in crate::file_system::write_operations) fn use_store_in(data_dir: &std::path::Path) -> StoreGuard {
        let guard = take_store();
        let log = File::options()
            .create(true)
            .append(true)
            .open(data_dir.join(super::STORE_FILENAME))
            .expect("open a test in-flight temp ledger");
        log.set_len(0).expect("start the test ledger empty");
        STORE.lock_ignore_poison().log = Some(log);
        guard
    }

    impl StoreGuard {
        /// Drops the process's handle on the ledger without giving the
        /// singleton back to the next test: what a crash looks like to the next
        /// launch. The log on disk is left exactly as it was, which is the
        /// whole point of the fixture.
        pub(in crate::file_system::write_operations) fn simulate_process_exit(&self) {
            let mut store = STORE.lock_ignore_poison();
            store.log = None;
            store.logged_bytes = 0;
            store.live.clear();
        }
    }

    impl Drop for StoreGuard {
        fn drop(&mut self) {
            let mut store = STORE.lock_ignore_poison();
            store.log = self.previous.take();
            store.logged_bytes = self.previous_bytes;
            store.live.clear();
        }
    }

    /// The set the process currently believes is in flight.
    ///
    /// Process-wide, so a concurrent transfer test's staged write shows up here
    /// too: ask whether it holds the path under test, ❌ never whether it's
    /// empty.
    pub(in crate::file_system::write_operations) fn live_paths() -> Vec<PathBuf> {
        STORE.lock_ignore_poison().live.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;
    use cmdr_fs::staging::StagingTemp;
    use std::sync::Arc;
    use std::time::Duration;

    fn state() -> Arc<WriteOperationState> {
        Arc::new(WriteOperationState::new(Duration::from_millis(50)))
    }

    /// The whole point of persisting: a partial recorded by a run that never
    /// came back is gone at the next launch, however young it is. The
    /// directory scan's one-hour gate protects against a concurrent instance;
    /// a path we recorded ourselves needs no such protection, and waiting an
    /// hour to clear it is the gap this closes.
    #[test]
    fn a_recorded_orphan_is_swept_at_startup_however_fresh_it_is() {
        let dir = TestDir::new("in_flight_temps_startup_sweep");
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let store = test_support::use_store_in(&data_dir);

        // A previous run: it registered a partial and then died.
        let state = state();
        let temp = StagingTemp::mint(&dir.join("holiday.raw"), None);
        std::fs::write(temp.path(), b"half a photo").unwrap();
        register(&state, temp.path());
        let orphan = temp.path().to_path_buf();
        assert!(orphan.exists(), "the fixture partial must be on disk");
        // The process is gone: only the file in the data dir remembers it.
        store.simulate_process_exit();

        // Joining the sweep is what keeps this honest. It runs off the startup
        // thread (a partial can live on a dead mount), and a deadline racing it
        // would fail on load rather than on a real break.
        init_and_sweep(&data_dir).wait();

        assert!(
            !orphan.exists(),
            "the recorded orphan must be swept at startup, with no age gate"
        );
        assert!(
            !test_support::live_paths().contains(&orphan),
            "and the new session must not start with the swept path in flight"
        );
        assert!(
            !read_recorded(&data_dir.join(STORE_FILENAME)).contains(&orphan),
            "the swept record must be retired on disk too, so the next launch has nothing to redo"
        );
    }

    /// A temp that landed is recorded as gone, so replaying the log doesn't
    /// resurrect it as an orphan. Without the `-` record every file ever copied
    /// would be a sweep candidate at the next launch.
    #[test]
    fn a_landed_temp_is_retired_from_the_log() {
        let dir = TestDir::new("in_flight_temps_retired");
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let _store = test_support::use_store_in(&data_dir);

        let state = state();
        let temp = dir.join("holiday.raw.cmdr-tmp-1234");
        register(&state, &temp);
        deregister(&state, &temp);

        assert!(
            !read_recorded(&data_dir.join(STORE_FILENAME)).contains(&temp),
            "a temp that came and went must replay as nothing in flight"
        );
    }

    /// Compaction runs on size alone, ❌ not on "nothing is in flight" — the
    /// concurrent cross-volume driver holds a window open for a whole transfer,
    /// so an idle-only rule would let a big copy grow megabytes. So it has to
    /// survive being run while something IS in flight: the still-live partial
    /// must still be there to sweep afterwards.
    #[test]
    fn compaction_shrinks_the_log_without_forgetting_what_is_still_in_flight() {
        let dir = TestDir::new("in_flight_temps_compaction");
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let log_path = data_dir.join(STORE_FILENAME);
        let _store = test_support::use_store_in(&data_dir);

        let state = state();
        let long_lived = dir.join("a-big-download.iso.cmdr-tmp-0000");
        register(&state, &long_lived);

        // Enough churn to cross the compaction threshold several times over.
        let churned: Vec<PathBuf> = (0..400)
            .map(|i| dir.join(format!("small-file-{i:04}.txt.cmdr-tmp-{i:04}")))
            .collect();
        for churn in &churned {
            register(&state, churn);
            deregister(&state, churn);
        }

        assert!(
            std::fs::metadata(&log_path).unwrap().len() < COMPACT_ABOVE_BYTES * 2,
            "the log must stay bounded while a long transfer churns through it"
        );
        let replayed = read_recorded(&log_path);
        assert!(
            replayed.contains(&long_lived),
            "compaction must keep the partial that is still being written"
        );
        assert!(
            churned.iter().all(|churn| !replayed.contains(churn)),
            "and must forget every partial that already landed"
        );
    }

    /// The log is a stream of appends, so the process can die mid-`write`. The
    /// replay has to drop the torn tail and keep everything before it — the
    /// records before the tear are the ones naming real orphans.
    #[test]
    fn a_torn_last_line_doesnt_cost_the_records_before_it() {
        let dir = TestDir::new("in_flight_temps_torn");
        let good = dir.join("a.raw.cmdr-tmp-1111");
        let log = dir.join(STORE_FILENAME);
        let mut contents = format!("+{}\n", serde_json::to_string(&good).unwrap());
        contents.push_str("+\"/half-a-pa"); // the process died here
        std::fs::write(&log, contents).unwrap();

        assert_eq!(read_recorded(&log), vec![good]);
    }

    /// A temp that landed under its real name before the crash leaves a
    /// recorded path pointing at nothing. That's the common case and must be
    /// silent, not an error.
    #[test]
    fn a_recorded_path_that_is_already_gone_sweeps_cleanly() {
        let dir = TestDir::new("in_flight_temps_already_gone");
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let gone = dir.join("gone.txt.cmdr-tmp-abc");
        std::fs::write(
            data_dir.join(STORE_FILENAME),
            format!("+{}\n", serde_json::to_string(&gone).unwrap()),
        )
        .unwrap();
        let _store = test_support::take_store();

        // Joining also keeps the sweep from outliving `dir`, which would leave
        // it walking a directory `TestDir` is deleting.
        init_and_sweep(&data_dir).wait();

        assert!(
            !test_support::live_paths().contains(&gone),
            "a record whose file is already gone must not come back as in flight"
        );
    }

    /// The sweep deletes files. It follows the ledger, so it checks that what
    /// the ledger names really is one of our scratch files before removing it —
    /// a corrupted or hand-edited store must not become a delete-anything
    /// primitive.
    #[test]
    fn the_sweep_refuses_a_recorded_path_that_isnt_one_of_our_scratch_files() {
        let dir = TestDir::new("in_flight_temps_not_ours");
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let precious = dir.join("taxes.pdf");
        std::fs::write(&precious, b"the user's own file").unwrap();
        // A real temp recorded alongside it, as the positive control: with the
        // sweep joined below, this one being gone proves the sweep ran and does
        // delete, so `taxes.pdf` surviving is the marker check saving it rather
        // than the sweep never getting that far.
        let real_temp = dir.join("holiday.raw.cmdr-tmp-9999");
        std::fs::write(&real_temp, b"half a photo").unwrap();
        std::fs::write(
            data_dir.join(STORE_FILENAME),
            format!(
                "+{}\n+{}\n",
                serde_json::to_string(&precious).unwrap(),
                serde_json::to_string(&real_temp).unwrap()
            ),
        )
        .unwrap();
        let _store = test_support::take_store();

        init_and_sweep(&data_dir).wait();

        assert!(
            !real_temp.exists(),
            "the sweep must remove the recorded partial carrying our scratch marker"
        );
        assert!(
            precious.exists(),
            "the sweep must only ever remove files carrying our scratch marker"
        );
    }

    /// Registering and deregistering keep both ledgers in step, so nothing
    /// sweeps a file that landed.
    #[test]
    fn deregistering_clears_both_ledgers() {
        let dir = TestDir::new("in_flight_temps_both_ledgers");
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let _store = test_support::use_store_in(&data_dir);

        let state = state();
        let temp = dir.join("notes.txt.cmdr-tmp-1234");
        register(&state, &temp);
        assert_eq!(state.in_flight_temps.lock_ignore_poison().len(), 1);
        assert!(test_support::live_paths().contains(&temp));

        deregister(&state, &temp);
        assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
        assert!(!test_support::live_paths().contains(&temp));
        assert!(
            !read_recorded(&data_dir.join(STORE_FILENAME)).contains(&temp),
            "the log must replay as nothing in flight"
        );
    }
}
