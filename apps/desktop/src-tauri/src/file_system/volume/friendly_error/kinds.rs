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

pub(super) fn not_found(path_display: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
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
        explanation: format!(
            "Cmdr can't read `{}` because macOS hasn't granted access to this folder yet. \
            This is a privacy gate, not a Cmdr bug.",
            path_display
        ),
        suggestion: "Two ways to fix:\n\
            - Grant Cmdr **Full Disk Access** in **System Settings → Privacy & Security → Full Disk Access** — \
            removes all such limits at once.\n\
            - Or grant per-folder access for just this folder in **System Settings → Privacy & Security → Files and Folders → Cmdr**."
            .into(),
        raw_detail,
        retry_hint: false,
        action_kind: Some(ErrorActionKind::OpenPrivacySettings),
    }
}

pub(super) fn permission_denied(path_display: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
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
        raw_detail,
        retry_hint: false,
        action_kind: Some(ErrorActionKind::OpenPrivacySettings),
    }
}

pub(super) fn already_exists(path_display: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "Already exists".into(),
        explanation: format!(
            "A file or folder already exists at `{}`, so Cmdr can't create a new one there.",
            path_display
        ),
        suggestion: "Rename the existing item or choose a different name for the new one.".into(),
        raw_detail,
        retry_hint: false,
        action_kind: None,
    }
}

pub(super) fn cancelled(raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::Transient,
        title: "Cancelled".into(),
        explanation: "The operation was cancelled before it could finish.".into(),
        suggestion: "Navigate here again whenever you're ready to retry.".into(),
        raw_detail,
        retry_hint: true,
        action_kind: None,
    }
}

pub(super) fn device_disconnected(path_display: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "Device disconnected".into(),
        explanation: format!(
            "The device holding `{}` was disconnected during the operation. \
            This can happen if a USB cable comes loose, a phone goes to sleep, \
            or a network drive drops its connection.",
            path_display
        ),
        suggestion: "Here's what to try:\n\
            - Reconnect the device and make sure the cable is secure\n\
            - If it's a phone, unlock it and make sure file transfer mode is active\n\
            - Navigate here again once the device is back"
            .into(),
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
        explanation: "This volume is read-only, so Cmdr can't make changes to it. This could \
            be because the device has a physical write-protection switch, the disk image was \
            mounted as read-only, or the file system doesn't support writing."
            .into(),
        suggestion: "Here's what to try:\n\
            - If the device has a physical write-protection switch (common on SD cards), flip it off\n\
            - If this is a disk image, remount it with write access\n\
            - Otherwise, copy the files to a writable location first"
            .into(),
        raw_detail,
        retry_hint: false,
        action_kind: None,
    }
}

pub(super) fn storage_full(raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "Disk is full".into(),
        explanation: "There isn't enough free space on this volume to complete the operation.".into(),
        suggestion: "Here's what to try:\n\
            - Free up space by moving or deleting files you no longer need\n\
            - Empty the Trash (right-click the Trash icon in the Dock)\n\
            - In Terminal, run `df -h` to see how much space is left on each volume"
            .into(),
        raw_detail,
        retry_hint: false,
        action_kind: None,
    }
}

pub(super) fn connection_timeout(raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::Transient,
        title: "Connection timed out".into(),
        explanation: "Cmdr tried to access this resource but the connection didn't respond in time. \
            This usually means the server or device is slow to respond, or the network \
            connection is unstable."
            .into(),
        suggestion: "Here's what to try:\n\
            - Check that the device or server is powered on and reachable\n\
            - Check your Wi-Fi or Ethernet connection\n\
            - In Terminal, try `ping <hostname>` to test if the server is reachable\n\
            - Try again"
            .into(),
        raw_detail,
        retry_hint: true,
        action_kind: None,
    }
}

pub(super) fn not_supported(raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::NeedsAction,
        title: "Not supported".into(),
        explanation: "This operation isn't supported on this type of volume. Some volumes \
            (like phone storage or certain network drives) don't support all operations."
            .into(),
        suggestion: "Try a different approach, or use Finder for this operation.".into(),
        raw_detail,
        retry_hint: false,
        action_kind: None,
    }
}

pub(super) fn io_serious(path_display: &str, message: &str, raw_detail: String) -> FriendlyError {
    FriendlyError {
        category: ErrorCategory::Serious,
        title: "Couldn't read this folder".into(),
        explanation: format!(
            "Cmdr ran into a problem with `{}`: {}. This could be a temporary glitch \
            or a sign that the disk or device needs attention.",
            path_display, message
        ),
        suggestion: "Here's what to try:\n\
            - Check that the disk or device is still connected\n\
            - Try again\n\
            - If this keeps happening, try running **Disk Utility > First Aid** on this volume"
            .into(),
        raw_detail,
        retry_hint: true,
        action_kind: None,
    }
}
