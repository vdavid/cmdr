//! The queue window's Retry on a "Couldn't finish" row: how a retained failure
//! keeps a way to start again, and the one call that does it.
//!
//! The manager holds the recipe as a [`StartAgain`] trait object, never as the
//! routing type that implements it (`routing/retry.rs`): the recipe re-enters
//! routing, routing starts operations through the manager, and a manager that
//! named the routing type would weld the two into one module cycle.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use super::super::Operations;
use super::super::types::{RetryOutcome, WriteOperationError, WriteOperationStartResult};
use super::{OperationManager, OperationSnapshot};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_operation_log::types::OperationId;

/// Starts a failed operation again, as a NEW operation. Implemented by the
/// routing that started the original (`routing/retry.rs`), which says what
/// carries over and what resets.
pub(crate) trait StartAgain: Send + Sync {
    /// Starts it on `engine`, the engine whose queue holds the failed row.
    fn start_again<'a>(
        &'a self,
        engine: &'a Operations,
    ) -> Pin<Box<dyn Future<Output = Result<WriteOperationStartResult, WriteOperationError>> + Send + 'a>>;
}

/// One retained failure: the row the queue window shows, and how to start the
/// operation again when the person asks.
pub(super) struct RetainedFailure {
    pub(super) snapshot: OperationSnapshot,
    retry: Option<Arc<dyn StartAgain>>,
    /// A Retry for this row is starting right now. A second click in that
    /// window must not start a second copy of the same transfer.
    retrying: bool,
}

impl RetainedFailure {
    pub(super) fn new(snapshot: OperationSnapshot, retry: Option<Arc<dyn StartAgain>>) -> Self {
        Self {
            snapshot,
            retry,
            retrying: false,
        }
    }

    /// The row as the snapshot carries it.
    pub(super) fn row(&self) -> OperationSnapshot {
        OperationSnapshot {
            can_retry: self.retry.is_some() && !self.retrying,
            ..self.snapshot.clone()
        }
    }
}

impl OperationManager {
    /// Gives `operation_id` a way to start again if it fails. Called by the
    /// routing that started it, right after the start returned the id.
    ///
    /// The operation may have run and failed in the moment between the two (a
    /// destination refusing its first write), so a retained failure takes the
    /// recipe too, and the queue re-broadcasts so its row grows the Retry.
    pub(crate) fn attach_retry(&self, operation_id: &str, retry: Arc<dyn StartAgain>) {
        let failure_changed = {
            let mut inner = self.inner.lock_ignore_poison();
            if let Some(record) = inner.records.get_mut(operation_id) {
                record.retry = Some(Arc::clone(&retry));
            }
            match inner
                .failures
                .iter_mut()
                .find(|failure| failure.snapshot.operation_id == operation_id)
            {
                Some(failure) if failure.retry.is_none() => {
                    failure.retry = Some(retry);
                    true
                }
                _ => false,
            }
        };
        if failure_changed {
            self.emit_changed();
        }
    }

    /// Starts a retained failure again. On a start, the failed row is gone (the
    /// new operation's row replaces it); on a refusal, it stays, with its Retry,
    /// and the refusal goes back to the caller.
    pub(super) async fn retry_failure(
        &self,
        engine: &Operations,
        operation_id: &OperationId,
    ) -> Result<RetryOutcome, WriteOperationError> {
        let retry = {
            let mut inner = self.inner.lock_ignore_poison();
            let Some(failure) = inner
                .failures
                .iter_mut()
                .find(|failure| failure.snapshot.operation_id == operation_id.as_str())
            else {
                return Ok(RetryOutcome::NothingToRetry);
            };
            match (&failure.retry, failure.retrying) {
                (Some(retry), false) => {
                    failure.retrying = true;
                    Arc::clone(retry)
                }
                _ => return Ok(RetryOutcome::NothingToRetry),
            }
        };
        log::info!(target: "op_manager", "retry failed op={operation_id}");
        let started = retry.start_again(engine).await;
        match started {
            Ok(result) => {
                self.dismiss_failure(operation_id);
                Ok(RetryOutcome::Started {
                    operation_id: result.operation_id,
                })
            }
            Err(error) => {
                if let Some(failure) = self
                    .inner
                    .lock_ignore_poison()
                    .failures
                    .iter_mut()
                    .find(|failure| failure.snapshot.operation_id == operation_id.as_str())
                {
                    failure.retrying = false;
                }
                Err(error)
            }
        }
    }
}

impl Operations {
    /// Starts a failed operation again from its queue row. Backs the
    /// `retry_failed_operation` IPC command.
    ///
    /// [`RetryOutcome::NothingToRetry`] for an id with no retained failure, one
    /// with no way to start again, or one already starting; a refused start (a
    /// volume that is gone) comes back as the typed error and leaves the row as it
    /// was.
    pub async fn retry_failed_operation(
        &self,
        operation_id: &OperationId,
    ) -> Result<RetryOutcome, WriteOperationError> {
        self.manager().retry_failure(self, operation_id).await
    }
}
