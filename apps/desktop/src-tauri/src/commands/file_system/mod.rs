//! Tauri commands for file system operations.

mod archive;
mod drag;
#[cfg(any(feature = "playwright-e2e", debug_assertions))]
mod e2e_support;
mod git;
mod listing;
mod stat;
mod volume_copy;
mod write_ops;

pub use archive::*;
pub use drag::*;
#[cfg(any(feature = "playwright-e2e", debug_assertions))]
pub use e2e_support::*;
pub use git::*;
pub use listing::*;
pub use stat::*;
pub use volume_copy::*;
pub use write_ops::*;

/// Expands tilde (~) to the user's home directory.
pub(crate) fn expand_tilde(path: &str) -> String {
    if (path.starts_with("~/") || path == "~")
        && let Some(home) = dirs::home_dir()
    {
        return path.replacen("~", &home.to_string_lossy(), 1);
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::util::blocking_with_timeout;
    use tokio::time::Duration;

    // Create-op tests (mkdir/mkfile core + managed wrappers) live with the logic
    // in `file_system::write_operations::create`.

    /// How long the two fallback tests' fake blocking work runs. It has to outlast
    /// the 50 ms timeout under those tests, and it is ALSO what the whole test costs:
    /// the assertion lands at the timeout, then the runtime waits out the blocking
    /// thread before the test can end.
    const SLOW_CLOSURE: Duration = Duration::from_millis(500);

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/Documents");
        assert!(expanded.starts_with('/'));
        assert!(expanded.contains("Documents"));
        assert!(!expanded.contains('~'));
    }

    #[test]
    fn test_expand_tilde_alone() {
        let expanded = expand_tilde("~");
        assert!(expanded.starts_with('/'));
        assert!(!expanded.contains('~'));
    }

    #[test]
    fn test_no_tilde() {
        let path = "/usr/local/bin";
        assert_eq!(expand_tilde(path), path);
    }

    #[tokio::test]
    async fn test_blocking_with_timeout_fast_closure_returns_value() {
        let result = blocking_with_timeout(Duration::from_secs(2), false, || true).await;
        assert!(result);
    }

    #[tokio::test]
    async fn test_blocking_with_timeout_slow_closure_returns_fallback() {
        let result = blocking_with_timeout(Duration::from_millis(50), false, || {
            // allowed-test-sleep: this closure fakes slow blocking work; overrunning the 50 ms
            // timeout is exactly what makes `blocking_with_timeout` return its fallback.
            // 500 ms is a 10x margin over the timeout and `thread::sleep` can never return
            // early, so the only way this flips is the 50 ms timer being 450 ms late.
            std::thread::sleep(SLOW_CLOSURE);
            true
        })
        .await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_blocking_with_timeout_returns_custom_fallback() {
        let result = blocking_with_timeout(Duration::from_millis(50), 42, || {
            // allowed-test-sleep: this closure fakes slow blocking work; overrunning the 50 ms
            // timeout is what makes `blocking_with_timeout` return the custom fallback
            std::thread::sleep(SLOW_CLOSURE);
            99
        })
        .await;
        assert_eq!(result, 42);
    }
}
