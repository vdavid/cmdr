//! `VolumeError → FriendlyError`.
//!
//! Maps the typed `VolumeError` variants directly. For `IoError` with a raw errno,
//! delegates to `errno::friendly_error_from_errno`. Provider-specific suggestions
//! are layered on top by `enrich_with_provider` (in the sibling `provider` module).

use std::path::Path;

use super::errno::friendly_error_from_errno;
use super::{ErrorActionKind, ErrorCategory, FriendlyError};
use crate::file_system::volume::VolumeError;

/// Converts a `VolumeError` into a user-facing `FriendlyError`.
///
/// For `IoError` with a `raw_os_error`, matches against platform-specific errno codes.
/// For typed `VolumeError` variants, maps directly to the right category.
///
/// Git failures arrive as `VolumeError::FriendlyGit(FriendlyGitError)` from the
/// `file_system::git` volume hooks; we hand the carried payload straight to
/// `to_friendly_error` so `ErrorPane` shows git-specific titles and suggestions
/// instead of the generic I/O copy.
pub fn friendly_error_from_volume_error(err: &VolumeError, path: &Path) -> FriendlyError {
    let path_display = path.display().to_string();

    match err {
        VolumeError::FriendlyGit(git_err) => git_err.to_friendly_error(),

        VolumeError::NotFound(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Path not found".into(),
            explanation: format!(
                "Cmdr couldn't find `{}`. It may have been moved, renamed, or deleted \
                while Cmdr was trying to access it.",
                path_display
            ),
            suggestion: "Here's what to try:\n\
                - Check that the path is spelled correctly\n\
                - If this is on a network drive, make sure it's connected and the share is accessible\n\
                - Navigate to the parent folder and look for the item there\n\
                - In Terminal, run `ls -la` on the parent folder to see what's there"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: false,
            action_kind: None,
        },

        VolumeError::PermissionDenied(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "No permission".into(),
            explanation: format!(
                "Cmdr doesn't have permission to access `{}`. macOS controls which apps \
                can access which folders, and Cmdr hasn't been granted access to this one yet.",
                path_display
            ),
            suggestion: "Here's what to try:\n\
                - Open **System Settings > Privacy & Security > Files and Folders** and grant Cmdr access\n\
                - Check the folder's permissions in Finder: right-click the folder, choose Get Info, \
                and look under Sharing & Permissions\n\
                - If this is a shared folder, ask the owner to update permissions\n\
                - In Terminal, run `ls -la` on the path to see the current permissions"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: false,
            action_kind: Some(ErrorActionKind::OpenPrivacySettings),
        },

        VolumeError::AlreadyExists(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Already exists".into(),
            explanation: format!(
                "A file or folder already exists at `{}`, so Cmdr can't create a new one there.",
                path_display
            ),
            suggestion: "Rename the existing item or choose a different name for the new one.".into(),
            raw_detail: err.to_string(),
            retry_hint: false,
            action_kind: None,
        },

        VolumeError::NotSupported => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Not supported".into(),
            explanation: "This operation isn't supported on this type of volume. Some volumes \
                (like phone storage or certain network drives) don't support all operations."
                .into(),
            suggestion: "Try a different approach, or use Finder for this operation.".into(),
            raw_detail: err.to_string(),
            retry_hint: false,
            action_kind: None,
        },

        VolumeError::DeviceDisconnected(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Device disconnected".into(),
            explanation: "The device was disconnected while Cmdr was reading from it. This \
                can happen if a USB cable comes loose, a phone goes to sleep, or a network \
                drive drops its connection."
                .into(),
            suggestion: "Here's what to try:\n\
                - Reconnect the device and make sure the cable is secure\n\
                - If it's a phone, unlock it and make sure file transfer mode is active\n\
                - Navigate here again once the device is back"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: false,
            action_kind: None,
        },

        VolumeError::ReadOnly(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Read-only".into(),
            explanation: "This volume is read-only, so Cmdr can't make changes to it. This could \
                be because the device has a physical write-protection switch, the disk image was \
                mounted as read-only, or the file system doesn't support writing."
                .into(),
            suggestion: "Here's what to try:\n\
                - If the device has a physical write-protection switch (common on SD cards), flip it off\n\
                - If this is a disk image, remount it with write access\n\
                - Otherwise, copy the files to a writable location first"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: false,
            action_kind: None,
        },

        VolumeError::StorageFull { .. } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Disk is full".into(),
            explanation: "There isn't enough free space on this volume to complete the operation.".into(),
            suggestion: "Here's what to try:\n\
                - Free up space by moving or deleting files you no longer need\n\
                - Empty the Trash (right-click the Trash icon in the Dock)\n\
                - In Terminal, run `df -h` to see how much space is left on each volume"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: false,
            action_kind: None,
        },

        VolumeError::ConnectionTimeout(_) => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Connection timed out".into(),
            explanation: "Cmdr tried to read this folder but the connection didn't respond in time. \
                This usually means the server or device is slow to respond, or the network \
                connection is unstable."
                .into(),
            suggestion: "Here's what to try:\n\
                - Check that the device or server is powered on and reachable\n\
                - Check your Wi-Fi or Ethernet connection\n\
                - In Terminal, try `ping <hostname>` to test if the server is reachable\n\
                - Navigate here again to retry"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: true,
            action_kind: None,
        },

        VolumeError::Cancelled(_) => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Cancelled".into(),
            explanation: "The operation was cancelled before it could finish.".into(),
            suggestion: "Navigate here again whenever you're ready to retry.".into(),
            raw_detail: err.to_string(),
            retry_hint: true,
            action_kind: None,
        },

        VolumeError::IsADirectory(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "This is a folder, not a file".into(),
            explanation: format!("Cmdr tried to open `{}` as a file, but it's a folder.", path_display),
            suggestion: "Navigate into the folder instead of opening it as a file.".into(),
            raw_detail: err.to_string(),
            retry_hint: false,
            action_kind: None,
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
            explanation: format!(
                "Cmdr ran into a problem reading `{}`: {}. This could be a temporary glitch \
                or a sign that the disk or device needs attention.",
                path_display, message
            ),
            suggestion: "Here's what to try:\n\
                - Check that the disk or device is still connected\n\
                - Navigate here again to retry\n\
                - If this keeps happening, try running **Disk Utility > First Aid** on this volume"
                .into(),
            raw_detail: err.to_string(),
            retry_hint: true,
            action_kind: None,
        },
    }
}
