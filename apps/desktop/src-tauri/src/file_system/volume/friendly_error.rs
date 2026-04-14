//! Friendly error mapping: turns raw `VolumeError` + path into user-facing error info.
//!
//! Two-layer design:
//! 1. `friendly_error_from_volume_error` maps the error variant (and raw errno on macOS) to a
//!    `FriendlyError` with category, title, explanation, suggestion, and retry hint.
//! 2. `enrich_with_provider` (in sibling module `provider.rs`) detects the cloud/mount provider
//!    from the path and overwrites the suggestion with provider-specific advice.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::VolumeError;

// Re-export so callers can keep importing from `friendly_error::enrich_with_provider`.
pub use super::provider::enrich_with_provider;

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
        },

        VolumeError::Cancelled(_) => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Cancelled".into(),
            explanation: "The operation was cancelled before it could finish.".into(),
            suggestion: "Navigate here again whenever you're ready to retry.".into(),
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
            explanation: "A system operation was interrupted before it could finish. This is \
                almost always a one-off, caused by a signal or background process momentarily \
                getting in the way."
                .into(),
            suggestion: "Navigate here again to retry. This kind of interruption almost never \
                happens twice in a row."
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ENOMEM: Not enough memory
        12 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Not enough memory".into(),
            explanation: "The system ran out of available memory (RAM) while reading this folder. \
                This can happen when many apps are open at once, or when a folder contains a very \
                large number of files."
                .into(),
            suggestion: "Here's what to try:\n\
                - Close some apps to free up memory, especially ones using lots of resources \
                (browsers with many tabs, editors, media apps)\n\
                - Check memory usage in **Activity Monitor** (search for it in Spotlight)\n\
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
                "Cmdr couldn't access `{}` because another app or process is currently using it \
                exclusively. This is usually temporary.",
                path_display
            ),
            suggestion: "Wait a moment, then navigate here again. If it keeps happening, check \
                which app might be holding the file open (in Terminal, run \
                `lsof +D <folder-path>` to see which processes are using this folder)."
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // EAGAIN: Resource temporarily unavailable
        35 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Temporarily unavailable".into(),
            explanation: "The system is momentarily too busy to handle this request. This is a \
                transient condition that typically clears up on its own within seconds."
                .into(),
            suggestion: "Navigate here again to retry. This usually resolves on its own. If it \
                keeps happening, the system might be under heavy load. Check \
                **Activity Monitor** for apps consuming a lot of resources."
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ENETDOWN: Network is down
        50 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Network is down".into(),
            explanation: "Your Mac's network connection is down, so Cmdr can't reach this \
                volume. This could mean Wi-Fi is disconnected, an Ethernet cable is unplugged, \
                or the network interface is disabled."
                .into(),
            suggestion: "Here's what to try:\n\
                - Check Wi-Fi or Ethernet status in **System Settings > Network**\n\
                - If you're on Wi-Fi, try turning it off and on again\n\
                - In Terminal, run `ping google.com` to test your connection\n\
                - Navigate here again once you're back online"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ENETRESET: Network dropped connection on reset
        52 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Network connection dropped".into(),
            explanation: "The network connection was unexpectedly reset while Cmdr was reading \
                this folder. This can happen when a router restarts, a VPN reconnects, or the \
                network is temporarily unstable."
                .into(),
            suggestion: "Here's what to try:\n\
                - Check your network connection\n\
                - If you're on a VPN, make sure it's still connected\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ECONNABORTED: Connection aborted
        53 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Connection dropped".into(),
            explanation: "The connection was dropped by the server or the network before Cmdr \
                could finish reading. This often means the server is overloaded or restarting."
                .into(),
            suggestion: "Here's what to try:\n\
                - Check that the server is running and responsive\n\
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
            explanation: "The remote server closed the connection unexpectedly. This can happen \
                when the server restarts, hits a timeout, or runs into an internal problem."
                .into(),
            suggestion: "Here's what to try:\n\
                - Check that the server is running\n\
                - In Terminal, try `ping <hostname>` to test if the server is reachable\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ETIMEDOUT: Operation timed out
        60 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Connection timed out".into(),
            explanation: "Cmdr tried to read this folder but the connection didn't respond in \
                time. This usually means the server or device is slow, unreachable, or \
                the network between you and it is congested."
                .into(),
            suggestion: "Here's what to try:\n\
                - Check that the device or server is powered on and reachable\n\
                - Check your Wi-Fi or Ethernet connection\n\
                - In Terminal, try `ping <hostname>` to test connectivity\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // EHOSTDOWN: Host is down
        64 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Host is down".into(),
            explanation: "The remote host (the computer or server hosting this volume) isn't \
                responding. It may be powered off, sleeping, or temporarily unreachable."
                .into(),
            suggestion: "Here's what to try:\n\
                - Check that the host is powered on and connected to the network\n\
                - In Terminal, try `ping <hostname>` to test if it's reachable\n\
                - If it's a NAS or server, check its management interface\n\
                - Navigate here again once the host is back"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ESTALE: Stale NFS file handle
        70 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Stale connection".into(),
            explanation: "Cmdr is trying to access this folder using an old reference that \
                the server no longer recognizes. This commonly happens with network drives \
                (NFS, SMB) after the server restarts, the share is remounted, or the \
                connection was interrupted."
                .into(),
            suggestion: "Here's what to try:\n\
                - Navigate away from this folder and come back\n\
                - If this is a network drive, try unmounting and remounting it in Finder\n\
                - Check that the server hosting this folder is running\n\
                - In Terminal, run `mount` to see currently mounted volumes"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ENOLCK: No locks available
        77 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Lock unavailable".into(),
            explanation: "The system ran out of file locks. File locks are how apps coordinate \
                access to shared files (preventing two apps from writing to the same file at \
                once). Running out usually means too many apps are accessing files simultaneously."
                .into(),
            suggestion: "Here's what to try:\n\
                - Close some apps, especially ones that work with many files (editors, IDEs, \
                backup tools)\n\
                - In Terminal, run `lsof | wc -l` to see how many files are open across all apps\n\
                - If the problem keeps happening, you can raise the limit with \
                `ulimit -n 4096` in Terminal\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // ECANCELED: Operation canceled
        89 => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Cancelled".into(),
            explanation: "The operation was cancelled before it could finish.".into(),
            suggestion: "Navigate here again whenever you're ready to retry.".into(),
            raw_detail,
            retry_hint: true,
        },

        // ── NeedsAction ─────────────────────────────────────────────────
        // EPERM: Operation not permitted
        1 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Not permitted".into(),
            explanation: format!(
                "macOS blocked Cmdr from accessing `{}`. This usually means the folder is \
                protected by macOS security policies, or Cmdr hasn't been granted the right \
                permissions yet.",
                path_display
            ),
            suggestion: "Here's what to try:\n\
                - Open **System Settings > Privacy & Security > Files and Folders** and grant \
                Cmdr access\n\
                - If this is a system-protected folder (like system directories), you may \
                need to grant Cmdr **Full Disk Access** under Privacy & Security\n\
                - In Terminal, run `ls -la` on this path to check ownership and permissions"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOENT: No such file or directory
        2 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Path not found".into(),
            explanation: format!(
                "Cmdr couldn't find `{}`. It may have been moved, renamed, or deleted \
                while Cmdr was trying to access it.",
                path_display
            ),
            suggestion: "Here's what to try:\n\
                - Check that the path is spelled correctly\n\
                - If this is on a network drive, make sure it's connected and the share is \
                accessible\n\
                - Navigate to the parent folder and look for the item there\n\
                - In Terminal, run `ls -la` on the parent folder to see what's there"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // EACCES: Permission denied
        13 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "No permission".into(),
            explanation: format!(
                "Cmdr doesn't have permission to access `{}`. macOS controls which apps \
                can access which folders, and Cmdr hasn't been granted access to this one yet.",
                path_display
            ),
            suggestion: "Here's what to try:\n\
                - Open **System Settings > Privacy & Security > Files and Folders** and grant \
                Cmdr access\n\
                - Check the folder's permissions in Finder: right-click it, choose Get Info, \
                and look under Sharing & Permissions\n\
                - If this is a shared folder, ask the owner to update permissions\n\
                - In Terminal, run `ls -la` on this path to see the current permissions"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // EEXIST: File exists
        17 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Already exists".into(),
            explanation: format!(
                "A file or folder already exists at `{}`, so Cmdr can't create a new one there.",
                path_display
            ),
            suggestion: "Rename the existing item or choose a different name for the new one.".into(),
            raw_detail,
            retry_hint: false,
        },
        // EXDEV: Cross-device link
        18 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Cross-device operation".into(),
            explanation: "Cmdr can't move this item directly because the source and destination \
                are on different volumes (for example, an internal drive and a USB stick). Moving \
                across volumes requires copying the data and then removing the original."
                .into(),
            suggestion: "Copy the item to the destination instead of moving it. Cmdr will handle \
                the copy automatically."
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOTDIR: Not a directory
        20 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Not a folder".into(),
            explanation: format!(
                "Cmdr expected `{}` to be a folder, but it's a file. This can happen if \
                something was recently renamed or replaced.",
                path_display
            ),
            suggestion: "Check the path and make sure it points to a folder, not a file.".into(),
            raw_detail,
            retry_hint: false,
        },
        // EISDIR: Is a directory
        21 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Is a folder".into(),
            explanation: format!(
                "Cmdr expected `{}` to be a file, but it's a folder. This can happen if \
                something was recently renamed or replaced.",
                path_display
            ),
            suggestion: "Check the path and make sure it points to a file, not a folder.".into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOSPC: No space left on device
        28 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Disk is full".into(),
            explanation: "There isn't enough free space on this volume to complete the operation.".into(),
            suggestion: "Here's what to try:\n\
                - Free up space by moving or deleting files you no longer need\n\
                - Empty the Trash (right-click the Trash icon in the Dock)\n\
                - In Terminal, run `df -h` to see how much space is left on each volume\n\
                - Check **System Settings > General > Storage** for a breakdown of what's \
                using space"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // EROFS: Read-only file system
        30 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Read-only volume".into(),
            explanation: "This volume is mounted as read-only, so Cmdr can't make changes to it. \
                This could be because the device has a physical write-protection switch, the \
                disk image was mounted read-only, or the file system doesn't support writing."
                .into(),
            suggestion: "Here's what to try:\n\
                - If the device has a physical write-protection switch (common on SD cards), \
                flip it off\n\
                - If this is a disk image, remount it with write access\n\
                - Otherwise, copy the files to a writable location first"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOTSUP: Operation not supported
        45 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Not supported".into(),
            explanation: "This operation isn't supported on this file system. Different file \
                systems (like FAT32, NTFS, or network shares) support different features, and \
                this one doesn't support what Cmdr is trying to do."
                .into(),
            suggestion: "Try a different approach, or use Finder for this operation. If you're \
                working with an external drive, it might be formatted with a file system that \
                has limitations (for example, FAT32 can't store files larger than 4 GB)."
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENETUNREACH: Network is unreachable
        51 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Network unreachable".into(),
            explanation: "Cmdr can't reach the network this volume is on. This often means \
                you're not connected to the right network, or a VPN isn't active."
                .into(),
            suggestion: "Here's what to try:\n\
                - Check your Wi-Fi or Ethernet connection\n\
                - Make sure you're on the right network (for example, your office Wi-Fi or VPN)\n\
                - In Terminal, try `ping <hostname>` to test if the server is reachable\n\
                - Navigate here again once you're connected"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ECONNREFUSED: Connection refused
        61 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Connection refused".into(),
            explanation: "The server actively refused the connection. This usually means the \
                server software (for example, an SMB or NFS service) isn't running, or it's \
                configured to reject connections from this Mac."
                .into(),
            suggestion: "Here's what to try:\n\
                - Check that the server is running and its file sharing service is active\n\
                - Verify the server address and port are correct\n\
                - In Terminal, try `ping <hostname>` to check if the server is reachable at all\n\
                - Navigate here again to retry"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ELOOP: Too many levels of symbolic links
        62 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Symlink loop".into(),
            explanation: format!(
                "Cmdr found a circular chain of symbolic links (shortcuts that point to other \
                shortcuts) at `{}`. Following these links leads in a circle, so Cmdr can't \
                reach the actual file or folder.",
                path_display
            ),
            suggestion: "Here's what to try:\n\
                - In Terminal, run `ls -la` on this path to see where the symbolic links point\n\
                - Find and fix the link that creates the loop\n\
                - If you're not sure which link is the problem, follow them one by one with \
                `readlink <path>`"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENAMETOOLONG: File name too long
        63 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Name too long".into(),
            explanation: "The file or folder name exceeds the system's limit (255 characters on \
                most Mac volumes). This can also happen when the full path (all folders combined) \
                exceeds the system's maximum path length."
                .into(),
            suggestion: "Rename the item to use a shorter name. If the name looks reasonable, \
                the full path (including all parent folders) might be too long. Try moving \
                it to a shorter path."
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // EHOSTUNREACH: No route to host
        65 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Host unreachable".into(),
            explanation: "Cmdr can't find a network route to the host this volume is on. This \
                usually means the host is on a different network, behind a firewall, or the \
                routing configuration needs updating."
                .into(),
            suggestion: "Here's what to try:\n\
                - Check that the host is powered on and on the same network\n\
                - If you need a VPN to reach it, make sure the VPN is connected\n\
                - In Terminal, try `ping <hostname>` to test connectivity\n\
                - Navigate here again once the host is reachable"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOTEMPTY: Directory not empty
        66 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Folder not empty".into(),
            explanation: format!(
                "Cmdr can't remove `{}` because it still contains files or subfolders. The \
                system requires a folder to be empty before it can be removed this way.",
                path_display
            ),
            suggestion: "Delete the contents of the folder first, then try removing the folder \
                again."
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // EDQUOT: Disk quota exceeded
        69 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Quota exceeded".into(),
            explanation: "You've reached your disk quota (the maximum amount of space allocated \
                to your user account) on this volume. This is common on shared servers and \
                network drives where an administrator sets per-user limits."
                .into(),
            suggestion: "Here's what to try:\n\
                - Free up space by removing files you no longer need on this volume\n\
                - Ask your system administrator to increase your quota\n\
                - In Terminal, run `quota` to see your current usage and limit"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // EAUTH: Authentication error
        80 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Authentication required".into(),
            explanation: "Cmdr couldn't authenticate with this volume. Your saved credentials \
                may have expired, or the server is rejecting the current login."
                .into(),
            suggestion: "Here's what to try:\n\
                - Disconnect and reconnect the volume, and enter your username and password again\n\
                - Check that your password hasn't changed or expired\n\
                - If this is a company server, check with your IT team"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENEEDAUTH: Need authenticator
        81 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Authentication required".into(),
            explanation: "This volume requires you to log in, but no credentials have been \
                provided yet."
                .into(),
            suggestion: "Here's what to try:\n\
                - Disconnect and reconnect the volume in Finder\n\
                - Enter your username and password when prompted\n\
                - If you're not sure about the credentials, check with the server's administrator"
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // EPWROFF: Device power is off
        82 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Device powered off".into(),
            explanation: "The device is powered off or in a deep sleep state, so Cmdr can't \
                communicate with it."
                .into(),
            suggestion: "Turn on the device, wait for it to fully start up, then navigate here \
                again."
                .into(),
            raw_detail,
            retry_hint: false,
        },
        // ENOATTR: Attribute not found
        93 => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Attribute not found".into(),
            explanation: "Cmdr tried to read a file attribute (extra metadata like tags or \
                permissions) that doesn't exist on this item. This can happen when the file \
                system doesn't support extended attributes, or when the attribute was removed."
                .into(),
            suggestion: "This file system may not support the metadata Cmdr needs. Try the \
                operation on a different volume, or copy the file to your Mac's internal drive \
                first."
                .into(),
            raw_detail,
            retry_hint: false,
        },

        // ── Serious ─────────────────────────────────────────────────────
        // EIO: Input/output error
        5 => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Disk read problem".into(),
            explanation: format!(
                "Cmdr hit a hardware-level read problem at `{}`. This means the disk or device \
                had trouble reading the data, which could be a temporary glitch or a sign of \
                a failing disk.",
                path_display
            ),
            suggestion: "Here's what to try:\n\
                - Check that the disk or device is still properly connected\n\
                - Open **Disk Utility** (search for it in Spotlight) and run **First Aid** on \
                this volume\n\
                - If this keeps happening, back up your data as soon as possible. The disk \
                may be developing bad sectors or starting to wear out."
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // EINVAL: Invalid argument
        22 => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Unexpected system response".into(),
            explanation: "The system returned an unexpected response for this operation. This \
                can happen when a volume's file system has inconsistencies, or when the volume \
                is in an unusual state."
                .into(),
            suggestion: "Here's what to try:\n\
                - Navigate here again to retry\n\
                - If this keeps happening, open **Disk Utility** (search for it in Spotlight) \
                and run **First Aid** on this volume to check for file system problems"
                .into(),
            raw_detail,
            retry_hint: true,
        },
        // EDEVERR: Device error
        83 => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Device problem".into(),
            explanation: "The device reported a hardware-level problem. This could be a loose \
                connection, a worn-out cable, or an issue with the device itself."
                .into(),
            suggestion: "Here's what to try:\n\
                - Disconnect and reconnect the device\n\
                - Try a different USB port or cable\n\
                - If it's an external drive, try connecting it to a different computer to see \
                if the problem follows the device\n\
                - If this keeps happening, the device may need repair or replacement"
                .into(),
            raw_detail,
            retry_hint: true,
        },

        // ── Unknown errno ───────────────────────────────────────────────
        _ => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Couldn't read this folder".into(),
            explanation: format!(
                "Cmdr ran into an unexpected problem reading `{}`. Check the technical \
                details below for the specific system code, which can help with \
                troubleshooting.",
                path_display
            ),
            suggestion: "Here's what to try:\n\
                - Check that the disk or device is still connected\n\
                - Navigate here again to retry\n\
                - If this keeps happening, open **Disk Utility** and run **First Aid** on \
                this volume"
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
        explanation: format!(
            "Cmdr ran into a problem reading `{}`. Check the technical details below \
            for the specific system code, which can help with troubleshooting.",
            path_display
        ),
        suggestion: "Here's what to try:\n\
            - Check that the disk or device is still connected\n\
            - Navigate here again to retry\n\
            - If this keeps happening, check the health of the disk or device"
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
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
}
