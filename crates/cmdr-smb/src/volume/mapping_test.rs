//! Turning smb2's types and errors into Cmdr's.
//!
//! No session and no server: every case here is a pure function over a value the
//! protocol handed us.

use super::*;
use cmdr_fs::volume::EntryKind;

#[test]
fn filetime_to_unix_secs_known_date() {
    // 2024-01-01 00:00:00 UTC = FileTime(133_485_408_000_000_000)
    let ft = smb2::pack::FileTime(133_485_408_000_000_000);
    let secs = filetime_to_unix_secs(ft).unwrap();
    assert_eq!(secs, 1_704_067_200);
}

#[test]
fn filetime_to_unix_secs_zero_returns_none() {
    let ft = smb2::pack::FileTime::ZERO;
    assert!(filetime_to_unix_secs(ft).is_none());
}

#[test]
fn directory_entry_to_file_entry_file() {
    // `DirectoryEntry` is `#[non_exhaustive]`, so a fixture starts from `default()`.
    let mut entry = smb2::client::tree::DirectoryEntry::default();
    entry.name = "report.pdf".to_string();
    entry.size = 1024;
    entry.created = smb2::pack::FileTime(133_485_408_000_000_000);
    entry.modified = smb2::pack::FileTime(133_485_408_000_000_000);

    let fe = directory_entry_to_file_entry(&entry, "/Volumes/Share/Documents");
    assert_eq!(fe.name, "report.pdf");
    assert_eq!(fe.path, "/Volumes/Share/Documents/report.pdf");
    assert!(!fe.is_directory);
    assert!(!fe.is_symlink);
    assert_eq!(fe.size, Some(1024));
    assert_eq!(fe.modified_at, Some(1_704_067_200));
    assert_eq!(fe.created_at, Some(1_704_067_200));
    assert_eq!(fe.icon_id, "ext:pdf");
}

#[test]
fn directory_entry_to_file_entry_directory() {
    let mut entry = smb2::client::tree::DirectoryEntry::default();
    entry.name = "Photos".to_string();
    entry.is_directory = true;

    let fe = directory_entry_to_file_entry(&entry, "/Volumes/Share");
    assert_eq!(fe.name, "Photos");
    assert_eq!(fe.path, "/Volumes/Share/Photos");
    assert!(fe.is_directory);
    // An untagged folder is a real one, so every walk descends it.
    assert!(!fe.is_symlink);
    assert_eq!(fe.size, None);
    assert_eq!(fe.modified_at, None);
    assert_eq!(fe.icon_id, "dir");
}

/// A listed folder carrying `reparse_tag`, with the `REPARSE_POINT` bit beside it.
fn tagged_folder(tag: smb2::ReparseTag) -> smb2::client::tree::DirectoryEntry {
    let mut entry = smb2::client::tree::DirectoryEntry::default();
    entry.name = "Games".to_string();
    entry.is_directory = true;
    entry.attributes = FileAttributes::new(FileAttributes::DIRECTORY | FileAttributes::REPARSE_POINT);
    entry.reparse_tag = Some(tag);
    entry
}

#[test]
fn a_junction_maps_to_a_link_so_no_walk_descends_into_its_target() {
    // A junction lists as a folder; a delete that walked into it deleted the
    // folder it points to, outside the selection (#516).
    let fe = directory_entry_to_file_entry(&tagged_folder(smb2::ReparseTag::MOUNT_POINT), "/Volumes/Share");
    assert!(fe.is_symlink);
    assert!(
        fe.is_directory,
        "a link to a folder still lists as one, like a local symlink"
    );
    assert_eq!(EntryKind::of(&fe), EntryKind::Symlink);
}

#[test]
fn a_windows_symlink_maps_to_a_link() {
    let fe = directory_entry_to_file_entry(&tagged_folder(smb2::ReparseTag::SYMLINK), "/Volumes/Share");
    assert!(fe.is_symlink);
    assert_eq!(EntryKind::of(&fe), EntryKind::Symlink);
}

#[test]
fn dedup_and_cloud_placeholders_stay_ordinary_entries() {
    // They carry the reparse bit too, but they are real files and folders: as
    // links, a delete would refuse their folder and a copy would skip its contents.
    for tag in [
        smb2::ReparseTag::DEDUP,
        smb2::ReparseTag::CLOUD,
        smb2::ReparseTag(0x9000_101A),
    ] {
        let fe = directory_entry_to_file_entry(&tagged_folder(tag), "/Volumes/Share");
        assert!(!fe.is_symlink, "{tag:?} must not map to a link");
        assert_eq!(EntryKind::of(&fe), EntryKind::Directory, "{tag:?}");
    }
}

