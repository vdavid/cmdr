//! Validation helpers for write operations.
//!
//! Source/destination existence and type checks, the destination-inside-source
//! guard, writability and disk-space checks, same-file / same-filesystem inode
//! comparisons, path/name length limits, and symlink-loop detection.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::types::{PermissionSide, WriteOperationError};

/// An explicit transfer leaf names exactly one source and never escapes its parent.
pub(crate) fn validate_transfer_destination_name(
    sources: &[PathBuf],
    destination: &Path,
    name: Option<&str>,
) -> Result<(), WriteOperationError> {
    let Some(name) = name else { return Ok(()) };
    let named_path = destination.join(name);
    if sources.len() != 1
        || sources[0].file_name().is_none()
        || name.is_empty()
        || name.contains(['/', '\0'])
        || name == "."
        || name == ".."
    {
        return Err(WriteOperationError::InvalidName {
            path: named_path.display().to_string(),
            message: "Choose one item and a filename without path separators".to_string(),
        });
    }
    validate_path_length(&named_path)
}

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

/// Refuses a transfer whose top-level items share a name.
///
/// Two same-named sources both want `<destination>/<name>`, and neither engine
/// has an answer for that. A cross-filesystem move stages both under that one
/// name inside `.cmdr-staging-<op>/`, so the second one's children meet the
/// first one's staged files rather than an empty slot: they resolve as conflicts
/// against a copy the user never put there, and the rename phase then looks for
/// a staged tree the first source already carried away. Refusing up front is the
/// only outcome that doesn't ask the user about a clash they didn't create.
///
/// Byte-exact names only. Whether two names differing in case or normalization
/// are one file is the destination filesystem's call, not ours — the same rule
/// `DestNameIndex` follows for a fold-only match — and refusing them here would
/// block a legitimate transfer onto a case-sensitive volume.
pub(crate) fn validate_source_names_are_distinct(sources: &[PathBuf]) -> Result<(), WriteOperationError> {
    let mut seen: HashMap<&std::ffi::OsStr, &PathBuf> = HashMap::with_capacity(sources.len());
    for source in sources {
        // A path with no final component (`/`, a trailing `..`) can't be a
        // selected item; the existence check above already spoke for it.
        let Some(name) = source.file_name() else {
            continue;
        };
        if let Some(first) = seen.insert(name, source) {
            return Err(WriteOperationError::DuplicateSourceNames {
                name: name.to_string_lossy().into_owned(),
                first: first.display().to_string(),
                second: source.display().to_string(),
            });
        }
    }
    Ok(())
}

