//! Tests for `transfer_error.rs`: the one place a `VolumeError` becomes the
//! typed `WriteOperationError` the FE renders from.
//!
//! Most of what's asserted here is the ROLE, not the errno. `NotFound` carries
//! no side, so `PathRole` is the only thing separating "your file is gone" from
//! "there was nowhere to put it"; getting it backwards sends someone hunting
//! for data loss that never happened.
//!
//! A `#[path]` child of `transfer_error`, so `super::` here is `transfer_error`
//! and `super::super::` is `volume` (the one-level-shallower rule every
//! `*_tests.rs` in this directory follows). The end-to-end counterpart, where a
//! real copy into an unaddressable destination must still name the destination,
//! lives with the copy engine in `copy_tests/destination.rs`.

use super::*;
use crate::types::PermissionRefusal;

#[test]
fn test_map_volume_error_not_found() {
    let err = map_volume_error(
        "/ctx",
        PathRole::Source,
        VolumeError::NotFound("/test/path".to_string()),
    );
    assert!(matches!(err, WriteOperationError::SourceNotFound { path } if path == "/test/path"));
}

/// ERR-P7XKX through the mapping: a local destination's `EROFS`, classified where
/// it happened, reaches the dialog as a read-only DESTINATION, not a generic
/// failure. `ENOSPC` is the same shape with a different fix.
#[cfg(unix)]
#[test]
fn a_local_destination_refusing_the_write_names_why() {
    let refused = |errno| {
        VolumeError::from_io_at(
            &std::io::Error::from_raw_os_error(errno),
            "/Volumes/Installer/.cmdr-tmp-1",
        )
    };

    let read_only = map_volume_error("/naspi/shortcut.lnk", PathRole::Destination, refused(libc::EROFS));
    assert!(
        matches!(
            read_only,
            WriteOperationError::ReadOnlyDevice {
                side: ReadOnlySide::Destination,
                ..
            }
        ),
        "got {read_only:?}"
    );

    let full = map_volume_error("/naspi/shortcut.lnk", PathRole::Destination, refused(libc::ENOSPC));
    assert!(
        matches!(&full, WriteOperationError::DestinationFull { path } if path == "/naspi/shortcut.lnk"),
        "got {full:?}"
    );
}

#[test]
fn a_not_found_from_the_destination_is_not_a_missing_source() {
    // One errno, two stories. The volume can't say which side it was, so the
    // role the caller passes is the only thing standing between "there was
    // nowhere to put your file" and "your file is gone" — and the second one
    // sends someone looking for data loss that didn't happen.
    let err = map_volume_error(
        "/ctx",
        PathRole::Destination,
        VolumeError::NotFound("/photos".to_string()),
    );
    assert!(
        matches!(err, WriteOperationError::DestinationNotFound { path } if path == "/photos"),
        "a destination NotFound must never be reported as a missing source"
    );
}

#[test]
fn a_source_name_two_items_answer_to_is_not_a_taken_destination() {
    // An MTP folder can hold two same-named files, and the backend refuses to
    // read either rather than pick one (#523). "Already exists" would send the
    // user to a destination that holds nothing.
    let err = map_volume_error(
        "/DCIM/IMG_1.JPG",
        PathRole::Source,
        VolumeError::DuplicateName("/DCIM/IMG_1.JPG".to_string()),
    );
    assert!(
        !matches!(err, WriteOperationError::DestinationExists { .. }),
        "a source refusal must not read as a taken destination, got {err:?}"
    );
    let device_dest = map_volume_error(
        "/ctx",
        PathRole::Destination,
        VolumeError::DuplicateName("/DCIM/IMG_1.JPG".to_string()),
    );
    assert!(matches!(device_dest, WriteOperationError::DestinationExists { path } if path == "/DCIM/IMG_1.JPG"));
    let dest = map_volume_error(
        "/ctx",
        PathRole::Destination,
        VolumeError::AmbiguousName("/backup/IMG_1.JPG".to_string()),
    );
    assert!(matches!(dest, WriteOperationError::DestinationExists { path } if path == "/backup/IMG_1.JPG"));
}

