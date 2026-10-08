//! An S3 error answer in the `Volume` vocabulary.
//!
//! ❌ Nothing here reads `<Message>`: the `<Code>` and the status decide, the
//! way `S3Error`'s own predicates do. The text that rides in an `IoError` is a
//! log diagnostic, never shown.

use cmdr_fs::volume::VolumeError;
use log::debug;

use crate::error::{S3Error, S3ErrorCode};

/// Turns an S3 error answer into the `Volume` vocabulary, for an operation on
/// `path`.
///
/// ❗ **`path` is the payload, not context.** `NotFound` and
/// `PermissionDenied` are DEFINED to carry the path, and the transfer layer
/// renders it as the name of the file the user is missing
/// (`conformance::assert_not_found_carries_the_path`).
///
/// A refusal is `PermissionDenied` whichever code said it (`AccessDenied`, or
/// keys that stopped working mid-session): an operation can't mend keys, and
/// the volume's sign-in path is where a person does.
pub(crate) fn map_s3_error(error: &S3Error, path: &str) -> VolumeError {
    if error.is_not_found() {
        debug!("S3 path={path:?}: backend=s3, error_kind=not_found, code={error}");
        return VolumeError::NotFound(path.to_string());
    }
    // A no-overwrite precondition (`If-None-Match: *`, R2's copy header), the
    // only kind Cmdr sends: the name is taken and what's there stayed.
    if error.is_precondition_failed() {
        debug!("S3 path={path:?}: backend=s3, error_kind=already_exists, code={error}");
        return VolumeError::AlreadyExists(path.to_string());
    }
    // An archived object (Glacier Flexible Retrieval, Deep Archive, an
    // Intelligent-Tiering archive tier) until it's restored: typed, because a
    // retry can only meet it again and the fix is a restore, not new keys.
    if error.code == S3ErrorCode::InvalidObjectState {
        debug!("S3 path={path:?}: backend=s3, error_kind=cold_storage, code={error}");
        return VolumeError::ColdStorage(path.to_string());
    }
    // GCS refuses a name it can't store (a CR or LF) with `400
    // InvalidObjectName`: the fix is another name, so a retry is no use.
    if error.code == S3ErrorCode::InvalidObjectName {
        debug!("S3 path={path:?}: backend=s3, error_kind=invalid_name, code={error}");
        return VolumeError::InvalidName(error.to_string());
    }
    if error.is_not_implemented() || error.status == http::StatusCode::METHOD_NOT_ALLOWED {
        return VolumeError::NotSupported;
    }
    // R2 answers keys it doesn't know with `401 Unauthorized`, bodyless on a HEAD.
    let refused = error.status == http::StatusCode::UNAUTHORIZED
        || match error.code {
            S3ErrorCode::AccessDenied | S3ErrorCode::SignatureDoesNotMatch | S3ErrorCode::InvalidAccessKeyId => true,
            S3ErrorCode::NoBody => error.status == http::StatusCode::FORBIDDEN,
            _ => false,
        };
    if refused {
        debug!("S3 path={path:?}: backend=s3, error_kind=permission_denied, code={error}");
        return VolumeError::PermissionDenied {
            path: path.to_string(),
            raw_os_error: None,
        };
    }
    VolumeError::IoError {
        message: error.to_string(),
        raw_os_error: None,
    }
}

#[cfg(test)]
#[path = "errors_test.rs"]
mod errors_test;
