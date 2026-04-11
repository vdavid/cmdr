//! Friendly error mapping: turns raw `VolumeError` + path into user-facing error info.
//!
//! Two-layer design:
//! 1. `friendly_error_from_volume_error` maps the error variant (and raw errno on macOS) to a
//!    `FriendlyError` with category, title, explanation, suggestion, and retry hint.
//! 2. `enrich_with_provider` detects the cloud/mount provider from the path and overwrites the
//!    suggestion with provider-specific advice.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::VolumeError;

// ============================================================================
// Data model
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendlyError {
    pub category: ErrorCategory,
    pub title: String,
    /// Markdown (rendered by snarkdown on FE).
    pub explanation: String,
    /// Markdown rendered by snarkdown on the frontend.
    pub suggestion: String,
    /// For the technical details disclosure, for example "ETIMEDOUT (os error 60)".
    pub raw_detail: String,
    /// FE shows a "Try again" button when true.
    pub retry_hint: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Might work if you retry (timeouts, temporary resource issues).
    Transient,
    /// User must do something (permission denied, disk full, device disconnected).
    NeedsAction,
    /// Something is genuinely broken (I/O hardware issues, corrupted data).
    Serious,
}

// ============================================================================
// Layer 1: VolumeError → FriendlyError
// ============================================================================

/// Converts a `VolumeError` into a user-facing `FriendlyError`.
///
/// For `IoError` with a `raw_os_error`, matches against platform-specific errno codes.
/// For typed `VolumeError` variants, maps directly to the right category.
pub fn friendly_error_from_volume_error(err: &VolumeError, path: &Path) -> FriendlyError {
    let path_display = path.display().to_string();

    match err {
        VolumeError::NotFound(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Path not found".into(),
            explanation: format!(
                "Cmdr couldn't find `{}`. It may have been moved, renamed, or deleted.",
                path_display
            ),
            suggestion: "Here's what to try:\n\
                - Check that the path is correct\n\
                - If this is on a network drive, make sure it's connected\n\
                - Navigate to a parent folder and look for the item there"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: false,
        },

        VolumeError::PermissionDenied(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "No permission".into(),
            explanation: format!("Cmdr doesn't have permission to access `{}`.", path_display),
            suggestion: "Here's what to try:\n\
                - Open **System Settings > Privacy & Security > Files and Folders** and grant Cmdr access\n\
                - Check the folder's permissions in Finder via Get Info\n\
                - If this is a shared folder, ask the owner to update permissions"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: false,
        },

        VolumeError::AlreadyExists(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Already exists".into(),
            explanation: format!("Something already exists at `{}`.", path_display),
            suggestion: "Rename the existing item or choose a different name.".into(),
            raw_detail: err.to_string(),
            retry_hint: false,
        },

        VolumeError::NotSupported => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Not supported".into(),
            explanation: "This operation isn't supported for this volume type.".into(),
            suggestion: "Try a different approach or use Finder for this operation.".into(),
            raw_detail: err.to_string(),
            retry_hint: false,
        },

        VolumeError::DeviceDisconnected(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Device disconnected".into(),
            explanation: "The device went away while Cmdr was reading this folder.".into(),
            suggestion: "Here's what to try:\n\
                - Reconnect the device\n\
                - Make sure cables are secure\n\
                - Navigate here again once the device is back"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: false,
        },

        VolumeError::ReadOnly(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Read-only".into(),
            explanation: "This volume or device is read-only.".into(),
            suggestion: "If the device has a write-protection switch, turn it off. \
                Otherwise, copy the files to a writable location first."
                .into(),
            raw_detail: err.to_string(),
            retry_hint: false,
        },

        VolumeError::StorageFull { .. } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Disk is full".into(),
            explanation: "There isn't enough space on this volume to complete the operation.".into(),
            suggestion: "Free up space by moving or deleting files, then try again.".into(),
            raw_detail: err.to_string(),
            retry_hint: false,
        },

        VolumeError::ConnectionTimeout(_) => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Connection timed out".into(),
            explanation: "Cmdr tried to read this folder but the connection didn't respond in time.".into(),
            suggestion: "Here's what to try:\n\
                - Check that the device or server is reachable\n\
                - Check your network connection\n\
                - Navigate here again to retry"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: true,
        },

        VolumeError::Cancelled(_) => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Cancelled".into(),
            explanation: "The operation was cancelled.".into(),
            suggestion: "Navigate here again to retry.".into(),
            raw_detail: err.to_string(),
            retry_hint: true,
        },

        VolumeError::IoError {
            raw_os_error: Some(errno),
            ..
        } => friendly_error_from_errno(*errno, path, err),

        VolumeError::IoError {
            raw_os_error: None,
            message,
        } => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Couldn't read this folder".into(),
            explanation: format!("Cmdr ran into a problem reading `{}`: {}", path_display, message),
            suggestion: "Here's what to try:\n\
                - Check that the disk or device is still connected\n\
                - Navigate here again to retry"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: true,
        },
    }
}

// ============================================================================
// Errno → FriendlyError (macOS)
// ============================================================================