#[test]
fn a_file_in_the_way_of_the_destination_folder_keeps_its_own_path() {
    // The volume names the thing in the way, which is often an ancestor of the
    // folder the copy was told to land in (`/ctx`). That path is the only fact
    // that tells the user what to move aside, so it must not be swapped for the
    // context path or flattened into a generic I/O refusal with a Retry.
    let err = map_volume_error(
        "/photos/2026/trip",
        PathRole::Destination,
        VolumeError::NotADirectory("/photos/2026".to_string()),
    );
    assert!(
        matches!(&err, WriteOperationError::DestinationNotAFolder { path } if path == "/photos/2026"),
        "got {err:?}"
    );
}

/// cmdr-reports#17: a move whose source delete hit `EPERM` (a Finder-locked
/// file) told the user "you don't have permission to move files here". The
/// errno died inside `VolumeError::PermissionDenied`, so the refusal read as
/// `Unclassified`, the macOS-protection advice that already existed was
/// unreachable, and the details block printed the path twice.
#[cfg(unix)]
#[test]
fn a_refusal_keeps_its_errno_through_the_volume_layer() {
    let eperm = VolumeError::from_io_at(&std::io::Error::from_raw_os_error(libc::EPERM), "/Users/me/locked.jpg");
    let err = map_volume_error("/Users/me/locked.jpg", PathRole::Source, eperm);

    let WriteOperationError::PermissionDenied {
        errno,
        refusal,
        message,
        ..
    } = &err
    else {
        panic!("expected PermissionDenied, got {err:?}");
    };
    assert_eq!(*errno, Some(libc::EPERM));
    assert_eq!(*refusal, PermissionRefusal::SystemProtected);
    // The OS's own sentence, not the path a second time.
    assert_eq!(message, &std::io::Error::from_raw_os_error(libc::EPERM).to_string());

    // And that's what the frontend receives.
    let json = serde_json::to_value(&err).expect("serializes");
    assert_eq!(json["refusal"], "systemProtected");
    assert_eq!(json["errno"], libc::EPERM);
}

#[test]
fn test_map_volume_error_permission_denied() {
    let err = map_volume_error(
        "/ctx",
        PathRole::Source,
        VolumeError::PermissionDenied {
            path: "Access denied".to_string(),
            raw_os_error: None,
        },
    );
    assert!(
        matches!(err, WriteOperationError::PermissionDenied { path, message, errno: None, refusal: PermissionRefusal::Unclassified, refused_folder: None, side: Some(PermissionSide::Source) } if message == "Access denied" && path == "/ctx")
    );
}

/// ❗ An S3 refusal can be the key's permissions OR the provider pausing the
/// account (a usage cap, a billing hold: B2's daily cap answers `403
/// AccessDenied`, live), and the answer can't say which, so the refusal is
/// typed for advice naming both. Read off the app path's own scheme, ❌ never a
/// message.
#[test]
fn an_s3_refusal_is_the_object_store_accounts() {
    let err = map_volume_error(
        "s3://AKIATEST@s3.eu-central-003.backblazeb2.com:443/photos/a.jpg",
        PathRole::Source,
        VolumeError::PermissionDenied {
            path: "/photos/a.jpg".to_string(),
            raw_os_error: None,
        },
    );
    assert!(
        matches!(
            &err,
            WriteOperationError::PermissionDenied {
                refusal: PermissionRefusal::ObjectStoreAccount,
                errno: None,
                side: Some(PermissionSide::Source),
                ..
            }
        ),
        "{err:?}"
    );
    let local = map_volume_error(
        "/Users/me/a.jpg",
        PathRole::Source,
        VolumeError::PermissionDenied {
            path: "/Users/me/a.jpg".to_string(),
            raw_os_error: None,
        },
    );
    assert!(
        matches!(
            &local,
            WriteOperationError::PermissionDenied {
                refusal: PermissionRefusal::Unclassified,
                ..
            }
        ),
        "{local:?}"
    );
}

#[test]
fn test_map_volume_error_already_exists() {
    let err = map_volume_error(
        "/ctx",
        PathRole::Source,
        VolumeError::AlreadyExists("/existing".to_string()),
    );
    assert!(matches!(err, WriteOperationError::DestinationExists { path } if path == "/existing"));
}

