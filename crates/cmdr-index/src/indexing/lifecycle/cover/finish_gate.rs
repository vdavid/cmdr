//! Parks a walk thread between its last write and its release: the rows it wrote
//! are on their way to the database, and the branch set hasn't heard the walk
//! ended. Test builds only.
//!
//! A stop that lands in that gap meets rows on disk for ground the walk still
//! holds, and nothing joins the walk thread, so the gap is a one-in-dozens race
//! under stress (#582). A gate makes it the only interleaving there is.

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;

/// Gates waiting for their volume's next walk to end. One-shot: the first arrival
/// takes it.
static ARMED: Mutex<Vec<(String, Arc<GateState>)>> = Mutex::new(Vec::new());

/// Long enough for any machine on any load; it only ever fires on a broken test.
const DEADLOCK_GUARD: Duration = Duration::from_secs(60);

#[derive(Default)]
struct GateState {
    /// `(parked, released)`.
    stage: Mutex<(bool, bool)>,
    moved: Condvar,
}

impl GateState {
    fn release(&self) {
        self.stage.lock_ignore_poison().1 = true;
        self.moved.notify_all();
    }
}

/// Park `volume_id`'s next walk once it has walked, before it releases.
pub(in crate::indexing) fn arm(volume_id: &str) -> FinishGate {
    let state = Arc::new(GateState::default());
    ARMED
        .lock_ignore_poison()
        .push((volume_id.to_string(), Arc::clone(&state)));
    FinishGate(state)
}

/// See [`arm`]. Releases on drop, so a failing test never leaves a walk parked.
pub(in crate::indexing) struct FinishGate(Arc<GateState>);

impl FinishGate {
    /// Block until the walk is parked in front of its release.
    pub(in crate::indexing) fn wait_until_parked(&self) {
        let stage = self.0.stage.lock_ignore_poison();
        let (stage, timeout) = self
            .0
            .moved
            .wait_timeout_while(stage, DEADLOCK_GUARD, |(parked, _)| !*parked)
            .unwrap_or_else(PoisonError::into_inner);
        drop(stage);
        assert!(!timeout.timed_out(), "no walk ever reached the gate");
    }

    /// Let the parked walk release what it holds.
    pub(in crate::indexing) fn release(&self) {
        self.0.release();
    }
}

impl Drop for FinishGate {
    fn drop(&mut self) {
        self.0.release();
        ARMED
            .lock_ignore_poison()
            .retain(|(_, state)| !Arc::ptr_eq(state, &self.0));
    }
}

/// Called on the walk thread between the walk and its release: parks there if a
/// gate is armed for the volume, until the test releases it.
pub(super) fn arrive(volume_id: &str) {
    let state = {
        let mut armed = ARMED.lock_ignore_poison();
        let Some(index) = armed.iter().position(|(id, _)| id == volume_id) else {
            return;
        };
        armed.remove(index).1
    };
    let mut stage = state.stage.lock_ignore_poison();
    stage.0 = true;
    state.moved.notify_all();
    let (stage, _) = state
        .moved
        .wait_timeout_while(stage, DEADLOCK_GUARD, |(_, released)| !*released)
        .unwrap_or_else(PoisonError::into_inner);
    drop(stage);
}
