//! Pure type-mapping helpers: smb2 types -> Volume types, and smb2 errors
//! -> `VolumeError`. No shared state; the cleanest extraction.

use super::*;

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

/// Converts an `smb2::Error` to `VolumeError`.
pub(super) fn map_smb_error(err: smb2::Error) -> VolumeError {
    use smb2::ErrorKind;
    use smb2::types::status::NtStatus;

    // `STATUS_DELETE_PENDING` currently classifies as `ErrorKind::Other` in
    // smb2 (no typed variant yet), so we detect it via the raw NTSTATUS before
    // falling through to the generic kind match.
    if err.status() == Some(NtStatus::DELETE_PENDING) {
        return VolumeError::DeletePending(err.to_string());
    }

    match err.kind() {
        ErrorKind::NotFound => VolumeError::NotFound(err.to_string()),
        ErrorKind::AlreadyExists => VolumeError::AlreadyExists(err.to_string()),
        ErrorKind::IsADirectory => VolumeError::IsADirectory(err.to_string()),
        ErrorKind::AccessDenied | ErrorKind::AuthRequired | ErrorKind::SigningRequired => {
            VolumeError::PermissionDenied(err.to_string())
        }
        ErrorKind::ConnectionLost | ErrorKind::SessionExpired => VolumeError::DeviceDisconnected(err.to_string()),
        ErrorKind::TimedOut => VolumeError::ConnectionTimeout(err.to_string()),
        ErrorKind::DiskFull => VolumeError::StorageFull {
            message: err.to_string(),
        },
        ErrorKind::Cancelled => VolumeError::Cancelled("Operation cancelled by user".to_string()),
        _ => VolumeError::IoError {
            message: err.to_string(),
            raw_os_error: None,
        },
    }
}
