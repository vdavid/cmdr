//! One place where an mtp-rs connection error becomes a `VolumeError`, so every
//! MTP call classifies a failure the same way.

use super::VolumeError;
use crate::mtp::connection::MtpConnectionError;

/// `ENOTEMPTY`, which POSIX numbers differently per platform. MTP builds on
/// macOS and Linux only, so those are the two that exist.
#[cfg(target_os = "linux")]
const ENOTEMPTY: i32 = 39;
#[cfg(not(target_os = "linux"))]
const ENOTEMPTY: i32 = 66;

/// Maps MTP connection errors to Volume errors.
pub(super) fn map_mtp_error(e: MtpConnectionError) -> VolumeError {
    match e {
        MtpConnectionError::DeviceNotFound { .. } | MtpConnectionError::NotConnected { .. } => {
            VolumeError::NotFound(e.to_string())
        }
        MtpConnectionError::ObjectNotFound { path, .. } => VolumeError::NotFound(path),
        MtpConnectionError::StaleParentHandle { dest_folder, .. } => VolumeError::StaleDestinationHandle(dest_folder),
        MtpConnectionError::ExclusiveAccess { .. } | MtpConnectionError::PermissionDenied { .. } => {
            VolumeError::PermissionDenied(e.to_string())
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
