//! Shared utilities for Tauri command modules.

use serde::Serialize;
use tokio::time::Duration;

/// Wraps a value with a flag indicating whether the operation timed out.
/// Used by commands returning collections or Option to let the frontend
/// distinguish "genuinely empty/none" from "timed out before completing."
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimedOut<T: Serialize> {
    pub data: T,
    pub timed_out: bool,
}

/// Structured IPC error with a timeout flag.
/// Used by commands returning `Result<T, IpcError>` so the frontend can
/// distinguish timeout errors from real failures without string matching.
#[derive(Debug, Clone, Serialize)]
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
pub async fn blocking_with_timeout_flag<T: Send + Serialize + 'static>(
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
