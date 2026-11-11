//! The characterization tables: every errno up to 4096, through the public entry point, with
//! the whole outcome it had before the listing errors read classes. macOS's as literal numbers
//! (the table the errno arms matched), Linux's by `libc` name.

use std::path::Path;

use super::tests::make_io_error;
use super::*;

/// The frozen characterization table for macOS: the 37 numbers the errno table matched
/// before it read classes, as literals, each with its whole wire outcome (category, reason
/// and its path, retry hint, action, and the `NAME (os error N)` detail); every other
/// number up to 4096 is the unknown read problem named `UNKNOWN`. Literal numbers on
/// purpose: this proves the class table reproduces the number table on a real Mac, not a
/// re-reading of it.
#[cfg(target_os = "macos")]
#[test]
fn every_macos_errno_keeps_its_listing_outcome() {
    const P: &str = "/p";
    let table = [
        (
            1,
            "EPERM",
            ErrorCategory::NeedsAction,
            false,
            true,
            ListingErrorReason::NotPermitted { path: P.to_string() },
        ),
        (
            2,
            "ENOENT",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::PathNotFoundErrno { path: P.to_string() },
        ),
        (
            4,
            "EINTR",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::Interrupted,
        ),
        (
            5,
            "EIO",
            ErrorCategory::Serious,
            true,
            false,
            ListingErrorReason::DiskReadProblem { path: P.to_string() },
        ),
        (
            12,
            "ENOMEM",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::NotEnoughMemory,
        ),
        (
            13,
            "EACCES",
            ErrorCategory::NeedsAction,
            false,
            true,
            ListingErrorReason::NoPermissionErrno { path: P.to_string() },
        ),
        (
            16,
            "EBUSY",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::ResourceBusy { path: P.to_string() },
        ),
        (
            17,
            "EEXIST",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::AlreadyExistsErrno { path: P.to_string() },
        ),
        (
            18,
            "EXDEV",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::CrossDeviceOperation,
        ),
        (
            20,
            "ENOTDIR",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::NotAFolder { path: P.to_string() },
        ),
        (
            21,
            "EISDIR",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::IsAFolderErrno { path: P.to_string() },
        ),
        (
            22,
            "EINVAL",
            ErrorCategory::Serious,
            true,
            false,
            ListingErrorReason::UnexpectedSystemResponse,
        ),
        (
            28,
            "ENOSPC",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::DiskFullErrno,
        ),
        (
            30,
            "EROFS",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::ReadOnlyVolumeErrno,
        ),
        (
            35,
            "EAGAIN",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::TemporarilyUnavailable,
        ),
        (
            45,
            "ENOTSUP",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::NotSupportedErrno,
        ),
        (
            50,
            "ENETDOWN",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::NetworkDown,
        ),
        (
            51,
            "ENETUNREACH",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::NetworkUnreachable,
        ),
        (
            52,
            "ENETRESET",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::NetworkConnectionDropped,
        ),
        (
            53,
            "ECONNABORTED",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::ConnectionDropped,
        ),
        (
            54,
            "ECONNRESET",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::ConnectionReset,
        ),
        (
            60,
            "ETIMEDOUT",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::ConnectionTimedOutErrno,
        ),
        (
            61,
            "ECONNREFUSED",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::ConnectionRefused,
        ),
        (
            62,
            "ELOOP",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::SymlinkLoopErrno { path: P.to_string() },
        ),
        (
            63,
            "ENAMETOOLONG",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::NameTooLongErrno,
        ),
        (
            64,
            "EHOSTDOWN",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::HostDown,
        ),
        (
            65,
            "EHOSTUNREACH",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::HostUnreachable,
        ),
        (
            66,
            "ENOTEMPTY",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::FolderNotEmpty { path: P.to_string() },
        ),
        (
            69,
            "EDQUOT",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::QuotaExceeded,
        ),
        (
            70,
            "ESTALE",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::StaleConnection,
        ),
        (
            77,
            "ENOLCK",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::LockUnavailable,
        ),
        (
            80,
            "EAUTH",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::AuthRequiredEauth,
        ),
        (
            81,
            "ENEEDAUTH",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::AuthRequiredEneedauth,
        ),
        (
            82,
            "EPWROFF",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::DevicePoweredOff,
        ),
        (
            83,
            "EDEVERR",
            ErrorCategory::Serious,
            true,
            false,
            ListingErrorReason::DeviceProblem,
        ),
        (
            89,
            "ECANCELED",
            ErrorCategory::Transient,
            true,
            false,
            ListingErrorReason::CancelledErrno,
        ),
        (
            93,
            "ENOATTR",
            ErrorCategory::NeedsAction,
            false,
            false,
            ListingErrorReason::AttributeNotFound,
        ),
    ];
    let wire = |listed: &ListingError| serde_json::to_value(listed).expect("a listing error serializes");
    for code in 0..=4096 {
        let expected = match table.iter().find(|row| row.0 == code) {
            Some((_, name, category, retry_hint, privacy, reason)) => ListingError {
                category: *category,
                reason: reason.clone(),
                provider: None,
                action_kind: privacy.then_some(ErrorActionKind::OpenPrivacySettings),
                retry_hint: *retry_hint,
                raw_detail: format!("{name} (os error {code})"),
            },
            None => ListingError {
                category: ErrorCategory::Serious,
                reason: ListingErrorReason::CouldntReadUnknown { path: P.to_string() },
                provider: None,
                action_kind: None,
                retry_hint: true,
                raw_detail: format!("UNKNOWN (os error {code})"),
            },
        };
        let listed = listing_error_from_volume_error(&make_io_error(code), Path::new(P));
        assert_eq!(wire(&listed), wire(&expected), "errno {code}");
    }
}

