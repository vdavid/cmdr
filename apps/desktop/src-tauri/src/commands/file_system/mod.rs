//! Tauri commands for file system operations.

mod drag;
mod e2e_support;
mod listing;
mod volume_copy;
mod write_ops;

pub use drag::*;
pub use e2e_support::*;
pub use listing::*;
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
    use std::fs;
    use std::path::PathBuf;
    use tokio::time::Duration;

    fn create_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cmdr_fs_cmd_test_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("Failed to create test directory");
        dir
    }

    fn cleanup_test_dir(path: &PathBuf) {
        let _ = fs::remove_dir_all(path);
    }

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
    async fn test_create_directory_success() {
        let tmp = create_test_dir("create_success");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory_core(None, &parent, "new-folder").await;
        assert!(result.is_ok());
        let (created_path, _) = result.unwrap();
        assert!(created_path.is_dir());
        assert!(created_path.to_string_lossy().ends_with("new-folder"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_directory_already_exists() {
        let tmp = create_test_dir("create_exists");
        let parent = tmp.to_string_lossy().to_string();
        fs::create_dir(tmp.join("existing")).unwrap();
        let result = create_directory_core(None, &parent, "existing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("already exists"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_directory_empty_name() {
        let tmp = create_test_dir("create_empty");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory_core(None, &parent, "").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("cannot be empty"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_directory_invalid_chars() {
        let tmp = create_test_dir("create_invalid");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory_core(None, &parent, "foo/bar").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid characters"));

        let result = create_directory_core(None, &parent, "foo\0bar").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid characters"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_directory_nonexistent_parent() {
        let result = create_directory_core(None, "/nonexistent_path_12345", "test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_file_success() {
        let tmp = create_test_dir("create_file_success");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_file_core(None, &parent, "new-file.txt").await;
        assert!(result.is_ok());
        let (created_path, _) = result.unwrap();
        assert!(created_path.is_file());
        assert!(created_path.to_string_lossy().ends_with("new-file.txt"));
        assert_eq!(fs::read(&created_path).unwrap(), b"");
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_file_already_exists() {
        let tmp = create_test_dir("create_file_exists");
        let parent = tmp.to_string_lossy().to_string();
        fs::write(tmp.join("existing.txt"), b"hello").unwrap();
        let result = create_file_core(None, &parent, "existing.txt").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("already exists"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_file_empty_name() {
        let tmp = create_test_dir("create_file_empty");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_file_core(None, &parent, "").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("cannot be empty"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_file_invalid_chars() {
        let tmp = create_test_dir("create_file_invalid");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_file_core(None, &parent, "foo/bar.txt").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid characters"));

        let result = create_file_core(None, &parent, "foo\0bar.txt").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid characters"));
        cleanup_test_dir(&tmp);
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
