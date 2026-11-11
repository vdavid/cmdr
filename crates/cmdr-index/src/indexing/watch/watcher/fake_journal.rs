//! Tests that drive a whole volume through the handle put its root on a fake
//! journal: no stream, synthetic event IDs, and a SCRIPTED journal of the changes
//! the test says it made.
//!
//! ⚠️ **Why it exists**: every real stream start and every
//! [`current_event_id`](super::current_event_id) is a round trip to `fseventsd`,
//! ONE daemon for the whole machine. With other processes' disk churn pegging it,
//! each call took 0.7–3.7 s, so the phase-machine tests (36 processes, a few calls
//! each) blew the 8 s cap at full parallelism while each took 0.13 s alone
//! (verified on macOS 27, `sample` + timing logs, 2026-10-07, issue #374). No lock
//! or group in the test runner helps: the contention is with every process on the
//! machine. A cell waiting on a real REPLAY had it worse: 30 s wasn't always
//! enough (#418).
//!
//! **The scripted journal.** A test that writes a file and calls [`record_created_file`]
//! gets what FSEvents would give it: every running fake watcher whose root holds
//! the path hears it at once, and a watcher started later with a `since_when` below
//! its id replays it, then a `HistoryDone`. That's enough to drive the index's own
//! replay, restart, and move logic deterministically. What it can't prove is the
//! OS half (that FSEvents really replays a path from an old id): the cells pinning
//! that run on the real journal (`lifecycle/cover/DETAILS.md` § "What the cold-drive
//! cells pin about FSEvents").
//!
//! ❌ An unrecorded change is invisible here, as a dropped event would be.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use tokio::sync::mpsc;

use super::{FsChangeEvent, FsEventFlags, WatcherBackend};

/// Scoped by root, so a real-stream test sharing the process under plain
/// `cargo test` keeps its real journal.
static ROOTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Far from zero, which reads as "no stored ID" to every caller.
static NEXT_EVENT_ID: AtomicU64 = AtomicU64::new(1_000_000);

/// Fake the journal for `root` and everything under it until the guard drops.
pub(crate) fn fake_for(root: &Path) -> Guard {
    ROOTS.lock_ignore_poison().push(root.to_path_buf());
    Guard(root.to_path_buf())
}

pub(super) fn covers(path: &Path) -> bool {
    ROOTS.lock_ignore_poison().iter().any(|root| path.starts_with(root))
}

pub(super) fn next_event_id() -> u64 {
    NEXT_EVENT_ID.fetch_add(1, Ordering::Relaxed)
}

/// A running watcher with no stream behind it. Its task holds the sender until
/// `stop`, so the loop reading the other end waits the way it waits on a quiet
/// drive, ❌ never reads a closed channel as a watcher that died.
///
/// With a `since_when`, it first replays the recorded changes under `root` newer
/// than that id, then a `HistoryDone`, as a real stream does. Parks first while a
/// [`StreamGate`] holds `root`, the way a real start waits on a busy `fseventsd`.
pub(super) fn watcher(
    root: &Path,
    since_when: u64,
    event_sender: mpsc::UnboundedSender<FsChangeEvent>,
) -> FakeJournalBackend {
    let gate = GATES
        .lock_ignore_poison()
        .iter()
        .find(|(gated, _)| root.starts_with(gated))
        .map(|(_, gate)| Arc::clone(gate));
    if let Some(gate) = gate {
        gate.park();
    }
    let running = Arc::new(AtomicBool::new(true));
    let last_event_id = Arc::new(AtomicU64::new(0));
    // The journal delivers into a channel of the watcher's own, and only its task
    // holds the loop's sender, so `stop` closes the loop's channel the moment it
    // aborts the task, as a real watcher's does.
    let (journal_tx, mut journal_rx) = mpsc::unbounded_channel::<FsChangeEvent>();
    {
        // Replay and joining the live list happen under ONE lock, so a change
        // recorded meanwhile arrives exactly once, and after the `HistoryDone`.
        let mut journal = JOURNAL.lock_ignore_poison();
        if since_when != 0 {
            for event in journal
                .recorded
                .iter()
                .filter(|event| event.event_id > since_when && Path::new(&event.path).starts_with(root))
            {
                let _ = journal_tx.send(event.clone());
                last_event_id.store(event.event_id, Ordering::Relaxed);
            }
            let _ = journal_tx.send(FsChangeEvent {
                path: root.to_string_lossy().into_owned(),
                event_id: next_event_id(),
                flags: FsEventFlags {
                    history_done: true,
                    ..FsEventFlags::default()
                },
            });
        }
        journal.listening.push(Listener {
            root: root.to_path_buf(),
            sender: journal_tx,
            running: Arc::clone(&running),
            last_event_id: Arc::clone(&last_event_id),
        });
    }
    let forward_task = crate::indexing::host::runtime::spawn(async move {
        while let Some(event) = journal_rx.recv().await {
            let _ = event_sender.send(event);
        }
        // The journal holds its end for good, so this is never reached; the
        // sender stays held until `stop` aborts the task.
        std::future::pending::<()>().await;
    });
    FakeJournalBackend {
        running,
        last_event_id,
        overflow: Arc::new(AtomicBool::new(false)),
        forward_task: Some(forward_task),
    }
}