/// The characterization table for Linux, every errno: the nine `std` reads as a
/// connection blip or a busy resource are `Transient` with their own reason, and
/// every other one is the unknown read problem, retry hint on, no action.
#[cfg(target_os = "linux")]
#[test]
fn every_linux_errno_keeps_its_listing_outcome() {
    let path = Path::new("/p");
    let busy = ListingErrorReason::ResourceBusy { path: "/p".to_string() };
    let transient = [
        (libc::EINTR, ListingErrorReason::Interrupted),
        (libc::ENOMEM, ListingErrorReason::NotEnoughMemory),
        (libc::EBUSY, busy),
        (libc::EAGAIN, ListingErrorReason::TemporarilyUnavailable),
        (libc::ENETDOWN, ListingErrorReason::NetworkDown),
        (libc::ECONNABORTED, ListingErrorReason::ConnectionDropped),
        (libc::ECONNRESET, ListingErrorReason::ConnectionReset),
        (libc::ETIMEDOUT, ListingErrorReason::ConnectionTimedOutErrno),
        (libc::ESTALE, ListingErrorReason::StaleConnection),
    ];
    let unknown = ListingErrorReason::CouldntReadUnknown { path: "/p".to_string() };
    for code in 0..=4096 {
        let listed = listing_error_from_volume_error(&make_io_error(code), path);
        let (category, reason) = match transient.iter().find(|(errno, _)| *errno == code) {
            Some((_, reason)) => (ErrorCategory::Transient, reason),
            None => (ErrorCategory::Serious, &unknown),
        };
        assert_eq!(listed.category, category, "errno {code}");
        assert_eq!(
            std::mem::discriminant(&listed.reason),
            std::mem::discriminant(reason),
            "errno {code}: {:?}",
            listed.reason
        );
        assert!(listed.retry_hint, "errno {code}");
        assert_eq!(listed.action_kind, None, "errno {code}");
        assert_eq!(
            listed.raw_detail,
            format!("I/O error: test error {code}"),
            "errno {code}"
        );
    }
}