#[test]
fn a_stat_of_a_link_reports_the_link() {
    let mut info = smb2::client::tree::FileInfo::default();
    info.is_directory = true;
    info.reparse_tag = Some(smb2::ReparseTag::MOUNT_POINT);
    let fe = file_info_to_file_entry(&info, "Games".to_string(), "/Volumes/Share/Games".to_string());
    assert!(fe.is_symlink);
    assert!(fe.is_directory);

    info.reparse_tag = Some(smb2::ReparseTag::CLOUD);
    let fe = file_info_to_file_entry(&info, "Games".to_string(), "/Volumes/Share/Games".to_string());
    assert!(!fe.is_symlink);

    info.reparse_tag = None;
    let fe = file_info_to_file_entry(&info, "Games".to_string(), "/Volumes/Share/Games".to_string());
    assert!(!fe.is_symlink);
}

#[test]
fn fs_info_to_space_info_conversion() {
    let info = smb2::client::tree::FsInfo {
        total_bytes: 1_000_000_000,
        free_bytes: 400_000_000,
        total_free_bytes: 400_000_000,
        bytes_per_sector: 512,
        sectors_per_unit: 8,
    };

    let space = fs_info_to_space_info(&info);
    assert_eq!(
        space,
        SpaceInfo::Bounded {
            total_bytes: 1_000_000_000,
            available_bytes: 400_000_000,
            used_bytes: 600_000_000,
        }
    );
}

#[test]
fn map_smb_error_not_found() {
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::OBJECT_NAME_NOT_FOUND,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(
        matches!(&ve, VolumeError::NotFound(path) if path == "/Volumes/share/holiday.raw"),
        "NotFound must carry the path the frontend renders as the file's name, got: {ve:?}",
    );
}

#[test]
fn map_smb_error_delete_pending() {
    // STATUS_DELETE_PENDING surfaces when a delete has been requested but at
    // least one open handle is keeping the file alive. smb2 currently classifies
    // it as `ErrorKind::Other`, so `map_smb_error` must dispatch on the raw
    // NTSTATUS to produce the typed `VolumeError::DeletePending` variant —
    // otherwise the FE falls back to the generic "disk needs attention" copy
    // instead of the transient "file is being removed" message.
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::DELETE_PENDING,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(
        matches!(&ve, VolumeError::DeletePending(path) if path == "/Volumes/share/holiday.raw"),
        "STATUS_DELETE_PENDING should map to VolumeError::DeletePending, got: {:?}",
        ve,
    );
}

#[test]
fn map_smb_error_invalid_name() {
    // STATUS_OBJECT_NAME_INVALID means the server can't hold this name at all, so
    // the copy can only succeed under a different one. smb2 ≥ 0.18 maps the
    // characters SMB2 forbids outright into the private-use area, so what's left
    // here is a reserved device name, a name past the server's length limit, or a
    // character its filesystem can't store. It has to reach the FE as its own
    // typed variant: as a generic IoError the dialog offers a pointless retry and
    // never tells the user that renaming is the fix.
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::OBJECT_NAME_INVALID,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(
        matches!(ve, VolumeError::InvalidName(_)),
        "STATUS_OBJECT_NAME_INVALID should map to VolumeError::InvalidName, got: {:?}",
        ve,
    );
}

#[test]
fn map_smb_error_access_denied() {
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::ACCESS_DENIED,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(
        matches!(&ve, VolumeError::PermissionDenied { path, .. } if path == "/Volumes/share/holiday.raw"),
        "PermissionDenied must carry the path, got: {ve:?}",
    );
}

#[test]
fn map_smb_error_disconnected() {
    let err = smb2::Error::Disconnected;
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(matches!(ve, VolumeError::DeviceDisconnected(_)));
}

#[test]
fn map_smb_error_timeout() {
    let err = smb2::Error::Timeout;
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(matches!(ve, VolumeError::ConnectionTimeout(_)));
}

