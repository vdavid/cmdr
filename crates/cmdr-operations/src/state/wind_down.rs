//! What a cancelled transfer's own reversal sees of the operation it undoes.

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::rt::Runtime as Handle;

use super::super::event_sinks::OperationEventSink;
use super::super::types::{WriteOperationPhase, WriteProgressEvent};
use super::{StopMeans, WriteOperationState};

/// A Rollback-cancelled transfer, winding itself down by reversing what it
/// wrote.
///
/// The operation's intent already reads `RollingBack`, which is the
/// instruction to reverse, so only `Stopped` stops the reversal
/// ([`StopMeans::IntentReachesStopped`]). A reversal that asked
/// `is_cancelled` instead would stop on its first item, having reversed
/// nothing. This type is what holds that: the wind-down reversals take a
/// `WindDown` and no `WriteOperationState`, and it hands out no intent.
pub(crate) struct WindDown {
    state: Arc<WriteOperationState>,
}

impl WindDown {
    /// `state`'s wind-down. The bar drains from here, so the estimator learns
    /// which way it runs before the first frame.
    pub(crate) fn of(state: &Arc<WriteOperationState>) -> Self {
        state.reversal_drains_the_bar();
        Self {
            state: Arc::clone(state),
        }
    }

    /// `true` once somebody stopped the reversal itself.
    pub(crate) fn stop_requested(&self) -> bool {
        StopMeans::IntentReachesStopped.requested(self.state.intent())
    }

    /// How often a progress frame may go out.
    pub(crate) fn progress_interval(&self) -> Duration {
        self.state.progress_interval
    }

    /// The host's rollback pace (`Pacing::rollback_pace`), taken after each item
    /// the reversal walks. `None` in every real build; under E2E it holds the bar
    /// long enough to watch it drain.
    pub(crate) fn pace(&self) -> Option<Duration> {
        self.state.scope().host().pacing().rollback_pace()
    }

    /// The engine's runtime, whose blocking pool a removal's syscalls run on.
    pub(crate) fn runtime(&self) -> Handle {
        self.state.scope().host().runtime()
    }

    /// Emits one reverse-progress frame and keeps the status cache in step with
    /// it.
    pub(crate) fn report(&self, events: &dyn OperationEventSink, event: WriteProgressEvent) {
        let status = (
            event.current_file.clone(),
            event.files_done,
            event.files_total,
            event.bytes_done,
            event.bytes_total,
        );
        let operation_id = event.operation_id.clone();
        self.state.emit_progress_via_sink(events, event);
        let (current_file, files_done, files_total, bytes_done, bytes_total) = status;
        self.state.scope().status().update(
            &operation_id,
            WriteOperationPhase::RollingBack,
            current_file,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
        );
    }
}
