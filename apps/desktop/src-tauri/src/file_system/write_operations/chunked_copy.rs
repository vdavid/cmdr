//! Chunked file copy with cancellation support for network filesystems.
//!
//! macOS `copyfile()` ignores `COPYFILE_QUIT` on network filesystems - the syscall
//! continues until buffered I/O drains. This module provides a chunked read/write
//! alternative that checks cancellation between chunks, allowing immediate response
//! to user cancellation requests.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::types::WriteOperationError;

/// Progress callback for chunked copy operations.
/// Called after each chunk with (bytes_copied_so_far, total_bytes).
pub type ChunkedCopyProgressFn<'a> = &'a dyn Fn(u64, u64);

/// Chunk size for network file copies (1MB).
/// This provides a good balance between:
/// - Cancellation responsiveness (checked every 1MB)
/// - I/O efficiency (not too many small writes)
const CHUNK_SIZE: usize = 1024 * 1024;

// ============================================================================
// Network filesystem detection
// ============================================================================

/// Detects if the given path is on a network filesystem.
///
/// Returns `true` for SMB, NFS, AFP, and WebDAV filesystems.
/// Returns `false` for local filesystems (APFS, HFS+, etc.) or if detection fails.
#[cfg(target_os = "macos")]
pub fn is_network_filesystem(path: &Path) -> bool {
    use std::ffi::CString;

    // For non-existent paths, check the parent directory
    let check_path = if path.exists() {
        path.to_path_buf()
    } else {
        match path.parent() {
            Some(p) if p.exists() => p.to_path_buf(),
            _ => return false,
        }
    };

    let c_path = match CString::new(check_path.to_string_lossy().as_bytes()) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stat) } != 0 {
        return false;
    }

    // SAFETY: f_fstypename is a null-terminated C string
    let fstype = unsafe { std::ffi::CStr::from_ptr(stat.f_fstypename.as_ptr()).to_string_lossy() };

    let is_network = matches!(fstype.as_ref(), "smbfs" | "nfs" | "afpfs" | "webdav");

    if is_network {
        log::debug!(
            "is_network_filesystem: {} is on network filesystem type '{}'",
            path.display(),
            fstype
        );
    }

    is_network
}

#[cfg(not(target_os = "macos"))]
pub fn is_network_filesystem(_path: &Path) -> bool {
    // On non-macOS platforms, assume local filesystem
    // This can be extended later for Linux (check /proc/mounts)
    false
}

// ============================================================================
// Chunked copy with metadata
// ============================================================================

/// Copies a file using chunked read/write with cancellation checks.
///
/// This is used for network filesystems where `copyfile()` doesn't respond
/// to cancellation in a timely manner. The copy checks for cancellation
/// between each 1MB chunk, allowing near-instant response to cancel requests.
///
/// After the data copy, all metadata is preserved:
/// - Extended attributes (includes macOS resource forks, Finder info)
/// - ACLs (access control lists)
/// - Timestamps (modification time, access time)
/// - Permissions
///
/// The optional progress callback is called after each chunk with
/// (bytes_copied_so_far, total_bytes).
pub fn chunked_copy_with_metadata(
    source: &Path,
    dest: &Path,
    cancelled: &Arc<AtomicBool>,
    progress_callback: Option<ChunkedCopyProgressFn>,
) -> Result<u64, WriteOperationError> {
    log::info!(
        "chunked_copy: starting chunked copy from {} to {}",
        source.display(),
        dest.display()
    );

    // Get source size for progress reporting
    let source_size = std::fs::metadata(source).map(|m| m.len()).unwrap_or(0);

    // 1. Chunked data copy with cancellation checks
    let bytes = copy_data_chunked(source, dest, cancelled, source_size, progress_callback)?;

    // 2. Copy all metadata (best effort - log warnings but don't fail)
    if let Err(e) = copy_metadata(source, dest) {
        log::warn!(
            "chunked_copy: failed to copy some metadata from {} to {}: {:?}",
            source.display(),
            dest.display(),
            e
        );
    }

    log::info!(
        "chunked_copy: completed {} bytes from {} to {}",
        bytes,
        source.display(),
        dest.display()
    );

    Ok(bytes)
}