/// Maps a raw macOS errno to a `FriendlyError`.
#[cfg(target_os = "macos")]
fn friendly_error_from_errno(errno: i32, path: &Path, _err: &VolumeError) -> FriendlyError {
    let path_display = path.display().to_string();
    let raw_detail = format!("{} (os error {})", errno_name(errno), errno);

    match errno {
        // ── Transient (retry-worthy) ────────────────────────────────────
        // EINTR: Interrupted system call
        4 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Interrupted".into(),
            explanation: "A system call was interrupted before it could finish.".into(),
            suggestion: "Navigate here again to retry. This is usually a one-off.".into(),
            raw_detail,
            retry_hint: true,
        },
        // ENOMEM: Not enough memory
        12 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Not enough memory".into(),
            explanation: "The system ran out of memory while reading this folder.".into(),
            suggestion: "Here's what to try:\n\
                - Close some apps to free up memory\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // EBUSY: Resource busy
        16 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Resource busy".into(),
            explanation: format!(
                "Cmdr couldn't access `{}` because another process is using it.",
                path_display
            ),
            suggestion: "Wait a moment, then navigate here again.".into(),
            raw_detail,
            retry_hint: true,
        },
        // EAGAIN: Resource temporarily unavailable
        35 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Temporarily unavailable".into(),
            explanation: "The resource is temporarily unavailable.".into(),
            suggestion: "Navigate here again to retry. This usually resolves on its own.".into(),
            raw_detail,
            retry_hint: true,
        },
        // ENETDOWN: Network is down
        50 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Network is down".into(),
            explanation: "The network interface is down.".into(),
            suggestion: "Here's what to try:\n\
                - Check your network connection\n\
                - Check Wi-Fi or Ethernet status in System Settings\n\
                - Navigate here again once you're back online"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ENETRESET: Network dropped connection on reset
        52 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Network connection dropped".into(),
            explanation: "The network connection was reset.".into(),
            suggestion: "Here's what to try:\n\
                - Check your network connection\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ECONNABORTED: Connection aborted
        53 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Connection dropped".into(),
            explanation: "The connection was aborted by the server or network.".into(),
            suggestion: "Here's what to try:\n\
                - Check that the server is running\n\
                - Check your network connection\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ECONNRESET: Connection reset by peer
        54 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Connection reset".into(),
            explanation: "The remote server closed the connection unexpectedly.".into(),
            suggestion: "Here's what to try:\n\
                - Check that the server is running\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ETIMEDOUT: Operation timed out
        60 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Connection timed out".into(),
            explanation: "Cmdr tried to read this folder but the connection didn't respond in time.".into(),
            suggestion: "Here's what to try:\n\
                - Check that the device or server is reachable\n\
                - Check your network connection\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // EHOSTDOWN: Host is down
        64 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Host is down".into(),
            explanation: "The remote host isn't responding.".into(),
            suggestion: "Here's what to try:\n\
                - Check that the host is powered on and reachable\n\
                - Check your network connection\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ESTALE: Stale NFS file handle
        70 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Stale connection".into(),
            explanation: "The file handle has gone stale. This happens with network drives when the \
                server restarts or the share is remounted."
                .into(),
            suggestion: "Here's what to try:\n\
                - Disconnect and reconnect the network drive\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ENOLCK: No locks available
        77 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Lock unavailable".into(),
            explanation: "The system ran out of file locks.".into(),
            suggestion: "Wait a moment, then navigate here again. \
                This usually resolves on its own."
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ECANCELED: Operation canceled
        89 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Cancelled".into(),
            explanation: "The operation was cancelled.".into(),
            suggestion: "Navigate here again to retry.".into(),
            raw_detail,
            retry_hint: true,
        },

        // ── NeedsAction ─────────────────────────────────────────────────
        // EPERM: Operation not permitted
        1 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Not permitted".into(),
            explanation: format!("The system blocked Cmdr from accessing `{}`.", path_display),
            suggestion: "Here's what to try:\n\
                - Open **System Settings > Privacy & Security > Files and Folders** and grant Cmdr access\n\
                - If this is a system-protected folder, you may need to grant Full Disk Access"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOENT: No such file or directory
        2 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Path not found".into(),
            explanation: format!(
                "Cmdr couldn't find `{}`. It may have been moved, renamed, or deleted.",
                path_display
            ),
            suggestion: "Here's what to try:\n\
                - Check that the path is correct\n\
                - If this is on a network drive, make sure it's connected\n\
                - Navigate to a parent folder and look for the item there"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // EACCES: Permission denied
        13 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "No permission".into(),
            explanation: format!("Cmdr doesn't have permission to access `{}`.", path_display),
            suggestion: "Here's what to try:\n\
                - Open **System Settings > Privacy & Security > Files and Folders** and grant Cmdr access\n\
                - Check the folder's permissions in Finder via Get Info\n\
                - If this is a shared folder, ask the owner to update permissions"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // EEXIST: File exists
        17 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Already exists".into(),
            explanation: format!("Something already exists at `{}`.", path_display),
            suggestion: "Rename the existing item or choose a different name.".into(),
            raw_detail,
            retry_hint: false,
        },
        // EXDEV: Cross-device link
        18 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Cross-device operation".into(),
            explanation: "This operation can't be done across different volumes.".into(),
            suggestion: "Copy the item instead of moving it.".into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOTDIR: Not a directory
        20 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Not a folder".into(),
            explanation: format!("Cmdr expected `{}` to be a folder, but it's a file.", path_display),
            suggestion: "Check the path and try again.".into(),
            raw_detail,
            retry_hint: false,
        },
        // EISDIR: Is a directory
        21 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Is a folder".into(),
            explanation: format!("Cmdr expected `{}` to be a file, but it's a folder.", path_display),
            suggestion: "Check the path and try again.".into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOSPC: No space left on device
        28 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Disk is full".into(),
            explanation: "There isn't enough space on this volume.".into(),
            suggestion: "Free up space by moving or deleting files, then try again.".into(),
            raw_detail,
            retry_hint: false,
        },
        // EROFS: Read-only file system
        30 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Read-only volume".into(),
            explanation: "This volume is mounted as read-only.".into(),
            suggestion: "If the device has a write-protection switch, turn it off. \
                Otherwise, copy the files to a writable location first."
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOTSUP: Operation not supported
        45 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Not supported".into(),
            explanation: "This operation isn't supported on this file system.".into(),
            suggestion: "Try a different approach or use Finder for this operation.".into(),
            raw_detail,
            retry_hint: false,
        },
        // ENETUNREACH: Network is unreachable
        51 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Network unreachable".into(),
            explanation: "Cmdr can't reach the network this volume is on.".into(),
            suggestion: "Here's what to try:\n\
                - Check your network connection\n\
                - Make sure you're on the right Wi-Fi network or VPN\n\
                - Navigate here again once you're connected"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ECONNREFUSED: Connection refused
        61 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Connection refused".into(),
            explanation: "The server actively refused the connection.".into(),
            suggestion: "Here's what to try:\n\
                - Check that the server is running and accepting connections\n\
                - Verify the address and port are correct\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ELOOP: Too many levels of symbolic links
        62 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Symlink loop".into(),
            explanation: format!("Cmdr found a circular chain of symbolic links at `{}`.", path_display),
            suggestion: "Check and fix the symbolic links at this path.".into(),
            raw_detail,
            retry_hint: false,
        },
        // ENAMETOOLONG: File name too long
        63 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Name too long".into(),
            explanation: "The file or folder name exceeds the system limit.".into(),
            suggestion: "Rename the item to a shorter name.".into(),
            raw_detail,
            retry_hint: false,
        },
        // EHOSTUNREACH: No route to host
        65 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Host unreachable".into(),
            explanation: "Cmdr can't reach the host this volume is on.".into(),
            suggestion: "Here's what to try:\n\
                - Check that the host is powered on\n\
                - Check your network connection and routing\n\
                - Navigate here again once the host is reachable"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOTEMPTY: Directory not empty
        66 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Folder not empty".into(),
            explanation: format!("Cmdr can't remove `{}` because it still contains items.", path_display),
            suggestion: "Delete the contents first, or use a recursive delete.".into(),
            raw_detail,
            retry_hint: false,
        },
        // EDQUOT: Disk quota exceeded
        69 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Quota exceeded".into(),
            explanation: "You've reached your disk quota on this volume.".into(),
            suggestion: "Free up space or ask an administrator to increase your quota.".into(),
            raw_detail,
            retry_hint: false,
        },
        // EAUTH: Authentication error
        80 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Authentication required".into(),
            explanation: "Cmdr couldn't authenticate with this volume.".into(),
            suggestion: "Here's what to try:\n\
                - Reconnect the volume and enter your credentials again\n\
                - Check that your password hasn't expired"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENEEDAUTH: Need authenticator
        81 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Authentication required".into(),
            explanation: "This volume requires authentication that hasn't been provided.".into(),
            suggestion: "Here's what to try:\n\
                - Disconnect and reconnect the volume\n\
                - Enter your credentials when prompted"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // EPWROFF: Device power is off
        82 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Device powered off".into(),
            explanation: "The device is powered off.".into(),
            suggestion: "Turn on the device, then navigate here again.".into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOATTR: Attribute not found
        93 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Attribute not found".into(),
            explanation: "A required file attribute is missing.".into(),
            suggestion: "This may indicate the file system doesn't support extended attributes. \
                Try the operation on a different volume."
                .into(),
            raw_detail,
            retry_hint: false,
        },

        // ── Serious ─────────────────────────────────────────────────────
        // EIO: Input/output error
        5 => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Disk read problem".into(),
            explanation: format!("Cmdr hit a hardware-level read problem at `{}`.", path_display),
            suggestion: "Here's what to try:\n\
                - Check that the disk or device is still connected\n\
                - Run Disk Utility's First Aid on this volume\n\
                - If this keeps happening, back up your data. The disk may be failing."
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // EINVAL: Invalid argument
        22 => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Unexpected system response".into(),
            explanation: "The system returned an unexpected response for this operation.".into(),
            suggestion: "Here's what to try:\n\
                - Navigate here again to retry\n\
                - If this keeps happening, the volume may have issues. \
                  Run Disk Utility's First Aid."
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // EDEVERR: Device error
        83 => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Device problem".into(),
            explanation: "The device reported a hardware-level problem.".into(),
            suggestion: "Here's what to try:\n\
                - Disconnect and reconnect the device\n\
                - Try a different USB port or cable\n\
                - If this keeps happening, the device may need repair"
                .into(),
            raw_detail,
            retry_hint: true,
        },

        // ── Unknown errno ───────────────────────────────────────────────
        _ => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Couldn't read this folder".into(),
            explanation: format!("Cmdr ran into an unexpected problem reading `{}`.", path_display),
            suggestion: "Here's what to try:\n\
                - Check that the disk or device is still connected\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
    }
}

