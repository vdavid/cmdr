//! `WriteOperationError → FriendlyError`.
//!
//! Used by `WriteErrorEvent::new` so every `write-error` event the FE receives
//! carries a rendered payload, even on local-FS paths where the original
//! `VolumeError` is no longer in scope. Volume-aware paths that still hold
//! the originating `VolumeError + path` should prefer
//! `friendly_error_from_volume_error` + `enrich_with_provider` (this is a
//! parallel, not a replacement).

use super::{ErrorActionKind, ErrorCategory, FriendlyError};

/// Converts a `WriteOperationError` into a `FriendlyError` for the transfer-error
/// dialog. Mirrors `friendly_error_from_volume_error` but works downstream of the
/// `map_volume_error` conversion, so it covers the local-FS error paths too.
///
/// For volume-aware paths that still have the original `VolumeError + path`,
/// prefer `friendly_error_from_volume_error` followed by `enrich_with_provider` —
/// that route gets the provider-specific suggestions. This function is the
/// fallback when only the typed write error is in scope.
pub fn friendly_from_write_error(err: &crate::file_system::write_operations::WriteOperationError) -> FriendlyError {
    use crate::file_system::write_operations::WriteOperationError as W;

    let raw_detail = format!("{err:?}");
    match err {
        W::SourceNotFound { path } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Couldn't find that file".into(),
            explanation: format!(
                "The file or folder at `{path}` isn't there anymore — it may have been moved or deleted while Cmdr was working."
            ),
            suggestion: "Refresh the source folder, then try again with the file you want.".into(),
            raw_detail,
            retry_hint: false,
            action_kind: None,
        },
        W::DestinationExists { path } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Name is already taken".into(),
            explanation: format!("Something already exists at `{path}`."),
            suggestion: "Pick a different name, or remove the existing item first.".into(),
            raw_detail,
            retry_hint: false,
            action_kind: None,
        },
        W::PermissionDenied { path, .. } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Couldn't access this location".into(),
            explanation: format!("Cmdr doesn't have permission to write to `{path}`."),
            suggestion: "Check folder permissions, or try a different destination. On macOS, you may need to grant **Full Disk Access** in System Settings > Privacy & Security."
                .into(),
            raw_detail,
            retry_hint: false,
            action_kind: Some(ErrorActionKind::OpenPrivacySettings),
        },
        W::InsufficientSpace {
            required,
            available,
            volume_name,
        } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Not enough space".into(),
            explanation: format!(
                "{} needs {} bytes but only has {} bytes free.",
                volume_name.as_deref().unwrap_or("The destination"),
                required,
                available,
            ),
            suggestion: "Free up space at the destination, or pick a different one.".into(),
            raw_detail,
            retry_hint: false,
            action_kind: None,
        },
        W::SameLocation { path } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Same source and destination".into(),
            explanation: format!("`{path}` is both the source and the destination — nothing to do."),
            suggestion: "Pick a different destination.".into(),
            raw_detail,
            retry_hint: false,
            action_kind: None,
        },
        W::DestinationInsideSource { source, destination } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Destination is inside the source".into(),
            explanation: format!("Cmdr can't copy `{source}` into `{destination}` — that would loop forever."),
            suggestion: "Pick a destination outside the source folder.".into(),
            raw_detail,
            retry_hint: false,
            action_kind: None,
        },
        W::SymlinkLoop { path } => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Symlink loop".into(),
            explanation: format!("`{path}` contains symlinks that point back at themselves."),
            suggestion: "Resolve the loop manually before retrying.".into(),
            raw_detail,
            retry_hint: false,
            action_kind: None,
        },
        W::Cancelled { .. } => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Cancelled".into(),
            explanation: "The operation was cancelled.".into(),
            suggestion: "Start it again whenever you're ready.".into(),
            raw_detail,
            retry_hint: true,
            action_kind: None,
        },
        W::DeviceDisconnected { path } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Device disconnected".into(),
            explanation: format!("The device holding `{path}` was disconnected during the operation."),
            suggestion: "Reconnect the device, then try again.".into(),
            raw_detail,
            retry_hint: true,
            action_kind: None,
        },
        W::ReadOnlyDevice { path, device_name } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Read-only device".into(),
            explanation: format!(
                "{} can't be written to. Tried `{}`.",
                device_name.as_deref().unwrap_or("This device"),
                path,
            ),
            suggestion: "Pick a destination that supports writing.".into(),
            raw_detail,
            retry_hint: false,
            action_kind: None,
        },
        W::FileLocked { path } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "File is locked".into(),
            explanation: format!("`{path}` is locked and can't be changed."),
            suggestion: "On macOS, unlock it via Finder > Get Info > uncheck Locked. Then try again.".into(),
            raw_detail,
            retry_hint: false,
            action_kind: None,
        },
        W::TrashNotSupported { path } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Trash not supported here".into(),
            explanation: format!(
                "`{path}` is on a volume that doesn't have a trash (network shares, FAT-formatted drives, …)."
            ),
            suggestion: "Delete the file directly instead of moving it to trash.".into(),
            raw_detail,
            retry_hint: false,
            action_kind: None,
        },
        W::ConnectionInterrupted { path } => FriendlyError {
            category: ErrorCategory::Transient,
            title: "Connection interrupted".into(),
            explanation: format!("The network connection to `{path}` dropped before the operation finished."),
            suggestion: "Check your connection, then try again.".into(),
            raw_detail,
            retry_hint: true,
            action_kind: None,
        },
        W::ReadError { path, message } => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Couldn't read the source".into(),
            explanation: format!("Cmdr ran into a problem reading `{path}`: {message}."),
            suggestion: "Check that the source is still there and readable, then try again.".into(),
            raw_detail,
            retry_hint: true,
            action_kind: None,
        },
        W::WriteError { path, message } => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Couldn't write to the destination".into(),
            explanation: format!("Cmdr ran into a problem writing to `{path}`: {message}."),
            suggestion: "Check that the destination is still reachable and has space, then try again.".into(),
            raw_detail,
            retry_hint: true,
            action_kind: None,
        },
        W::NameTooLong { path } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Name too long".into(),
            explanation: format!("The filesystem at `{path}` doesn't accept names this long."),
            suggestion: "Rename the file to something shorter and try again.".into(),
            raw_detail,
            retry_hint: false,
            action_kind: None,
        },
        W::InvalidName { path, message } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Invalid file name".into(),
            explanation: format!("The destination at `{path}` won't accept this name: {message}."),
            suggestion: "Rename the file (avoid special characters), then try again.".into(),
            raw_detail,
            retry_hint: false,
            action_kind: None,
        },
        W::IoError { path, message } => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Something went wrong".into(),
            explanation: format!("Cmdr ran into an unexpected problem at `{path}`: {message}."),
            suggestion: "Try again. If it keeps happening, expand **Technical details** below to share the specifics."
                .into(),
            raw_detail,
            retry_hint: true,
            action_kind: None,
        },
    }
}
