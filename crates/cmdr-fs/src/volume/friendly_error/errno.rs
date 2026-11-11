//! `OsErrorClass → ListingError`.
//!
//! macOS-only mapping with a non-macOS fallback. Called from
//! `volume_error::listing_error_from_volume_error` for an `IoError` that carries an OS
//! code, with that code's class (`crate::os_error`).
//! Kept separate because it's a bulk of independent errno arms, and folding it in
//! with the rest of the classification would dwarf the genuinely semantic logic.
//!
//! Each arm emits a typed `reason` + params; the user-facing words live on the
//! frontend (`src/lib/error-messages/listing-error-messages.ts`).

use std::path::Path;

#[cfg(target_os = "macos")]
use super::ErrorActionKind;
use super::{ErrorCategory, ListingError, ListingErrorReason};
use crate::os_error::OsErrorClass;
use crate::volume::VolumeError;

/// Maps a macOS errno's class to a `ListingError`; `errno` is the number itself, for
/// the technical detail.
#[cfg(target_os = "macos")]
pub(super) fn listing_error_from_class(
    class: OsErrorClass,
    errno: i32,
    path: &Path,
    _err: &VolumeError,
) -> ListingError {
    let path_display = path.display().to_string();
    let raw_detail = format!("{} (os error {})", errno_name(class), errno);

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

    match class {
        // ── Transient (retry-worthy) ────────────────────────────────────
        OsErrorClass::Interrupted => transient(ListingErrorReason::Interrupted),
        OsErrorClass::OutOfMemory => transient(ListingErrorReason::NotEnoughMemory),
        OsErrorClass::Busy => transient(ListingErrorReason::ResourceBusy { path: path_display }),
        OsErrorClass::WouldBlock => transient(ListingErrorReason::TemporarilyUnavailable),
        OsErrorClass::NetworkDown => transient(ListingErrorReason::NetworkDown),
        OsErrorClass::NetworkReset => transient(ListingErrorReason::NetworkConnectionDropped),
        OsErrorClass::ConnectionAborted => transient(ListingErrorReason::ConnectionDropped),
        OsErrorClass::ConnectionReset => transient(ListingErrorReason::ConnectionReset),
        OsErrorClass::TimedOut => transient(ListingErrorReason::ConnectionTimedOutErrno),
        OsErrorClass::HostDown => transient(ListingErrorReason::HostDown),
        OsErrorClass::Stale => transient(ListingErrorReason::StaleConnection),
        OsErrorClass::NoLocks => transient(ListingErrorReason::LockUnavailable),
        OsErrorClass::Canceled => transient(ListingErrorReason::CancelledErrno),

        // ── NeedsAction ─────────────────────────────────────────────────
        OsErrorClass::NotPermitted => ListingError {
            category: ErrorCategory::NeedsAction,
            reason: ListingErrorReason::NotPermitted { path: path_display },
            provider: None,
            action_kind: Some(ErrorActionKind::OpenPrivacySettings),
            retry_hint: false,
            raw_detail,
        },
        OsErrorClass::NotFound => needs_action(ListingErrorReason::PathNotFoundErrno { path: path_display }),
        OsErrorClass::AccessDenied => ListingError {
            category: ErrorCategory::NeedsAction,
            reason: ListingErrorReason::NoPermissionErrno { path: path_display },
            provider: None,
            action_kind: Some(ErrorActionKind::OpenPrivacySettings),
            retry_hint: false,
            raw_detail,
        },
        OsErrorClass::AlreadyExists => needs_action(ListingErrorReason::AlreadyExistsErrno { path: path_display }),
        OsErrorClass::CrossDevice => needs_action(ListingErrorReason::CrossDeviceOperation),
        OsErrorClass::NotADirectory => needs_action(ListingErrorReason::NotAFolder { path: path_display }),
        OsErrorClass::IsADirectory => needs_action(ListingErrorReason::IsAFolderErrno { path: path_display }),
        OsErrorClass::NoSpace => needs_action(ListingErrorReason::DiskFullErrno),
        OsErrorClass::ReadOnlyFilesystem => needs_action(ListingErrorReason::ReadOnlyVolumeErrno),
        OsErrorClass::Unsupported => needs_action(ListingErrorReason::NotSupportedErrno),
        OsErrorClass::NetworkUnreachable => needs_action(ListingErrorReason::NetworkUnreachable),
        OsErrorClass::ConnectionRefused => needs_action(ListingErrorReason::ConnectionRefused),
        OsErrorClass::LinkLoop => needs_action(ListingErrorReason::SymlinkLoopErrno { path: path_display }),
        OsErrorClass::NameTooLong => needs_action(ListingErrorReason::NameTooLongErrno),
        OsErrorClass::HostUnreachable => needs_action(ListingErrorReason::HostUnreachable),
        OsErrorClass::NotEmpty => needs_action(ListingErrorReason::FolderNotEmpty { path: path_display }),
        OsErrorClass::QuotaExceeded => needs_action(ListingErrorReason::QuotaExceeded),
        OsErrorClass::AuthFailed => needs_action(ListingErrorReason::AuthRequiredEauth),
        OsErrorClass::AuthNeeded => needs_action(ListingErrorReason::AuthRequiredEneedauth),
        OsErrorClass::DevicePoweredOff => needs_action(ListingErrorReason::DevicePoweredOff),
        OsErrorClass::NoAttribute => needs_action(ListingErrorReason::AttributeNotFound),

        // ── Serious ─────────────────────────────────────────────────────
        OsErrorClass::Io => serious(ListingErrorReason::DiskReadProblem { path: path_display }),
        OsErrorClass::InvalidArgument => serious(ListingErrorReason::UnexpectedSystemResponse),
        OsErrorClass::DeviceError => serious(ListingErrorReason::DeviceProblem),

        // ── No reason of its own ────────────────────────────────────────
        // Listed, not wildcarded, so a new class decides where it goes.
        OsErrorClass::DeviceNotConfigured
        | OsErrorClass::NoDevice
        | OsErrorClass::InUseByAnotherApp
        | OsErrorClass::InappropriateIoctl
        | OsErrorClass::NotImplemented
        | OsErrorClass::OperationNotSupported
        | OsErrorClass::FileTooLarge
        | OsErrorClass::TooManyLinks
        | OsErrorClass::BrokenPipe
        | OsErrorClass::NotConnected
        | OsErrorClass::Remote
        | OsErrorClass::Protocol
        | OsErrorClass::BadMessage
        | OsErrorClass::Other => serious(ListingErrorReason::CouldntReadUnknown { path: path_display }),
    }
}