/// Copies file data in chunks, checking cancellation between each chunk.
fn copy_data_chunked(
    source: &Path,
    dest: &Path,
    cancelled: &Arc<AtomicBool>,
    source_size: u64,
    progress_callback: Option<ChunkedCopyProgressFn>,
) -> Result<u64, WriteOperationError> {
    let mut src_file = std::fs::File::open(source).map_err(|e| WriteOperationError::IoError {
        path: source.display().to_string(),
        message: format!("Failed to open source file: {}", e),
    })?;

    let mut dst_file = std::fs::File::create(dest).map_err(|e| WriteOperationError::IoError {
        path: dest.display().to_string(),
        message: format!("Failed to create destination file: {}", e),
    })?;

    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut total_bytes = 0u64;

    loop {
        // Check cancellation BEFORE each read
        if cancelled.load(Ordering::Relaxed) {
            log::info!(
                "chunked_copy: cancellation detected after {} bytes, cleaning up",
                total_bytes
            );
            // Clean up partial file
            drop(dst_file);
            let _ = std::fs::remove_file(dest);
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        let bytes_read = src_file.read(&mut buffer).map_err(|e| WriteOperationError::IoError {
            path: source.display().to_string(),
            message: format!("Failed to read from source: {}", e),
        })?;

        if bytes_read == 0 {
            break; // EOF
        }

        dst_file
            .write_all(&buffer[..bytes_read])
            .map_err(|e| WriteOperationError::IoError {
                path: dest.display().to_string(),
                message: format!("Failed to write to destination: {}", e),
            })?;

        total_bytes += bytes_read as u64;

        // Report progress after each chunk
        if let Some(cb) = progress_callback {
            cb(total_bytes, source_size);
        }
    }

    // Note: sync_all() removed - network writes are synchronous and the async sync
    // at operation completion handles durability. Blocking sync defeats cancellation.

    Ok(total_bytes)
}

// ============================================================================
// Metadata copying
// ============================================================================

/// Copies all metadata from source to destination.
fn copy_metadata(source: &Path, dest: &Path) -> Result<(), WriteOperationError> {
    // 1. Copy extended attributes (includes resource forks, Finder info)
    copy_xattrs(source, dest)?;

    // 2. Copy ACLs
    copy_acls(source, dest)?;

    // 3. Copy timestamps
    copy_timestamps(source, dest)?;

    // 4. Copy permissions
    copy_permissions(source, dest)?;

    Ok(())
}

/// Copies extended attributes from source to destination.
fn copy_xattrs(source: &Path, dest: &Path) -> Result<(), WriteOperationError> {
    // List all xattrs on source
    let xattrs = match xattr::list(source) {
        Ok(iter) => iter.collect::<Vec<_>>(),
        Err(e) => {
            log::debug!("copy_xattrs: failed to list xattrs on {}: {}", source.display(), e);
            return Ok(()); // Not all filesystems support xattrs
        }
    };

    for attr_name in xattrs {
        // Get the attribute value from source
        let value = match xattr::get(source, &attr_name) {
            Ok(Some(v)) => v,
            Ok(None) => continue, // Attribute disappeared
            Err(e) => {
                log::debug!(
                    "copy_xattrs: failed to get xattr {:?} from {}: {}",
                    attr_name,
                    source.display(),
                    e
                );
                continue;
            }
        };

        // Set the attribute on destination
        if let Err(e) = xattr::set(dest, &attr_name, &value) {
            log::debug!(
                "copy_xattrs: failed to set xattr {:?} on {}: {}",
                attr_name,
                dest.display(),
                e
            );
            // Continue with other xattrs
        }
    }

    Ok(())
}

/// Copies ACLs (access control lists) from source to destination.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "freebsd"))]
fn copy_acls(source: &Path, dest: &Path) -> Result<(), WriteOperationError> {
    use exacl::{AclOption, getfacl, setfacl};

    // Get ACL from source (best effort - not all filesystems support ACLs)
    let acl = match getfacl(source, AclOption::empty()) {
        Ok(acl) => acl,
        Err(e) => {
            log::debug!("copy_acls: failed to get ACL from {}: {}", source.display(), e);
            return Ok(());
        }
    };

    // Set ACL on destination
    if let Err(e) = setfacl(&[dest], &acl, AclOption::empty()) {
        log::debug!("copy_acls: failed to set ACL on {}: {}", dest.display(), e);
    }

    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "freebsd")))]
fn copy_acls(_source: &Path, _dest: &Path) -> Result<(), WriteOperationError> {
    // ACLs not supported on this platform
    Ok(())
}

/// Copies timestamps (mtime, atime) from source to destination.
fn copy_timestamps(source: &Path, dest: &Path) -> Result<(), WriteOperationError> {
    let metadata = std::fs::metadata(source).map_err(|e| WriteOperationError::IoError {
        path: source.display().to_string(),
        message: format!("Failed to read source metadata: {}", e),
    })?;

    let mtime = filetime::FileTime::from_last_modification_time(&metadata);
    let atime = filetime::FileTime::from_last_access_time(&metadata);

    filetime::set_file_times(dest, atime, mtime).map_err(|e| WriteOperationError::IoError {
        path: dest.display().to_string(),
        message: format!("Failed to set timestamps: {}", e),
    })?;

    Ok(())
}