/// A dead DFS link is a refusal of that path alone: the share's own connection is
/// fine, so it carries the path and reads as nothing that ends the session
/// (vdavid/smb2#28). As a lost connection, one dead link marked the share gone.
#[test]
fn map_smb_error_dead_link_target_refuses_the_path_and_leaves_the_session() {
    let err = smb2::Error::DfsTargetUnreachable {
        path: "data/holiday.raw".to_string(),
        target_count: 2,
        source: Box::new(smb2::Error::Disconnected),
    };
    assert!(!super::super::state::ends_the_session(&err));
    let ve = map_smb_error(err, "/Volumes/share/data/holiday.raw");
    match ve {
        VolumeError::LinkTargetUnreachable(path) => assert_eq!(path, "/Volumes/share/data/holiday.raw"),
        other => panic!("got: {other:?}"),
    }
}

/// A link nobody allowed refuses its path with both servers named, so a pane can
/// ask, and reads as nothing that ends the session.
#[test]
fn map_smb_error_link_without_consent_refuses_the_path_and_leaves_the_session() {
    let err = smb2::Error::DfsTargetNotAllowed {
        path: "data/a".to_string(),
        server: "nas".to_string(),
        target: "fs01".to_string(),
    };
    assert!(!super::super::state::ends_the_session(&err));
    match map_smb_error(err, "/Volumes/share/data/a") {
        VolumeError::LinkNeedsConsent(sign_in) => {
            assert_eq!(sign_in.path, "/Volumes/share/data/a");
            assert_eq!(sign_in.server, "nas");
            assert_eq!(sign_in.target, "fs01");
        }
        other => panic!("got: {other:?}"),
    }
}

/// A target name that could spoof the prompt gets no prompt at all.
#[test]
fn map_smb_error_unreadable_link_target_is_a_plain_refusal() {
    let err = smb2::Error::DfsTargetNotAllowed {
        path: "data/a".to_string(),
        server: "nas".to_string(),
        target: "fs01\u{202E}evil".to_string(),
    };
    assert!(matches!(
        map_smb_error(err, "/Volumes/share/data/a"),
        VolumeError::PermissionDenied { .. }
    ));
}

/// A tree or handle from a session that has ended reads as a lost connection:
/// that's what sends the volume through its one reconnect path and lets the
/// transfer layer retry the file on the rebuilt session, both of which heal it.
/// As an `IoError` it would reach the user as a failed file with nothing to retry.
#[test]
fn map_smb_error_handle_invalidated_is_a_lost_connection() {
    let err = smb2::Error::HandleInvalidated {
        opened_on: 1,
        current: 2,
    };
    assert_eq!(err.kind(), smb2::ErrorKind::ConnectionLost);
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(matches!(ve, VolumeError::DeviceDisconnected(_)), "got: {ve:?}");
}

#[test]
fn map_smb_error_disk_full() {
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::DISK_FULL,
        command: smb2::types::Command::Write,
    };
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(matches!(ve, VolumeError::StorageFull { .. }));
}

#[test]
fn map_smb_error_session_expired() {
    let err = smb2::Error::SessionExpired;
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(matches!(ve, VolumeError::DeviceDisconnected(_)));
}

#[test]
fn map_smb_error_auth_required() {
    let err = smb2::Error::Auth {
        message: "Authentication failed".to_string(),
    };
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(
        matches!(&ve, VolumeError::PermissionDenied { path, .. } if path == "/Volumes/share/holiday.raw"),
        "PermissionDenied must carry the path, got: {ve:?}",
    );
}

