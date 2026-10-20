//! One place where an mtp-rs connection error becomes a `VolumeError`, so every
//! MTP call classifies a failure the same way.

use crate::connection::MtpConnectionError;
use cmdr_fs::volume::VolumeError;

/// `ENOTEMPTY`, which POSIX numbers differently per platform. MTP builds on
/// macOS and Linux only, so those are the two that exist.
#[cfg(target_os = "linux")]
const ENOTEMPTY: i32 = 39;
#[cfg(not(target_os = "linux"))]
const ENOTEMPTY: i32 = 66;

/// Maps MTP connection errors to Volume errors.
pub(super) fn map_mtp_error(e: MtpConnectionError) -> VolumeError {
    match e {
        // A volume outlives its session in whoever still holds it (a conflict
        // check, a copy, a pane re-read), so no session here means the phone
        // went away mid-operation. ❌ Never `NotFound`: callers read that as "the
        // path doesn't exist", and the conflict scan answered a gone phone with
        // "nothing clashes".
        MtpConnectionError::DeviceNotFound { .. } | MtpConnectionError::NotConnected { .. } => {
            VolumeError::DeviceDisconnected(e.to_string())
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
        MtpConnectionError::Cancelled { .. } => VolumeError::Cancelled(e.to_string()),
        MtpConnectionError::Disconnected { .. } => VolumeError::DeviceDisconnected(e.to_string()),
        // ❌ NOT `DeviceDisconnected`: a session reset leaves the device plugged
        // in and reopenable, so tearing down the volume would throw away a live
        // device. It's a RECOVERABLE failure of this one operation — the
        // connection layer already has a reopen running — so it carries its own
        // retryable variant rather than a dead-end `IoError`.
        MtpConnectionError::SessionReset { .. } => VolumeError::DeviceSessionReset(e.to_string()),
        MtpConnectionError::Timeout { .. } => VolumeError::ConnectionTimeout(e.to_string()),
        MtpConnectionError::StorageFull { .. } => VolumeError::StorageFull { message: e.to_string() },
        MtpConnectionError::StoreReadOnly { .. } => VolumeError::ReadOnly(e.to_string()),
        // The trait contract's refusal, carrying the errno POSIX would have
        // raised, so a caller that classifies on `raw_os_error` sees the same
        // thing here as it does over `LocalPosixVolume` or SMB.
        MtpConnectionError::DirectoryNotEmpty { .. } => VolumeError::IoError {
            message: e.to_string(),
            raw_os_error: Some(ENOTEMPTY),
        },
        _ => VolumeError::IoError {
            message: e.to_string(),
            raw_os_error: None,
        },
    }
}

#[cfg(test)]
#[path = "mapping_test.rs"]
mod mapping_test;