#[test]
fn test_map_volume_error_not_supported_names_which_volume_refused() {
    // `NotSupported` has no typed sub-variant, so it lands in `IoError` with a
    // technical-details message. ❗ That message must name the ROLE: the bare
    // "Operation not supported by this volume type" it used to carry left a
    // reader unable to tell which of the two volumes refused, and a comment in
    // `approved_op_parity_tests.rs` called it out as the worst message in the
    // suite to debug cold. `role` is the one fact the caller has and the error
    // doesn't.
    for (role, expected) in [
        (PathRole::Source, "The source volume does not support this operation"),
        (
            PathRole::Destination,
            "The destination volume does not support this operation",
        ),
    ] {
        let err = map_volume_error("/ctx", role, VolumeError::NotSupported);
        // allowed-error-string-match: asserting the technical-details STRING this
        // variant carries, not recovering a classification from it. The typed
        // variant is matched on separately, right here.
        let WriteOperationError::IoError { path, message } = err else {
            panic!("NotSupported must map to IoError, got {err:?}");
        };
        assert_eq!(path, "/ctx");
        assert_eq!(message, expected);
    }
}

#[test]
fn test_map_volume_error_delete_pending() {
    // STATUS_DELETE_PENDING surfaces when a delete was requested but an open
    // handle is keeping the file alive on the server. It MUST become a typed
    // `WriteOperationError::DeletePending` so the write-error event carries
    // the transient "file is being removed" friendly copy — not the generic
    // IoError fallback.
    let err = map_volume_error(
        "/ctx",
        PathRole::Source,
        VolumeError::DeletePending("STATUS_DELETE_PENDING".to_string()),
    );
    assert!(matches!(err, WriteOperationError::DeletePending { path } if path == "/ctx"));
}

#[test]
fn test_map_volume_error_cold_storage() {
    // An S3 object in Glacier can't be read until it's restored. It must stay
    // typed to the dialog: as an `IoError` it would offer a Retry that can only
    // meet the same archived object again.
    let err = map_volume_error(
        "/ctx",
        PathRole::Source,
        VolumeError::ColdStorage("/bucket/old.tar".to_string()),
    );
    assert!(matches!(err, WriteOperationError::SourceInColdStorage { path } if path == "/ctx"));
}

#[test]
fn test_map_volume_error_source_changed() {
    // A server-side copy whose source was replaced mid-copy published nothing.
    // Typed to the dialog, so the user hears what happened rather than a
    // generic I/O failure.
    let err = map_volume_error(
        "/ctx",
        PathRole::Source,
        VolumeError::SourceChanged("/bucket/report.pdf".to_string()),
    );
    assert!(matches!(err, WriteOperationError::SourceChanged { path } if path == "/ctx"));
}

#[test]
fn test_map_volume_error_invalid_name() {
    // A name the destination can't store (an SMB server answering
    // STATUS_OBJECT_NAME_INVALID) MUST become the typed
    // `WriteOperationError::InvalidName`, carrying the file that failed. As a
    // generic IoError the dialog says "couldn't copy the file" and offers a
    // retry, which can only fail again: renaming is the one thing that works.
    let err = map_volume_error(
        "/Volumes/naspi/export/report:2026.json",
        PathRole::Source,
        VolumeError::InvalidName("Protocol error: STATUS_OBJECT_NAME_INVALID during Create".into()),
    );
    assert!(
        matches!(
            &err,
            WriteOperationError::InvalidName { path, .. } if path == "/Volumes/naspi/export/report:2026.json"
        ),
        "expected a typed InvalidName naming the failing file, got: {err:?}"
    );
}

#[test]
fn test_map_volume_error_needs_password() {
    // Extracting from a password-protected archive must become the typed
    // `ArchiveNeedsPassword` (carrying the wrong-attempt flag) so the FE prompts
    // and retries via `set_archive_password`, never a generic read error.
    let fresh = map_volume_error(
        "/ctx",
        PathRole::Source,
        VolumeError::NeedsPassword { wrong_attempt: false },
    );
    assert!(matches!(
        fresh,
        WriteOperationError::ArchiveNeedsPassword { path, wrong_attempt: false } if path == "/ctx"
    ));
    let retried = map_volume_error(
        "/ctx",
        PathRole::Source,
        VolumeError::NeedsPassword { wrong_attempt: true },
    );
    assert!(matches!(
        retried,
        WriteOperationError::ArchiveNeedsPassword { path, wrong_attempt: true } if path == "/ctx"
    ));
}

