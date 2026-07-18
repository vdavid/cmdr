//! The coalescing coordinator (pure, testable — a `PassCoordinator` clone): one pass
//! per volume at a time, folding concurrent requests into a single re-run. Lives apart
//! from the `MediaScheduler` state machine so the "sweep + concurrent ScanCompleted ⇒
//! one pass" contract is unit-testable (see `coalescing_tests`) without an app or a
//! runtime.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::ignore_poison::IgnorePoison;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PassSlot {
    running: bool,
    rerun_requested: bool,
}

/// The outcome of requesting a pass for a volume.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BeginOutcome {
    /// No pass was running; the caller should start one now.
    Start,
    /// A pass is already running; the request set the re-run flag (coalesced).
    Coalesced,
}

/// The outcome of finishing a pass for a volume.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FinishOutcome {
    /// No re-run was requested; the volume is now idle.
    Done,
    /// A re-run was requested during the pass; the caller should run once more.
    RunAgain,
}

/// The coalescing core: one pass per volume at a time, folding concurrent requests
/// into a single re-run. Pure and lock-guarded, so the "sweep + concurrent
/// ScanCompleted ⇒ one pass" contract is unit-testable without an app or a runtime.
#[derive(Default)]
pub(crate) struct PassCoordinator {
    slots: Mutex<HashMap<String, PassSlot>>,
}

impl PassCoordinator {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Request a pass for `volume_id`. Returns [`BeginOutcome::Start`] exactly when
    /// the caller should begin a pass; a request arriving while a pass runs returns
    /// [`BeginOutcome::Coalesced`] and sets the re-run flag.
    pub(crate) fn request(&self, volume_id: &str) -> BeginOutcome {
        let mut slots = self.slots.lock_ignore_poison();
        let slot = slots.entry(volume_id.to_string()).or_default();
        if slot.running {
            slot.rerun_requested = true;
            BeginOutcome::Coalesced
        } else {
            slot.running = true;
            slot.rerun_requested = false;
            BeginOutcome::Start
        }
    }

    /// Whether a pass is currently running for `volume_id`. Drives the honest
    /// "still indexing images…" coverage state the search UI shows (a snapshot, not
    /// a subscription — the minimal per-volume enrichment signal).
    pub(crate) fn is_running(&self, volume_id: &str) -> bool {
        self.slots
            .lock_ignore_poison()
            .get(volume_id)
            .is_some_and(|slot| slot.running)
    }

    /// Finish the running pass for `volume_id`. Returns [`FinishOutcome::RunAgain`]
    /// (keeping the slot running) if a re-run was requested, else
    /// [`FinishOutcome::Done`].
    pub(crate) fn finish(&self, volume_id: &str) -> FinishOutcome {
        let mut slots = self.slots.lock_ignore_poison();
        let slot = slots.entry(volume_id.to_string()).or_default();
        if slot.rerun_requested {
            slot.rerun_requested = false;
            FinishOutcome::RunAgain
        } else {
            slot.running = false;
            FinishOutcome::Done
        }
    }
}
