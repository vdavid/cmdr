//! Cancellation-aware execution and detached background cleanup.
//!
//! `run_cancellable` / `run_cancellable_scoped` run blocking work on a
//! separate thread while polling the operation's cancellation flag, so a
//! stuck network mount can't wedge a non-cancellable copy. The background
//! cleanup helpers detach best-effort deletes off the operation's critical
//! path.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use super::state::WriteOperationState;
use super::types::WriteOperationError;
use cmdr_fs::log_private::LogPrivate;

// ============================================================================
// Background cleanup (detached, best-effort)
// ============================================================================

/// Starts `work` on its own thread named `name`. The same panic a bare
/// `thread::spawn` raises when the OS refuses a thread; the name is what tells
/// the threads apart in a sample or a hang dump.
fn spawn_named<T: Send + 'static>(name: &str, work: impl FnOnce() -> T + Send + 'static) -> std::thread::JoinHandle<T> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(work)
        .expect("the OS starts a write operation's worker thread")
}

/// Deletes a file on a detached thread. Returns immediately. Best-effort.
pub(super) fn remove_file_in_background(path: PathBuf) {
    // allowed-discarded-outcome: a detached best-effort cleanup; nothing waits on it, and it logs its own failure.
    spawn_named("cmdr-cleanup", move || {
        if let Err(e) = fs::remove_file(&path) {
            log::warn!(
                "background cleanup: failed to remove {}: {}",
                LogPrivate::file_path(&path),
                e
            );
        }
    });
}

/// Deletes a cross-FS move's staging tree on a detached thread. Returns
/// immediately. Best-effort.
///
/// The tree is the move's own, so an item in it that refuses its unlink (a
/// Locked or `deny delete` copy) is cleared and the removal retried, the same
/// clearing every other reversal does before removing what it made
/// (`cmdr_fs::metadata::remove_tree_clearing_blockers`). A plain tree costs one
/// `remove_dir_all`.
pub(super) fn remove_staging_tree_in_background(path: PathBuf) {
    // allowed-discarded-outcome: a detached best-effort cleanup; nothing waits on it, and it logs its own failure.
    spawn_named("cmdr-cleanup", move || {
        if let Err(e) = cmdr_fs::metadata::remove_tree_clearing_blockers(&path) {
            log::warn!(
                "background cleanup: failed to remove {}: {}",
                LogPrivate::dir_path(&path),
                e
            );
        }
    });
}

// ============================================================================
// Cancellation-aware execution
// ============================================================================

/// Interval for checking cancellation while waiting for blocking operations.
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Runs a closure on a background thread with polling-based cancellation.
///
/// Spawns `work` on a new thread and polls for results every 100ms, checking the
/// cancellation flag between polls. This ensures quick cancellation response even
/// when filesystem I/O blocks (for example, on stuck network drives).
pub(super) fn run_cancellable<T>(
    work: impl FnOnce() -> Result<T, WriteOperationError> + Send + 'static,
    state: &Arc<WriteOperationState>,
    context: &str,
    operation_id: &str,
) -> Result<T, WriteOperationError>
where
    T: Send + 'static,
{
    let (tx, rx) = mpsc::channel();

    spawn_named("cmdr-cancellable-work", move || {
        let _ = tx.send(work());
    });

    await_worker_result(&rx, state, context, operation_id)
}

/// Scoped variant of [`run_cancellable`] that allows the work closure to borrow
/// non-`'static` data (for example, a `&dyn OperationEventSink` reference).
///
/// Uses `std::thread::scope`, so the call blocks until the worker thread
/// finishes or cancellation is observed. Behavior is otherwise identical to
/// `run_cancellable`: the cancellation flag is polled every 100ms while the
/// worker runs, and the function returns early on cancellation.
pub(super) fn run_cancellable_scoped<'env, T, F>(
    work: F,
    state: &Arc<WriteOperationState>,
    context: &str,
    operation_id: &str,
) -> Result<T, WriteOperationError>
where
    F: FnOnce() -> Result<T, WriteOperationError> + Send + 'env,
    T: Send + 'env,
{
    let (tx, rx) = mpsc::channel();

    std::thread::scope(|scope| {
        // The same panic `scope.spawn` raises when the OS refuses a thread.
        std::thread::Builder::new()
            .name("cmdr-cancellable-work".to_string())
            .spawn_scoped(scope, move || {
                let _ = tx.send(work());
            })
            .expect("the OS starts a write operation's worker thread");

        await_worker_result(&rx, state, context, operation_id)
    })
}

/// Waits for the worker's result, checking the cancellation flag between polls.
///
/// A worker that hangs up without sending (it panicked) surfaces as an `IoError`
/// naming the context, so the operation reports instead of waiting forever.
fn await_worker_result<T>(
    rx: &mpsc::Receiver<Result<T, WriteOperationError>>,
    state: &WriteOperationState,
    context: &str,
    operation_id: &str,
) -> Result<T, WriteOperationError> {
    loop {
        if state.intent().is_cancelled() {
            log::debug!("{context}: cancellation detected during polling op={operation_id}");
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".into(),
            });
        }

        match rx.recv_timeout(CANCELLATION_POLL_INTERVAL) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(WriteOperationError::IoError {
                    path: context.to_string(),
                    message: format!("{context} thread terminated unexpectedly").into(),
                });
            }
        }
    }
}
