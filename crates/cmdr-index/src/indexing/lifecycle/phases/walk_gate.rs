//! Parks the machine at the one moment a teardown can slip past it: it has
//! decided to walk (`may_run` said yes) and hasn't resolved the walk's context
//! yet. Test builds only.
//!
//! The driver runs on its own thread and nothing joins it, so a stop that lands
//! in that gap is a one-in-hundreds race under stress (issue #375). A gate makes
//! it the only interleaving there is.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;

/// Gates waiting for their volume's next walk. One-shot: the first arrival takes it.
static ARMED: Mutex<Vec<(String, Arc<WalkGate>)>> = Mutex::new(Vec::new());

/// Long enough for any machine on any load; it only ever fires on a broken test.
const DEADLOCK_GUARD: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Stage {
    Armed,
    Parked,
    Released,
    Resolved,
}

/// Where one gated walk is.
pub(crate) struct WalkGate {
    stage: Mutex<Stage>,
    moved: Condvar,
}

impl WalkGate {
    fn advance_to(&self, stage: Stage) {
        let mut current = self.stage.lock_ignore_poison();
        if *current < stage {
            *current = stage;
        }
        self.moved.notify_all();
    }

    fn wait_for(&self, stage: Stage) {
        let current = self.stage.lock_ignore_poison();
        let (current, timeout) = self
            .moved
            .wait_timeout_while(current, DEADLOCK_GUARD, |current| *current < stage)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(
            !timeout.timed_out(),
            "the gated walk never reached {stage:?} (it's at {:?})",
            *current
        );
    }

    /// Block until the machine is parked in front of its walk.
    pub(crate) fn wait_until_parked(&self) {
        self.wait_for(Stage::Parked);
    }

    /// Let the parked walk go on and resolve its context.
    pub(crate) fn release(&self) {
        self.advance_to(Stage::Released);
    }

    /// Block until the released walk has resolved its context, whatever that
    /// resolution did.
    pub(crate) fn wait_until_resolved(&self) {
        self.wait_for(Stage::Resolved);
    }
}

/// Park `volume_id`'s next walk in front of its context.
pub(crate) fn arm(volume_id: &str) -> Arc<WalkGate> {
    let gate = Arc::new(WalkGate {
        stage: Mutex::new(Stage::Armed),
        moved: Condvar::new(),
    });
    ARMED
        .lock_ignore_poison()
        .push((volume_id.to_string(), Arc::clone(&gate)));
    gate
}

/// A walk that met a gate. Dropping it says the context is resolved.
pub(super) struct Arrival(Arc<WalkGate>);

impl Drop for Arrival {
    fn drop(&mut self) {
        self.0.advance_to(Stage::Resolved);
    }
}

/// Called by the driver just before it resolves a walk's context: parks there if
/// a gate is armed for the volume, until the test releases it.
pub(super) fn arrive(volume_id: &str) -> Option<Arrival> {
    let gate = {
        let mut armed = ARMED.lock_ignore_poison();
        let index = armed.iter().position(|(id, _)| id == volume_id)?;
        armed.remove(index).1
    };
    gate.advance_to(Stage::Parked);
    gate.wait_for(Stage::Released);
    Some(Arrival(gate))
}
