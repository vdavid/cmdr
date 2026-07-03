//! Tauri commands for file system operations.

mod drag;
#[cfg(any(feature = "playwright-e2e", debug_assertions))]
mod e2e_support;
mod git;
mod listing;
mod stat;
mod volume_copy;
mod write_ops;

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
            std::thread::sleep(Duration::from_secs(2));
            true
        })
        .await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_blocking_with_timeout_returns_custom_fallback() {
        let result = blocking_with_timeout(Duration::from_millis(50), 42, || {
            std::thread::sleep(Duration::from_secs(2));
            99
        })
        .await;
        assert_eq!(result, 42);
    }
}
