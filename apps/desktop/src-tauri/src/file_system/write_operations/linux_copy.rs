//! Linux-native file copy using `copy_file_range(2)`.
//!
//! Performs kernel-side copies with progress tracking and cancellation.
//! On btrfs/XFS this produces reflink copies (instant, COW) similar to
//! APFS clonefile on macOS. On other filesystems it falls back to an
//! in-kernel data copy, still faster than userspace read/write.

use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;

use super::types::WriteOperationError;

/// Chunk size per `copy_file_range` call (4 MB).
/// Larger than chunked_copy's 1 MB because this is an in-kernel operation
/// with no userspace buffer overhead. Cancellation is still checked between
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
/// * `cancelled` - Cancellation flag checked between chunks
/// * `progress_callback` - Optional callback called with (bytes_so_far, total)
pub fn copy_single_file_linux(
    source: &Path,
    destination: &Path,
    overwrite: bool,
    cancelled: &Arc<AtomicU8>,
    progress_callback: Option<&dyn Fn(u64, u64)>,
) -> Result<u64, WriteOperationError> {
    let src_file = fs::File::open(source).map_err(|e| map_io_error(e, source, destination))?;

    let src_metadata = src_file.metadata().map_err(|e| WriteOperationError::ReadError {
        path: source.display().to_string(),
        message: format!("Failed to read source metadata: {}", e),
    })?;
    let total_size = src_metadata.len();

    // Open destination, respecting overwrite flag
    let dst_file = if overwrite {
        fs::File::create(destination)
    } else {
        fs::OpenOptions::new().write(true).create_new(true).open(destination)
    }
    .map_err(|e| map_io_error(e, source, destination))?;

    // Pre-allocate space to avoid fragmentation
    if total_size > 0 {
        let _ = unsafe { libc::posix_fallocate(dst_file.as_raw_fd(), 0, total_size as libc::off_t) };
    }

    let src_fd = src_file.as_raw_fd();
    let dst_fd = dst_file.as_raw_fd();
    let mut bytes_copied: u64 = 0;
    let mut src_offset: libc::off64_t = 0;
    let mut dst_offset: libc::off64_t = 0;

    while bytes_copied < total_size {
        // Check cancellation before each chunk
        if super::state::is_cancelled(cancelled) {
            log::debug!("linux_copy: cancelled after {} bytes, cleaning up", bytes_copied);
            drop(dst_file);
            let _ = fs::remove_file(destination);
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
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
            return Err(map_io_error(err, source, destination));
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

    // Copy permissions from source
    let permissions = src_metadata.permissions();
    if let Err(e) = fs::set_permissions(destination, permissions) {
        log::warn!(
            "linux_copy: failed to set permissions on {}: {}",
            destination.display(),
            e
        );
    }

    log::debug!(
        "linux_copy: copied {} bytes from {} to {}",
        bytes_copied,
        source.display(),
        destination.display()
    );

    Ok(bytes_copied)
}

/// Maps an IO error to a WriteOperationError with path context.
fn map_io_error(err: std::io::Error, source: &Path, destination: &Path) -> WriteOperationError {
    match err.kind() {
        std::io::ErrorKind::NotFound => WriteOperationError::SourceNotFound {
            path: source.display().to_string(),
        },
        std::io::ErrorKind::PermissionDenied => WriteOperationError::PermissionDenied {
            path: destination.display().to_string(),
            message: format!("Cannot write to {}: permission denied", destination.display()),
        },
        std::io::ErrorKind::AlreadyExists => WriteOperationError::DestinationExists {
            path: destination.display().to_string(),
        },
        _ => {
            if let Some(os_err) = err.raw_os_error() {
                match os_err {
                    libc::ENOSPC => {
                        return WriteOperationError::InsufficientSpace {
                            required: 0,
                            available: 0,
                            volume_name: None,
                        };
                    }
                    // These use destination path (classify_io_error can't pick the right one)
                    libc::ENAMETOOLONG => {
                        return WriteOperationError::NameTooLong {
                            path: destination.display().to_string(),
                        };
                    }
                    libc::EROFS => {
                        return WriteOperationError::ReadOnlyDevice {
                            path: destination.display().to_string(),
                            device_name: None,
                        };
                    }
                    libc::ENOTCONN | libc::ENETDOWN | libc::ENETUNREACH | libc::EHOSTUNREACH | libc::ETIMEDOUT => {
                        return WriteOperationError::ConnectionInterrupted {
                            path: source.display().to_string(),
                        };
                    }
                    libc::ENODEV => {
                        return WriteOperationError::DeviceDisconnected {
                            path: source.display().to_string(),
                        };
                    }
                    _ => {}
                }
            }
            WriteOperationError::IoError {
                path: source.display().to_string(),
                message: err.to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn create_temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cmdr_linux_copy_test_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("Failed to create temp dir");
        dir
    }

    fn cleanup_temp_dir(path: &std::path::PathBuf) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn test_copy_single_file() {
        let dir = create_temp_dir("single_file");
        let src = dir.join("source.txt");
        let dst = dir.join("dest.txt");

        fs::write(&src, "Hello, Linux!").unwrap();
        let cancelled = Arc::new(AtomicU8::new(0));

        let result = copy_single_file_linux(&src, &dst, false, &cancelled, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 13);
        assert_eq!(fs::read_to_string(&dst).unwrap(), "Hello, Linux!");

        cleanup_temp_dir(&dir);
    }

    #[test]
    fn test_copy_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = create_temp_dir("permissions");
        let src = dir.join("source.sh");
        let dst = dir.join("dest.sh");

        fs::write(&src, "#!/bin/bash\necho hi").unwrap();
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();
        let cancelled = Arc::new(AtomicU8::new(0));

        let result = copy_single_file_linux(&src, &dst, false, &cancelled, None);
        assert!(result.is_ok());

        let dst_perms = fs::metadata(&dst).unwrap().permissions().mode();
        assert_eq!(dst_perms & 0o777, 0o755);

        cleanup_temp_dir(&dir);
    }

    #[test]
    fn test_copy_empty_file() {
        let dir = create_temp_dir("empty");
        let src = dir.join("empty.txt");
        let dst = dir.join("dest.txt");

        fs::write(&src, "").unwrap();
        let cancelled = Arc::new(AtomicU8::new(0));

        let result = copy_single_file_linux(&src, &dst, false, &cancelled, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        assert_eq!(fs::read_to_string(&dst).unwrap(), "");

        cleanup_temp_dir(&dir);
    }

    #[test]
    fn test_copy_cancellation() {
        let dir = create_temp_dir("cancel");
        let src = dir.join("source.txt");
        let dst = dir.join("dest.txt");

        // File must be > 0 bytes to enter the copy loop
        fs::write(&src, "content").unwrap();

        // Pre-cancelled
        let cancelled = Arc::new(AtomicU8::new(2));
        let result = copy_single_file_linux(&src, &dst, false, &cancelled, None);

        assert!(matches!(result, Err(WriteOperationError::Cancelled { .. })));
        assert!(!dst.exists());

        cleanup_temp_dir(&dir);
    }

    #[test]
    fn test_copy_exclusive_fails_on_existing() {
        let dir = create_temp_dir("exclusive");
        let src = dir.join("source.txt");
        let dst = dir.join("dest.txt");

        fs::write(&src, "source").unwrap();
        fs::write(&dst, "existing").unwrap();

        let cancelled = Arc::new(AtomicU8::new(0));
        let result = copy_single_file_linux(&src, &dst, false, &cancelled, None);
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "existing");

        cleanup_temp_dir(&dir);
    }

    #[test]
    fn test_copy_overwrite() {
        let dir = create_temp_dir("overwrite");
        let src = dir.join("source.txt");
        let dst = dir.join("dest.txt");

        fs::write(&src, "new content").unwrap();
        fs::write(&dst, "old content").unwrap();

        let cancelled = Arc::new(AtomicU8::new(0));
        let result = copy_single_file_linux(&src, &dst, true, &cancelled, None);
        assert!(result.is_ok());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "new content");

        cleanup_temp_dir(&dir);
    }

    #[test]
    fn test_copy_progress_callback() {
        use std::sync::atomic::AtomicU64;

        let dir = create_temp_dir("progress");
        let src = dir.join("source.txt");
        let dst = dir.join("dest.txt");

        let content = "x".repeat(1000);
        fs::write(&src, &content).unwrap();

        let cancelled = Arc::new(AtomicU8::new(0));
        let last_bytes = Arc::new(AtomicU64::new(0));
        let last_bytes_clone = Arc::clone(&last_bytes);

        let cb = move |bytes: u64, total: u64| {
            assert_eq!(total, 1000);
            last_bytes_clone.store(bytes, Ordering::Relaxed);
        };

        let result = copy_single_file_linux(&src, &dst, false, &cancelled, Some(&cb));
        assert!(result.is_ok());
        assert_eq!(last_bytes.load(Ordering::Relaxed), 1000);

        cleanup_temp_dir(&dir);
    }
}
