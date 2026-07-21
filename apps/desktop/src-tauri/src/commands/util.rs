//! Shared utilities for Tauri command modules.

use serde::Serialize;
use std::future::Future;
use tokio::time::Duration;

/// Wraps a value with a flag indicating whether the operation timed out.
/// Used by commands returning collections or Option to let the frontend
/// distinguish "genuinely empty/none" from "timed out before completing."
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TimedOut<T: Serialize + specta::Type> {
    pub data: T,
    pub timed_out: bool,
}

/// Structured IPC error with a timeout flag.
/// Used by commands returning `Result<T, IpcError>` so the frontend can
/// distinguish timeout errors from real failures without string matching.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub message: String,
    pub timed_out: bool,
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl IpcError {
    pub fn timeout() -> Self {
        Self {
            message: "Operation timed out (the volume may be slow or unresponsive)".to_string(),
            timed_out: true,
        }
    }

    pub fn from_err(err: impl std::fmt::Display) -> Self {
        Self {
            message: err.to_string(),
            timed_out: false,
        }
    }
}

/// Runs a blocking closure on the blocking thread pool with a timeout.
/// Returns the fallback value if the closure doesn't complete in time.
pub async fn blocking_with_timeout<T: Send + 'static>(
    timeout_duration: Duration,
    fallback: T,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    match tokio::time::timeout(timeout_duration, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(result)) => result,
        _ => fallback, // Timeout or JoinError
    }
}

/// Like `blocking_with_timeout`, but returns `TimedOut<T>` so the caller
/// knows whether the fallback was returned due to a timeout.
pub async fn blocking_with_timeout_flag<T: Send + Serialize + specta::Type + 'static>(
    timeout_duration: Duration,
    fallback: T,
    f: impl FnOnce() -> T + Send + 'static,
) -> TimedOut<T> {
    match tokio::time::timeout(timeout_duration, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(result)) => TimedOut {
            data: result,
            timed_out: false,
        },
        _ => TimedOut {
            data: fallback,
            timed_out: true,
        },
    }
}

/// Like `blocking_with_timeout`, but for closures returning `Result`.
/// On timeout, returns `Err(IpcError::timeout())`.
pub async fn blocking_result_with_timeout<T: Send + 'static>(
    timeout_duration: Duration,
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, IpcError> {
    match tokio::time::timeout(timeout_duration, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(result)) => result.map_err(IpcError::from_err),
        Ok(Err(e)) => Err(IpcError::from_err(e)),
        Err(_) => Err(IpcError::timeout()),
    }
}

/// Bounds how long the FRONTEND waits, never the work itself.
///
/// `fut` runs in its own task and the timeout races that task's join handle. On
/// expiry the handle is dropped, which DETACHES the task: it keeps running to
/// its own end. The caller gets `IpcError::timeout()` promptly, the work
/// finishes safely behind it.
///
/// ❌ Use this, not a bare `tokio::time::timeout(d, fut)`, for anything that can
/// reach a device backend. A bare timeout DROPS the future wherever it happens
/// to be, and on MTP that abandons an in-flight PTP transaction and wedges the
/// user's phone (`mtp/connection/CLAUDE.md`). An IPC deadline is a promise about
/// the reply, not permission to abandon a half-written transaction.
pub async fn timeout_detached<T, E>(
    timeout_duration: Duration,
    fut: impl Future<Output = Result<T, E>> + Send + 'static,
) -> Result<T, IpcError>
where
    T: Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    match tokio::time::timeout(timeout_duration, tokio::spawn(fut)).await {
        Ok(Ok(result)) => result.map_err(IpcError::from_err),
        Ok(Err(join_err)) => Err(IpcError::from_err(format!("Task failed: {join_err}"))),
        Err(_) => Err(IpcError::timeout()),
    }
}
