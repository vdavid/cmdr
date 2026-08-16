//! A first index that stopped with ground still on its frontier, and the retry
//! that finishes it.
//!
//! The state under test is the one `churn_bench` measured over a six-figure tree
//! and minutes of real writing: a machine runs out of passes, the drive never
//! gets its completion marker, and before this nothing in the session went back
//! for it. These drive the same ending in milliseconds, by writing the row the
//! live reconciler writes for a folder somebody just created — present, and listed
//! by nobody, which is frontier by the descent rule.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::*;
use crate::indexing::events::{EventSink, IndexEvent, RecordingSink};
use crate::indexing::lifecycle::completion_retry;
use crate::indexing::lifecycle::state;
use crate::indexing::store::EntryRow;
use crate::indexing::writer::IndexWriter;

/// A machine that ran out of passes with ground still on its frontier asks for
/// another go.
///
/// Before this the drive stayed unmarked for the whole session, however long that
/// was: the `Fresh` badge, the scan calibration, the `dir_stats` ledger heal, the
/// sweep ledger, and rescan routing all wait on the marker, and nothing
/// re-triggered the machine in-session.
#[test]
fn a_machine_that_stops_short_asks_for_another_go() {
    let (drive, churn) = churning_drive("phased-retry-armed");
    drive.start();
    drive.wait_for_the_machine();

    assert!(
        churn.folders_created() > 0,
        "precondition: nothing simulated the churn, so this drive had no reason to stop short"
    );
    assert_eq!(
        drive.meta("scan_completed_at"),
        None,
        "precondition: the machine has to have stopped SHORT for there to be anything to retry"
    );
    assert!(
        !drive.frontier(&drive.path("")).is_empty(),
        "precondition: the folders the churn added are what it stopped short of"
    );
    assert!(
        completion_retry::is_waiting(drive.volume_id),
        "❌ the drive stays unmarked until somebody relaunches Cmdr: nothing asked for another pass"
    );
}

/// And the retry finishes the drive once the writing stops, which is the whole
/// point: the same ~2 s resume a relaunch runs, without the relaunch.
#[test]
fn the_retry_completes_the_drive_once_the_churn_stops() {
    let (drive, churn) = churning_drive("phased-retry-completes");
    drive.start();
    drive.wait_for_the_machine();
    assert_eq!(drive.meta("scan_completed_at"), None, "precondition: it stopped short");

    // The build finished, the package manager stopped unpacking. Nothing else
    // changes: the retry is the ordinary resume over what is left.
    churn.stop();
    completion_retry::nudge_at(drive.volume_id, crate::indexing::store::now_unix() + 61);
    drive.wait_for_the_machine();

    assert!(
        drive.meta("scan_completed_at").is_some(),
        "the retry has to leave the drive marked complete, or the wait is still the next launch"
    );
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and it got there by WALKING the leftovers, ❌ never by claiming ground nobody walked"
    );
    assert!(
        !completion_retry::is_waiting(drive.volume_id),
        "a completed drive is owed nothing, and the next ladder starts at a minute"
    );
    assert_eq!(
        drive.scans_started(),
        2,
        "one machine for the first index and one for the retry, ❌ never a third running alongside"
    );
}

