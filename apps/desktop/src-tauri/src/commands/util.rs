//! Shared utilities for Tauri command modules.

use tokio::time::Duration;

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