/// Ensures the destination exists as a directory, creating it (and any missing
/// ancestors) when absent. This is the local copy/move paths' destination gate:
/// a transfer into a not-yet-existing folder creates it first, matching the
/// dialog's "this folder will be created" preview. A path that exists but isn't a
/// directory is rejected (we won't transfer into a file). Symlinks are followed,
/// so a symlink to a real directory is a valid destination.
///
/// A file where the destination, or a folder above it, should be is refused as
/// [`WriteOperationError::DestinationNotAFolder`] naming that file, the same
/// answer the volume engines give. The OS says only `ENOTDIR` (or `EEXIST`) for
/// the path it was asked about, so the file is looked for after the refusal and
/// the path that works pays nothing for it.
pub(crate) fn ensure_destination_dir(destination: &Path) -> Result<(), WriteOperationError> {
    // What a refusal becomes once it's known whether a file is in the way.
    let refused = |fallback: WriteOperationError| match file_in_the_way(destination) {
        Some(in_the_way) => WriteOperationError::DestinationNotAFolder {
            path: in_the_way.display().to_string(),
        },
        None => fallback,
    };
    match fs::metadata(destination) {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(WriteOperationError::DestinationNotAFolder {
            path: destination.display().to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(destination).map_err(|e| match e.kind() {
                // The folder that refused is an ANCESTOR (the deepest one that exists),
                // never the one we failed to create, so the probe walks up to find it.
                std::io::ErrorKind::PermissionDenied => WriteOperationError::permission_denied(
                    destination.display().to_string(),
                    "Couldn't create the destination folder. Check folder permissions in Finder.".to_string(),
                    e.raw_os_error(),
                    refusing_folder(destination),
                    Some(PermissionSide::Destination),
                ),
                _ => refused(WriteOperationError::IoError {
                    path: destination.display().to_string(),
                    message: format!("Couldn't create the destination folder: {e}"),
                }),
            })
        }
        Err(e) => Err(refused(WriteOperationError::IoError {
            path: destination.display().to_string(),
            message: format!("Couldn't access the destination folder: {e}"),
        })),
    }
}

/// The nearest thing at or above `destination` that exists, when it doesn't
/// lead to a folder: the file a create tripped over. `None` when that nearest
/// thing is a folder (a link to one included), so the OS's own refusal stands.
fn file_in_the_way(destination: &Path) -> Option<PathBuf> {
    for level in destination.ancestors() {
        if level.as_os_str().is_empty() {
            break;
        }
        match fs::metadata(level) {
            Ok(meta) if meta.is_dir() => return None,
            Ok(_) => return Some(level.to_path_buf()),
            // A link to nothing holds the name without leading to a folder.
            Err(_) if fs::symlink_metadata(level).is_ok() => return Some(level.to_path_buf()),
            Err(_) => {}
        }
    }
    None
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
    let canonical_dest = canonicalize_or_nearest_ancestor(destination).map_err(|e| WriteOperationError::IoError {
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

/// Canonicalizes `path`. When the path doesn't exist yet (the legitimate case for
/// a not-yet-created destination), it walks up to the NEAREST existing ancestor,
/// canonicalizes that, and re-appends the missing trailing segments. This lets the
/// destination-inside-source guard resolve symlinks and `..` segments on a dest the
/// op is about to create, even when several levels of it don't exist yet. Any
/// non-NotFound I/O error propagates so the caller can fail closed.
fn canonicalize_or_nearest_ancestor(path: &Path) -> std::io::Result<PathBuf> {
    match path.canonicalize() {
        Ok(canonical) => return Ok(canonical),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }

    let mut missing_tail: Vec<std::ffi::OsString> = Vec::new();
    let mut current = path;
    loop {
        // Refuse to fall back on a trailing `..` / `.` / empty segment.
        let name = current
            .file_name()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        missing_tail.push(name.to_os_string());

        let parent = current
            .parent()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        match parent.canonicalize() {
            Ok(canonical_parent) => {
                let mut result = canonical_parent;
                for seg in missing_tail.iter().rev() {
                    result.push(seg);
                }
                return Ok(result);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                current = parent;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Asks the OS whether `folder` takes writes from us, answering with the errno when
/// it doesn't and `None` when it does.
///
/// `access(W_OK)` evaluates ACLs as well as the mode bits on macOS, so it's the honest
/// question rather than a mode-bit guess. The one place that asks it: the destination
/// pre-flight below, and `error_classification::refusing_folder` on the refusal path.
///
/// ❗ It's a syscall, so on a hung network mount it blocks like every other one
/// (`file_system/CLAUDE.md`). `None` also for a path the OS can't be handed at all (an
/// interior NUL, which no path the OS produced has); the write itself then refuses it.
#[cfg(unix)]
pub(crate) fn folder_write_refusal(folder: &Path) -> Option<i32> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(folder.as_os_str().as_bytes()).ok()?;
    // SAFETY: `c_path` is a valid NUL-terminated C string that outlives the call.
    let refused = unsafe { libc::access(c_path.as_ptr(), libc::W_OK) } != 0;
    refused.then(|| std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EACCES))
}

/// The folder whose permissions refuse a write, for a refusal that happened at `path`.
///
/// A `rename(2)` needs write access to BOTH parent folders, a `unlink(2)` to the one it
/// removes from, and a `create_dir_all` to the deepest ancestor that exists; no errno
/// says which of them refused. So instead of inferring one from the operation's shape,
/// we ask the OS. In order: the folder the entry lives in, since its write bit is what
/// governs creating, removing, and renaming an entry, walking up past ancestors that
/// don't exist yet; then the entry itself when it's a folder, since writing INTO one
/// needs its own write bit.
///
/// ❗ `None` when every candidate answers "writable": an ACL the probe and the write
/// read differently, a race, or a read refusal rather than a write one. The message
/// then stays the generic one. ❌ Never name a folder this couldn't prove — a refused
/// move telling the user to check the destination when the SOURCE folder said no is
/// exactly the report (ERR-4TEMD) this exists for.
///
/// On the REFUSAL path only, so the happy path pays nothing: at most two `access(2)`
/// calls plus the walk's stats, on a path the failing syscall just answered for.
#[cfg(unix)]
pub(crate) fn refusing_folder(path: &Path) -> Option<String> {
    let mut candidate = path.parent();
    while let Some(dir) = candidate {
        if dir.as_os_str().is_empty() {
            break;
        }
        if dir.exists() {
            if folder_write_refusal(dir).is_some() {
                return Some(dir.display().to_string());
            }
            break;
        }
        candidate = dir.parent();
    }
    if path.is_dir() && folder_write_refusal(path).is_some() {
        return Some(path.display().to_string());
    }
    None
}

#[cfg(not(unix))]
pub(crate) fn refusing_folder(_path: &Path) -> Option<String> {
    None
}

/// Checks whether the destination directory is writable using access(W_OK).
#[cfg(unix)]
pub(crate) fn validate_destination_writable(destination: &Path) -> Result<(), WriteOperationError> {
    let Some(errno) = folder_write_refusal(destination) else {
        return Ok(());
    };
    // The probe IS the proof here, so the folder is named with no second guess.
    Err(WriteOperationError::permission_denied(
        destination.display().to_string(),
        "Destination folder is not writable. Check folder permissions in Finder.".to_string(),
        Some(errno),
        Some(destination.display().to_string()),
        Some(PermissionSide::Destination),
    ))
}

#[cfg(not(unix))]
pub(crate) fn validate_destination_writable(_destination: &Path) -> Result<(), WriteOperationError> {
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

/// Is `path` a directory in its own right, rather than a symlink pointing at
/// one? This is the ONE question a move asks before merging two directories.
///
/// `Path::is_dir()` is `fs::metadata`-based, so it says `true` for a link to a
/// directory. A merge that believes it walks `read_dir` through the link and
/// renames the TARGET's entries out of a folder the user never selected; a
/// destination-side link is the mirror image, landing the user's files wherever
/// it points. So a link is an opaque leaf to every move engine: it's renamed as
/// a link, and a link meeting a directory is a type mismatch the conflict
/// resolver decides. `transfer/DETAILS.md` § "Symlinks are opaque to a move".
pub(crate) fn is_real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).map(|m| m.is_dir()).unwrap_or(false)
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
mod refusing_folder_tests {
    //! What ERR-4TEMD needed and didn't get: a refusal that names the folder that
    //! actually said no.
    //!
    //! The user moved two `root:admin` files out of a folder their macOS user can't
    //! change. Both ends of the `rename(2)` are candidates and the errno names
    //! neither, so Cmdr's copy blamed the destination, which was fine, and the user
    //! finished the job with `sudo`. Every case here is about proving the folder
    //! rather than inferring it.
    use super::*;
    use crate::file_system::write_operations::types::PermissionRefusal;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    /// Whether this process can be refused by a folder's permission bits at all.
    ///
    /// Root isn't: it bypasses the write bit, so the `0o555` folder below still
    /// takes writes and `refusing_folder` correctly finds nothing to name. Every
    /// test here that needs a folder to REALLY refuse therefore returns early
    /// under root, mirroring the guard `validation_integration_test.rs` uses.
    ///
    /// ❗ The Linux lane runs as root in Docker, so on that lane these four tests
    /// prove nothing and pass. That's the honest answer — the alternative is a
    /// red lane asserting something the environment can't express — but it does
    /// mean macOS is where this behavior is actually covered. Don't "simplify"
    /// the guard away to make the tests look universal.
    fn a_folder_can_refuse_us() -> bool {
        // SAFETY: (test) `geteuid` takes no arguments, shares no memory, and can't fail — it
        // just returns the caller's effective uid. We compare the returned integer to 0.
        unsafe { libc::geteuid() != 0 }
    }

    /// A folder nobody can write into, and the file sitting in it.
    fn locked_folder_with_file(temp: &TempDir) -> (PathBuf, PathBuf) {
        let folder = temp.path().join("locked");
        fs::create_dir(&folder).expect("create the folder");
        let file = folder.join("script.js");
        fs::write(&file, b"contents").expect("write the file");
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).expect("lock the folder");
        (folder, file)
    }

    /// The report's own shape: the refusal happened on the FILE, and the folder it
    /// lives in is what refuses.
    #[test]
    fn the_folder_an_entry_lives_in_is_named() {
        if !a_folder_can_refuse_us() {
            return;
        }
        let temp = TempDir::new().expect("tempdir");
        let (folder, file) = locked_folder_with_file(&temp);

        let named = refusing_folder(&file);

        assert_eq!(named.as_deref(), Some(folder.display().to_string().as_str()));
    }

    /// A folder that takes writes is never named, so a refusal we can't explain keeps
    /// the generic sentence instead of pointing at an innocent folder.
    #[test]
    fn a_writable_folder_is_never_named() {
        let temp = TempDir::new().expect("tempdir");
        let file = temp.path().join("ordinary.txt");
        fs::write(&file, b"contents").expect("write the file");

        assert_eq!(refusing_folder(&file), None);
    }

    /// Writing INTO a folder needs its own write bit, so a refusal ON a folder names
    /// that folder and not its (writable) parent.
    #[test]
    fn a_folder_that_takes_no_writes_itself_is_named() {
        if !a_folder_can_refuse_us() {
            return;
        }
        let temp = TempDir::new().expect("tempdir");
        let (folder, _file) = locked_folder_with_file(&temp);

        let named = refusing_folder(&folder);

        assert_eq!(named.as_deref(), Some(folder.display().to_string().as_str()));
    }

    /// A refused `create_dir_all` is about the deepest ancestor that EXISTS, never the
    /// folder it couldn't create, so the walk skips the ones that aren't there yet.
    #[test]
    fn a_folder_that_cannot_be_created_names_the_deepest_ancestor_that_exists() {
        if !a_folder_can_refuse_us() {
            return;
        }
        let temp = TempDir::new().expect("tempdir");
        let (folder, _file) = locked_folder_with_file(&temp);
        let wanted = folder.join("new").join("deeper");

        let named = refusing_folder(&wanted);

        assert_eq!(named.as_deref(), Some(folder.display().to_string().as_str()));
    }

    /// `classify_io_error` is where the probe has to fire: every local-FS refusal
    /// reaches the typed variant through it.
    #[test]
    fn a_classified_refusal_carries_the_errno_and_the_folder() {
        if !a_folder_can_refuse_us() {
            return;
        }
        let temp = TempDir::new().expect("tempdir");
        let (folder, file) = locked_folder_with_file(&temp);

        let err = super::super::error_classification::classify_io_error(
            &std::io::Error::from_raw_os_error(libc::EACCES),
            file.display().to_string(),
        );

        let WriteOperationError::PermissionDenied {
            errno,
            refusal,
            refused_folder,
            ..
        } = err
        else {
            panic!("expected a permission refusal, got {err:?}");
        };
        assert_eq!(errno, Some(libc::EACCES));
        assert_eq!(refusal, PermissionRefusal::FolderPermissions);
        assert_eq!(refused_folder.as_deref(), Some(folder.display().to_string().as_str()));
    }

    /// `EACCES` and `EPERM` are opposite advice, and Rust folds both into one
    /// `ErrorKind`: an administrator can write into a folder whose permissions refuse,
    /// and can't touch what the OS itself protects.
    #[test]
    fn the_errno_decides_whether_administrator_rights_would_help() {
        assert_eq!(
            PermissionRefusal::from_errno(Some(libc::EACCES)),
            PermissionRefusal::FolderPermissions
        );
        assert_eq!(
            PermissionRefusal::from_errno(Some(libc::EPERM)),
            PermissionRefusal::SystemProtected
        );
        assert_eq!(PermissionRefusal::from_errno(None), PermissionRefusal::Unclassified);
        assert_eq!(
            PermissionRefusal::from_errno(Some(libc::EROFS)),
            PermissionRefusal::Unclassified
        );
    }
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
