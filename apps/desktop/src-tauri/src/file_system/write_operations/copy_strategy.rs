//! Copy strategy selection for file operations.
//!
//! Encapsulates the decision of which copy method to use:
//! - Network filesystems: chunked copy for responsive cancellation
//! - Safe overwrite needed: temp file + rename pattern
//! - Otherwise: native macOS copyfile (or std::fs::copy on other platforms)

#[cfg(not(target_os = "macos"))]
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

#[cfg(target_os = "macos")]
use crate::file_system::macos_copy::{CopyProgressContext, copy_single_file_native};

use super::chunked_copy::{ChunkedCopyProgressFn, chunked_copy_with_metadata, is_network_filesystem};
use super::helpers::safe_overwrite_file;
use super::types::WriteOperationError;

/// Copies file contents using the appropriate strategy based on destination type.
///
/// Strategy selection:
/// - Network filesystems: chunked copy for responsive cancellation (macOS copyfile ignores
///   COPYFILE_QUIT on network mounts)
/// - Safe overwrite needed: temp file + rename pattern to preserve original on failure
/// - Otherwise: native macOS copyfile or std::fs::copy
#[cfg(target_os = "macos")]
pub(super) fn copy_file_with_strategy(
    source: &Path,
    dest: &Path,
    needs_safe_overwrite: bool,
    cancelled: &Arc<AtomicBool>,
    progress_callback: Option<ChunkedCopyProgressFn>,
) -> Result<u64, WriteOperationError> {
    if is_network_filesystem(dest) {
        log::debug!(
            "copy_file_with_strategy: using chunked copy for network destination {}",
            dest.display()
        );
        chunked_copy_with_metadata(source, dest, cancelled, progress_callback)
    } else if needs_safe_overwrite {
        let context = CopyProgressContext::with_cancellation(Arc::clone(cancelled));
        safe_overwrite_file(source, dest, Some(&context))
    } else {
        let context = CopyProgressContext::with_cancellation(Arc::clone(cancelled));
        copy_single_file_native(source, dest, false, Some(&context))
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn copy_file_with_strategy(
    source: &Path,
    dest: &Path,
    needs_safe_overwrite: bool,
    cancelled: &Arc<AtomicBool>,
    progress_callback: Option<ChunkedCopyProgressFn>,
) -> Result<u64, WriteOperationError> {
    if is_network_filesystem(dest) {
        log::debug!(
            "copy_file_with_strategy: using chunked copy for network destination {}",
            dest.display()
        );
        chunked_copy_with_metadata(source, dest, cancelled, progress_callback)
    } else if needs_safe_overwrite {
        safe_overwrite_file(source, dest)
    } else {
        fs::copy(source, dest).map_err(|e| WriteOperationError::IoError {
            path: source.display().to_string(),
            message: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    fn create_temp_dir(name: &str) -> std::path::PathBuf {
        let temp_dir = std::env::temp_dir().join(format!("cmdr_copy_strategy_test_{}", name));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
        temp_dir
    }

    fn cleanup_temp_dir(path: &std::path::PathBuf) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn test_copy_file_with_strategy_basic() {
        let temp_dir = create_temp_dir("basic");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "Hello, copy strategy!").unwrap();

        let cancelled = Arc::new(AtomicBool::new(false));
        let result = copy_file_with_strategy(&src, &dst, false, &cancelled, None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 21);
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "Hello, copy strategy!");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_copy_file_with_strategy_safe_overwrite() {
        let temp_dir = create_temp_dir("safe_overwrite");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "New content").unwrap();
        fs::write(&dst, "Old content").unwrap();

        let cancelled = Arc::new(AtomicBool::new(false));
        let result = copy_file_with_strategy(&src, &dst, true, &cancelled, None);

        assert!(result.is_ok());
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "New content");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_copy_file_with_strategy_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = create_temp_dir("perms");
        let src = temp_dir.join("source.sh");
        let dst = temp_dir.join("dest.sh");

        fs::write(&src, "#!/bin/bash").unwrap();
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();

        let cancelled = Arc::new(AtomicBool::new(false));
        let result = copy_file_with_strategy(&src, &dst, false, &cancelled, None);

        assert!(result.is_ok());
        let dst_perms = fs::metadata(&dst).unwrap().permissions().mode();
        assert_eq!(dst_perms & 0o777, 0o755);

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_copy_file_with_strategy_empty_file() {
        let temp_dir = create_temp_dir("empty");
        let src = temp_dir.join("empty.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "").unwrap();

        let cancelled = Arc::new(AtomicBool::new(false));
        let result = copy_file_with_strategy(&src, &dst, false, &cancelled, None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "");

        cleanup_temp_dir(&temp_dir);
    }
}
