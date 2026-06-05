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

    /// Registers a real local-FS "root" volume in the global `VolumeManager` so
    /// `create_*_core` with `volume_id = None` (→ "root") exercises the timed
    /// `Volume` path, the same one production hits. In production
    /// `init_volume_manager()` registers "root" at startup; unit tests never
    /// call it, so without this the create-core calls would find no volume.
    /// Idempotent via `register_if_absent`.
    fn ensure_root_volume() {
        use crate::file_system::get_volume_manager;
        use crate::file_system::volume::LocalPosixVolume;
        use std::sync::Arc;
        get_volume_manager().register_if_absent("root", Arc::new(LocalPosixVolume::new("Test root", "/")));
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
        ensure_root_volume();
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
        ensure_root_volume();
        let tmp = create_test_dir("create_exists");
        let parent = tmp.to_string_lossy().to_string();
        fs::create_dir(tmp.join("existing")).unwrap();
        let result = create_directory_core(None, &parent, "existing").await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("already exists"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_directory_empty_name() {
        let tmp = create_test_dir("create_empty");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory_core(None, &parent, "").await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("cannot be empty"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_directory_invalid_chars() {
        let tmp = create_test_dir("create_invalid");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory_core(None, &parent, "foo/bar").await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("invalid characters"));

        let result = create_directory_core(None, &parent, "foo\0bar").await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("invalid characters"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_directory_nonexistent_parent() {
        let result = create_directory_core(None, "/nonexistent_path_12345", "test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_directory_unregistered_volume_errors_without_fs_write() {
        // An unregistered volume_id used to fall back to an untimed synchronous
        // `std::fs::create_dir` on the async executor. Now it returns a typed
        // "Volume not found" error and writes nothing.
        let tmp = create_test_dir("create_unregistered_vol");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory_core(Some("no-such-volume-xyz".to_string()), &parent, "would-be-folder").await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("Volume not found"));
        assert!(
            !tmp.join("would-be-folder").exists(),
            "no directory should be created when the volume isn't registered"
        );
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_file_unregistered_volume_errors_without_fs_write() {
        // Same contract as the directory case: an unregistered volume_id returns
        // a typed error instead of an untimed `std::fs::File::create_new`.
        let tmp = create_test_dir("create_file_unregistered_vol");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_file_core(Some("no-such-volume-xyz".to_string()), &parent, "would-be-file.txt").await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("Volume not found"));
        assert!(
            !tmp.join("would-be-file.txt").exists(),
            "no file should be created when the volume isn't registered"
        );
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_file_success() {
        ensure_root_volume();
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
        ensure_root_volume();
        let tmp = create_test_dir("create_file_exists");
        let parent = tmp.to_string_lossy().to_string();
        fs::write(tmp.join("existing.txt"), b"hello").unwrap();
        let result = create_file_core(None, &parent, "existing.txt").await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("already exists"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_file_empty_name() {
        let tmp = create_test_dir("create_file_empty");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_file_core(None, &parent, "").await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("cannot be empty"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_file_invalid_chars() {
        let tmp = create_test_dir("create_file_invalid");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_file_core(None, &parent, "foo/bar.txt").await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
        assert!(result.unwrap_err().message.contains("invalid characters"));

        let result = create_file_core(None, &parent, "foo\0bar.txt").await;
        assert!(result.is_err());
        // allowed-error-string-match: IpcError is a flat struct; message is the signal
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