/// A server reply refused for its signature stops the transfer with its own typed reason (#457),
/// ❌ never the generic `IoError`: the stopped state says the reply wasn't signed, and the backend
/// already knows a retry invites the same answer.
#[test]
fn a_reply_refused_for_its_signature_stops_with_its_own_reason() {
    let err = map_volume_error(
        "/smb/share/photos/a.jpg",
        PathRole::Source,
        VolumeError::SignatureInvalid("signature verification failed".into()),
    );
    assert!(
        matches!(
            &err,
            WriteOperationError::ReplySignatureRefused { path, .. } if path == "/smb/share/photos/a.jpg"
        ),
        "got {err:?}"
    );
}

/// A share's lost session, as the file it was on reports it.
fn lost_at(path: &str) -> PathedVolumeError {
    PathedVolumeError::new(
        PathRole::Destination,
        Path::new(path),
        VolumeError::DeviceDisconnected("session lost".into()),
    )
}

/// The outage wait gave up on the share: the person reads the connection as the
/// cause, with the server's name and how long Cmdr waited, and the file only as
/// context.
#[test]
fn a_server_connection_the_wait_gave_up_on_is_named_as_the_cause() {
    let failure = WriteFailure::from(
        lost_at("/smb/share/photos/a.jpg").with_lost_connection(Some(LostConnection {
            volume_name: "Naspolya".to_string(),
            gave_up_after: Some(std::time::Duration::from_secs(900)),
        })),
    );

    assert!(
        matches!(
            &failure.error,
            WriteOperationError::ServerConnectionLost { path, volume_name, gave_up_after_secs: Some(900) }
                if path == "/smb/share/photos/a.jpg" && volume_name == "Naspolya"
        ),
        "got {:?}",
        failure.error
    );
}

/// No verdict (a phone, a local drive, or a server the wait couldn't speak
/// for): the error keeps its own wording, exactly as before.
#[test]
fn a_disconnect_with_no_lost_connection_keeps_its_device_wording() {
    let failure = WriteFailure::from(lost_at("/mtp/phone/DCIM/a.jpg").with_lost_connection(None));

    assert!(
        matches!(&failure.error, WriteOperationError::DeviceDisconnected { path, side: None } if path == "/mtp/phone/DCIM/a.jpg"),
        "got {:?}",
        failure.error
    );
}

/// Where the user's only complete copy is outranks why the work stopped.
#[test]
fn where_the_new_data_is_outranks_a_lost_connection() {
    let mut error = lost_at("/smb/share/a.jpg").with_lost_connection(Some(LostConnection {
        volume_name: "Naspolya".to_string(),
        gave_up_after: None,
    }));
    error.aftermath = Some(LandingAftermath::NewDataKeptAt(PathBuf::from(
        "/smb/share/a (recovered).jpg",
    )));

    assert!(
        matches!(&WriteFailure::from(error).error, WriteOperationError::NewDataKeptAt { kept_at, .. } if kept_at == "/smb/share/a (recovered).jpg"),
    );
}

/// A refusal made before any volume call (a drive mounted inside a folder an
/// Overwrite would set aside) reaches the dialog as itself, never as the
/// I/O failure its `VolumeError` stand-in would map to.
#[test]
fn a_refusal_before_any_volume_call_reaches_the_dialog_as_itself() {
    let refusal = WriteOperationError::MountedVolumeInside {
        mount_point: "/Volumes/folder/Backup".to_string(),
        action: crate::types::MountedVolumeAction::Replace,
    };

    let error = PathedVolumeError::refused(Path::new("/Volumes/folder"), refusal);

    assert!(
        matches!(&WriteFailure::from(error).error, WriteOperationError::MountedVolumeInside { mount_point, .. } if mount_point == "/Volumes/folder/Backup"),
    );
}
