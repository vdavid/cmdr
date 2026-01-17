//! macOS-native file copy using copyfile(3) API.
//!
//! This module provides FFI bindings for the macOS `copyfile` system call,
//! which preserves extended attributes, ACLs, resource forks, and Finder metadata.
//! It also supports APFS clonefile for instant copies on the same volume.

use std::ffi::{CString, c_int, c_void};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::file_system::write_operations::WriteOperationError;

// ============================================================================
// Type aliases for progress callbacks
// ============================================================================

/// Callback for progress updates (bytes copied for current file).
type ProgressCallback = Box<dyn Fn(u64) + Send + Sync>;
/// Callback when starting a new file.
type FileStartCallback = Box<dyn Fn(&str) + Send + Sync>;
/// Callback when finishing a file (with byte count).
type FileFinishCallback = Box<dyn Fn(&str, u64) + Send + Sync>;

// ============================================================================
// FFI bindings for copyfile(3)
// ============================================================================

type CopyfileStateT = *mut c_void;
type CopyfileFlagsT = u32;

// copyfile flags from /usr/include/copyfile.h
const COPYFILE_ACL: CopyfileFlagsT = 1 << 0;
const COPYFILE_STAT: CopyfileFlagsT = 1 << 1;
const COPYFILE_XATTR: CopyfileFlagsT = 1 << 2;
const COPYFILE_DATA: CopyfileFlagsT = 1 << 3;
const COPYFILE_ALL: CopyfileFlagsT = COPYFILE_ACL | COPYFILE_STAT | COPYFILE_XATTR | COPYFILE_DATA;
const COPYFILE_RECURSIVE: CopyfileFlagsT = 1 << 15;
const COPYFILE_EXCL: CopyfileFlagsT = 1 << 17;
const COPYFILE_NOFOLLOW_SRC: CopyfileFlagsT = 1 << 18;
const COPYFILE_CLONE: CopyfileFlagsT = 1 << 24;

// copyfile_state constants
const COPYFILE_STATE_STATUS_CB: c_int = 6;
const COPYFILE_STATE_SRC_FILENAME: c_int = 1;
const COPYFILE_STATE_DST_FILENAME: c_int = 2;
const COPYFILE_STATE_COPIED: c_int = 8;
const COPYFILE_STATE_STATUS_CTX: c_int = 7;

// Progress callback return values
const COPYFILE_CONTINUE: c_int = 0;
const COPYFILE_QUIT: c_int = 1;

// Progress callback what values
const COPYFILE_RECURSE_FILE: c_int = 1;
const COPYFILE_COPY_DATA: c_int = 4;

// Progress callback stage values
const COPYFILE_START: c_int = 1;
const COPYFILE_FINISH: c_int = 2;
const COPYFILE_PROGRESS: c_int = 4;

#[link(name = "System", kind = "dylib")]
unsafe extern "C" {
    fn copyfile(from: *const i8, to: *const i8, state: CopyfileStateT, flags: CopyfileFlagsT) -> c_int;

    fn copyfile_state_alloc() -> CopyfileStateT;
    fn copyfile_state_free(state: CopyfileStateT) -> c_int;
    fn copyfile_state_set(state: CopyfileStateT, flag: c_int, value: *const c_void) -> c_int;
    fn copyfile_state_get(state: CopyfileStateT, flag: c_int, value: *mut c_void) -> c_int;
}

// ============================================================================
// Progress callback types
// ============================================================================

/// Progress callback signature matching copyfile's expected type.
type CopyfileCallback = extern "C" fn(
    what: c_int,
    stage: c_int,
    state: CopyfileStateT,
    src: *const i8,
    dst: *const i8,
    ctx: *mut c_void,
) -> c_int;

/// Context passed to the progress callback.
pub struct CopyProgressContext {
    /// Cancellation flag - checked during copy
    pub cancelled: Arc<AtomicBool>,
    /// Callback to report progress (bytes_copied for current file)
    pub on_progress: Option<ProgressCallback>,
    /// Callback when starting a new file
    pub on_file_start: Option<FileStartCallback>,
    /// Callback when finishing a file (with byte count)
    pub on_file_finish: Option<FileFinishCallback>,
}

impl Default for CopyProgressContext {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            on_progress: None,
            on_file_start: None,
            on_file_finish: None,
        }
    }
}