/// The third watcher backend: a fake journal's listener, stopped like a real stream.
pub(crate) struct FakeJournalBackend {
    running: Arc<AtomicBool>,
    last_event_id: Arc<AtomicU64>,
    overflow: Arc<AtomicBool>,
    forward_task: Option<tokio::task::JoinHandle<()>>,
}

impl WatcherBackend for FakeJournalBackend {
    /// One journal covers the whole root, as an FSEvents stream does.
    fn watch_branch(&self, _branch: &Path) {}

    fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(task) = self.forward_task.take() {
            task.abort();
        }
    }

    fn last_event_id(&self) -> u64 {
        self.last_event_id.load(Ordering::Relaxed)
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    fn overflow_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.overflow)
    }
}

impl Drop for FakeJournalBackend {
    fn drop(&mut self) {
        if self.running.load(Ordering::Relaxed) {
            self.stop();
        }
    }
}

/// Every change a test recorded, and the fake watchers listening for the next one.
static JOURNAL: Mutex<Journal> = Mutex::new(Journal {
    recorded: Vec::new(),
    listening: Vec::new(),
});

struct Journal {
    /// Oldest first, which is id order.
    recorded: Vec<FsChangeEvent>,
    listening: Vec<Listener>,
}

/// A running fake watcher, as the journal delivers to it.
struct Listener {
    root: PathBuf,
    sender: mpsc::UnboundedSender<FsChangeEvent>,
    /// The watcher's own flag: `stop` clears it, and the journal stops delivering.
    running: Arc<AtomicBool>,
    last_event_id: Arc<AtomicU64>,
}

/// Say that the test just created the file at `path`: every fake watcher whose
/// root holds it hears it now, and a later start replays it from an older id.
pub(crate) fn record_created_file(path: &Path) {
    let event = FsChangeEvent {
        path: path.to_string_lossy().into_owned(),
        event_id: next_event_id(),
        flags: FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..FsEventFlags::default()
        },
    };
    let mut journal = JOURNAL.lock_ignore_poison();
    journal
        .listening
        .retain(|listener| listener.running.load(Ordering::Relaxed) && !listener.sender.is_closed());
    for listener in journal
        .listening
        .iter()
        .filter(|listener| path.starts_with(&listener.root))
    {
        let _ = listener.sender.send(event.clone());
        listener.last_event_id.store(event.event_id, Ordering::Relaxed);
    }
    journal.recorded.push(event);
}

pub(crate) struct Guard(PathBuf);

impl Drop for Guard {
    fn drop(&mut self) {
        let mut roots = ROOTS.lock_ignore_poison();
        if let Some(index) = roots.iter().position(|root| *root == self.0) {
            roots.remove(index);
        }
    }
}

/// Stream starts held at a gate, by root.
static GATES: Mutex<Vec<(PathBuf, Arc<GateState>)>> = Mutex::new(Vec::new());

/// Long enough for any machine on any load; it only ever fires on a broken test.
const DEADLOCK_GUARD: Duration = Duration::from_secs(60);

#[derive(Default)]
struct GateState {
    /// `(parked, open)`.
    stage: Mutex<(bool, bool)>,
    moved: Condvar,
}

impl GateState {
    fn park(&self) {
        let mut stage = self.stage.lock_ignore_poison();
        stage.0 = true;
        self.moved.notify_all();
        let (stage, _) = self
            .moved
            .wait_timeout_while(stage, DEADLOCK_GUARD, |(_, open)| !*open)
            .unwrap_or_else(PoisonError::into_inner);
        drop(stage);
    }

    fn open(&self) {
        self.stage.lock_ignore_poison().1 = true;
        self.moved.notify_all();
    }
}

/// Hold every fake stream start under `root` until [`StreamGate::open`], so a test
/// can pin what callers do while one is in flight: the `fseventsd` round trip a
/// real start makes took up to 5.8 s under load (2026-10-06, live app). Opens on
/// drop, so a failing test never leaves a start parked.
pub(crate) fn park_stream_starts(root: &Path) -> StreamGate {
    let state = Arc::new(GateState::default());
    GATES
        .lock_ignore_poison()
        .push((root.to_path_buf(), Arc::clone(&state)));
    StreamGate(state)
}

/// See [`park_stream_starts`].
pub(crate) struct StreamGate(Arc<GateState>);

impl StreamGate {
    /// Block until a stream start is parked at the gate.
    pub(crate) fn wait_until_parked(&self) {
        let stage = self.0.stage.lock_ignore_poison();
        let (stage, timeout) = self
            .0
            .moved
            .wait_timeout_while(stage, DEADLOCK_GUARD, |(parked, _)| !*parked)
            .unwrap_or_else(PoisonError::into_inner);
        drop(stage);
        assert!(!timeout.timed_out(), "no stream start ever reached the gate");
    }

    /// Let every parked start, and every later one, through.
    pub(crate) fn open(&self) {
        self.0.open();
    }
}

impl Drop for StreamGate {
    fn drop(&mut self) {
        self.0.open();
        GATES
            .lock_ignore_poison()
            .retain(|(_, state)| !Arc::ptr_eq(state, &self.0));
    }
}
