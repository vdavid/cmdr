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
//! lives with the copy engine in `copy_tests.rs`.

use super::*;

#[test]
fn test_map_volume_error_not_found() {
    let err = map_volume_error(
        "/ctx",
        PathRole::Source,
        VolumeError::NotFound("/test/path".to_string()),
    );
    assert!(matches!(err, WriteOperationError::SourceNotFound { path } if path == "/test/path"));
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
fn test_map_volume_error_permission_denied() {
    let err = map_volume_error(
        "/ctx",
        PathRole::Source,
        VolumeError::PermissionDenied("Access denied".to_string()),
    );
    assert!(
        matches!(err, WriteOperationError::PermissionDenied { path, message } if message == "Access denied" && path == "/ctx")
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
fn test_map_volume_error_invalid_name() {
    // A name the destination can't store (an SMB server answering
    // STATUS_OBJECT_NAME_INVALID) MUST become the typed
    // `WriteOperationError::InvalidName`, carrying the file that failed. As a
    // generic IoError the dialog says "couldn't copy the file" and offers a
    // retry, which can only fail again: renaming is the one thing that works.
    let err = map_volume_error(
        "/Volumes/naspi/export/report:2026.json",
        PathRole::Source,
        VolumeError::InvalidName("Protocol error: STATUS_OBJECT_NAME_INVALID during Create".to_string()),
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