/// Fallback for non-macOS platforms (mapping will be expanded later).
#[cfg(not(target_os = "macos"))]
fn friendly_error_from_errno(_errno: i32, path: &Path, err: &VolumeError) -> FriendlyError {
    let path_display = path.display().to_string();
    FriendlyError {
        category: ErrorCategory::Serious,
        title: "Couldn't read this folder".into(),
        explanation: format!("Cmdr ran into a problem reading `{}`.", path_display),
        suggestion: "Here's what to try:\n\
            - Check that the disk or device is still connected\n\
            - Navigate here again to retry"
            .into(),
        raw_detail: err.to_string(),
        retry_hint: true,
    }
}

/// Returns the C constant name for a macOS errno.
#[cfg(target_os = "macos")]
fn errno_name(errno: i32) -> &'static str {
    match errno {
        1 => "EPERM",
        2 => "ENOENT",
        4 => "EINTR",
        5 => "EIO",
        12 => "ENOMEM",
        13 => "EACCES",
        16 => "EBUSY",
        17 => "EEXIST",
        18 => "EXDEV",
        20 => "ENOTDIR",
        21 => "EISDIR",
        22 => "EINVAL",
        28 => "ENOSPC",
        30 => "EROFS",
        35 => "EAGAIN",
        45 => "ENOTSUP",
        50 => "ENETDOWN",
        51 => "ENETUNREACH",
        52 => "ENETRESET",
        53 => "ECONNABORTED",
        54 => "ECONNRESET",
        60 => "ETIMEDOUT",
        61 => "ECONNREFUSED",
        62 => "ELOOP",
        63 => "ENAMETOOLONG",
        64 => "EHOSTDOWN",
        65 => "EHOSTUNREACH",
        66 => "ENOTEMPTY",
        69 => "EDQUOT",
        70 => "ESTALE",
        77 => "ENOLCK",
        80 => "EAUTH",
        81 => "ENEEDAUTH",
        82 => "EPWROFF",
        83 => "EDEVERR",
        89 => "ECANCELED",
        93 => "ENOATTR",
        _ => "UNKNOWN",
    }
}

// ============================================================================
// Layer 2: Path-based provider enrichment
// ============================================================================

/// Detects the cloud/mount provider from the path and overwrites `suggestion`
/// (and sometimes `explanation`) with provider-specific advice.
///
/// Leaves `title`, `category`, and `retry_hint` unchanged.
pub fn enrich_with_provider(error: &mut FriendlyError, path: &Path) {
    let Some(provider) = detect_provider(path) else {
        return;
    };

    // Build provider-specific suggestion based on the error category and provider.
    let suggestion = provider_suggestion(&provider, error);
    error.suggestion = suggestion;
}

