//! Validation helpers for write operations.
//!
//! Source/destination existence and type checks, same-location and
//! destination-inside-source guards, writability and disk-space checks,
//! same-file / same-filesystem inode comparisons, path/name length limits,
//! and symlink-loop detection.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::types::WriteOperationError;

pub(crate) fn validate_sources(sources: &[PathBuf]) -> Result<(), WriteOperationError> {
    for source in sources {
        // Use symlink_metadata to check existence without following symlinks
        if fs::symlink_metadata(source).is_err() {
            return Err(WriteOperationError::SourceNotFound {
                path: source.display().to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn validate_destination(destination: &Path) -> Result<(), WriteOperationError> {
    // Destination must exist and be a directory
    if !destination.exists() {
        return Err(WriteOperationError::SourceNotFound {
            path: destination.display().to_string(),
        });
    }
    if !destination.is_dir() {
        return Err(WriteOperationError::IoError {
            path: destination.display().to_string(),
            message: "Destination must be a directory".to_string(),
        });
    }
    Ok(())
}

pub(crate) fn validate_not_same_location(sources: &[PathBuf], destination: &Path) -> Result<(), WriteOperationError> {
    for source in sources {
        if let Some(parent) = source.parent()
            && parent == destination
        {
            return Err(WriteOperationError::SameLocation {
                path: source.display().to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn validate_destination_not_inside_source(
    sources: &[PathBuf],
    destination: &Path,
) -> Result<(), WriteOperationError> {
    // Canonicalize destination to resolve symlinks and ".." segments that could
    // bypass a naive starts_with check (like /foo/bar/../foo/sub → /foo/sub).
    //
    // Pre-fix this used `unwrap_or_else(|_| destination.to_path_buf())` for
    // both paths, silently degrading the guard to a naive `starts_with` on
    // raw inputs whenever canonicalize failed. That's the data-safety bug —
    // a `dest` that lexically doesn't start with `source` but canonically
    // does (symlink shenanigans) would pass the check and the copy would
    // recurse into itself until disk-full. Fail closed instead.
    let canonical_dest = canonicalize_or_parent(destination).map_err(|e| WriteOperationError::IoError {
        path: destination.display().to_string(),
        message: format!("Couldn't resolve destination path: {e}"),
    })?;

    for source in sources {
        if source.is_dir() {
            let canonical_source = source.canonicalize().map_err(|e| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: format!("Couldn't resolve source path: {e}"),
            })?;
            if canonical_dest.starts_with(&canonical_source) {
                return Err(WriteOperationError::DestinationInsideSource {
                    source: source.display().to_string(),
                    destination: destination.display().to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Canonicalizes `path`, falling back to canonicalizing its parent and
/// re-appending the trailing segment when the path doesn't exist yet (the
/// only legitimate case for `canonicalize` to fail on the destination during
/// a write op). Any other I/O error propagates so the caller can fail closed.
fn canonicalize_or_parent(path: &Path) -> std::io::Result<PathBuf> {
    match path.canonicalize() {
        Ok(canonical) => Ok(canonical),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or(e)?;
            let canonical_parent = parent.canonicalize()?;
            match path.file_name() {
                Some(name) => Ok(canonical_parent.join(name)),
                // Path was just `..` / `.` / empty — refuse to fall back.
                None => Ok(canonical_parent),
            }
        }
        Err(e) => Err(e),
    }
}

/// Checks whether the destination directory is writable using access(W_OK).
#[cfg(unix)]
pub(crate) fn validate_destination_writable(destination: &Path) -> Result<(), WriteOperationError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(destination.as_os_str().as_bytes()).map_err(|_| WriteOperationError::IoError {
        path: destination.display().to_string(),
        message: "Invalid path".to_string(),
    })?;

    // SAFETY: c_path is a valid null-terminated C string
    let result = unsafe { libc::access(c_path.as_ptr(), libc::W_OK) };
    if result != 0 {
        return Err(WriteOperationError::PermissionDenied {
            path: destination.display().to_string(),
            message: "Destination folder is not writable. Check folder permissions in Finder.".to_string(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn validate_destination_writable(_destination: &Path) -> Result<(), WriteOperationError> {
    Ok(())
}

/// Checks available disk space on the destination volume against required bytes.
///
/// On macOS, uses `NSURLVolumeAvailableCapacityForImportantUsageKey` which includes purgeable
/// space (APFS snapshots, iCloud caches), matching what Finder reports. Falls back to `statvfs`
/// if the NSURL query fails. On Linux, uses `statvfs` directly (no purgeable space concept).
#[cfg(unix)]
pub(crate) fn validate_disk_space(destination: &Path, required_bytes: u64) -> Result<(), WriteOperationError> {
    let available = get_available_space(destination).unwrap_or({
        // Cannot determine space. Return u64::MAX so the check passes and we let the OS
        // report ENOSPC if it actually happens during the copy.
        u64::MAX
    });

    if required_bytes > available {
        let volume_name = destination
            .ancestors()
            .find(|p| p.parent().is_some_and(|pp| pp == Path::new("/Volumes")))
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());

        return Err(WriteOperationError::InsufficientSpace {
            required: required_bytes,
            available,
            volume_name,
        });
    }

    Ok(())
}

/// Returns available bytes for a path, using the best API for the platform.
///
/// macOS: `NSURLVolumeAvailableCapacityForImportantUsageKey` (includes purgeable space).
/// Linux: `statvfs` `f_bavail * f_frsize`.
#[cfg(unix)]
fn get_available_space(path: &Path) -> Option<u64> {
    // On macOS, prefer the NSURL API that accounts for purgeable space.
    #[cfg(target_os = "macos")]
    {
        if let Some(space) = crate::volumes::get_volume_space(&path.to_string_lossy()) {
            return Some(space.available_bytes);
        }
    }

    // Fallback (and Linux primary path): statvfs
    get_available_space_statvfs(path)
}

/// Returns available bytes using `statvfs`. Used as the primary method on Linux and as a
/// fallback on macOS.
#[cfg(unix)]
fn get_available_space_statvfs(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: c_path is a valid null-terminated C string, stat is a valid pointer
    let result = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    // SAFETY: statvfs succeeded, stat is initialized
    let stat = unsafe { stat.assume_init() };
    #[allow(
        clippy::unnecessary_cast,
        reason = "Required for macOS where statvfs fields are not u64"
    )]
    Some(stat.f_bavail as u64 * stat.f_frsize as u64)
}

#[cfg(not(unix))]
pub(crate) fn validate_disk_space(_destination: &Path, _required_bytes: u64) -> Result<(), WriteOperationError> {
    Ok(())
}

/// Maximum number of offending files to name in the error (the rest are
/// summarized as a count). Keeps the dialog readable on a tree of many big files.
const MAX_OVERSIZED_FILES_TO_REPORT: usize = 10;

/// Blocks the operation when any scanned file exceeds the destination
/// filesystem's per-file size limit (FAT32's 4 GiB cap). All-or-nothing: returns
/// the first such failure carrying up to [`MAX_OVERSIZED_FILES_TO_REPORT`]
/// offenders (largest first) plus the true total count.
///
/// A no-op for any destination without a known cap — the common case (APFS,
/// exFAT, NTFS, ext4, ...) and anything we can't classify — so it never raises a
/// false alarm. Run after the scan and before the first byte is written, so a
/// 5 GB file buried under one of several selected folders is caught up front
/// instead of failing the copy partway through.
pub(crate) fn validate_file_sizes_for_filesystem(
    destination: &Path,
    files: &[super::state::FileInfo],
) -> Result<(), WriteOperationError> {
    use crate::file_system::filesystem_kind::{MaxFileSize, detect_filesystem_for_path};

    let filesystem = detect_filesystem_for_path(destination);
    let MaxFileSize::Limited { bytes: max_size } = filesystem.kind.max_file_size() else {
        return Ok(());
    };

    let mut offenders: Vec<&super::state::FileInfo> = files.iter().filter(|f| f.size > max_size).collect();
    if offenders.is_empty() {
        return Ok(());
    }
    // Largest first, so the dialog leads with the worst offender.
    offenders.sort_by_key(|f| std::cmp::Reverse(f.size));

    let total_count = offenders.len();
    let reported = offenders
        .iter()
        .take(MAX_OVERSIZED_FILES_TO_REPORT)
        .map(|f| super::types::OversizedFile {
            name: f
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            size: f.size,
        })
        .collect();

    Err(WriteOperationError::FilesTooLargeForFilesystem {
        filesystem: filesystem.kind,
        max_size,
        files: reported,
        total_count,
    })
}

/// Checks if source and destination resolve to the same file (same inode + device).
/// This prevents data loss when copying a file over itself via a symlink.
#[cfg(unix)]
pub(crate) fn is_same_file(source: &Path, destination: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    let src_meta = match fs::metadata(source) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let dst_meta = match fs::metadata(destination) {
        Ok(m) => m,
        Err(_) => return false,
    };

    src_meta.dev() == dst_meta.dev() && src_meta.ino() == dst_meta.ino()
}

#[cfg(not(unix))]
pub(crate) fn is_same_file(_source: &Path, _destination: &Path) -> bool {
    false
}

/// Returns `true` when `path` already names something we should treat as a
/// conflict — including dangling symlinks.
///
/// `Path::exists()` follows symlinks: it returns `false` for a symlink whose
/// target is missing. Using it alone for the "does the destination exist?"
/// gate lets a dangling symlink slip past conflict resolution; the subsequent
/// write then follows the symlink and either clobbers wherever it points or
/// surfaces a confusing `ENOENT` from the target's parent. Pair it with
/// `symlink_metadata` so the gate fires for symlinks (broken or not).
pub(crate) fn path_exists_or_is_symlink(path: &Path) -> bool {
    path.exists() || fs::symlink_metadata(path).is_ok()
}

// ============================================================================
// Path length validation
// ============================================================================

/// Maximum file name length in bytes (APFS/HFS+ limit)
const MAX_NAME_BYTES: usize = 255;
/// Maximum path length in bytes (macOS PATH_MAX)
const MAX_PATH_BYTES: usize = 1024;

/// Validates that a destination path doesn't exceed filesystem name/path length limits.
pub(crate) fn validate_path_length(dest_path: &Path) -> Result<(), WriteOperationError> {
    // Check total path length
    let path_str = dest_path.as_os_str();
    if path_str.len() > MAX_PATH_BYTES {
        return Err(WriteOperationError::IoError {
            path: dest_path.display().to_string(),
            message: format!("Path exceeds maximum length of {} bytes", MAX_PATH_BYTES),
        });
    }

    // Check file name component length
    if let Some(name) = dest_path.file_name()
        && name.len() > MAX_NAME_BYTES
    {
        return Err(WriteOperationError::IoError {
            path: dest_path.display().to_string(),
            message: format!("File name exceeds maximum length of {} bytes", MAX_NAME_BYTES),
        });
    }

    Ok(())
}

// ============================================================================
// Symlink loop detection
// ============================================================================

/// Checks if a path creates a symlink loop.
pub(super) fn is_symlink_loop(path: &Path, visited: &HashSet<PathBuf>) -> bool {
    if let Ok(canonical) = path.canonicalize() {
        visited.contains(&canonical)
    } else {
        false
    }
}

// ============================================================================
// Filesystem detection
// ============================================================================

/// Checks if two paths are on the same filesystem using device IDs.
#[cfg(unix)]
pub(crate) fn is_same_filesystem(source: &Path, destination: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let source_meta = fs::metadata(source)?;
    let dest_meta = fs::metadata(destination)?;

    Ok(source_meta.dev() == dest_meta.dev())
}

#[cfg(not(unix))]
pub(crate) fn is_same_filesystem(_source: &Path, _destination: &Path) -> std::io::Result<bool> {
    // On non-Unix, assume different filesystem to be safe (will use copy+delete)
    Ok(false)
}

#[cfg(all(test, unix))]
mod path_exists_or_is_symlink_tests {
    //! Regression for the medium-severity audit finding: the regular-file
    //! copy branch (and both move-op branches) used `Path::exists()` for
    //! conflict detection, which follows symlinks and returns `false` for
    //! a dangling symlink at the destination — the copy then followed the
    //! symlink and silently clobbered (or failed mid-batch with a confusing
    //! ENOENT against the target's parent).
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    #[test]
    fn flags_dangling_symlink_at_destination() {
        let temp = TempDir::new().unwrap();
        let dest = temp.path().join("notes.txt");
        // Symlink target intentionally never exists.
        symlink(temp.path().join("missing-target"), &dest).unwrap();

        // `Path::exists()` is the pre-fix gate — must return false for a
        // dangling symlink (this is the trap).
        assert!(!dest.exists(), "exists() must NOT see a dangling symlink");
        // Our helper closes the trap.
        assert!(
            path_exists_or_is_symlink(&dest),
            "dangling symlink must be treated as an existing destination"
        );
    }

    #[test]
    fn flags_live_symlink_and_regular_paths() {
        let temp = TempDir::new().unwrap();
        let real = temp.path().join("real.txt");
        fs::write(&real, b"data").unwrap();
        let link = temp.path().join("link.txt");
        symlink(&real, &link).unwrap();

        assert!(path_exists_or_is_symlink(&real));
        assert!(path_exists_or_is_symlink(&link));
    }

    #[test]
    fn returns_false_for_missing_path() {
        let temp = TempDir::new().unwrap();
        assert!(!path_exists_or_is_symlink(&temp.path().join("absent")));
    }
}
