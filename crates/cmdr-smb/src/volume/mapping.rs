//! Pure type-mapping helpers: smb2 types -> Volume types, and smb2 errors
//! -> `VolumeError`. No shared state; the cleanest extraction.

use cmdr_fs::entry::FileEntry;
use cmdr_fs::metadata::UserFlags;
use cmdr_fs::volume::{SpaceInfo, VolumeError};
use smb2::FileAttributes;

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

    // `is_link` is mount points and symlinks (name surrogates) only: a dedup
    // file or cloud placeholder carries the reparse bit too, and as a "link" a
    // delete would refuse its folder and a copy would skip its contents.
    let mut fe = FileEntry::new(entry.name.clone(), path, entry.is_directory, entry.is_link());
    fe.size = if entry.is_directory { None } else { Some(entry.size) };
    fe.modified_at = filetime_to_unix_secs(entry.modified);
    fe.created_at = filetime_to_unix_secs(entry.created);
    fe
}

/// Converts a `stat`'s `smb2::FileInfo` to a `FileEntry` named `name` at the
/// display path `path`. smb2's `stat` describes a link itself, never its
/// target, so a link reads as one, the answer `Volume::entry_kind` needs.
pub(super) fn file_info_to_file_entry(info: &smb2::client::tree::FileInfo, name: String, path: String) -> FileEntry {
    let mut fe = FileEntry::new(name, path, info.is_directory, info.is_link());
    fe.size = if info.is_directory { None } else { Some(info.size) };
    fe.modified_at = filetime_to_unix_secs(info.modified);
    fe.created_at = filetime_to_unix_secs(info.created);
    fe
}

/// The user flags a copy carries off an SMB item named `name`: Locked from
/// `READONLY`, Hidden from `HIDDEN`, except on a dot-name. Samba reports every
/// dotfile as Hidden (`hide dot files`, on by default), and its name already
/// hides it everywhere; a local copy that also wore `UF_HIDDEN` would stay
/// hidden after a rename to a plain name, which nobody asked for.
pub(super) fn user_flags_of(attributes: FileAttributes, name: &str) -> UserFlags {
    let mut flags = UserFlags::NONE;
    if attributes.is_hidden() && !name.starts_with('.') {
        flags = flags | UserFlags::HIDDEN;
    }
    if attributes.is_readonly() {
        flags = flags | UserFlags::LOCKED;
    }
    flags
}

/// The DOS attributes that stand for `flags` on a share: `HIDDEN` and
/// `READONLY` (Finder shows the latter as Locked through a mount). Append-only
/// and no-dump have no SMB attribute and drop. Empty means "none of them",
/// which on the wire CLEARS the settable set, so a caller sends it only when it
/// means that.
pub(super) fn attributes_of(flags: UserFlags) -> FileAttributes {
    let mut bits = 0;
    if flags.contains(UserFlags::HIDDEN) {
        bits |= FileAttributes::HIDDEN;
    }
    if flags.contains(UserFlags::LOCKED) {
        bits |= FileAttributes::READONLY;
    }
    FileAttributes::new(bits)
}

/// Converts an `smb2::FsInfo` to `SpaceInfo`. A share always has a size, so it
/// is always [`SpaceInfo::Bounded`].
pub(super) fn fs_info_to_space_info(info: &smb2::client::tree::FsInfo) -> SpaceInfo {
    SpaceInfo::bounded(info.total_bytes, info.free_bytes)
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
    // A Read-only item refusing its own removal: a permission refusal like
    // local disk's `EPERM` on a Locked file, which is what lets a reversal
    // clear it and try again (`Volume::clear_blockers`). smb2 files it under
    // `ErrorKind::Other`.
    if err.status() == Some(NtStatus::CANNOT_DELETE) {
        return VolumeError::PermissionDenied {
            path: path.to_string(),
            raw_os_error: None,
        };
    }

    match err.kind() {
        ErrorKind::NotFound => VolumeError::NotFound(path.to_string()),
        ErrorKind::AlreadyExists => VolumeError::AlreadyExists(path.to_string()),
        ErrorKind::IsADirectory => VolumeError::IsADirectory(path.to_string()),
        ErrorKind::AccessDenied
        | ErrorKind::AuthRequired
        | ErrorKind::SigningRequired
        | ErrorKind::GuestSessionRefused => VolumeError::PermissionDenied {
            path: path.to_string(),
            raw_os_error: None,
        },
        ErrorKind::ConnectionLost | ErrorKind::SessionExpired => {
            VolumeError::DeviceDisconnected(err.to_string().into())
        }
        ErrorKind::TimedOut => VolumeError::ConnectionTimeout(err.to_string().into()),
        ErrorKind::DiskFull => VolumeError::StorageFull {
            message: err.to_string().into(),
        },
        ErrorKind::Cancelled => VolumeError::Cancelled("Operation cancelled by user".into()),
        // The server refused the NAME, not the operation, so it never looked for
        // the file and retrying the same name can only fail again. smb2 already
        // maps the characters SMB2 forbids outright into the private-use area, so
        // what lands here is a reserved device name, a name past the server's
        // length limit, or a character its filesystem can't store — all of which
        // the user fixes by renaming, and none of which a retry helps.
        ErrorKind::InvalidName => VolumeError::InvalidName(err.to_string().into()),
        // Typed, never an `IoError`: a refused reply isn't a transport blip to retry.
        ErrorKind::SignatureInvalid => VolumeError::SignatureInvalid(err.to_string().into()),
        _ => VolumeError::IoError {
            raw_os_error: local_errno(&err),
            message: err.to_string().into(),
        },
    }
}

/// The OS error number of a LOCAL failure (smb2's `Io`, which on an established
/// connection never means the connection was lost, since smb2 0.32.0), so the UI
/// can say what went wrong. Read off the typed `io::Error`, ❌ never parsed from
/// a message. `SendFailed` carries one too, but it maps to `DeviceDisconnected`
/// by kind and its source is for the log only.
fn local_errno(err: &smb2::Error) -> Option<i32> {
    match err {
        smb2::Error::Io(io) => io.raw_os_error(),
        _ => None,
    }
}

#[cfg(test)]
#[path = "mapping_test.rs"]
mod mapping_test;