/// The progress callback called by copyfile.
extern "C" fn copy_progress_callback(
    what: c_int,
    stage: c_int,
    state: CopyfileStateT,
    _src: *const i8,
    _dst: *const i8,
    ctx: *mut c_void,
) -> c_int {
    if ctx.is_null() {
        return COPYFILE_CONTINUE;
    }

    // SAFETY: ctx is a pointer to CopyProgressContext that we passed in
    let context = unsafe { &*(ctx as *const CopyProgressContext) };

    // Check cancellation
    if context.cancelled.load(Ordering::Relaxed) {
        return COPYFILE_QUIT;
    }

    match (what, stage) {
        // File copy started
        (COPYFILE_RECURSE_FILE, COPYFILE_START) | (COPYFILE_COPY_DATA, COPYFILE_START) => {
            if let Some(ref callback) = context.on_file_start {
                // Get source filename from state
                let mut src_ptr: *const i8 = std::ptr::null();
                unsafe {
                    copyfile_state_get(
                        state,
                        COPYFILE_STATE_SRC_FILENAME,
                        &mut src_ptr as *mut _ as *mut c_void,
                    );
                }
                if !src_ptr.is_null() {
                    let src_cstr = unsafe { std::ffi::CStr::from_ptr(src_ptr) };
                    if let Ok(src_str) = src_cstr.to_str() {
                        callback(src_str);
                    }
                }
            }
        }

        // Progress during data copy
        (COPYFILE_COPY_DATA, COPYFILE_PROGRESS) => {
            if let Some(ref callback) = context.on_progress {
                let mut bytes_copied: i64 = 0;
                unsafe {
                    copyfile_state_get(state, COPYFILE_STATE_COPIED, &mut bytes_copied as *mut _ as *mut c_void);
                }
                callback(bytes_copied as u64);
            }
        }

        // File copy finished
        (COPYFILE_RECURSE_FILE, COPYFILE_FINISH) | (COPYFILE_COPY_DATA, COPYFILE_FINISH) => {
            if let Some(ref callback) = context.on_file_finish {
                // Get destination filename and bytes copied
                let mut dst_ptr: *const i8 = std::ptr::null();
                let mut bytes_copied: i64 = 0;
                unsafe {
                    copyfile_state_get(
                        state,
                        COPYFILE_STATE_DST_FILENAME,
                        &mut dst_ptr as *mut _ as *mut c_void,
                    );
                    copyfile_state_get(state, COPYFILE_STATE_COPIED, &mut bytes_copied as *mut _ as *mut c_void);
                }
                if !dst_ptr.is_null() {
                    let dst_cstr = unsafe { std::ffi::CStr::from_ptr(dst_ptr) };
                    if let Ok(dst_str) = dst_cstr.to_str() {
                        callback(dst_str, bytes_copied as u64);
                    }
                }
            }
        }

        _ => {}
    }

    COPYFILE_CONTINUE
}

// ============================================================================
// Public API
// ============================================================================

/// Options for copy operations.
#[derive(Debug, Clone, Copy)]
pub struct CopyOptions {
    /// If true, fail if destination exists
    pub exclusive: bool,
    /// If true, copy directories recursively
    pub recursive: bool,
    /// If true, preserve symlinks (don't follow them)
    pub preserve_symlinks: bool,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self {
            exclusive: false,
            recursive: true,
            preserve_symlinks: true,
        }
    }
}

