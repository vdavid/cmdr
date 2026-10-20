//! The live transfers' table and how an operation joins it: [`Probes`], one
//! per engine, and [`register_operation`], whose guard leaves it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::super::super::event_sinks::OperationEventSink;
use super::super::super::state::WriteOperationState;
use super::super::super::types::{TransferActivity, WriteProgressEvent};
use super::{
    DriverDetail, DriverPhase, NO_INBOUND_RATE, OperationProbe, TransferEnds, spawn_watchdog, stall_abort_after,
};

/// One engine's live transfers, so a watchdog tick (and any future debug
/// command) can see every transfer at once. Reached through each operation's
/// scope.
///
/// A probe holds its operation's state, whose scope holds this table, so a
/// registered probe and the table keep each other alive. The guard that
/// [`register_operation`] returns breaks that on drop, as every transfer ends.
pub(crate) struct Probes {
    live: Mutex<HashMap<String, Arc<OperationProbe>>>,
}

impl Probes {
    /// No transfer is running.
    pub(crate) fn new() -> Self {
        Self {
            live: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn insert(&self, operation_id: &str, probe: Arc<OperationProbe>) {
        self.live.lock_ignore_poison().insert(operation_id.to_owned(), probe);
    }

    pub(super) fn remove(&self, operation_id: &str) {
        self.live.lock_ignore_poison().remove(operation_id);
    }

    pub(super) fn get(&self, operation_id: &str) -> Option<Arc<OperationProbe>> {
        self.live.lock_ignore_poison().get(operation_id).cloned()
    }

    /// The live activity for an operation, if it keeps an in-flight table.
    ///
    /// `None` for operations with no probe (local copy, delete, trash, and the
    /// pre-registration window), where the UI simply shows nothing extra.
    /// Called from `WriteOperationState::enrich_progress`, so every progress
    /// event from every emit site carries it without a single caller having to
    /// remember.
    ///
    /// `sending` is the progress event this activity will ride, when there is
    /// one: an event that moved the counters reads as moving, whatever the
    /// watchdog saw on its last tick ([`OperationProbe::activity_for_event`]).
    /// `None` for a snapshot with no event behind it (`cmdr://state`).
    pub(crate) fn activity_for(
        &self,
        operation_id: &str,
        sending: Option<&WriteProgressEvent>,
    ) -> Option<TransferActivity> {
        let probe = self.get(operation_id)?;
        Some(match sending {
            Some(event) => probe.activity_for_event(event),
            None => probe.activity(),
        })
    }
}

/// Register an operation on its engine's probes and start its stall watchdog.
///
/// The returned guard deregisters on drop, which also stops the watchdog on its
/// next tick.
pub(in crate::transfer) fn register_operation(
    operation_id: &str,
    concurrency: usize,
    total_files: usize,
    // Source and destination, held only so the watchdog can ask their
    // connections whether either is proven dead before it acts on a stall, and
    // how fast the source is receiving while nothing lands.
    ends: TransferEnds,
    state: Arc<WriteOperationState>,
    sink: Arc<dyn OperationEventSink>,
) -> OperationProbeGuard {
    let probes = Arc::clone(state.scope().stores().probes());
    let runtime = state.scope().host().runtime();
    let probe = Arc::new(OperationProbe {
        operation_id: operation_id.to_owned(),
        concurrency,
        total_files,
        driver_phase: AtomicU8::new(DriverPhase::Starting as u8),
        driver_detail: Mutex::new(DriverDetail::Label("")),
        tasks: Mutex::new(Vec::new()),
        sink: Mutex::new(Some(sink)),
        still_for_seconds: AtomicU64::new(0),
        stall_abort_after: stall_abort_after(),
        ends,
        source_inbound_rate: AtomicU64::new(NO_INBOUND_RATE),
        state,
        started: Instant::now(),
    });
    probes.insert(operation_id, Arc::clone(&probe));
    spawn_watchdog(&runtime, Arc::downgrade(&probes), operation_id.to_owned());
    OperationProbeGuard {
        operation_id: operation_id.to_owned(),
        probe,
        probes,
    }
}

#[cfg(any(test, feature = "testing"))]
impl super::super::super::Operations {
    /// The in-flight table of a still-running operation on this engine,
    /// rendered.
    ///
    /// The watchdog logs the same text once an operation has been still for
    /// `STALL_AFTER`, which is right for a user's session and useless to a
    /// test: a test that bounds its own wait needs the table AT the moment its
    /// deadline expires, in its panic message, where a human will actually read
    /// it. The SMB full-concurrency suite calls this before it abandons a copy
    /// that overran, so a red run names the phase every task was parked in
    /// instead of only saying "timed out".
    ///
    /// `None` once the operation has settled (its guard deregistered it), so
    /// the caller must keep the copy alive — awaiting a `JoinHandle`, not the
    /// copy future itself, which would drop the guard before the dump could be
    /// taken.
    pub fn render_live_transfer_dump(
        &self,
        operation_id: &cmdr_operation_log::types::OperationId,
        reason: &str,
    ) -> Option<String> {
        self.stores()
            .probes()
            .get(operation_id.as_str())
            .map(|probe| probe.render_dump(reason))
    }
}

/// Deregisters its operation on drop.
pub(in crate::transfer) struct OperationProbeGuard {
    operation_id: String,
    probe: Arc<OperationProbe>,
    /// The table it's registered in.
    probes: Arc<Probes>,
}

impl OperationProbeGuard {
    pub(in crate::transfer) fn probe(&self) -> Arc<OperationProbe> {
        Arc::clone(&self.probe)
    }
}

impl Drop for OperationProbeGuard {
    fn drop(&mut self) {
        self.probes.remove(&self.operation_id);
    }
}