// ── Provider detection ──────────────────────────────────────────────────

/// Known cloud/mount provider.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Provider {
    Dropbox,
    GoogleDrive,
    OneDrive,
    Box,
    PCloud,
    Nextcloud,
    SynologyDrive,
    Tresorit,
    ProtonDrive,
    Sync,
    Egnyte,
    MacDroid,
    ICloud,
    PCloudFuse,
    MacFuse,
    VeraCrypt,
    CmVolumes,
    /// Any unrecognized dir under `~/Library/CloudStorage/`.
    GenericCloudStorage,
}

impl Provider {
    fn display_name(&self) -> &'static str {
        match self {
            Self::Dropbox => "Dropbox",
            Self::GoogleDrive => "Google Drive",
            Self::OneDrive => "OneDrive",
            Self::Box => "Box",
            Self::PCloud => "pCloud",
            Self::Nextcloud => "Nextcloud",
            Self::SynologyDrive => "Synology Drive",
            Self::Tresorit => "Tresorit",
            Self::ProtonDrive => "Proton Drive",
            Self::Sync => "Sync.com",
            Self::Egnyte => "Egnyte",
            Self::MacDroid => "MacDroid",
            Self::ICloud => "iCloud Drive",
            Self::PCloudFuse => "pCloud",
            Self::MacFuse => "macFUSE",
            Self::VeraCrypt => "VeraCrypt",
            Self::CmVolumes => "Cloud mount",
            Self::GenericCloudStorage => "your cloud provider",
        }
    }

    fn app_name(&self) -> Option<&'static str> {
        match self {
            Self::Dropbox => Some("Dropbox"),
            Self::GoogleDrive => Some("Google Drive"),
            Self::OneDrive => Some("OneDrive"),
            Self::Box => Some("Box Drive"),
            Self::PCloud | Self::PCloudFuse => Some("pCloud Drive"),
            Self::MacFuse => None, // macFUSE is a framework, not a single app
            Self::Nextcloud => Some("Nextcloud"),
            Self::SynologyDrive => Some("Synology Drive"),
            Self::Tresorit => Some("Tresorit"),
            Self::ProtonDrive => Some("Proton Drive"),
            Self::Sync => Some("Sync.com"),
            Self::Egnyte => Some("Egnyte Connect"),
            Self::MacDroid => Some("MacDroid"),
            Self::ICloud => None, // Built into macOS
            Self::VeraCrypt => Some("VeraCrypt"),
            Self::CmVolumes => None,
            Self::GenericCloudStorage => None,
        }
    }
}

/// Reads the filesystem type for a path via `libc::statfs`.
///
/// Returns `None` if the `statfs` call fails (for example, the path doesn't exist).
#[cfg(target_os = "macos")]
fn get_fs_type_for_path(path: &Path) -> Option<String> {
    use std::ffi::CString;

    let c_path = CString::new(path.to_string_lossy().as_bytes()).ok()?;
    let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();

    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

    let stat = unsafe { stat.assume_init() };
    let name_bytes: Vec<u8> = stat
        .f_fstypename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(name_bytes).ok()
}

/// Detects the provider from the path.
fn detect_provider(path: &Path) -> Option<Provider> {
    let path_str = path.to_string_lossy();

    // Expand ~ to the home directory for matching.
    let home = dirs::home_dir().unwrap_or_default();
    let cloud_storage_prefix = home.join("Library/CloudStorage");
    let mobile_docs_prefix = home.join("Library/Mobile Documents");
    let cm_volumes_prefix = home.join(".CMVolumes");

    let cloud_storage_str = cloud_storage_prefix.to_string_lossy();
    let mobile_docs_str = mobile_docs_prefix.to_string_lossy();
    let cm_volumes_str = cm_volumes_prefix.to_string_lossy();

    // 1. CloudStorage prefix providers
    if path_str.starts_with(cloud_storage_str.as_ref()) {
        // Get the directory name right after CloudStorage/
        let remainder = &path_str[cloud_storage_str.len()..];
        let remainder = remainder.strip_prefix('/').unwrap_or(remainder);
        let dir_name = remainder.split('/').next().unwrap_or("");

        return Some(if dir_name.starts_with("Dropbox") {
            Provider::Dropbox
        } else if dir_name.starts_with("GoogleDrive") {
            Provider::GoogleDrive
        } else if dir_name.starts_with("OneDrive") {
            Provider::OneDrive
        } else if dir_name.starts_with("Box") {
            Provider::Box
        } else if dir_name.starts_with("pCloud") {
            Provider::PCloud
        } else if dir_name.starts_with("Nextcloud") {
            Provider::Nextcloud
        } else if dir_name.starts_with("SynologyDrive") {
            Provider::SynologyDrive
        } else if dir_name.starts_with("Tresorit") {
            Provider::Tresorit
        } else if dir_name.starts_with("ProtonDrive") {
            Provider::ProtonDrive
        } else if dir_name.starts_with("Sync") {
            Provider::Sync
        } else if dir_name.starts_with("Egnyte") {
            Provider::Egnyte
        } else if dir_name.starts_with("MacDroid") {
            Provider::MacDroid
        } else {
            Provider::GenericCloudStorage
        });
    }

    // 2. iCloud: ~/Library/Mobile Documents/
    if path_str.starts_with(mobile_docs_str.as_ref()) {
        return Some(Provider::ICloud);
    }

    // 3. Specific paths
    if path_str.starts_with("/Volumes/pCloudDrive") {
        return Some(Provider::PCloudFuse);
    }
    if path_str.starts_with("/Volumes/veracrypt") {
        return Some(Provider::VeraCrypt);
    }
    if path_str.starts_with(cm_volumes_str.as_ref()) {
        return Some(Provider::CmVolumes);
    }

    // 4. statfs-based FUSE detection for mounts not covered by known path patterns.
    #[cfg(target_os = "macos")]
    if let Some(fs_type) = get_fs_type_for_path(path) {
        match fs_type.as_str() {
            "macfuse" | "osxfuse" => return Some(Provider::MacFuse),
            "pcloudfs" => return Some(Provider::PCloudFuse),
            _ => {}
        }
    }

    None
}