/// A retry that lands while the machine is still working runs NOTHING and comes
/// back later.
///
/// ⚠️ The failure this rules out is the one the whole subsystem is built against:
/// two machines walking one volume allocate different ids for the same names, and
/// `INSERT OR IGNORE` against `UNIQUE (parent_id, name_folded)` makes the loser
/// lose its whole subtree. A happy-path test would never see it, so the retry is
/// fired from inside the run, at a moment a walk is provably in flight.
#[test]
fn a_retry_that_lands_mid_run_never_starts_a_second_machine() {
    let recorder = Arc::new(RecordingSink::new());
    let asker = Arc::new(AsksForARetryMidRun::new("phased-retry-mid-run", Arc::clone(&recorder)));
    let drive = Drive::assembled(
        "phased-retry-mid-run",
        |root| {
            for name in ["a", "b", "c"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        |_, _| {},
        &["a"],
        true,
        Arc::clone(&asker) as Arc<dyn EventSink>,
        recorder,
    );

    drive.start();
    drive.wait_for_the_machine();

    assert!(
        asker.asked(),
        "precondition: nothing asked for a retry while the machine was walking"
    );
    assert!(
        asker.still_waiting_after_the_ask(),
        "the retry has to RESCHEDULE against a working machine, ❌ never spend its turn and go quiet"
    );
    assert_eq!(
        drive.scans_started(),
        1,
        "❌ a second machine started on a volume one was already walking"
    );
}

// ── The fixture ──────────────────────────────────────────────────────

/// A drive with a folder appearing under its root after every walk, which is what
/// keeps its frontier non-empty however many passes the machine spends.
fn churning_drive(volume_id: &'static str) -> (Drive, Arc<CreatesAFolderAfterEveryWalk>) {
    let recorder = Arc::new(RecordingSink::new());
    let churn = Arc::new(CreatesAFolderAfterEveryWalk::new(volume_id, Arc::clone(&recorder)));
    let drive = Drive::assembled(
        volume_id,
        |root| {
            for name in ["a", "b"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        |_, _| {},
        &["a"],
        true,
        Arc::clone(&churn) as Arc<dyn EventSink>,
        recorder,
    );
    churn.write_into(drive.tree.path());
    (drive, churn)
}

/// The live half of a drive somebody is writing to: after every walk it creates a
/// folder on ground the index already covers, and writes the row the live
/// reconciler would write for it.
///
/// ⚠️ **The row is the point, not the folder.** A folder alone changes nothing
/// until something notices it; what makes it frontier is a row with
/// `listed_epoch = 0`, which is what the reconciler writes for a created directory
/// because nothing has listed it. Doing it from the event sink rather than through
/// the real watcher is what makes this deterministic: FSEvents coalesces on its own
/// latency, so over a tree this small a real watcher delivers nothing before the
/// machine stops (`churn_bench` needs 300,000 directories for exactly that reason).
struct CreatesAFolderAfterEveryWalk {
    volume_id: &'static str,
    recorder: Arc<RecordingSink>,
    /// The tree, handed over once the fixture that owns it exists.
    tree: std::sync::OnceLock<PathBuf>,
    /// How many folders have appeared, which is also the next one's name.
    created: AtomicU64,
    /// Whether the writing is still going.
    churning: AtomicBool,
}

impl CreatesAFolderAfterEveryWalk {
    fn new(volume_id: &'static str, recorder: Arc<RecordingSink>) -> Self {
        Self {
            volume_id,
            recorder,
            tree: std::sync::OnceLock::new(),
            created: AtomicU64::new(0),
            churning: AtomicBool::new(true),
        }
    }

    /// Point it at the tree the fixture built. ⚠️ Before the drive starts, or the
    /// first walk finds nothing to write into.
    fn write_into(&self, tree: &Path) {
        self.tree.set(tree.to_path_buf()).expect("one tree per fixture");
    }

    /// The writing stops, the way a build eventually ends.
    fn stop(&self) {
        self.churning.store(false, Ordering::Relaxed);
    }

    fn folders_created(&self) -> u64 {
        self.created.load(Ordering::Relaxed)
    }

    /// One folder on covered ground, plus the row that makes it frontier.
    fn create_one(&self) {
        let Some(tree) = self.tree.get() else {
            return;
        };
        let name = format!("churn-{}", self.created.fetch_add(1, Ordering::Relaxed));
        std::fs::create_dir_all(tree.join(&name)).expect("the folder somebody just created");
        let Some((writer, _)) = state::get_writer_and_scanning_for(self.volume_id) else {
            return;
        };
        // The volume root, which the stitch listed on its way to the first phase
        // root, so a folder under it lands on ground the index already claims.
        let Some(parent_id) = id_of(&writer, tree, &tree.to_string_lossy()) else {
            return;
        };
        let id = writer.next_id().fetch_add(1, Ordering::Relaxed);
        writer
            .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
                id,
                parent_id,
                name,
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            }]))
            .expect("the writer takes the new folder's row");
    }
}

impl EventSink for CreatesAFolderAfterEveryWalk {
    fn emit(&self, event: IndexEvent) {
        let mine = matches!(&event, IndexEvent::CoverageBranchEnded { volume_id, .. } if volume_id == self.volume_id);
        self.recorder.emit(event);
        if mine && self.churning.load(Ordering::Relaxed) {
            self.create_one();
        }
    }
}

/// Asks for a retry from inside the run, while a walk is provably reading the
/// disk, and records what the answer left behind.
struct AsksForARetryMidRun {
    volume_id: &'static str,
    recorder: Arc<RecordingSink>,
    asked: AtomicBool,
    still_waiting: AtomicBool,
}

impl AsksForARetryMidRun {
    fn new(volume_id: &'static str, recorder: Arc<RecordingSink>) -> Self {
        completion_retry::forget(volume_id);
        Self {
            volume_id,
            recorder,
            asked: AtomicBool::new(false),
            still_waiting: AtomicBool::new(false),
        }
    }

    fn asked(&self) -> bool {
        self.asked.load(Ordering::Relaxed)
    }

    /// Whether the volume was still owed an attempt right after the refusal, which
    /// is what "rescheduled" looks like from outside.
    fn still_waiting_after_the_ask(&self) -> bool {
        self.still_waiting.load(Ordering::Relaxed)
    }
}

impl EventSink for AsksForARetryMidRun {
    fn emit(&self, event: IndexEvent) {
        let walking =
            matches!(&event, IndexEvent::CoverageBranchStarted { volume_id, .. } if volume_id == self.volume_id);
        self.recorder.emit(event);
        if !walking || self.asked.swap(true, Ordering::Relaxed) {
            return;
        }
        // A window opened a while ago, and a tick that finds it due — the retry
        // arriving at the worst possible moment, on purpose.
        completion_retry::arm(self.volume_id, 0);
        completion_retry::nudge_at(self.volume_id, 10_000);
        self.still_waiting
            .store(completion_retry::is_waiting(self.volume_id), Ordering::Relaxed);
    }
}

/// The index id for an absolute path on this drive, the way `Drive::id_of` does it
/// — off the writer's own database, since the sink has no fixture to ask.
fn id_of(writer: &IndexWriter, tree: &Path, path: &str) -> Option<i64> {
    let conn = IndexStore::open_read_connection(&writer.db_path()).ok()?;
    let space = IndexPathSpace::mount_rooted(tree.join("").to_string_lossy().into_owned());
    let relative = space.index_relative(path)?;
    crate::indexing::store::resolve_path(&conn, &relative).ok().flatten()
}