/// Fallback for every other platform: nine of the classes the macOS table calls
/// transient (the ones `std`'s `ErrorKind` named when this read through it), and
/// everything else as an unknown read problem. Windows gets the same answer until
/// its codes are characterized: by class, so no number is assumed.
///
/// The transient arm matters beyond the copy: a stalled listing retries exactly
/// the `Transient` category (`crates/cmdr-listing/src/stall.rs` in the app).
#[cfg(any(target_os = "linux", target_os = "windows", target_family = "wasm"))]
pub(super) fn listing_error_from_class(
    class: OsErrorClass,
    _errno: i32,
    path: &Path,
    err: &VolumeError,
) -> ListingError {
    let transient = match class {
        OsErrorClass::Interrupted => Some(ListingErrorReason::Interrupted),
        OsErrorClass::OutOfMemory => Some(ListingErrorReason::NotEnoughMemory),
        OsErrorClass::Busy => Some(ListingErrorReason::ResourceBusy {
            path: path.display().to_string(),
        }),
        OsErrorClass::WouldBlock => Some(ListingErrorReason::TemporarilyUnavailable),
        OsErrorClass::NetworkDown => Some(ListingErrorReason::NetworkDown),
        OsErrorClass::ConnectionAborted => Some(ListingErrorReason::ConnectionDropped),
        OsErrorClass::ConnectionReset => Some(ListingErrorReason::ConnectionReset),
        OsErrorClass::TimedOut => Some(ListingErrorReason::ConnectionTimedOutErrno),
        OsErrorClass::Stale => Some(ListingErrorReason::StaleConnection),
        _ => None,
    };
    let (category, reason) = match transient {
        Some(reason) => (ErrorCategory::Transient, reason),
        None => (
            ErrorCategory::Serious,
            ListingErrorReason::CouldntReadUnknown {
                path: path.display().to_string(),
            },
        ),
    };
    ListingError {
        category,
        reason,
        provider: None,
        action_kind: None,
        retry_hint: true,
        raw_detail: err.to_string(),
    }
}

/// The C constant name of a macOS errno, by its class: the names the table above
/// reads, and `UNKNOWN` for the rest.
#[cfg(target_os = "macos")]
fn errno_name(class: OsErrorClass) -> &'static str {
    match class {
        OsErrorClass::NotPermitted => "EPERM",
        OsErrorClass::NotFound => "ENOENT",
        OsErrorClass::Interrupted => "EINTR",
        OsErrorClass::Io => "EIO",
        OsErrorClass::OutOfMemory => "ENOMEM",
        OsErrorClass::AccessDenied => "EACCES",
        OsErrorClass::Busy => "EBUSY",
        OsErrorClass::AlreadyExists => "EEXIST",
        OsErrorClass::CrossDevice => "EXDEV",
        OsErrorClass::NotADirectory => "ENOTDIR",
        OsErrorClass::IsADirectory => "EISDIR",
        OsErrorClass::InvalidArgument => "EINVAL",
        OsErrorClass::NoSpace => "ENOSPC",
        OsErrorClass::ReadOnlyFilesystem => "EROFS",
        OsErrorClass::WouldBlock => "EAGAIN",
        OsErrorClass::Unsupported => "ENOTSUP",
        OsErrorClass::NetworkDown => "ENETDOWN",
        OsErrorClass::NetworkUnreachable => "ENETUNREACH",
        OsErrorClass::NetworkReset => "ENETRESET",
        OsErrorClass::ConnectionAborted => "ECONNABORTED",
        OsErrorClass::ConnectionReset => "ECONNRESET",
        OsErrorClass::TimedOut => "ETIMEDOUT",
        OsErrorClass::ConnectionRefused => "ECONNREFUSED",
        OsErrorClass::LinkLoop => "ELOOP",
        OsErrorClass::NameTooLong => "ENAMETOOLONG",
        OsErrorClass::HostDown => "EHOSTDOWN",
        OsErrorClass::HostUnreachable => "EHOSTUNREACH",
        OsErrorClass::NotEmpty => "ENOTEMPTY",
        OsErrorClass::QuotaExceeded => "EDQUOT",
        OsErrorClass::Stale => "ESTALE",
        OsErrorClass::NoLocks => "ENOLCK",
        OsErrorClass::AuthFailed => "EAUTH",
        OsErrorClass::AuthNeeded => "ENEEDAUTH",
        OsErrorClass::DevicePoweredOff => "EPWROFF",
        OsErrorClass::DeviceError => "EDEVERR",
        OsErrorClass::Canceled => "ECANCELED",
        OsErrorClass::NoAttribute => "ENOATTR",
        _ => "UNKNOWN",
    }
}