#[test]
fn map_smb_error_io() {
    let err = smb2::Error::Io(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broke"));
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    // IO errors (callback errors, etc.) are not connection losses; they map to IoError.
    // Real connection losses come through `Disconnected` / `SendFailed` → ConnectionLost.
    assert!(matches!(ve, VolumeError::IoError { .. }));
}

/// A local I/O failure keeps its errno: it's what tells the UI what went wrong
/// (here, a permission refusal), and `None` would hide it.
#[test]
fn a_local_io_failure_keeps_its_errno() {
    const EACCES: i32 = 13;
    let err = smb2::Error::Io(std::io::Error::from_raw_os_error(EACCES));
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(
        matches!(
            ve,
            VolumeError::IoError {
                raw_os_error: Some(EACCES),
                ..
            }
        ),
        "got: {ve:?}"
    );
}

/// The request whose own send broke the connection (smb2 0.32.0's `SendFailed`)
/// reads as a lost device, like every other waiter's `Disconnected`, so the
/// transfer layer's outage wait and retry see it.
#[test]
fn a_failed_send_is_a_lost_connection() {
    const EPIPE: i32 = 32;
    let err = smb2::Error::SendFailed {
        command: smb2::types::Command::Write,
        source: std::io::Error::from_raw_os_error(EPIPE),
    };
    assert_eq!(err.kind(), smb2::ErrorKind::ConnectionLost);
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(matches!(ve, VolumeError::DeviceDisconnected(_)), "got: {ve:?}");
}

#[test]
fn map_smb_error_already_exists() {
    // STATUS_OBJECT_NAME_COLLISION (returned by Create when the name exists) must
    // surface as AlreadyExists so the `volume::strategy` merge-directory path can
    // swallow it instead of bubbling a generic IO error to the user.
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::OBJECT_NAME_COLLISION,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(
        matches!(&ve, VolumeError::AlreadyExists(path) if path == "/Volumes/share/holiday.raw"),
        "AlreadyExists must carry the path, got: {ve:?}",
    );
}

#[test]
fn map_smb_error_file_is_a_directory() {
    // STATUS_FILE_IS_A_DIRECTORY is returned when delete_file is called on a dir.
    // smb2 0.8.0 exposes this as the typed `ErrorKind::IsADirectory` variant, so
    // `map_smb_error` surfaces it as `VolumeError::IsADirectory`; the delete
    // fast-path matches on that to decide whether to retry with delete_directory.
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::FILE_IS_A_DIRECTORY,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(
        matches!(&ve, VolumeError::IsADirectory(path) if path == "/Volumes/share/holiday.raw"),
        "IsADirectory must carry the path, got: {ve:?}",
    );
}

#[test]
fn map_smb_error_access_denied_is_not_misclassified() {
    // Non-directory errors must not be classified as IsADirectory.
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::ACCESS_DENIED,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err, "/Volumes/share/holiday.raw");
    assert!(
        matches!(&ve, VolumeError::PermissionDenied { path, .. } if path == "/Volumes/share/holiday.raw"),
        "PermissionDenied must carry the path, got: {ve:?}",
    );
}

/// A Read-only item refusing its own removal is a permission refusal, which is
/// what a reversal clears and retries on.
#[test]
fn map_smb_error_cannot_delete_is_a_permission_refusal() {
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::CANNOT_DELETE,
        command: smb2::types::Command::SetInfo,
    };
    assert!(matches!(
        map_smb_error(err, "/Volumes/Share/locked.txt"),
        VolumeError::PermissionDenied { path, .. } if path == "/Volumes/Share/locked.txt"
    ));
}

#[test]
fn user_flags_of_maps_hidden_and_read_only() {
    let both = FileAttributes::new(FileAttributes::HIDDEN | FileAttributes::READONLY | FileAttributes::ARCHIVE);
    assert_eq!(user_flags_of(both, "notes.txt"), UserFlags::HIDDEN | UserFlags::LOCKED);
    assert_eq!(
        user_flags_of(FileAttributes::new(FileAttributes::ARCHIVE), "notes.txt"),
        UserFlags::NONE
    );
}

/// Samba marks every dotfile Hidden; its name already hides it, so a copy
/// doesn't add a flag on top.
#[test]
fn a_dot_names_hidden_is_its_name_not_a_flag() {
    let both = FileAttributes::new(FileAttributes::HIDDEN | FileAttributes::READONLY);
    assert_eq!(user_flags_of(both, ".profile"), UserFlags::LOCKED);
}

#[test]
fn attributes_of_maps_the_flags_an_smb_item_can_wear() {
    assert_eq!(
        attributes_of(UserFlags::HIDDEN | UserFlags::LOCKED | UserFlags::NO_DUMP),
        FileAttributes::new(FileAttributes::HIDDEN | FileAttributes::READONLY)
    );
    assert_eq!(
        attributes_of(UserFlags::APPEND_ONLY | UserFlags::NO_DUMP).bits(),
        0,
        "no SMB attribute stands for these"
    );
}

#[test]
fn a_refused_signature_maps_to_its_typed_volume_error() {
    let err = smb2::Error::SignatureInvalid {
        command: smb2::types::Command::Read,
        message_id: smb2::types::MessageId(42),
        reason: smb2::error::SignatureFailure::Mismatch,
    };
    assert!(matches!(
        map_smb_error(err, "/share/a.txt"),
        VolumeError::SignatureInvalid(_)
    ));
}

#[test]
fn a_refused_guest_session_maps_like_a_wrong_password() {
    let err = smb2::Error::GuestSessionRefused {
        username: "david".to_string(),
        anonymous: true,
    };
    assert!(matches!(
        map_smb_error(err, "/share"),
        VolumeError::PermissionDenied { raw_os_error: None, .. }
    ));
}
