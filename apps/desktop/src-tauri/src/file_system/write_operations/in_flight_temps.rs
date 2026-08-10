//! The `.cmdr-tmp-*` partials Cmdr is writing RIGHT NOW, kept in two ledgers.
//!
//! - **The operation's own**, `WriteOperationState::in_flight_temps`, in memory.
//!   It answers "what did this operation's abandoned tasks leave behind?" while
//!   the operation is still alive, and its sweep is
//!   `transfer::volume::cleanup::clean_abandoned_staged_writes`.
//! - **A process-wide file in the app data dir**, so the answer survives the
//!   process. An in-memory list dies with a force-quit or a crash — exactly the
//!   two endings that leave partials — and the directory scan that would
//!   otherwise find them ([`transfer::volume::cleanup::reap_stale_transfer_temps`])
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
//! - **Compaction is free**: the log is truncated whenever nothing is in flight
//!   and it has grown past [`COMPACT_ABOVE_BYTES`], which for a serial copy is
//!   between two files. Nothing accumulates across a session.
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

/// Truncate the log once nothing is in flight and it has grown past this. Small
/// enough that a session never carries a big file, large enough that a serial
/// copy doesn't truncate after every single file.
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
    compact_if_idle(&mut store);
}

/// Points the persisted ledger at the app data dir and clears whatever an
/// earlier run left behind. Call once at startup, before any copy can start.
pub fn init_and_sweep(data_dir: &Path) {
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

    sweep_persisted_orphans(&recorded);
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

/// Truncates the log once nothing is in flight and it has grown past
/// [`COMPACT_ABOVE_BYTES`], so a long session doesn't leave a big file behind.
fn compact_if_idle(store: &mut Store) {
    if !store.live.is_empty() || store.logged_bytes < COMPACT_ABOVE_BYTES {
        return;
    }
    let Some(log) = &mut store.log else {
        return;
    };
    // The handle is in append mode, so writes go to the new end without a seek.
    match log.set_len(0) {
        Ok(()) => store.logged_bytes = 0,
        Err(e) => log::debug!(target: "copy", "couldn't compact the in-flight temp ledger: {e}"),
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use super::{File, PathBuf, STORE};
    use crate::ignore_poison::IgnorePoison;

    /// Points the persisted ledger at `data_dir` for the length of the guard,
    /// restoring the previous target on drop so one test can't leak its log
    /// into another.
    pub(in crate::file_system::write_operations) struct StoreGuard {
        previous: Option<File>,
        previous_bytes: u64,
    }

    pub(in crate::file_system::write_operations) fn use_store_in(data_dir: &std::path::Path) -> StoreGuard {
        let log = File::options()
            .create(true)
            .append(true)
            .open(data_dir.join(super::STORE_FILENAME))
            .expect("open a test in-flight temp ledger");
        log.set_len(0).expect("start the test ledger empty");
        let mut store = STORE.lock_ignore_poison();
        let previous = store.log.replace(log);
        let previous_bytes = std::mem::take(&mut store.logged_bytes);
        store.live.clear();
        StoreGuard {
            previous,
            previous_bytes,
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
    pub(in crate::file_system::write_operations) fn live_paths() -> Vec<PathBuf> {
        STORE.lock_ignore_poison().live.iter().cloned().collect()
    }

    /// Forgets everything, so a test starts from a clean process-wide state.
    pub(in crate::file_system::write_operations) fn clear() {
        let mut store = STORE.lock_ignore_poison();
        store.live.clear();
        store.log = None;
        store.logged_bytes = 0;
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
        test_support::clear();

        // A previous run: it registered a partial and then died.
        let orphan = {
            let guard = test_support::use_store_in(&data_dir);
            let state = state();
            let temp = StagingTemp::mint(&dir.join("holiday.raw"), None);
            std::fs::write(temp.path(), b"half a photo").unwrap();
            register(&state, temp.path());
            drop(guard);
            temp.path().to_path_buf()
        };
        assert!(orphan.exists(), "the fixture partial must be on disk");
        // The process is gone: only the file in the data dir remembers it.
        test_support::clear();

        init_and_sweep(&data_dir);

        assert!(
            !orphan.exists(),
            "a partial recorded by an earlier run must be swept at startup, with no age gate"
        );
        assert!(
            test_support::live_paths().is_empty(),
            "and the ledger must start the new session empty"
        );
        assert_eq!(
            std::fs::read_to_string(data_dir.join(STORE_FILENAME)).unwrap(),
            "",
            "the swept records must be retired on disk too, so the next launch has nothing to redo"
        );
        test_support::clear();
    }

    /// A temp that landed is recorded as gone, so replaying the log doesn't
    /// resurrect it as an orphan. Without the `-` record every file ever copied
    /// would be a sweep candidate at the next launch.
    #[test]
    fn a_landed_temp_is_retired_from_the_log() {
        let dir = TestDir::new("in_flight_temps_retired");
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        test_support::clear();

        {
            let _guard = test_support::use_store_in(&data_dir);
            let state = state();
            let temp = dir.join("holiday.raw.cmdr-tmp-1234");
            register(&state, &temp);
            deregister(&state, &temp);
        }

        assert!(
            read_recorded(&data_dir.join(STORE_FILENAME)).is_empty(),
            "a temp that came and went must replay as nothing in flight"
        );
        test_support::clear();
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
        std::fs::write(
            data_dir.join(STORE_FILENAME),
            format!(
                "+{}\n",
                serde_json::to_string(&dir.join("gone.txt.cmdr-tmp-abc")).unwrap()
            ),
        )
        .unwrap();
        test_support::clear();

        init_and_sweep(&data_dir);

        assert!(test_support::live_paths().is_empty());
        test_support::clear();
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
        std::fs::write(
            data_dir.join(STORE_FILENAME),
            format!("+{}\n", serde_json::to_string(&precious).unwrap()),
        )
        .unwrap();
        test_support::clear();

        init_and_sweep(&data_dir);

        assert!(
            precious.exists(),
            "the sweep must only ever remove files carrying our scratch marker"
        );
        test_support::clear();
    }

    /// Registering and deregistering keep both ledgers in step, so nothing
    /// sweeps a file that landed.
    #[test]
    fn deregistering_clears_both_ledgers() {
        let dir = TestDir::new("in_flight_temps_both_ledgers");
        let data_dir = dir.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        test_support::clear();
        let _guard = test_support::use_store_in(&data_dir);

        let state = state();
        let temp = dir.join("notes.txt.cmdr-tmp-1234");
        register(&state, &temp);
        assert_eq!(state.in_flight_temps.lock_ignore_poison().len(), 1);
        assert_eq!(test_support::live_paths(), vec![temp.clone()]);

        deregister(&state, &temp);
        assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
        assert!(test_support::live_paths().is_empty());
        assert!(
            read_recorded(&data_dir.join(STORE_FILENAME)).is_empty(),
            "the log must replay as nothing in flight"
        );
    }
}
