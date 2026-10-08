//! Tests that drive a whole volume through the handle, and never wait on a
//! delivery, put its root on a fake journal: no stream, and synthetic event IDs.
//!
//! ⚠️ **Why it exists**: every real stream start and every
//! [`current_event_id`](super::current_event_id) is a round trip to `fseventsd`,
//! ONE daemon for the whole machine. With other processes' disk churn pegging it,
//! each call took 0.7–3.7 s, so the phase-machine tests (36 processes, a few calls
//! each) blew the 8 s cap at full parallelism while each took 0.13 s alone
//! (verified on macOS 27, `sample` + timing logs, 2026-10-07, issue #374). No lock
//! or group in the test runner helps: the contention is with every process on the
//! machine.
//!
//! ❌ Not for a test that asserts on what a watcher DELIVERS: that one needs the
//! real stream (`.config/nextest.toml`'s `real-notify` group).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use tokio::sync::mpsc;

use super::{DriveWatcher, FsChangeEvent};

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
/// Parks first while a [`StreamGate`] holds `root`, the way a real start waits on
/// a busy `fseventsd`.
pub(super) fn watcher(root: &Path, event_sender: mpsc::UnboundedSender<FsChangeEvent>) -> DriveWatcher {
    let gate = GATES
        .lock_ignore_poison()
        .iter()
        .find(|(gated, _)| root.starts_with(gated))
        .map(|(_, gate)| Arc::clone(gate));
    if let Some(gate) = gate {
        gate.park();
    }
    let forward_task = crate::indexing::host::runtime::spawn(async move {
        let _held = event_sender;
        std::future::pending::<()>().await;
    });
    DriveWatcher {
        running: Arc::new(AtomicBool::new(true)),
        last_event_id: Arc::new(AtomicU64::new(0)),
        overflow: Arc::new(AtomicBool::new(false)),
        handler: None,
        forward_task: Some(forward_task),
    }
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
