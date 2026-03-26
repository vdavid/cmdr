//! Copy strategy selection for file operations.
//!
//! The only reason to use platform-native copy APIs (`copyfile(3)`, `copy_file_range(2)`) is
//! filesystem-level cloning (APFS clonefile, btrfs/XFS reflink) — instant, zero-cost copies
//! that create a copy-on-write pointer instead of copying bytes. In all other cases, our chunked
//! copy is equivalent in speed and strictly better for progress reporting and cancellation.
//!
//! Strategy (macOS):
//! - Same APFS volume → `copyfile(3)` with `COPYFILE_CLONE` for instant clonefile
//! - Everything else → chunked copy (1 MB chunks, cancellation between chunks)
//!
//! Strategy (Linux):
//! - Local, non-network → `copy_file_range(2)` (kernel handles reflink on btrfs/XFS)
//! - Network → chunked copy
//!
//! We evaluated `copyfile` on non-APFS filesystems (HFS+, exFAT, FAT32, NTFS-3G) and found no
//! practical benefit: no clonefile support, and the metadata advantages (birthtime, file flags)
//! either don't apply (exFAT/FAT32 don't store them) or aren't worth the cancellation tradeoff
//! (NTFS-3G is FUSE-based and has the same buffering issues as network mounts). See CLAUDE.md.

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

#[cfg(target_os = "linux")]
use super::linux_copy::copy_single_file_linux;
#[cfg(target_os = "macos")]
use super::macos_copy::{CopyProgressContext, copy_single_file_native};

use super::chunked_copy::ChunkedCopyProgressFn;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use super::chunked_copy::chunked_copy_with_metadata;
#[cfg(target_os = "linux")]
use super::chunked_copy::is_network_filesystem;
use super::helpers::safe_overwrite_file;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use super::types::IoResultExt;
use super::types::WriteOperationError;

// ============================================================================
// macOS: APFS clonefile detection
// ============================================================================

/// Returns true if source and dest are on the same APFS volume (clonefile is possible).
///
/// Checks two things:
/// 1. Same volume via `st_dev` (device ID from `stat`) — same approach as `is_same_filesystem`
/// 2. Filesystem type is APFS via `statfs.f_fstypename`
///
/// Handles non-existent destination paths by checking the parent directory.
#[cfg(target_os = "macos")]
fn is_same_apfs_volume(source: &Path, dest: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    // Check same volume via device ID (works even when dest doesn't exist — we check parent)
    let src_dev = match std::fs::metadata(source) {
        Ok(m) => m.dev(),
        Err(_) => return false,
    };
    let dest_check_path = if dest.exists() {
        dest.to_path_buf()
    } else {
        match dest.parent() {
            Some(p) if p.exists() => p.to_path_buf(),
            _ => return false,
        }
    };
    let dest_dev = match std::fs::metadata(&dest_check_path) {
        Ok(m) => m.dev(),
        Err(_) => return false,
    };
    if src_dev != dest_dev {
        return false;
    }

    // Same volume — now check if it's APFS (only APFS supports clonefile)
    is_apfs(source)
}

/// Returns true if the path is on an APFS volume.
#[cfg(target_os = "macos")]
fn is_apfs(path: &Path) -> bool {
    use std::ffi::CString;

    let c_path = match CString::new(path.to_string_lossy().as_bytes()) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stat) } != 0 {
        return false;
    }
    let fstype = unsafe { std::ffi::CStr::from_ptr(stat.f_fstypename.as_ptr()).to_string_lossy() };
    fstype == "apfs"
}

// ============================================================================
// Strategy selection
// ============================================================================

/// Copies file contents using the best strategy for the source/destination combination.
///
/// On macOS, uses `copyfile(3)` only for same-APFS-volume copies (APFS clonefile — instant,
/// zero-cost copy-on-write). All other cases use chunked copy for reliable cancellation and
/// progress reporting.
#[cfg(target_os = "macos")]
pub(super) fn copy_file_with_strategy(
    source: &Path,
    dest: &Path,
    needs_safe_overwrite: bool,
    cancelled: &Arc<AtomicBool>,
    progress_callback: Option<ChunkedCopyProgressFn>,
) -> Result<u64, WriteOperationError> {
    if is_same_apfs_volume(source, dest) {
        log::debug!(
            "copy_file_with_strategy: same APFS volume, using copyfile for clonefile (src={}, dest={})",
            source.display(),
            dest.display()
        );
        let context = CopyProgressContext::with_cancellation(Arc::clone(cancelled));
        if needs_safe_overwrite {
            safe_overwrite_file(source, dest, Some(&context))
        } else {
            copy_single_file_native(source, dest, false, Some(&context))
        }
    } else {
        log::debug!(
            "copy_file_with_strategy: different volumes or non-APFS, using chunked copy (src={}, dest={})",
            source.display(),
            dest.display()
        );
        chunked_copy_with_metadata(source, dest, cancelled, progress_callback)
    }
}

#[cfg(target_os = "linux")]
pub(super) fn copy_file_with_strategy(
    source: &Path,
    dest: &Path,
    needs_safe_overwrite: bool,
    cancelled: &Arc<AtomicBool>,
    progress_callback: Option<ChunkedCopyProgressFn>,
) -> Result<u64, WriteOperationError> {
    if is_network_filesystem(source) || is_network_filesystem(dest) {
        log::debug!(
            "copy_file_with_strategy: using chunked copy for network path (src={}, dest={})",
            source.display(),
            dest.display()
        );
        chunked_copy_with_metadata(source, dest, cancelled, progress_callback)
    } else if needs_safe_overwrite {
        safe_overwrite_file(source, dest)
    } else {
        copy_single_file_linux(source, dest, false, cancelled, progress_callback)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn copy_file_with_strategy(
    source: &Path,
    dest: &Path,
    needs_safe_overwrite: bool,
    cancelled: &Arc<AtomicBool>,
    progress_callback: Option<ChunkedCopyProgressFn>,
) -> Result<u64, WriteOperationError> {
    let _ = (cancelled, progress_callback); // Unused on this platform
    if needs_safe_overwrite {
        safe_overwrite_file(source, dest)
    } else {
        fs::copy(source, dest).with_path(source)
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
