//! `errno → ListingError`.
//!
//! macOS-only mapping with a non-macOS fallback. Called from
//! `volume_error::listing_error_from_volume_error` for `IoError` with a `raw_os_error`.
//! Kept separate because it's a bulk of independent errno arms, and folding it in
//! with the rest of the classification would dwarf the genuinely semantic logic.
//!
//! Each arm emits a typed `reason` + params; the user-facing words live on the
//! frontend (`src/lib/errors/listing-error-messages.ts`).

use std::path::Path;

#[cfg(target_os = "macos")]
use super::ErrorActionKind;
use super::{ErrorCategory, ListingError, ListingErrorReason};
use crate::file_system::volume::VolumeError;

/// Maps a raw macOS errno to a `ListingError`.
#[cfg(target_os = "macos")]
pub(super) fn listing_error_from_errno(errno: i32, path: &Path, _err: &VolumeError) -> ListingError {
    let path_display = path.display().to_string();
    let raw_detail = format!("{} (os error {})", errno_name(errno), errno);

    // Helper: a transient errno with no path param and no special action.
    let transient = |reason: ListingErrorReason| ListingError {
        category: ErrorCategory::Transient,
        reason,
        provider: None,
        action_kind: None,
        retry_hint: true,
        raw_detail: raw_detail.clone(),
    };
    // Helper: a needs-action errno with no path param and no special action.
    let needs_action = |reason: ListingErrorReason| ListingError {
        category: ErrorCategory::NeedsAction,
        reason,
        provider: None,
        action_kind: None,
        retry_hint: false,
        raw_detail: raw_detail.clone(),
    };
    // Helper: a serious errno (retry hint on, no path param).
    let serious = |reason: ListingErrorReason| ListingError {
        category: ErrorCategory::Serious,
        reason,
        provider: None,
        action_kind: None,
        retry_hint: true,
        raw_detail: raw_detail.clone(),
    };

    match errno {
        // ── Transient (retry-worthy) ────────────────────────────────────
        4 => transient(ListingErrorReason::Interrupted),      // EINTR
        12 => transient(ListingErrorReason::NotEnoughMemory), // ENOMEM
        16 => transient(ListingErrorReason::ResourceBusy { path: path_display }), // EBUSY
        35 => transient(ListingErrorReason::TemporarilyUnavailable), // EAGAIN
        50 => transient(ListingErrorReason::NetworkDown),     // ENETDOWN
        52 => transient(ListingErrorReason::NetworkConnectionDropped), // ENETRESET
        53 => transient(ListingErrorReason::ConnectionDropped), // ECONNABORTED
        54 => transient(ListingErrorReason::ConnectionReset), // ECONNRESET
        60 => transient(ListingErrorReason::ConnectionTimedOutErrno), // ETIMEDOUT
        64 => transient(ListingErrorReason::HostDown),        // EHOSTDOWN
        70 => transient(ListingErrorReason::StaleConnection), // ESTALE
        77 => transient(ListingErrorReason::LockUnavailable), // ENOLCK
        89 => transient(ListingErrorReason::CancelledErrno),  // ECANCELED

        // ── NeedsAction ─────────────────────────────────────────────────
        // EPERM: action_kind = OpenPrivacySettings.
        1 => ListingError {
            category: ErrorCategory::NeedsAction,
            reason: ListingErrorReason::NotPermitted { path: path_display },
            provider: None,
            action_kind: Some(ErrorActionKind::OpenPrivacySettings),
            retry_hint: false,
            raw_detail,
        },
        2 => needs_action(ListingErrorReason::PathNotFoundErrno { path: path_display }), // ENOENT
        // EACCES: action_kind = OpenPrivacySettings.
        13 => ListingError {
            category: ErrorCategory::NeedsAction,
            reason: ListingErrorReason::NoPermissionErrno { path: path_display },
            provider: None,
            action_kind: Some(ErrorActionKind::OpenPrivacySettings),
            retry_hint: false,
            raw_detail,
        },
        17 => needs_action(ListingErrorReason::AlreadyExistsErrno { path: path_display }), // EEXIST
        18 => needs_action(ListingErrorReason::CrossDeviceOperation),                      // EXDEV
        20 => needs_action(ListingErrorReason::NotAFolder { path: path_display }),         // ENOTDIR
        21 => needs_action(ListingErrorReason::IsAFolderErrno { path: path_display }),     // EISDIR
        28 => needs_action(ListingErrorReason::DiskFullErrno),                             // ENOSPC
        30 => needs_action(ListingErrorReason::ReadOnlyVolumeErrno),                       // EROFS
        45 => needs_action(ListingErrorReason::NotSupportedErrno),                         // ENOTSUP
        51 => needs_action(ListingErrorReason::NetworkUnreachable),                        // ENETUNREACH
        61 => needs_action(ListingErrorReason::ConnectionRefused),                         // ECONNREFUSED
        62 => needs_action(ListingErrorReason::SymlinkLoopErrno { path: path_display }),   // ELOOP
        63 => needs_action(ListingErrorReason::NameTooLongErrno),                          // ENAMETOOLONG
        65 => needs_action(ListingErrorReason::HostUnreachable),                           // EHOSTUNREACH
        66 => needs_action(ListingErrorReason::FolderNotEmpty { path: path_display }),     // ENOTEMPTY
        69 => needs_action(ListingErrorReason::QuotaExceeded),                             // EDQUOT
        80 => needs_action(ListingErrorReason::AuthRequiredEauth),                         // EAUTH
        81 => needs_action(ListingErrorReason::AuthRequiredEneedauth),                     // ENEEDAUTH
        82 => needs_action(ListingErrorReason::DevicePoweredOff),                          // EPWROFF
        93 => needs_action(ListingErrorReason::AttributeNotFound),                         // ENOATTR

        // ── Serious ─────────────────────────────────────────────────────
        5 => serious(ListingErrorReason::DiskReadProblem { path: path_display }), // EIO
        22 => serious(ListingErrorReason::UnexpectedSystemResponse),              // EINVAL
        83 => serious(ListingErrorReason::DeviceProblem),                         // EDEVERR

        // ── Unknown errno ───────────────────────────────────────────────
        _ => serious(ListingErrorReason::CouldntReadUnknown { path: path_display }),
    }
}

/// Fallback for non-macOS platforms (mapping will be expanded later).
#[cfg(not(target_os = "macos"))]
pub(super) fn listing_error_from_errno(_errno: i32, path: &Path, err: &VolumeError) -> ListingError {
    ListingError {
        category: ErrorCategory::Serious,
        reason: ListingErrorReason::CouldntReadUnknown {
            path: path.display().to_string(),
        },
        provider: None,
        action_kind: None,
        retry_hint: true,
        raw_detail: err.to_string(),
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
