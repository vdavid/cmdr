//! Linux-native file copy using `copy_file_range(2)`.
//!
//! Performs kernel-side copies with progress tracking and cancellation.
//! On btrfs/XFS this produces reflink copies (instant, COW) similar to
//! APFS clonefile on macOS. On other filesystems it falls back to an
//! in-kernel data copy, still faster than userspace read/write.

use super::super::state::WriteOperationState;
use std::fs;
use std::os::unix::fs::OpenOptionsExt as _;
use std::os::unix::io::AsRawFd;
use std::path::Path;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, StagedItem, StagedTimes, apply_staged};

use super::super::error_classification::classify_copy_io_error;
use super::super::types::WriteOperationError;

/// Chunk size per `copy_file_range` call (4 MB).
/// Larger than chunked_copy's 1 MB because this is an in-kernel operation
/// with no userspace buffer overhead. Cancel and pause are still asked between
/// iterations.
const COPY_RANGE_CHUNK: usize = 4 * 1024 * 1024;

/// Copies a single file using `copy_file_range(2)` with progress tracking.
///
/// Returns the number of bytes copied on success.
///
/// # Arguments
/// * `source` - Source file path
/// * `destination` - Destination file path (will be created/overwritten)
/// * `overwrite` - If true, overwrite existing destination
/// * `state` - The operation, asked to stop or park between chunks
/// * `progress_callback` - Optional callback called with (bytes_so_far, total)
pub(crate) fn copy_single_file_linux(
    source: &Path,
    destination: &Path,
    overwrite: bool,
    state: &WriteOperationState,
    progress_callback: Option<&dyn Fn(u64, u64)>,
) -> Result<u64, WriteOperationError> {
    let src_file = fs::File::open(source).map_err(|e| classify_copy_io_error(&e, source, destination))?;

    let src_metadata = src_file.metadata().map_err(|e| WriteOperationError::ReadError {
        path: source.display().to_string(),
        message: format!("Failed to read source metadata: {}", e).into(),
    })?;
    let total_size = src_metadata.len();

    // Open destination, respecting overwrite flag. Owner-only until the source's
    // mode goes on after the bytes, so a private file never sits readable by
    // others mid-copy.
    let mut options = fs::OpenOptions::new();
    options.write(true).mode(0o600);
    if overwrite {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let dst_file = options
        .open(destination)
        .map_err(|e| classify_copy_io_error(&e, source, destination))?;

    // Pre-allocate space to avoid fragmentation.
    if total_size > 0 {
        // SAFETY: `dst_file` is a live, open `File` whose fd stays valid for this call (and the
        // rest of the function), and offset 0 with length `total_size` is a valid range.
        // `posix_fallocate` returns an errno without touching memory; we ignore it (best-effort).
        let _ = unsafe { libc::posix_fallocate(dst_file.as_raw_fd(), 0, total_size as libc::off_t) };
    }

    let src_fd = src_file.as_raw_fd();
    let dst_fd = dst_file.as_raw_fd();
    let mut bytes_copied: u64 = 0;
    let mut src_offset: libc::off64_t = 0;
    let mut dst_offset: libc::off64_t = 0;

    while bytes_copied < total_size {
        // The chunk boundary: stop on a cancel, park while paused.
        if state.stop_or_park_sync() {
            log::debug!("linux_copy: cancelled after {} bytes, cleaning up", bytes_copied);
            drop(dst_file);
            let _ = fs::remove_file(destination);
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string().into(),
            });
        }

        let remaining = total_size - bytes_copied;
        let chunk = remaining.min(COPY_RANGE_CHUNK as u64) as usize;

        // SAFETY: File descriptors are valid (owned by src_file/dst_file).
        // Offsets are valid pointers to stack variables.
        let result = unsafe {
            libc::copy_file_range(
                src_fd,
                &mut src_offset,
                dst_fd,
                &mut dst_offset,
                chunk,
                0, // flags (must be 0)
            )
        };

        if result < 0 {
            let err = std::io::Error::last_os_error();
            drop(dst_file);
            let _ = fs::remove_file(destination);
            return Err(classify_copy_io_error(&err, source, destination));
        }

        if result == 0 {
            // Unexpected EOF - source file may have been truncated
            break;
        }

        bytes_copied += result as u64;

        if let Some(cb) = progress_callback {
            cb(bytes_copied, total_size);
        }
    }

    // `copy_file_range` moves data only. The `user.*` xattrs, the mode, and the
    // dates go on through the fd (`cmdr_fs::metadata::apply_staged`), best
    // effort. The mtime is load-bearing beyond display: Phase 3 of
    // `move_with_staging` compares the staged copy's mtime against the dest for
    // `OverwriteOlder`, so a freshly-stamped staged file would look newer and
    // wrongly overwrite. Linux can't set a birth time.
    let carried = CarriedMetadata::of_local_file(&src_file, &src_metadata);
    let times = StagedTimes {
        modified: src_metadata.modified().ok(),
        accessed: src_metadata.accessed().ok(),
    };
    let report = apply_staged(StagedItem::File(&dst_file), &carried, times);
    if report.any_refused() {
        log::warn!(
            "linux_copy: {} took only some of the metadata of {}: {report:?}",
            LogPrivate::file_path(destination),
            LogPrivate::file_path(source)
        );
    }

    log::debug!(
        "linux_copy: copied {} bytes from {} to {}",
        bytes_copied,
        LogPrivate::file_path(source),
        LogPrivate::file_path(destination)
    );

    Ok(bytes_copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestOperations;
    use cmdr_fs::testing::TestDir;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    fn create_temp_dir(name: &str) -> TestDir {
        TestDir::new(&format!("linux_copy_test_{}", name))
    }

    /// A live operation to copy under; the engine has to outlive it.
    fn running_state() -> (TestOperations, Arc<WriteOperationState>) {
        let engine = TestOperations::new();
        let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
        (engine, state)
    }

    #[test]
    fn test_copy_single_file() {
        let dir = create_temp_dir("single_file");
        let src = dir.join("source.txt");
        let dst = dir.join("dest.txt");

        fs::write(&src, "Hello, Linux!").unwrap();
        let (_engine, state) = running_state();

        let result = copy_single_file_linux(&src, &dst, false, &state, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 13);
        assert_eq!(fs::read_to_string(&dst).unwrap(), "Hello, Linux!");
    }

    #[test]
    fn test_copy_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = create_temp_dir("permissions");
        let src = dir.join("source.sh");
        let dst = dir.join("dest.sh");

        fs::write(&src, "#!/bin/bash\necho hi").unwrap();
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();
        let (_engine, state) = running_state();

        let result = copy_single_file_linux(&src, &dst, false, &state, None);
        assert!(result.is_ok());

        let dst_perms = fs::metadata(&dst).unwrap().permissions().mode();
        assert_eq!(dst_perms & 0o777, 0o755);
    }

    #[test]
    fn test_copy_empty_file() {
        let dir = create_temp_dir("empty");
        let src = dir.join("empty.txt");
        let dst = dir.join("dest.txt");

        fs::write(&src, "").unwrap();
        let (_engine, state) = running_state();

        let result = copy_single_file_linux(&src, &dst, false, &state, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        assert_eq!(fs::read_to_string(&dst).unwrap(), "");
    }

    #[test]
    fn test_copy_cancellation() {
        let dir = create_temp_dir("cancel");
        let src = dir.join("source.txt");
        let dst = dir.join("dest.txt");

        // File must be > 0 bytes to enter the copy loop
        fs::write(&src, "content").unwrap();

        // Pre-cancelled
        let (_engine, state) = running_state();
        state.intent().set_for_test(crate::OperationIntent::Stopped);
        let result = copy_single_file_linux(&src, &dst, false, &state, None);

        assert!(matches!(result, Err(WriteOperationError::Cancelled { .. })));
        assert!(!dst.exists());
    }

    #[test]
    fn test_copy_exclusive_fails_on_existing() {
        let dir = create_temp_dir("exclusive");
        let src = dir.join("source.txt");
        let dst = dir.join("dest.txt");

        fs::write(&src, "source").unwrap();
        fs::write(&dst, "existing").unwrap();

        let (_engine, state) = running_state();
        let result = copy_single_file_linux(&src, &dst, false, &state, None);
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "existing");
    }

    #[test]
    fn test_copy_overwrite() {
        let dir = create_temp_dir("overwrite");
        let src = dir.join("source.txt");
        let dst = dir.join("dest.txt");

        fs::write(&src, "new content").unwrap();
        fs::write(&dst, "old content").unwrap();

        let (_engine, state) = running_state();
        let result = copy_single_file_linux(&src, &dst, true, &state, None);
        assert!(result.is_ok());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "new content");
    }

    #[test]
    fn test_copy_progress_callback() {
        use std::sync::atomic::AtomicU64;

        let dir = create_temp_dir("progress");
        let src = dir.join("source.txt");
        let dst = dir.join("dest.txt");

        let content = "x".repeat(1000);
        fs::write(&src, &content).unwrap();

        let (_engine, state) = running_state();
        let last_bytes = Arc::new(AtomicU64::new(0));
        let last_bytes_clone = Arc::clone(&last_bytes);

        let cb = move |bytes: u64, total: u64| {
            assert_eq!(total, 1000);
            last_bytes_clone.store(bytes, Ordering::Relaxed);
        };

        let result = copy_single_file_linux(&src, &dst, false, &state, Some(&cb));
        assert!(result.is_ok());
        assert_eq!(last_bytes.load(Ordering::Relaxed), 1000);
    }
}
