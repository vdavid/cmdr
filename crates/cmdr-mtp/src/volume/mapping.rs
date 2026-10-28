//! One place where an mtp-rs connection error becomes a `VolumeError`, so every
//! MTP call classifies a failure the same way.

use crate::connection::MtpConnectionError;
use cmdr_fs::error_detail::ErrorDetail;
use cmdr_fs::volume::VolumeError;

/// `ENOTEMPTY`, which POSIX numbers differently per platform. MTP builds on
/// macOS and Linux only, so those are the two that exist.
#[cfg(target_os = "linux")]
const ENOTEMPTY: i32 = 39;
#[cfg(not(target_os = "linux"))]
const ENOTEMPTY: i32 = 66;

/// Maps MTP connection errors to Volume errors. A message variant keeps the connection error
/// typed in its detail, so a log line marks the path or name it carries.
pub(super) fn map_mtp_error(e: MtpConnectionError) -> VolumeError {
    match e {
        // A volume outlives its session in whoever still holds it (a conflict
        // check, a copy, a pane re-read), so no session here means the phone
        // went away mid-operation. ❌ Never `NotFound`: callers read that as "the
        // path doesn't exist", and the conflict scan answered a gone phone with
        // "nothing clashes".
        MtpConnectionError::DeviceNotFound { .. } | MtpConnectionError::NotConnected { .. } => {
            VolumeError::DeviceDisconnected(ErrorDetail::of(e))
        }
        MtpConnectionError::ObjectNotFound { path, .. } => VolumeError::NotFound(path),
        // Same-named siblings: handled like SMB's look-alike `AmbiguousName`, but
        // its own variant so the person reads the device's cause and fix (#523).
        MtpConnectionError::AmbiguousName { path, .. } => VolumeError::DuplicateName(path),
        MtpConnectionError::StaleParentHandle { dest_folder, .. } => VolumeError::StaleDestinationHandle(dest_folder),
        MtpConnectionError::ExclusiveAccess { .. } | MtpConnectionError::PermissionDenied { .. } => {
            VolumeError::PermissionDenied {
                path: e.to_string(),
                raw_os_error: None,
            }
        }
        MtpConnectionError::Cancelled { .. } => VolumeError::Cancelled(ErrorDetail::of(e)),
        MtpConnectionError::Disconnected { .. } => VolumeError::DeviceDisconnected(ErrorDetail::of(e)),
        // ❌ NOT `DeviceDisconnected`: a session reset leaves the device plugged
        // in and reopenable, so tearing down the volume would throw away a live
        // device. It's a RECOVERABLE failure of this one operation — the
        // connection layer already has a reopen running — so it carries its own
        // retryable variant rather than a dead-end `IoError`.
        MtpConnectionError::SessionReset { .. } => VolumeError::DeviceSessionReset(ErrorDetail::of(e)),
        MtpConnectionError::Timeout { .. } => VolumeError::ConnectionTimeout(ErrorDetail::of(e)),
        MtpConnectionError::StorageFull { .. } => VolumeError::StorageFull {
            message: ErrorDetail::of(e),
        },
        MtpConnectionError::StoreReadOnly { .. } => VolumeError::ReadOnly(e.to_string()),
        // Only a different name can work, like ADB's `ENAMETOOLONG`: ❌ never the
        // retryable `IoError`.
        MtpConnectionError::NameTooLong { .. } => VolumeError::InvalidName(ErrorDetail::of(e)),
        // The trait contract's refusal, carrying the errno POSIX would have
        // raised, so a caller that classifies on `raw_os_error` sees the same
        // thing here as it does over `LocalPosixVolume` or SMB.
        MtpConnectionError::DirectoryNotEmpty { .. } => VolumeError::IoError {
            message: ErrorDetail::of(e),
            raw_os_error: Some(ENOTEMPTY),
        },
        _ => VolumeError::IoError {
            message: ErrorDetail::of(e),
            raw_os_error: None,
        },
    }
}

#[cfg(test)]
#[path = "mapping_test.rs"]
mod mapping_test;
