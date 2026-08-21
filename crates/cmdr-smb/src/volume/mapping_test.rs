//! Turning smb2's types and errors into Cmdr's.
//!
//! No session and no server: every case here is a pure function over a value the
//! protocol handed us.

use super::*;

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
    let entry = smb2::client::tree::DirectoryEntry {
        name: "report.pdf".to_string(),
        size: 1024,
        is_directory: false,
        created: smb2::pack::FileTime(133_485_408_000_000_000),
        modified: smb2::pack::FileTime(133_485_408_000_000_000),
    };

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
    let entry = smb2::client::tree::DirectoryEntry {
        name: "Photos".to_string(),
        size: 0,
        is_directory: true,
        created: smb2::pack::FileTime::ZERO,
        modified: smb2::pack::FileTime::ZERO,
    };

    let fe = directory_entry_to_file_entry(&entry, "/Volumes/Share");
    assert_eq!(fe.name, "Photos");
    assert_eq!(fe.path, "/Volumes/Share/Photos");
    assert!(fe.is_directory);
    assert_eq!(fe.size, None);
    assert_eq!(fe.modified_at, None);
    assert_eq!(fe.icon_id, "dir");
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
    assert_eq!(space.total_bytes, 1_000_000_000);
    assert_eq!(space.available_bytes, 400_000_000);
    assert_eq!(space.used_bytes, 600_000_000);
}

#[test]
fn map_smb_error_not_found() {
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::OBJECT_NAME_NOT_FOUND,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::NotFound(_)));
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
    let ve = map_smb_error(err);
    assert!(
        matches!(ve, VolumeError::DeletePending(_)),
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
    let ve = map_smb_error(err);
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
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::PermissionDenied(_)));
}

#[test]
fn map_smb_error_disconnected() {
    let err = smb2::Error::Disconnected;
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::DeviceDisconnected(_)));
}

#[test]
fn map_smb_error_timeout() {
    let err = smb2::Error::Timeout;
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::ConnectionTimeout(_)));
}

#[test]
fn map_smb_error_disk_full() {
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::DISK_FULL,
        command: smb2::types::Command::Write,
    };
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::StorageFull { .. }));
}

#[test]
fn map_smb_error_session_expired() {
    let err = smb2::Error::SessionExpired;
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::DeviceDisconnected(_)));
}

#[test]
fn map_smb_error_auth_required() {
    let err = smb2::Error::Auth {
        message: "Authentication failed".to_string(),
    };
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::PermissionDenied(_)));
}

#[test]
fn map_smb_error_io() {
    let err = smb2::Error::Io(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broke"));
    let ve = map_smb_error(err);
    // IO errors (callback errors, etc.) are not connection losses; they map to IoError.
    // Real connection losses come through Error::Disconnected → ConnectionLost.
    assert!(matches!(ve, VolumeError::IoError { .. }));
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
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::AlreadyExists(_)));
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
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::IsADirectory(_)));
}

#[test]
fn map_smb_error_access_denied_is_not_misclassified() {
    // Non-directory errors must not be classified as IsADirectory.
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::ACCESS_DENIED,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::PermissionDenied(_)));
}
