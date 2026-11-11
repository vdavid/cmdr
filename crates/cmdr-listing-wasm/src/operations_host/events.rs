//! The engine's engine-wide facts as the page hears them, mirroring the
//! desktop's `operations_host/events.rs`: the queue snapshot, the busy volumes,
//! kept leftovers, and a scan's waiting progress go out as their Tauri events;
//! the indexing priority and analytics have no counterpart in a page, and an
//! error report goes to the console.

use std::sync::Arc;

use cmdr_operation_log::rollback::RollbackReport;
use cmdr_operation_log::types::Initiator;
use cmdr_operations::host::{EngineEvents, InstantMutationFact, OperationErrorReport};
use cmdr_operations::{LeftoversKeptEvent, OperationsChanged, WriteProgressEvent};

use super::wire::{self, VolumesBusyChanged, emit};
use crate::events::EventSink;

pub(crate) struct DemoEngineEvents {
    pub(crate) sink: Arc<dyn EventSink>,
}

impl EngineEvents for DemoEngineEvents {
    fn operations_changed(&self, snapshot: OperationsChanged) {
        emit(&*self.sink, wire::OPERATIONS_CHANGED, &snapshot);
    }

    fn volumes_busy_changed(&self, volume_ids: Vec<String>) {
        emit(
            &*self.sink,
            wire::VOLUMES_BUSY_CHANGED,
            &VolumesBusyChanged { volume_ids },
        );
    }

    fn transfer_started(&self, _volume_ids: &[String]) {}

    fn transfer_finished(&self, _volume_ids: &[String]) {}

    fn leftovers_kept(&self, notice: LeftoversKeptEvent) {
        emit(&*self.sink, wire::LEFTOVERS_KEPT, &notice);
    }

    fn instant_mutation_settled(&self, _fact: InstantMutationFact) {}

    fn rollback_settled(&self, _initiator: Initiator, _narrowed: bool, _report: &RollbackReport) {}

    fn error_report(&self, report: OperationErrorReport) {
        // The ledger runs unpersisted on purpose in a page; nothing to tell anyone.
        if !matches!(report, OperationErrorReport::LedgerUnpersisted) {
            log::warn!(target: "write_ops", "{report:?}");
        }
    }

    fn scan_wait_progress(&self, event: WriteProgressEvent) {
        emit(&*self.sink, wire::WRITE_PROGRESS, &event);
    }
}