/// Builds a provider-specific suggestion string.
fn provider_suggestion(provider: &Provider, error: &FriendlyError) -> String {
    let name = provider.display_name();

    match provider {
        Provider::MacDroid => match error.category {
            ErrorCategory::Transient => "This folder is managed by **MacDroid**. Here's what to try:\n\
                    - Open MacDroid and check that your phone is connected\n\
                    - Make sure your phone is unlocked and set to file transfer mode\n\
                    - Unplug and replug the USB cable, then navigate here again"
                .to_string(),
            ErrorCategory::NeedsAction => "This folder is managed by **MacDroid**. Here's what to try:\n\
                    - Open MacDroid and check that your phone is connected\n\
                    - Make sure your phone is unlocked with the screen on\n\
                    - Check that USB file transfer mode is enabled on your phone"
                .to_string(),
            ErrorCategory::Serious => "This folder is managed by **MacDroid**. Here's what to try:\n\
                    - Unplug and replug the USB cable\n\
                    - Restart MacDroid\n\
                    - Try a different USB port or cable"
                .to_string(),
        },

        Provider::ICloud => match error.category {
            ErrorCategory::Transient => format!(
                "This folder is managed by **{name}**. Here's what to try:\n\
                    - Check your internet connection\n\
                    - Make sure you're signed in to iCloud in System Settings\n\
                    - Navigate here again to retry"
            ),
            ErrorCategory::NeedsAction => format!(
                "This folder is managed by **{name}**. Here's what to try:\n\
                    - Check that iCloud Drive is enabled in **System Settings > Apple Account > iCloud**\n\
                    - Make sure you're signed in to the right Apple account\n\
                    - Check your iCloud storage isn't full"
            ),
            ErrorCategory::Serious => format!(
                "This folder is managed by **{name}**. Here's what to try:\n\
                    - Sign out and back in to iCloud in System Settings\n\
                    - Check Apple's [system status page](https://www.apple.com/support/systemstatus/)"
            ),
        },

        Provider::MacFuse => match error.category {
            ErrorCategory::Transient => "This is a **macFUSE** mount. The remote server may be slow or unreachable. \
                Here's what to try:\n\
                    - Check your network connection\n\
                    - Check that the remote server is running\n\
                    - Navigate here again to retry"
                .to_string(),
            ErrorCategory::Serious => "This is a **macFUSE** mount. The FUSE process backing it has likely \
                crashed or disconnected. Here's what to try:\n\
                    - Force-unmount the volume: run `umount -f /Volumes/<name>` in Terminal\n\
                    - Remount using the original mount command\n\
                    - If this keeps happening, check that macFUSE is up to date"
                .to_string(),
            ErrorCategory::NeedsAction => "This is a **macFUSE** mount. Here's what to try:\n\
                    - Check that the FUSE process backing this mount is still running\n\
                    - Force-unmount and remount the volume if needed\n\
                    - Make sure macFUSE is up to date in **System Settings > General > Login Items & Extensions**"
                .to_string(),
        },

        Provider::PCloudFuse => match error.category {
            ErrorCategory::Transient => "This folder is on **pCloud**'s virtual drive. Here's what to try:\n\
                    - Check your internet connection\n\
                    - Make sure the pCloud app is running\n\
                    - Navigate here again to retry"
                .to_string(),
            ErrorCategory::Serious => "This folder is on **pCloud**'s virtual drive. The pCloud FUSE process may have \
                crashed. Here's what to try:\n\
                    - Quit and reopen the pCloud app\n\
                    - If the drive doesn't reappear, force-unmount it: run `umount -f /Volumes/pCloudDrive` in Terminal\n\
                    - After a macOS update, re-approve pCloud's system extension in \
                      **System Settings > General > Login Items & Extensions**"
                .to_string(),
            ErrorCategory::NeedsAction => "This folder is on **pCloud**'s virtual drive. Here's what to try:\n\
                    - Make sure the pCloud app is running and you're signed in\n\
                    - Check your internet connection\n\
                    - After a macOS update, re-approve pCloud's system extension in \
                      **System Settings > General > Login Items & Extensions**"
                .to_string(),
        },

        Provider::VeraCrypt => match error.category {
            ErrorCategory::Transient => format!(
                "This is a **{name}** encrypted volume. Here's what to try:\n\
                    - Check that the VeraCrypt volume is still mounted\n\
                    - Navigate here again to retry"
            ),
            ErrorCategory::NeedsAction => format!(
                "This is a **{name}** encrypted volume. Here's what to try:\n\
                    - Open VeraCrypt and check that this volume is mounted\n\
                    - Dismount and remount the volume if needed"
            ),
            ErrorCategory::Serious => format!(
                "This is a **{name}** encrypted volume. Here's what to try:\n\
                    - Dismount and remount the volume in VeraCrypt\n\
                    - If the volume keeps having issues, check it with VeraCrypt's repair tools"
            ),
        },

        Provider::CmVolumes => match error.category {
            ErrorCategory::Transient => "This is a cloud mount. Here's what to try:\n\
                    - Check your internet connection\n\
                    - Check that the mount software (CloudMounter, Mountain Duck, etc.) is running\n\
                    - Navigate here again to retry"
                .to_string(),
            _ => "This is a cloud mount. Here's what to try:\n\
                    - Check that the mount software (CloudMounter, Mountain Duck, etc.) is running\n\
                    - Disconnect and reconnect the mount\n\
                    - Check your credentials haven't expired"
                .to_string(),
        },

        Provider::GenericCloudStorage => match error.category {
            ErrorCategory::Transient => "This folder is managed by a cloud provider. Here's what to try:\n\
                    - Check your internet connection\n\
                    - Check that the sync app is running\n\
                    - Navigate here again to retry"
                .to_string(),
            _ => "This folder is managed by a cloud provider. Here's what to try:\n\
                    - Check that the sync app is running\n\
                    - Sign out and back in to the cloud app\n\
                    - Check your internet connection"
                .to_string(),
        },

        // Cloud providers with an app name: Dropbox, Google Drive, OneDrive, Box,
        // pCloud, Nextcloud, SynologyDrive, Tresorit, ProtonDrive, Sync, Egnyte
        _ => {
            let app = provider.app_name().unwrap_or(name);
            match error.category {
                ErrorCategory::Transient => format!(
                    "This folder is managed by **{name}**. Here's what to try:\n\
                    - Check your internet connection\n\
                    - Open {app} and make sure it's running and synced\n\
                    - Navigate here again to retry"
                ),
                ErrorCategory::NeedsAction => format!(
                    "This folder is managed by **{name}**. Here's what to try:\n\
                    - Open {app} and check your sync status\n\
                    - Make sure you're signed in to {app}\n\
                    - Check that you have access to this folder in {name}"
                ),
                ErrorCategory::Serious => format!(
                    "This folder is managed by **{name}**. Here's what to try:\n\
                    - Quit and reopen {app}\n\
                    - Sign out and back in to {app}\n\
                    - Check {name}'s status page for outages"
                ),
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── Errno category tests ────────────────────────────────────────────

    #[cfg(target_os = "macos")]
    fn make_io_error(errno: i32) -> VolumeError {
        VolumeError::IoError {
            message: format!("test error {}", errno),
            raw_os_error: Some(errno),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn transient_errnos_map_to_transient() {
        let transient_errnos = [4, 12, 16, 35, 50, 52, 53, 54, 60, 64, 70, 77, 89];
        let path = Path::new("/test/path");

        for errno in transient_errnos {
            let err = make_io_error(errno);
            let friendly = friendly_error_from_volume_error(&err, path);
            assert_eq!(
                friendly.category,
                ErrorCategory::Transient,
                "errno {} should be Transient, got {:?}",
                errno,
                friendly.category
            );
            assert!(friendly.retry_hint, "errno {} should have retry_hint", errno);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn needs_action_errnos_map_to_needs_action() {
        let needs_action_errnos = [
            1, 2, 13, 17, 18, 20, 21, 28, 30, 45, 51, 61, 62, 63, 65, 66, 69, 80, 81, 82, 93,
        ];
        let path = Path::new("/test/path");

        for errno in needs_action_errnos {
            let err = make_io_error(errno);
            let friendly = friendly_error_from_volume_error(&err, path);
            assert_eq!(
                friendly.category,
                ErrorCategory::NeedsAction,
                "errno {} should be NeedsAction, got {:?}",
                errno,
                friendly.category
            );
            assert!(!friendly.retry_hint, "errno {} should not have retry_hint", errno);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn serious_errnos_map_to_serious() {
        let serious_errnos = [5, 22, 83];
        let path = Path::new("/test/path");

        for errno in serious_errnos {
            let err = make_io_error(errno);
            let friendly = friendly_error_from_volume_error(&err, path);
            assert_eq!(
                friendly.category,
                ErrorCategory::Serious,
                "errno {} should be Serious, got {:?}",
                errno,
                friendly.category
            );
            assert!(friendly.retry_hint, "errno {} should have retry_hint", errno);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn unknown_errno_falls_back_to_serious() {
        let err = make_io_error(9999);
        let path = Path::new("/test/path");
        let friendly = friendly_error_from_volume_error(&err, path);

        assert_eq!(friendly.category, ErrorCategory::Serious);
        assert!(friendly.retry_hint);
        assert!(friendly.title.contains("Couldn't read"));
    }

    // ── VolumeError variant tests ───────────────────────────────────────

    #[test]
    fn volume_error_variants_map_correctly() {
        let path = Path::new("/test/path");

        let cases: Vec<(VolumeError, ErrorCategory, bool)> = vec![
            (VolumeError::NotFound("x".into()), ErrorCategory::NeedsAction, false),
            (
                VolumeError::PermissionDenied("x".into()),
                ErrorCategory::NeedsAction,
                false,
            ),
            (
                VolumeError::AlreadyExists("x".into()),
                ErrorCategory::NeedsAction,
                false,
            ),
            (VolumeError::NotSupported, ErrorCategory::NeedsAction, false),
            (
                VolumeError::DeviceDisconnected("x".into()),
                ErrorCategory::NeedsAction,
                false,
            ),
            (VolumeError::ReadOnly("x".into()), ErrorCategory::NeedsAction, false),
            (
                VolumeError::StorageFull { message: "x".into() },
                ErrorCategory::NeedsAction,
                false,
            ),
            (
                VolumeError::ConnectionTimeout("x".into()),
                ErrorCategory::Transient,
                true,
            ),
            (VolumeError::Cancelled("x".into()), ErrorCategory::Transient, true),
            (
                VolumeError::IoError {
                    message: "x".into(),
                    raw_os_error: None,
                },
                ErrorCategory::Serious,
                true,
            ),
        ];

        for (err, expected_category, expected_retry) in cases {
            let friendly = friendly_error_from_volume_error(&err, path);
            assert_eq!(
                friendly.category, expected_category,
                "VolumeError {:?} should map to {:?}",
                err, expected_category
            );
            assert_eq!(
                friendly.retry_hint, expected_retry,
                "VolumeError {:?} retry_hint should be {}",
                err, expected_retry
            );
        }
    }

    // ── Provider detection tests ────────────────────────────────────────

    fn home_path(suffix: &str) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/Users/test"))
            .join(suffix)
    }

    #[test]
    fn detect_cloud_storage_providers() {
        let cases = [
            ("Library/CloudStorage/Dropbox/docs/file.txt", Provider::Dropbox),
            (
                "Library/CloudStorage/GoogleDrive-me@gmail.com/My Drive/file.txt",
                Provider::GoogleDrive,
            ),
            ("Library/CloudStorage/OneDrive-Personal/file.txt", Provider::OneDrive),
            ("Library/CloudStorage/Box-Enterprise/file.txt", Provider::Box),
            ("Library/CloudStorage/pCloud/file.txt", Provider::PCloud),
            ("Library/CloudStorage/Nextcloud-myserver/file.txt", Provider::Nextcloud),
            (
                "Library/CloudStorage/SynologyDrive-NAS/file.txt",
                Provider::SynologyDrive,
            ),
            ("Library/CloudStorage/Tresorit/file.txt", Provider::Tresorit),
            ("Library/CloudStorage/ProtonDrive-me/file.txt", Provider::ProtonDrive),
            ("Library/CloudStorage/Sync-myaccount/file.txt", Provider::Sync),
            ("Library/CloudStorage/Egnyte-Corp/file.txt", Provider::Egnyte),
            ("Library/CloudStorage/MacDroid-Phone/DCIM/photo.jpg", Provider::MacDroid),
            (
                "Library/CloudStorage/ExpanDrive-S3/file.txt",
                Provider::GenericCloudStorage,
            ),
        ];

        for (suffix, expected) in cases {
            let path = home_path(suffix);
            let detected = detect_provider(&path);
            assert_eq!(
                detected.as_ref(),
                Some(&expected),
                "Path suffix '{}' should detect {:?}, got {:?}",
                suffix,
                expected,
                detected
            );
        }
    }

    #[test]
    fn detect_icloud() {
        let path = home_path("Library/Mobile Documents/com~apple~CloudDocs/file.txt");
        assert_eq!(detect_provider(&path), Some(Provider::ICloud));
    }

    #[test]
    fn detect_pcloud_fuse() {
        let path = Path::new("/Volumes/pCloudDrive/folder/file.txt");
        assert_eq!(detect_provider(path), Some(Provider::PCloudFuse));
    }

    #[test]
    fn detect_veracrypt() {
        let path = Path::new("/Volumes/veracrypt1/secret/file.txt");
        assert_eq!(detect_provider(path), Some(Provider::VeraCrypt));
    }

    #[test]
    fn detect_cm_volumes() {
        let path = home_path(".CMVolumes/MyMount/file.txt");
        assert_eq!(detect_provider(&path), Some(Provider::CmVolumes));
    }

    #[test]
    fn detect_generic_cloud_storage_fallback() {
        let path = home_path("Library/CloudStorage/MountainDuck-S3/file.txt");
        assert_eq!(detect_provider(&path), Some(Provider::GenericCloudStorage));
    }

    #[test]
    fn no_provider_for_regular_path() {
        let path = Path::new("/Users/test/Documents/file.txt");
        assert_eq!(detect_provider(path), None);
    }

    // ── Enrichment behavior tests ───────────────────────────────────────

    #[test]
    fn enrichment_overwrites_suggestion_but_not_title_or_category() {
        let err = VolumeError::ConnectionTimeout("test".into());
        let path = home_path("Library/CloudStorage/Dropbox/some/folder");

        let mut friendly = friendly_error_from_volume_error(&err, &path);
        let original_title = friendly.title.clone();
        let original_category = friendly.category;
        let original_retry = friendly.retry_hint;
        let original_suggestion = friendly.suggestion.clone();

        enrich_with_provider(&mut friendly, &path);

        assert_eq!(friendly.title, original_title, "title should not change");
        assert_eq!(friendly.category, original_category, "category should not change");
        assert_eq!(friendly.retry_hint, original_retry, "retry_hint should not change");
        assert_ne!(
            friendly.suggestion, original_suggestion,
            "suggestion should be overwritten by provider enrichment"
        );
        assert!(
            friendly.suggestion.contains("Dropbox"),
            "enriched suggestion should mention Dropbox"
        );
    }

    #[test]
    fn enrichment_is_noop_for_unknown_path() {
        let err = VolumeError::ConnectionTimeout("test".into());
        let path = Path::new("/Users/test/Documents/folder");

        let mut friendly = friendly_error_from_volume_error(&err, path);
        let original_suggestion = friendly.suggestion.clone();

        enrich_with_provider(&mut friendly, path);

        assert_eq!(
            friendly.suggestion, original_suggestion,
            "suggestion should not change for unknown paths"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn errno_raw_detail_includes_name_and_code() {
        let err = make_io_error(60);
        let path = Path::new("/test/path");
        let friendly = friendly_error_from_volume_error(&err, path);

        assert!(
            friendly.raw_detail.contains("ETIMEDOUT"),
            "raw_detail should include errno name"
        );
        assert!(
            friendly.raw_detail.contains("60"),
            "raw_detail should include errno number"
        );
    }

    #[test]
    fn error_messages_never_contain_error_or_failed() {
        let path = Path::new("/test/path");

        // Test a selection of variants and errnos
        let errors: Vec<VolumeError> = vec![
            VolumeError::NotFound("x".into()),
            VolumeError::PermissionDenied("x".into()),
            VolumeError::ConnectionTimeout("x".into()),
            VolumeError::IoError {
                message: "x".into(),
                raw_os_error: None,
            },
            VolumeError::IoError {
                message: "x".into(),
                raw_os_error: Some(60),
            },
            VolumeError::IoError {
                message: "x".into(),
                raw_os_error: Some(13),
            },
            VolumeError::IoError {
                message: "x".into(),
                raw_os_error: Some(5),
            },
        ];

        for err in &errors {
            let friendly = friendly_error_from_volume_error(err, path);

            // Check title, explanation, and suggestion (not raw_detail, which is technical)
            let title_lower = friendly.title.to_lowercase();
            let explanation_lower = friendly.explanation.to_lowercase();
            let suggestion_lower = friendly.suggestion.to_lowercase();

            assert!(
                !title_lower.contains("error") && !title_lower.contains("failed"),
                "Title {:?} for {:?} contains 'error' or 'failed'",
                friendly.title,
                err
            );
            assert!(
                !explanation_lower.contains("an error") && !explanation_lower.contains("failed"),
                "Explanation {:?} for {:?} contains 'an error' or 'failed'",
                friendly.explanation,
                err
            );
            assert!(
                !suggestion_lower.contains("error") && !suggestion_lower.contains("failed"),
                "Suggestion {:?} for {:?} contains 'error' or 'failed'",
                friendly.suggestion,
                err
            );
        }
    }

    // ── Provider suggestion tests ───────────────────────────────────────

    #[test]
    fn all_providers_produce_specific_suggestions() {
        let providers_and_paths: Vec<(&str, Provider)> = vec![
            ("Library/CloudStorage/Dropbox/f", Provider::Dropbox),
            ("Library/CloudStorage/GoogleDrive-x/f", Provider::GoogleDrive),
            ("Library/CloudStorage/OneDrive-x/f", Provider::OneDrive),
            ("Library/CloudStorage/Box-x/f", Provider::Box),
            ("Library/CloudStorage/pCloud/f", Provider::PCloud),
            ("Library/CloudStorage/Nextcloud-x/f", Provider::Nextcloud),
            ("Library/CloudStorage/SynologyDrive-x/f", Provider::SynologyDrive),
            ("Library/CloudStorage/Tresorit/f", Provider::Tresorit),
            ("Library/CloudStorage/ProtonDrive-x/f", Provider::ProtonDrive),
            ("Library/CloudStorage/Sync-x/f", Provider::Sync),
            ("Library/CloudStorage/Egnyte-x/f", Provider::Egnyte),
            ("Library/CloudStorage/MacDroid-x/f", Provider::MacDroid),
            ("Library/CloudStorage/Unknown-x/f", Provider::GenericCloudStorage),
            ("Library/Mobile Documents/com~apple~CloudDocs/f", Provider::ICloud),
        ];

        for (suffix, expected_provider) in &providers_and_paths {
            let path = home_path(suffix);
            let err = VolumeError::ConnectionTimeout("test".into());
            let mut friendly = friendly_error_from_volume_error(&err, &path);
            enrich_with_provider(&mut friendly, &path);

            assert!(
                friendly.suggestion.contains(expected_provider.display_name())
                    || *expected_provider == Provider::GenericCloudStorage
                    || *expected_provider == Provider::CmVolumes,
                "Suggestion for {:?} should mention provider name. Got: {}",
                expected_provider,
                friendly.suggestion
            );
        }

        // Specific-path providers
        let specific_paths: Vec<(&str, Provider)> = vec![
            ("/Volumes/pCloudDrive/f", Provider::PCloudFuse),
            ("/Volumes/veracrypt1/f", Provider::VeraCrypt),
        ];

        for (path_str, expected_provider) in &specific_paths {
            let path = Path::new(path_str);
            let err = VolumeError::ConnectionTimeout("test".into());
            let mut friendly = friendly_error_from_volume_error(&err, path);
            enrich_with_provider(&mut friendly, path);

            assert!(
                friendly.suggestion.contains(expected_provider.display_name()),
                "Suggestion for {:?} should mention provider name. Got: {}",
                expected_provider,
                friendly.suggestion
            );
        }

        // CmVolumes
        let cm_path = home_path(".CMVolumes/MyMount/f");
        let err = VolumeError::ConnectionTimeout("test".into());
        let mut friendly = friendly_error_from_volume_error(&err, &cm_path);
        enrich_with_provider(&mut friendly, &cm_path);
        assert!(
            friendly.suggestion.contains("cloud mount"),
            "CmVolumes suggestion should mention cloud mount"
        );
    }

    // ── MacFuse and PCloudFuse suggestion tests ────────────────────────

    #[test]
    fn macfuse_suggestions_mention_macfuse() {
        let categories = [
            ErrorCategory::Transient,
            ErrorCategory::NeedsAction,
            ErrorCategory::Serious,
        ];
        for category in categories {
            let error = FriendlyError {
                category,
                title: "test".into(),
                explanation: "test".into(),
                suggestion: "placeholder".into(),
                raw_detail: "test".into(),
                retry_hint: false,
            };
            let suggestion = provider_suggestion(&Provider::MacFuse, &error);
            assert!(
                suggestion.contains("macFUSE"),
                "MacFuse {:?} suggestion should mention macFUSE. Got: {}",
                category,
                suggestion
            );
        }
    }

    #[test]
    fn pcloud_fuse_suggestions_mention_pcloud() {
        let categories = [
            ErrorCategory::Transient,
            ErrorCategory::NeedsAction,
            ErrorCategory::Serious,
        ];
        for category in categories {
            let error = FriendlyError {
                category,
                title: "test".into(),
                explanation: "test".into(),
                suggestion: "placeholder".into(),
                raw_detail: "test".into(),
                retry_hint: false,
            };
            let suggestion = provider_suggestion(&Provider::PCloudFuse, &error);
            assert!(
                suggestion.contains("pCloud"),
                "PCloudFuse {:?} suggestion should mention pCloud. Got: {}",
                category,
                suggestion
            );
        }
    }

    #[test]
    fn fuse_provider_suggestions_follow_style_guide() {
        let providers = [Provider::MacFuse, Provider::PCloudFuse];
        let categories = [
            ErrorCategory::Transient,
            ErrorCategory::NeedsAction,
            ErrorCategory::Serious,
        ];

        for provider in &providers {
            for category in &categories {
                let error = FriendlyError {
                    category: *category,
                    title: "test".into(),
                    explanation: "test".into(),
                    suggestion: "placeholder".into(),
                    raw_detail: "test".into(),
                    retry_hint: false,
                };
                let suggestion = provider_suggestion(provider, &error);
                let lower = suggestion.to_lowercase();

                assert!(
                    !lower.contains("error") && !lower.contains("failed"),
                    "{:?} {:?} suggestion contains 'error' or 'failed': {}",
                    provider,
                    category,
                    suggestion
                );
            }
        }
    }

    #[test]
    fn macfuse_serious_suggests_force_unmount() {
        let error = FriendlyError {
            category: ErrorCategory::Serious,
            title: "test".into(),
            explanation: "test".into(),
            suggestion: "placeholder".into(),
            raw_detail: "test".into(),
            retry_hint: false,
        };
        let suggestion = provider_suggestion(&Provider::MacFuse, &error);
        assert!(
            suggestion.contains("umount -f"),
            "MacFuse Serious suggestion should mention force-unmount. Got: {}",
            suggestion
        );
    }

    #[test]
    fn pcloud_fuse_serious_suggests_system_extension_reapproval() {
        let error = FriendlyError {
            category: ErrorCategory::Serious,
            title: "test".into(),
            explanation: "test".into(),
            suggestion: "placeholder".into(),
            raw_detail: "test".into(),
            retry_hint: false,
        };
        let suggestion = provider_suggestion(&Provider::PCloudFuse, &error);
        assert!(
            suggestion.contains("System Settings"),
            "PCloudFuse Serious suggestion should mention System Settings. Got: {}",
            suggestion
        );
    }
}
