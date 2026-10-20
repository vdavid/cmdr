//! What a terminal event does besides reaching the frontend, and in what order.
//!
//! `TauriEventSink`'s four terminal emits (`write-complete`, `write-cancelled`,
//! `write-error`, `write-settled`) each carry app side effects: the completion
//! analytics, the MCP terminal ring, the failure `warn!`, the retained failure,
//! and the free-space hint. The order is part of the contract (the ring and the
//! retained failure must land BEFORE the frontend hears, see each function), so
//! it lives here, once, as plain functions over a [`TerminalEffects`] port. The
//! sink passes [`AppTerminalEffects`] and its real `emit`; a test passes a
//! recorder and reads back the sequence (`terminal_effects_tests.rs`).
//!
//! ❌ Not on `OperationEventSink` or `CollectorEventSink`: test sinks stay
//! side-effect-free.

use super::analytics::emit_completion_analytics;
use crate::mcp::terminal_ops::{self, TerminalStatus};
use cmdr_operation_log::types::OperationId;
use cmdr_operations::{
    Operations, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationType, WriteSettledEvent,
};

/// The app side effects a terminal event can carry, one method each.
pub(crate) trait TerminalEffects {
    /// PII-free completion analytics for a progressed op.
    fn completion_analytics(&self, event: &WriteCompleteEvent);
    /// MCP's terminal ring, which its `await operation_complete` reads.
    fn record_terminal(&self, operation_id: &str, operation_type: WriteOperationType, status: TerminalStatus);
    /// The one log line that keeps a failure's technical reason.
    fn warn_failure(&self, event: &WriteErrorEvent);
    /// The manager's out-of-band failure list.
    fn retain_failure(&self, event: &WriteErrorEvent);
    /// The pane's free-space readout re-reads purgeable space on its next poll.
    fn expect_space_change(&self);
}

/// The production effects, retaining failures on `operations` (`None` only
/// before `install`, which no sink can reach).
pub(crate) struct AppTerminalEffects<'a> {
    pub(crate) operations: Option<&'a Operations>,
}

impl TerminalEffects for AppTerminalEffects<'_> {
    fn completion_analytics(&self, event: &WriteCompleteEvent) {
        emit_completion_analytics(event);
    }

    fn record_terminal(&self, operation_id: &str, operation_type: WriteOperationType, status: TerminalStatus) {
        terminal_ops::record(operation_id, operation_type, status);
    }

    fn warn_failure(&self, event: &WriteErrorEvent) {
        // The one place a failing write operation writes its technical reason
        // down. Everything else about the failure travels as the typed variant:
        // the frontend logs `error.type` alone, and the detail string reaches
        // only the error dialog's collapsed "Technical details". Without this
        // line an error-report bundle records `Trash error: io_error` and the
        // OS's actual words are gone (ERR-BPAPM, 2026-09-08).
        //
        // WARN, not `log_error!`, deliberately: the frontend already raises this
        // same failure at error level, and a second error-level line would push
        // one user-visible failure over the auto-dispatcher's "2 errors within
        // 60s" threshold on its own. The file target is always Debug, so a warn
        // still lands in every bundle.
        //
        // `{:?}` over a hand-written sentence: the variant carries the path and
        // the message in named fields, and Debug can't drift from them.
        log::warn!(
            target: "write_operations",
            "op {} ({:?}) failed: {:?}",
            event.operation_id,
            event.operation_type,
            event.error
        );
    }

    fn retain_failure(&self, event: &WriteErrorEvent) {
        // The engine's `retain_failure` itself decides what counts as a failure
        // and emits nothing while the op's record is live.
        if let Some(operations) = self.operations {
            operations.retain_failure(
                &OperationId::from(event.operation_id.as_str()),
                event.operation_type,
                &event.error,
            );
        }
    }

    fn expect_space_change(&self) {
        // A delete under a local snapshot frees space `statfs` never sees
        // (`volumes/live_space.rs`).
        #[cfg(target_os = "macos")]
        cmdr_local::live_space::expect_space_change();
    }
}

/// `write-complete`: analytics, then the ring, then the emit.
pub(crate) fn complete(fx: &impl TerminalEffects, event: WriteCompleteEvent, emit: impl FnOnce(WriteCompleteEvent)) {
    // Categorical only (op, a count bucket, a bool); never names or paths. Fires
    // before the emit so it can read the event the emit moves.
    fx.completion_analytics(&event);
    // Recorded for MCP before the frontend event fires (the emit-site pattern
    // `listing_errors` uses), since the manager removes the op before
    // `operations-changed` could carry a terminal status.
    fx.record_terminal(&event.operation_id, event.operation_type, TerminalStatus::Completed);
    emit(event);
}

/// `write-cancelled`: the ring, then the emit.
pub(crate) fn cancelled(fx: &impl TerminalEffects, event: WriteCancelledEvent, emit: impl FnOnce(WriteCancelledEvent)) {
    fx.record_terminal(&event.operation_id, event.operation_type, TerminalStatus::Cancelled);
    emit(event);
}

/// `write-error`: the ring, the `warn!`, the retained failure, then the emit.
pub(crate) fn error(fx: &impl TerminalEffects, event: WriteErrorEvent, emit: impl FnOnce(WriteErrorEvent)) {
    fx.record_terminal(&event.operation_id, event.operation_type, TerminalStatus::Failed);
    fx.warn_failure(&event);
    // Retained so the failure outlives the record the manager is about to
    // remove: a backgrounded operation that fails while the queue window is
    // closed must still be able to say why.
    fx.retain_failure(&event);
    emit(event);
}

/// `write-settled`: the free-space hint, then the emit.
pub(crate) fn settled(fx: &impl TerminalEffects, event: WriteSettledEvent, emit: impl FnOnce(WriteSettledEvent)) {
    fx.expect_space_change();
    emit(event);
}
