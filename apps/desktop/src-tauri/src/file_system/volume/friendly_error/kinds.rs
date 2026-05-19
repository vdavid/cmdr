//! Shared `FriendlyError` constructors keyed by conceptual error kind.
//!
//! Both `volume_error::friendly_error_from_volume_error` (listing path) and
//! `write_error::friendly_from_write_error` (copy/move/delete/trash error path)
//! map several variants to the same conceptual outcome: "not found",
//! "permission denied", "device disconnected", and so on. Without a single
//! source of truth the user could see different titles or suggestions for the
//! same situation depending on which layer the error originated in.
//!
//! Each function here returns the canonical `FriendlyError` for one kind. The
//! caller passes the raw-detail string (formatted differently per source:
//! `VolumeError::to_string()` vs `format!("{err:?}")`) and any kind-specific
//! data (the path, error message, etc.).
//!
//! Variants that don't share semantics across the two sources (e.g.
//! `WriteOperationError::SymlinkLoop`, `VolumeError::FriendlyGit`) stay inline
//! in their respective mapper.

use super::{ErrorActionKind, ErrorCategory, FriendlyError};
use crate::md;

pub(super) fn not_found(path_display: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "Path not found".into(),
        explanation: md!(
            "Cmdr couldn't find `{}`. It may have been moved, renamed, or deleted \
            while Cmdr was trying to access it.",
            path_display,
        ),
        suggestion: md!("Here's what to try:\n\
            - Check that the path is spelled correctly\n\
            - If this is on a network drive, make sure it's connected and the share is accessible\n\
            - Navigate to the parent folder and look for the item there\n\
            - In Terminal, run `ls -la` on the parent folder to see what's there"),
        raw_detail,
        retry_hint: false,
        action_kind: None,
    }
}

/// Permission-denied on a path that macOS guards via TCC (Downloads, Documents,
/// Desktop, Pictures, Movies, Music, iCloud Drive, FileProvider domains, network
/// volumes, etc.; see `crate::restricted_paths::tcc_paths`). The user has two
/// distinct escape hatches (Full Disk Access for everything, or per-folder
/// Files & Folders for just this one). The generic permission-denied copy only
/// mentions the per-folder pane, so we override it here.
pub(super) fn tcc_restricted(path_display: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "This folder is restricted by macOS".into(),
        explanation: md!(
            "Cmdr can't read `{}` because macOS hasn't granted access to this folder yet. \
            This is a privacy gate, not a Cmdr bug.",
            path_display,
        ),
        suggestion: md!("Two ways to fix:\n\
            - Grant Cmdr **Full Disk Access** in **System Settings → Privacy & Security → Full Disk Access** \
            to remove all such limits at once.\n\
            - Or grant per-folder access for just this folder in **System Settings → Privacy & Security → Files and Folders → Cmdr**."),
        raw_detail,
        retry_hint: false,
        action_kind: Some(ErrorActionKind::OpenPrivacySettings),
    }
}

pub(super) fn permission_denied(path_display: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "No permission".into(),
        explanation: md!(
            "Cmdr doesn't have permission to access `{}`. macOS controls which apps \
            can access which folders, and Cmdr hasn't been granted access to this one yet.",
            path_display,
        ),
        suggestion: md!("Here's what to try:\n\
            - Open **System Settings > Privacy & Security > Files and Folders** and grant Cmdr access\n\
            - Check the folder's permissions in Finder: right-click the folder, choose Get Info, \
            and look under Sharing & Permissions\n\
            - If this is a shared folder, ask the owner to update permissions\n\
            - In Terminal, run `ls -la` on the path to see the current permissions"),
        raw_detail,
        retry_hint: false,
        action_kind: Some(ErrorActionKind::OpenPrivacySettings),
    }
}

pub(super) fn already_exists(path_display: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "Already exists".into(),
        explanation: md!(
            "A file or folder already exists at `{}`, so Cmdr can't create a new one there.",
            path_display,
        ),
        suggestion: md!("Rename the existing item or choose a different name for the new one."),
        raw_detail,
        retry_hint: false,
        action_kind: None,
    }
}

pub(super) fn cancelled(raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::Transient,
        title: "Cancelled".into(),
        explanation: md!("The operation was cancelled before it could finish."),
        suggestion: md!("Navigate here again whenever you're ready to retry."),
        raw_detail,
        retry_hint: true,
        action_kind: None,
    }
}

