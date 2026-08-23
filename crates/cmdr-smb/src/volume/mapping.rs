//! Pure type-mapping helpers: smb2 types -> Volume types, and smb2 errors
//! -> `VolumeError`. No shared state; the cleanest extraction.

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{SpaceInfo, VolumeError};

/// Converts an `smb2::FileTime` to seconds since the Unix epoch, matching
/// `FileEntry.modified_at` / `created_at` (seconds, like `LocalPosixVolume`).
pub(super) fn filetime_to_unix_secs(ft: smb2::pack::FileTime) -> Option<u64> {
    let st = ft.to_system_time()?;
    let dur = st.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(dur.as_secs())
}

/// Converts an `smb2::DirectoryEntry` to a `FileEntry`.
///
/// `parent_path` is the absolute path of the parent directory (under the mount point).
pub(super) fn directory_entry_to_file_entry(
    entry: &smb2::client::tree::DirectoryEntry,
    parent_path: &str,
) -> FileEntry {
    let path = if parent_path.ends_with('/') {
        format!("{}{}", parent_path, entry.name)
    } else {
        format!("{}/{}", parent_path, entry.name)
    };

    let mut fe = FileEntry::new(entry.name.clone(), path, entry.is_directory, false);
    fe.size = if entry.is_directory { None } else { Some(entry.size) };
    fe.modified_at = filetime_to_unix_secs(entry.modified);
    fe.created_at = filetime_to_unix_secs(entry.created);
    fe
}

/// Converts an `smb2::FsInfo` to `SpaceInfo`.
pub(super) fn fs_info_to_space_info(info: &smb2::client::tree::FsInfo) -> SpaceInfo {
    let used = info.total_bytes.saturating_sub(info.free_bytes);
    SpaceInfo {
        total_bytes: info.total_bytes,
        available_bytes: info.free_bytes,
        used_bytes: used,
    }
}

/// Converts an `smb2::Error` to `VolumeError`, for an operation on `path`.
///
/// ❗ **`path` is the payload, not context.** [`VolumeError::NotFound`],
/// [`VolumeError::PermissionDenied`], [`VolumeError::AlreadyExists`],
/// [`VolumeError::IsADirectory`], and [`VolumeError::DeletePending`] are defined
/// to carry the path (`cmdr-fs/src/volume/types.rs`), and the transfer layer takes
/// that literally: `map_volume_error` forwards the string straight into
/// `SourceNotFound { path }`, which the frontend renders as the name of the file
/// the user is missing. Putting the NTSTATUS sentence there instead reads to a
/// user as a filename that was never on their share. The wording isn't lost, it
/// goes to the log via `handle_smb_result`. `assert_not_found_carries_the_path`
/// holds every backend to it.
///
/// `path` is the app-addressable display path (`to_display_path`), matching what
/// every other backend carries and what the pane shows.
pub(super) fn map_smb_error(err: smb2::Error, path: &str) -> VolumeError {
    use smb2::ErrorKind;
    use smb2::types::status::NtStatus;

    // `STATUS_DELETE_PENDING` currently classifies as `ErrorKind::Other` in
    // smb2 (no typed variant yet), so we detect it via the raw NTSTATUS before
    // falling through to the generic kind match.
    if err.status() == Some(NtStatus::DELETE_PENDING) {
        return VolumeError::DeletePending(path.to_string());
    }

    match err.kind() {
        ErrorKind::NotFound => VolumeError::NotFound(path.to_string()),
        ErrorKind::AlreadyExists => VolumeError::AlreadyExists(path.to_string()),
        ErrorKind::IsADirectory => VolumeError::IsADirectory(path.to_string()),
        ErrorKind::AccessDenied | ErrorKind::AuthRequired | ErrorKind::SigningRequired => {
            VolumeError::PermissionDenied(path.to_string())
        }
        ErrorKind::ConnectionLost | ErrorKind::SessionExpired => VolumeError::DeviceDisconnected(err.to_string()),
        ErrorKind::TimedOut => VolumeError::ConnectionTimeout(err.to_string()),
        ErrorKind::DiskFull => VolumeError::StorageFull {
            message: err.to_string(),
        },
        ErrorKind::Cancelled => VolumeError::Cancelled("Operation cancelled by user".to_string()),
        // The server refused the NAME, not the operation, so it never looked for
        // the file and retrying the same name can only fail again. smb2 already
        // maps the characters SMB2 forbids outright into the private-use area, so
        // what lands here is a reserved device name, a name past the server's
        // length limit, or a character its filesystem can't store — all of which
        // the user fixes by renaming, and none of which a retry helps.
        ErrorKind::InvalidName => VolumeError::InvalidName(err.to_string()),
        _ => VolumeError::IoError {
            message: err.to_string(),
            raw_os_error: None,
        },
    }
}

#[cfg(test)]
#[path = "mapping_test.rs"]
mod mapping_test;