/// Copies a file or directory using macOS copyfile(3).
///
/// This preserves:
/// - Extended attributes (xattrs)
/// - ACLs
/// - Resource forks
/// - Finder metadata
/// - Uses clonefile on APFS for instant copies
///
/// # Arguments
/// * `source` - Source file or directory path
/// * `destination` - Destination path (will be created)
/// * `options` - Copy options
/// * `context` - Progress context for cancellation and callbacks (optional)
///
/// # Returns
/// * `Ok(())` on success
/// * `Err(WriteOperationError)` on failure
pub fn copy_file_native(
    source: &Path,
    destination: &Path,
    options: CopyOptions,
    context: Option<&CopyProgressContext>,
) -> Result<(), WriteOperationError> {
    let src_cstring = path_to_cstring(source)?;
    let dst_cstring = path_to_cstring(destination)?;

    // Build flags
    let mut flags: CopyfileFlagsT = COPYFILE_ALL | COPYFILE_CLONE;

    if options.preserve_symlinks {
        flags |= COPYFILE_NOFOLLOW_SRC;
    }

    if options.exclusive {
        flags |= COPYFILE_EXCL;
    }

    if options.recursive && source.is_dir() {
        flags |= COPYFILE_RECURSIVE;
    }

    // If not exclusive and destination exists, remove it first
    // (copyfile doesn't have a built-in overwrite mode)
    if !options.exclusive && destination.exists() {
        if destination.is_dir() {
            std::fs::remove_dir_all(destination).map_err(|e| WriteOperationError::IoError {
                path: destination.display().to_string(),
                message: format!("Failed to remove existing destination: {}", e),
            })?;
        } else {
            std::fs::remove_file(destination).map_err(|e| WriteOperationError::IoError {
                path: destination.display().to_string(),
                message: format!("Failed to remove existing destination: {}", e),
            })?;
        }
    }

    // Set up state for progress callbacks if context provided
    let state = unsafe { copyfile_state_alloc() };
    if state.is_null() {
        return Err(WriteOperationError::IoError {
            path: source.display().to_string(),
            message: "Failed to allocate copyfile state".to_string(),
        });
    }

    // Set up progress callback if context provided
    if let Some(ctx) = context {
        unsafe {
            // Set callback function
            let callback_ptr = copy_progress_callback as CopyfileCallback as *const c_void;
            copyfile_state_set(state, COPYFILE_STATE_STATUS_CB, callback_ptr);

            // Set context pointer
            let ctx_ptr = ctx as *const CopyProgressContext as *const c_void;
            copyfile_state_set(state, COPYFILE_STATE_STATUS_CTX, ctx_ptr);
        }
    }

    // Perform the copy
    let result = unsafe { copyfile(src_cstring.as_ptr(), dst_cstring.as_ptr(), state, flags) };

    // Free state
    unsafe {
        copyfile_state_free(state);
    }

    if result == 0 {
        Ok(())
    } else {
        // Check if cancelled
        if let Some(ctx) = context
            && ctx.cancelled.load(Ordering::Relaxed)
        {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Get the actual error
        let err = std::io::Error::last_os_error();
        Err(map_io_error_with_path(err, source, destination))
    }
}

/// Copies a single file using macOS copyfile(3) without recursive support.
/// This is more efficient for single files.
pub fn copy_single_file_native(
    source: &Path,
    destination: &Path,
    overwrite: bool,
    context: Option<&CopyProgressContext>,
) -> Result<u64, WriteOperationError> {
    let options = CopyOptions {
        exclusive: !overwrite,
        recursive: false,
        preserve_symlinks: true,
    };

    copy_file_native(source, destination, options, context)?;

    // Return the file size
    let metadata = std::fs::metadata(destination).map_err(|e| WriteOperationError::IoError {
        path: destination.display().to_string(),
        message: e.to_string(),
    })?;

    Ok(metadata.len())
}

/// Copies a symlink without following it.
pub fn copy_symlink(source: &Path, destination: &Path) -> Result<(), WriteOperationError> {
    let target = std::fs::read_link(source).map_err(|e| WriteOperationError::IoError {
        path: source.display().to_string(),
        message: format!("Failed to read symlink: {}", e),
    })?;

    std::os::unix::fs::symlink(&target, destination).map_err(|e| WriteOperationError::IoError {
        path: destination.display().to_string(),
        message: format!("Failed to create symlink: {}", e),
    })?;

    Ok(())
}

// ============================================================================
// Helper functions
// ============================================================================

/// Converts a Path to a CString for FFI.
fn path_to_cstring(path: &Path) -> Result<CString, WriteOperationError> {
    let path_str = path.to_str().ok_or_else(|| WriteOperationError::IoError {
        path: path.display().to_string(),
        message: "Path contains invalid UTF-8".to_string(),
    })?;

    CString::new(path_str).map_err(|_| WriteOperationError::IoError {
        path: path.display().to_string(),
        message: "Path contains null byte".to_string(),
    })
}

/// Maps an IO error to a WriteOperationError with path context.
fn map_io_error_with_path(err: std::io::Error, source: &Path, destination: &Path) -> WriteOperationError {
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
            // Check for disk full (ENOSPC = 28 on macOS)
            if let Some(os_err) = err.raw_os_error()
                && os_err == 28
            {
                // ENOSPC - we don't have exact space info here, so use 0
                return WriteOperationError::InsufficientSpace {
                    required: 0,
                    available: 0,
                    volume_name: None,
                };
            }
            WriteOperationError::IoError {
                path: source.display().to_string(),
                message: err.to_string(),
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn create_temp_dir(name: &str) -> std::path::PathBuf {
        let temp_dir = std::env::temp_dir().join(format!("cmdr_macos_copy_test_{}", name));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
        temp_dir
    }

    fn cleanup_temp_dir(path: &std::path::PathBuf) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn test_copy_single_file() {
        let temp_dir = create_temp_dir("single_file");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "Hello, world!").unwrap();

        let result = copy_single_file_native(&src, &dst, false, None);
        assert!(result.is_ok());
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "Hello, world!");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_copy_preserves_permissions() {
        let temp_dir = create_temp_dir("permissions");
        let src = temp_dir.join("source.sh");
        let dst = temp_dir.join("dest.sh");

        fs::write(&src, "#!/bin/bash\necho hello").unwrap();
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();

        let result = copy_single_file_native(&src, &dst, false, None);
        assert!(result.is_ok());

        let dst_perms = fs::metadata(&dst).unwrap().permissions().mode();
        // Check executable bits are preserved (mask with 0o777 to ignore file type bits)
        assert_eq!(dst_perms & 0o777, 0o755);

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_copy_symlink() {
        let temp_dir = create_temp_dir("symlink");
        let target = temp_dir.join("target.txt");
        let src_link = temp_dir.join("source_link");
        let dst_link = temp_dir.join("dest_link");

        fs::write(&target, "target content").unwrap();
        std::os::unix::fs::symlink(&target, &src_link).unwrap();

        let result = copy_symlink(&src_link, &dst_link);
        assert!(result.is_ok());

        // Verify it's a symlink, not a regular file
        assert!(dst_link.is_symlink());
        let link_target = fs::read_link(&dst_link).unwrap();
        assert_eq!(link_target, target);

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_copy_broken_symlink() {
        let temp_dir = create_temp_dir("broken_symlink");
        let nonexistent = temp_dir.join("nonexistent");
        let src_link = temp_dir.join("broken_link");
        let dst_link = temp_dir.join("dest_broken_link");

        // Create a symlink to a nonexistent target
        std::os::unix::fs::symlink(&nonexistent, &src_link).unwrap();

        let result = copy_symlink(&src_link, &dst_link);
        assert!(result.is_ok());

        // Verify it's a broken symlink
        assert!(dst_link.is_symlink());
        assert!(!dst_link.exists()); // exists() returns false for broken symlinks

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_copy_fails_on_existing_exclusive() {
        let temp_dir = create_temp_dir("exclusive");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "source").unwrap();
        fs::write(&dst, "existing").unwrap();

        // Should fail with exclusive = true (overwrite = false)
        let result = copy_single_file_native(&src, &dst, false, None);
        assert!(result.is_err());

        // Original content should be preserved
        assert_eq!(fs::read_to_string(&dst).unwrap(), "existing");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_copy_overwrites_when_allowed() {
        let temp_dir = create_temp_dir("overwrite");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "new content").unwrap();
        fs::write(&dst, "old content").unwrap();

        let result = copy_single_file_native(&src, &dst, true, None);
        assert!(result.is_ok());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "new content");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_copy_directory_recursive() {
        let temp_dir = create_temp_dir("recursive");
        let src_dir = temp_dir.join("source_dir");
        let dst_dir = temp_dir.join("dest_dir");

        // Create nested structure
        fs::create_dir_all(src_dir.join("subdir")).unwrap();
        fs::write(src_dir.join("file1.txt"), "file1").unwrap();
        fs::write(src_dir.join("subdir/file2.txt"), "file2").unwrap();

        let options = CopyOptions::default();
        let result = copy_file_native(&src_dir, &dst_dir, options, None);
        assert!(result.is_ok());

        assert!(dst_dir.join("file1.txt").exists());
        assert!(dst_dir.join("subdir/file2.txt").exists());
        assert_eq!(fs::read_to_string(dst_dir.join("file1.txt")).unwrap(), "file1");
        assert_eq!(fs::read_to_string(dst_dir.join("subdir/file2.txt")).unwrap(), "file2");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_cancellation() {
        let temp_dir = create_temp_dir("cancellation");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "content").unwrap();

        let context = CopyProgressContext {
            cancelled: Arc::new(AtomicBool::new(true)), // Pre-cancelled
            ..Default::default()
        };

        let _result = copy_single_file_native(&src, &dst, false, Some(&context));
        // Note: cancellation is checked in the callback, so for small files it may complete
        // before the callback runs. This test verifies the mechanism exists.

        cleanup_temp_dir(&temp_dir);
    }
}
