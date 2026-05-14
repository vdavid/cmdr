//! `WriteOperationError → FriendlyError`.
//!
//! Used by `WriteErrorEvent::new` so every `write-error` event the FE receives
//! carries a rendered payload, even on local-FS paths where the original
//! `VolumeError` is no longer in scope. Variants that map cleanly back to a
//! "kind" delegate to the shared `kinds::*` constructors so the user sees
//! consistent copy regardless of which layer originated the error. Variants
//! unique to write operations (`InsufficientSpace`, `SymlinkLoop`,
//! `DestinationInsideSource`, `NameTooLong`, `InvalidName`, …) are handled
//! inline because they carry kind-specific data the shared constructors don't
//! take.
//!
//! Volume-aware paths (`volume_move`, `volume_copy`) keep the originating
//! `VolumeError + path` and prefer `friendly_error_from_volume_error` +
//! `enrich_with_provider` for richer provider-specific suggestions; this
//! function is the fallback when only the typed write error is in scope.

use super::{ErrorCategory, FriendlyError, kinds};

/// Converts a `WriteOperationError` into a `FriendlyError` for the transfer-error
/// dialog. Mirrors `friendly_error_from_volume_error` but works downstream of the
/// `map_volume_error` conversion, so it covers the local-FS error paths too.
pub fn friendly_from_write_error(err: &crate::file_system::write_operations::WriteOperationError) -> FriendlyError {
    use crate::file_system::write_operations::WriteOperationError as W;

    let raw = format!("{err:?}");
    match err {
        // Variants that map to a shared kind: same copy as the listing path.
        W::SourceNotFound { path } | W::SameLocation { path } => {
            // SameLocation and SourceNotFound both surface as "this file isn't usable" to the user.
            // Differentiating them here would just be noise; the typed variant is still in `error`
            // for programmatic FE handling if needed.
            kinds::not_found(path, raw)
        }
        W::DestinationExists { path } => kinds::already_exists(path, raw),
        W::PermissionDenied { path, .. } => kinds::permission_denied(path, raw),
        W::Cancelled { .. } => kinds::cancelled(raw),
        W::DeviceDisconnected { path } => {
            // Operation context: user can retry the move/copy after reconnecting.
            let mut friendly = kinds::device_disconnected(path, raw);
            friendly.retry_hint = true;
            friendly
        }
        W::ConnectionInterrupted { path } => {
            // ConnectionInterrupted is a transient version of ConnectionTimeout: a network
            // hiccup rather than a strict timeout. The kind copy is generic enough to fit both.
            let _ = path;
            kinds::connection_timeout(raw)
        }

        // Variants with kind-specific data: inline.
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
            raw_detail: raw,
            retry_hint: false,
            action_kind: None,
        },
        W::DestinationInsideSource { source, destination } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Destination is inside the source".into(),
            explanation: format!("Cmdr can't copy `{source}` into `{destination}` — that would loop forever."),
            suggestion: "Pick a destination outside the source folder.".into(),
            raw_detail: raw,
            retry_hint: false,
            action_kind: None,
        },
        W::SymlinkLoop { path } => FriendlyError {
            category: ErrorCategory::Serious,
            title: "Symlink loop".into(),
            explanation: format!("`{path}` contains symlinks that point back at themselves."),
            suggestion: "Resolve the loop manually before retrying.".into(),
            raw_detail: raw,
            retry_hint: false,
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
            raw_detail: raw,
            retry_hint: false,
            action_kind: None,
        },
        W::FileLocked { path } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "File is locked".into(),
            explanation: format!("`{path}` is locked and can't be changed."),
            suggestion: "On macOS, unlock it via Finder > Get Info > uncheck Locked. Then try again.".into(),
            raw_detail: raw,
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
            raw_detail: raw,
            retry_hint: false,
            action_kind: None,
        },
        W::ReadError { path, message } => kinds::io_serious(path, message, raw),
        W::WriteError { path, message } => {
            let mut friendly = kinds::io_serious(path, message, raw);
            friendly.title = "Couldn't write to the destination".into();
            friendly
        }
        W::NameTooLong { path } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Name too long".into(),
            explanation: format!("The filesystem at `{path}` doesn't accept names this long."),
            suggestion: "Rename the file to something shorter and try again.".into(),
            raw_detail: raw,
            retry_hint: false,
            action_kind: None,
        },
        W::InvalidName { path, message } => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "Invalid file name".into(),
            explanation: format!("The destination at `{path}` won't accept this name: {message}."),
            suggestion: "Rename the file (avoid special characters), then try again.".into(),
            raw_detail: raw,
            retry_hint: false,
            action_kind: None,
        },
        W::IoError { path, message } => kinds::io_serious(path, message, raw),
    }
}
