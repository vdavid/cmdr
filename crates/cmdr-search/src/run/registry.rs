//! The runs in flight: who asked, what supersedes what, and the tokens that stop
//! them. A value the host holds ([`SearchRuns`]), ❌ never a static here.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use tokio_util::sync::CancellationToken;

/// Who asked, which is what decides a new run's effect on the ones already in
/// flight.
///
/// ❌ Not a policy switch: both origins take the same path, walk the same
/// ground, and get the same answer (`docs/specs/unindexed-search-plan.md`
/// Decision 10). What differs is only who else is in the room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOrigin {
    /// The search dialog. One dialog asks one question at a time, so a new run
    /// supersedes its previous one, and closing the dialog stops what it left
    /// behind.
    Dialog {
        /// Where this run stands among the dialog's runs: a larger `order` is a
        /// question asked later. The frontend mints it when the run starts, so it
        /// holds the order the user asked in, whatever order the starts reach here.
        order: u64,
    },
    /// An agent's call (MCP). Its own asker with its own caller waiting, so it
    /// neither supersedes nor is superseded, and a dialog closing is none of its
    /// business. Only the app quitting stops it.
    Agent,
}

/// One live search run, from the query that started it to its terminal fact.
pub struct LiveRun {
    /// What every fact this run reports is stamped with.
    pub run_id: String,
    /// The one volume it covers.
    pub volume_id: String,
    /// Who asked for it.
    origin: RunOrigin,
    /// Stops the walk AND the run. It's the token the host hands its walk, so
    /// cancelling reaches the walk wherever it is rather than at the next batch
    /// boundary.
    cancel: CancellationToken,
    /// A newer run started. This one stops reporting; its walk carries on.
    superseded: AtomicBool,
    /// The run has reached its terminal fact. Set BEFORE that fact goes out, so a
    /// caller that installs its listeners and then sees [`is_live`](Self::is_live)
    /// answer `true` is guaranteed to hear the ending.
    ended: AtomicBool,
}

impl LiveRun {
    fn new(run_id: &str, volume_id: &str, origin: RunOrigin) -> Self {
        Self {
            run_id: run_id.to_string(),
            volume_id: volume_id.to_string(),
            origin,
            cancel: CancellationToken::new(),
            superseded: AtomicBool::new(false),
            ended: AtomicBool::new(false),
        }
    }

    /// Whether results from this run are still wanted. A cancelled run stops
    /// emitting rows too: its terminal fact is what it has left to say.
    pub fn wants_results(&self) -> bool {
        !self.superseded.load(Ordering::Relaxed) && !self.cancel.is_cancelled()
    }

    /// Whether this run is allowed to report anything at all. A superseded run
    /// isn't: the frontend has moved on, and a terminal event naming a run it
    /// dropped would be noise at best.
    pub fn wants_events(&self) -> bool {
        !self.superseded.load(Ordering::Relaxed)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Whether this run's ending is still to come AND will be reported: not
    /// superseded (a superseded run says nothing, terminal fact included) and not
    /// ended. What "Open in pane" asks before it waits on a run's ending
    /// (`apps/desktop/src/lib/search/walk-handoff.svelte.ts`): a `true` here, read
    /// after the listeners are in, means the terminal event can't be missed.
    pub fn is_live(&self) -> bool {
        self.wants_events() && !self.ended.load(Ordering::SeqCst)
    }

    /// The run is about to report its terminal fact (or would, if anyone were
    /// listening). [`ResultStream`](super::ResultStream)'s two endings call it first.
    pub(super) fn mark_ended(&self) {
        self.ended.store(true, Ordering::SeqCst);
    }

    /// The token to hand the walk.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub(super) fn supersede(&self) {
        self.superseded.store(true, Ordering::Relaxed);
    }

    /// A dialog run outside any registry, so a test never supersedes another's.
    #[cfg(any(test, feature = "testing"))]
    pub fn detached(run_id: &str, volume_id: &str) -> Arc<Self> {
        Arc::new(Self::new(run_id, volume_id, RunOrigin::Dialog { order: 0 }))
    }
}

