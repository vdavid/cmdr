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
use std::time::Duration;

use super::state::WriteOperationState;
use super::types::WriteOperationError;

// ============================================================================
// Background cleanup (detached, best-effort)
// ============================================================================

/// Deletes a file on a detached thread. Returns immediately. Best-effort.
pub(super) fn remove_file_in_background(path: PathBuf) {
    std::thread::spawn(move || {
        if let Err(e) = fs::remove_file(&path) {
            log::warn!("background cleanup: failed to remove {}: {}", path.display(), e);
        }
    });
}

/// Deletes a directory tree on a detached thread. Returns immediately. Best-effort.
pub(super) fn remove_dir_all_in_background(path: PathBuf) {
    std::thread::spawn(move || {
        if let Err(e) = fs::remove_dir_all(&path) {
            log::warn!("background cleanup: failed to remove {}: {}", path.display(), e);
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
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let _ = tx.send(work());
    });

    loop {
        if super::state::is_cancelled(&state.intent) {
            log::debug!("{context}: cancellation detected during polling op={operation_id}");
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        match rx.recv_timeout(CANCELLATION_POLL_INTERVAL) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(WriteOperationError::IoError {
                    path: context.to_string(),
                    message: format!("{context} thread terminated unexpectedly"),
                });
            }
        }
    }
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
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();

    std::thread::scope(|scope| {
        scope.spawn(move || {
            let _ = tx.send(work());
        });

        loop {
            if super::state::is_cancelled(&state.intent) {
                log::debug!("{context}: cancellation detected during polling op={operation_id}");
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }

            match rx.recv_timeout(CANCELLATION_POLL_INTERVAL) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(WriteOperationError::IoError {
                        path: context.to_string(),
                        message: format!("{context} thread terminated unexpectedly"),
                    });
                }
            }
        }
    })
}