/// Copies permissions from source to destination.
fn copy_permissions(source: &Path, dest: &Path) -> Result<(), WriteOperationError> {
    let metadata = std::fs::metadata(source).map_err(|e| WriteOperationError::IoError {
        path: source.display().to_string(),
        message: format!("Failed to read source metadata: {}", e),
    })?;

    std::fs::set_permissions(dest, metadata.permissions()).map_err(|e| WriteOperationError::IoError {
        path: dest.display().to_string(),
        message: format!("Failed to set permissions: {}", e),
    })?;

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_temp_dir(name: &str) -> std::path::PathBuf {
        let temp_dir = std::env::temp_dir().join(format!("cmdr_chunked_copy_test_{}", name));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
        temp_dir
    }

    fn cleanup_temp_dir(path: &std::path::PathBuf) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn test_is_network_filesystem_local() {
        // Local paths should return false
        assert!(!is_network_filesystem(Path::new("/")));
        assert!(!is_network_filesystem(Path::new("/tmp")));
        assert!(!is_network_filesystem(Path::new("/Users")));
    }

    #[test]
    fn test_is_network_filesystem_nonexistent() {
        // Non-existent paths should check parent
        assert!(!is_network_filesystem(Path::new("/tmp/nonexistent_file_12345")));
    }

    #[test]
    fn test_chunked_copy_basic() {
        let temp_dir = create_temp_dir("basic");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "Hello, chunked copy!").unwrap();

        let cancelled = Arc::new(AtomicBool::new(false));
        let result = chunked_copy_with_metadata(&src, &dst, &cancelled, None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 20); // "Hello, chunked copy!" is 20 bytes
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "Hello, chunked copy!");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_chunked_copy_cancellation() {
        let temp_dir = create_temp_dir("cancel");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        // Create a file larger than CHUNK_SIZE to ensure we hit the cancellation check
        let large_content = "x".repeat(CHUNK_SIZE + 1000);
        fs::write(&src, &large_content).unwrap();

        // Pre-cancelled
        let cancelled = Arc::new(AtomicBool::new(true));
        let result = chunked_copy_with_metadata(&src, &dst, &cancelled, None);

        assert!(matches!(result, Err(WriteOperationError::Cancelled { .. })));
        // Partial file should be cleaned up
        assert!(!dst.exists());

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_chunked_copy_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = create_temp_dir("perms");
        let src = temp_dir.join("source.sh");
        let dst = temp_dir.join("dest.sh");

        fs::write(&src, "#!/bin/bash").unwrap();
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();

        let cancelled = Arc::new(AtomicBool::new(false));
        let result = chunked_copy_with_metadata(&src, &dst, &cancelled, None);

        assert!(result.is_ok());
        let dst_perms = fs::metadata(&dst).unwrap().permissions().mode();
        assert_eq!(dst_perms & 0o777, 0o755);

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_chunked_copy_empty_file() {
        let temp_dir = create_temp_dir("empty");
        let src = temp_dir.join("empty.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "").unwrap();

        let cancelled = Arc::new(AtomicBool::new(false));
        let result = chunked_copy_with_metadata(&src, &dst, &cancelled, None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_chunked_copy_progress_callback() {
        use std::sync::atomic::AtomicU64;

        let temp_dir = create_temp_dir("progress");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        // Create a file larger than CHUNK_SIZE to ensure multiple callbacks
        let large_content = "x".repeat(CHUNK_SIZE * 2 + 1000);
        let expected_size = large_content.len() as u64;
        fs::write(&src, &large_content).unwrap();

        let cancelled = Arc::new(AtomicBool::new(false));
        let callback_count = Arc::new(AtomicU64::new(0));
        let last_bytes = Arc::new(AtomicU64::new(0));

        let callback_count_clone = Arc::clone(&callback_count);
        let last_bytes_clone = Arc::clone(&last_bytes);
        let progress_cb = move |bytes_done: u64, total: u64| {
            callback_count_clone.fetch_add(1, Ordering::Relaxed);
            last_bytes_clone.store(bytes_done, Ordering::Relaxed);
            assert_eq!(total, expected_size);
        };

        let result = chunked_copy_with_metadata(&src, &dst, &cancelled, Some(&progress_cb));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected_size);
        // Should have been called at least 3 times (for 3 chunks)
        assert!(callback_count.load(Ordering::Relaxed) >= 3);
        // Last callback should report all bytes
        assert_eq!(last_bytes.load(Ordering::Relaxed), expected_size);

        cleanup_temp_dir(&temp_dir);
    }
}