/// Every run that hasn't reached its terminal state, keyed by run id.
///
/// The host holds ONE per process (desktop: a static in `search/live.rs`), so
/// the dialog closing or the app quitting reaches every run from wherever it
/// notices.
#[derive(Default)]
pub struct SearchRuns {
    runs: Mutex<HashMap<String, Arc<LiveRun>>>,
}

impl SearchRuns {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a run, superseding the ones it speaks over.
    ///
    /// The dialog asks one question at a time, so an EARLIER dialog run's results
    /// are by definition for a query the user has moved on from. ❌ Superseding does
    /// not cancel: their walks keep filling the index, which is what makes the
    /// refined query cheaper than the first one.
    ///
    /// ❗ Earlier means a smaller [`RunOrigin::Dialog`] `order`, ❌ never "registered
    /// first". Starts reach this in whatever order the IPC runtime runs them, so a run
    /// that arrives after a later question has registered starts out superseded,
    /// rather than silencing the run the dialog is watching.
    ///
    /// ❌ It reaches no [`RunOrigin::Agent`] run, in either direction: an agent's
    /// call has its own caller waiting on its own answer, and a person typing has no
    /// business emptying it (nor the other way round).
    pub fn register(&self, run_id: &str, volume_id: &str, origin: RunOrigin) -> Arc<LiveRun> {
        let run = Arc::new(LiveRun::new(run_id, volume_id, origin));
        let mut runs = self.runs.lock_ignore_poison();
        if let RunOrigin::Dialog { order } = origin {
            for other in runs.values() {
                let RunOrigin::Dialog { order: other_order } = other.origin else {
                    continue;
                };
                if other_order <= order {
                    other.supersede();
                } else {
                    run.supersede();
                }
            }
        }
        runs.insert(run_id.to_string(), Arc::clone(&run));
        run
    }

    /// Forget a run that has reached its terminal state.
    pub fn deregister(&self, run_id: &str) {
        self.runs.lock_ignore_poison().remove(run_id);
    }

    /// Whether the run named is registered and still [`LiveRun::is_live`]. An
    /// unknown id (never registered, or already deregistered) is `false`.
    pub fn is_live(&self, run_id: &str) -> bool {
        self.runs
            .lock_ignore_poison()
            .get(run_id)
            .is_some_and(|run| run.is_live())
    }

    /// Stop one run and its walk. Returns whether there was one to stop.
    pub fn cancel(&self, run_id: &str) -> bool {
        let run = self.runs.lock_ignore_poison().get(run_id).cloned();
        match run {
            Some(run) => {
                run.cancel.cancel();
                true
            }
            None => false,
        }
    }

    /// Stop every run and every walk behind them.
    ///
    /// The app quitting calls this: nothing may leave a half-walked scope looking
    /// complete. It can't (a directory is marked listed only once its rows are
    /// written), but a walk still reading a disk nobody is waiting on is exactly the
    /// resource waste the app promises not to be.
    pub fn cancel_all(&self) {
        for run in self.runs.lock_ignore_poison().values() {
            run.cancel.cancel();
        }
    }

    /// Stop every DIALOG run except the one named, if any.
    ///
    /// The dialog closing calls this with the run it deliberately outlived: "Open in
    /// pane" promotes the results into a pane and leaves the walk filling it (the
    /// handoff, `apps/desktop/src/lib/search/walk-handoff.svelte.ts`), so that ONE
    /// run has a consumer even though the dialog doesn't. Every other run of the
    /// dialog's is a query nobody is reading.
    ///
    /// ❌ An agent's run is untouched: closing the dialog says nothing about an
    /// agent's call still waiting for its answer.
    pub fn cancel_dialog_runs_except(&self, keep_run_id: Option<&str>) {
        for run in self.runs.lock_ignore_poison().values() {
            if !matches!(run.origin, RunOrigin::Dialog { .. }) || keep_run_id.is_some_and(|keep| keep == run.run_id) {
                continue;
            }
            run.cancel.cancel();
        }
    }
}
