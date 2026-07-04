//! `VolumeError → ListingError`.
//!
//! Most variants delegate to the canonical `kinds::*` constructors so the typed
//! `reason` is the same regardless of which layer the error originated in. A few
//! variants are unique to this layer:
//! - `IoError { raw_os_error: Some(_) }` dispatches to `errno::listing_error_from_errno` for
//!   per-errno reasons
//! - `FriendlyGit(_)` carries a typed `FriendlyGitErrorKind` from the git module, which rides as
//!   the `Git` reason so the FE renders git copy from its parallel git factory

use std::path::Path;

use super::errno::listing_error_from_errno;
use super::{ErrorCategory, ListingError, ListingErrorReason, kinds};
use crate::file_system::volume::VolumeError;

/// Converts a `VolumeError` into a typed, word-free `ListingError`.
///
/// For `IoError` with a `raw_os_error`, matches against platform-specific errno codes.
/// For typed `VolumeError` variants, delegates to the shared `kinds::*` constructors.
///
/// Git failures arrive as `VolumeError::FriendlyGit(FriendlyGitError)` from the
/// `file_system::git` volume hooks; the `FriendlyGit` arm is matched FIRST (the
/// Layer-0 pass-through), before any errno mapping, so git copy isn't clobbered
/// by the generic I/O fallback. The carried kind rides as the `Git` reason and
/// the FE renders the git-specific copy.
/// The "this archive can't be read" listing error, produced at the listing seam
/// when a `.zip` browse fails on a damaged/encrypted/unsupported or mislabeled
/// archive (the `ArchiveVolume` collapses those to `NotSupported`/`IoError`, so
/// they're detected from the path + error kind, not a dedicated `VolumeError`).
/// `Serious` (corrupted data), no retry, no action button.
pub fn archive_unreadable_listing_error() -> ListingError {
    ListingError {
        category: ErrorCategory::Serious,
        reason: ListingErrorReason::ArchiveUnreadable,
        provider: None,
        action_kind: None,
        retry_hint: false,
        raw_detail: "archive unreadable".to_string(),
    }
}

pub fn listing_error_from_volume_error(err: &VolumeError, path: &Path) -> ListingError {
    let path_display = path.display().to_string();
    let raw = err.to_string();

    match err {
        VolumeError::FriendlyGit(git_err) => {
            let raw_detail = git_err.raw_detail();
            ListingError {
                category: git_err.kind.category(),
                reason: ListingErrorReason::Git { kind: git_err.kind },
                provider: None,
                action_kind: None,
                retry_hint: matches!(git_err.kind.category(), ErrorCategory::Transient),
                raw_detail,
            }
        }
        VolumeError::NotFound(_) => kinds::not_found(&path_display, raw),
        VolumeError::PermissionDenied(_) => {
            // If this is a known TCC-restricted path (Downloads/Documents/Desktop/...
            // or a network volume), surface the dedicated reason that points to both
            // Full Disk Access AND the per-folder Files & Folders pane. Otherwise
            // fall through to the generic permission-denied reason.
            if crate::restricted_paths::tcc_paths::is_potentially_tcc_restricted(path)
                || crate::restricted_paths::tcc_paths::is_network_volume_path(path)
            {
                kinds::tcc_restricted(&path_display, raw)
            } else {
                kinds::permission_denied(&path_display, raw)
            }
        }
        VolumeError::AlreadyExists(_) => kinds::already_exists(&path_display, raw),
        VolumeError::NotSupported => kinds::not_supported(raw),
        VolumeError::DeviceDisconnected(_) => kinds::device_disconnected(&path_display, raw),
        VolumeError::ReadOnly(_) => kinds::read_only(raw),
        VolumeError::StorageFull { .. } => kinds::storage_full(raw),
        VolumeError::ConnectionTimeout(_) => kinds::connection_timeout(raw),
        VolumeError::Cancelled(_) => kinds::cancelled(raw),
        VolumeError::DeletePending(_) => kinds::delete_pending(&path_display, raw),
        // Write-only error (the MTP upload path's stale-handle signal); it never
        // reaches the listing pipeline. Mapped defensively to a not-found on the
        // path actually being listed (never a source path), so the match stays
        // exhaustive without inventing a listing reason for a write condition.
        VolumeError::StaleDestinationHandle(_) => kinds::not_found(&path_display, raw),
        VolumeError::IsADirectory(_) => ListingError {
            category: ErrorCategory::NeedsAction,
            reason: ListingErrorReason::IsADirectory { path: path_display },
            provider: None,
            action_kind: None,
            retry_hint: false,
            raw_detail: raw,
        },
        VolumeError::IoError {
            raw_os_error: Some(errno),
            ..
        } => listing_error_from_errno(*errno, path, err),
        VolumeError::IoError {
            raw_os_error: None,
            message,
        } => kinds::io_serious(&path_display, message, raw),
    }
}