pub(super) fn device_disconnected(path_display: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "Device disconnected".into(),
        explanation: md!(
            "The device holding `{}` was disconnected during the operation. \
            This can happen if a USB cable comes loose, a phone goes to sleep, \
            or a network drive drops its connection.",
            path_display,
        ),
        suggestion: md!("Here's what to try:\n\
            - Reconnect the device and make sure the cable is secure\n\
            - If it's a phone, unlock it and make sure file transfer mode is active\n\
            - Navigate here again once the device is back"),
        raw_detail,
        // listing path doesn't show a Retry button (the user navigates back).
        // Operations (write_error) override to true so the dialog gets a Retry.
        retry_hint: false,
        action_kind: None,
    }
}

pub(super) fn read_only(raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "Read-only".into(),
        explanation: md!(
            "This volume is read-only, so Cmdr can't make changes to it. This could \
            be because the device has a physical write-protection switch, the disk image was \
            mounted as read-only, or the file system doesn't support writing."
        ),
        suggestion: md!("Here's what to try:\n\
            - If the device has a physical write-protection switch (common on SD cards), flip it off\n\
            - If this is a disk image, remount it with write access\n\
            - Otherwise, copy the files to a writable location first"),
        raw_detail,
        retry_hint: false,
        action_kind: None,
    }
}

pub(super) fn storage_full(raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "Disk is full".into(),
        explanation: md!("There isn't enough free space on this volume to complete the operation."),
        suggestion: md!("Here's what to try:\n\
            - Free up space by moving or deleting files you no longer need\n\
            - Empty the Trash (right-click the Trash icon in the Dock)\n\
            - In Terminal, run `df -h` to see how much space is left on each volume"),
        raw_detail,
        retry_hint: false,
        action_kind: None,
    }
}

pub(super) fn connection_timeout(raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::Transient,
        title: "Connection timed out".into(),
        explanation: md!(
            "Cmdr tried to access this resource but the connection didn't respond in time. \
            This usually means the server or device is slow to respond, or the network \
            connection is unstable."
        ),
        suggestion: md!("Here's what to try:\n\
            - Check that the device or server is powered on and reachable\n\
            - Check your Wi-Fi or Ethernet connection\n\
            - In Terminal, try `ping <hostname>` to test if the server is reachable\n\
            - Try again"),
        raw_detail,
        retry_hint: true,
        action_kind: None,
    }
}

pub(super) fn not_supported(raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "Not supported".into(),
        explanation: md!("This operation isn't supported on this type of volume. Some volumes \
            (like phone storage or certain network drives) don't support all operations."),
        suggestion: md!("Try a different approach, or use Finder for this operation."),
        raw_detail,
        retry_hint: false,
        action_kind: None,
    }
}

/// `STATUS_DELETE_PENDING`: the file has been marked for deletion on the server
/// but at least one open handle is keeping it alive. The file disappears the
/// moment the last handle closes, so retry-after-a-moment is the right hint.
pub(super) fn delete_pending(path_display: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::Transient,
        title: "File is being removed".into(),
        explanation: md!(
            "`{}` is on its way out. The server marked it for deletion, but another \
            open handle is keeping it around until that handle closes.",
            path_display,
        ),
        suggestion: md!("Here's what to try:\n\
            - Wait a moment and try again — once the last handle closes, the file disappears\n\
            - Close any other apps that might have this file open\n\
            - If it sticks around, restart Cmdr to drop any handles it might still hold"),
        raw_detail,
        retry_hint: true,
        action_kind: None,
    }
}

pub(super) fn io_serious(path_display: &str, message: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::Serious,
        title: "Couldn't read this folder".into(),
        explanation: md!(
            "Cmdr ran into a problem with `{}`: {}. This could be a temporary glitch \
            or a sign that the disk or device needs attention.",
            path_display,
            message,
        ),
        suggestion: md!("Here's what to try:\n\
            - Check that the disk or device is still connected\n\
            - Try again\n\
            - If this keeps happening, try running **Disk Utility > First Aid** on this volume"),
        raw_detail,
        retry_hint: true,
        action_kind: None,
    }
}
