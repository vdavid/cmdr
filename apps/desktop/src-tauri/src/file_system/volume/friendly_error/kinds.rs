//! Shared `ListingError` constructors keyed by conceptual error kind.
//!
//! `volume_error::listing_error_from_volume_error` (listing path) maps several
//! `VolumeError` variants to the same conceptual outcome: "not found",
//! "permission denied", "device disconnected", and so on. Routing them through
//! one constructor per kind keeps the typed `reason` consistent regardless of
//! which arm produced the error.
//!
//! Each function here returns the canonical `ListingError` for one kind. The
//! caller passes the raw-detail string and any kind-specific data (the path,
//! error message, etc.). The user-facing words live on the frontend, keyed off
//! the `reason` carried here.
//!
//! Variants that don't share semantics (e.g. `VolumeError::FriendlyGit`) stay
//! inline in their mapper.

use super::{ErrorActionKind, ErrorCategory, ListingError, ListingErrorReason};

pub(super) fn not_found(path_display: &str, raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::NeedsAction,
        reason: ListingErrorReason::NotFound {
            path: path_display.to_string(),
        },
        provider: None,
        action_kind: None,
        retry_hint: false,
        raw_detail,
    }
}

/// Permission-denied on a path that macOS guards via TCC (Downloads, Documents,
/// Desktop, Pictures, Movies, Music, iCloud Drive, FileProvider domains, network
/// volumes, etc.; see `crate::restricted_paths::tcc_paths`). The user has two
/// distinct escape hatches (Full Disk Access for everything, or per-folder
/// Files & Folders for just this one), so it carries its own `reason`.
pub(super) fn tcc_restricted(path_display: &str, raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::NeedsAction,
        reason: ListingErrorReason::TccRestricted {
            path: path_display.to_string(),
        },
        provider: None,
        action_kind: Some(ErrorActionKind::OpenPrivacySettings),
        retry_hint: false,
        raw_detail,
    }
}

pub(super) fn permission_denied(path_display: &str, raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::NeedsAction,
        reason: ListingErrorReason::PermissionDenied {
            path: path_display.to_string(),
        },
        provider: None,
        action_kind: Some(ErrorActionKind::OpenPrivacySettings),
        retry_hint: false,
        raw_detail,
    }
}

pub(super) fn already_exists(path_display: &str, raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::NeedsAction,
        reason: ListingErrorReason::AlreadyExists {
            path: path_display.to_string(),
        },
        provider: None,
        action_kind: None,
        retry_hint: false,
        raw_detail,
    }
}

pub(super) fn cancelled(raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::Transient,
        reason: ListingErrorReason::Cancelled,
        provider: None,
        action_kind: None,
        retry_hint: true,
        raw_detail,
    }
}

pub(super) fn device_disconnected(path_display: &str, raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::NeedsAction,
        reason: ListingErrorReason::DeviceDisconnected {
            path: path_display.to_string(),
        },
        provider: None,
        action_kind: None,
        // listing path doesn't show a Retry button (the user navigates back).
        retry_hint: false,
        raw_detail,
    }
}

/// The device's session died but the device is still attached and a reopen is
/// already running. `Transient` with a retry hint, deliberately unlike
/// [`device_disconnected`]: the user has nothing to plug in or unlock, they just
/// need to try again in a moment.
pub(super) fn device_reconnecting(path_display: &str, raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::Transient,
        reason: ListingErrorReason::DeviceReconnecting {
            path: path_display.to_string(),
        },
        provider: None,
        action_kind: None,
        retry_hint: true,
        raw_detail,
    }
}

pub(super) fn read_only(raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::NeedsAction,
        reason: ListingErrorReason::ReadOnly,
        provider: None,
        action_kind: None,
        retry_hint: false,
        raw_detail,
    }
}

pub(super) fn storage_full(raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::NeedsAction,
        reason: ListingErrorReason::StorageFull,
        provider: None,
        action_kind: None,
        retry_hint: false,
        raw_detail,
    }
}

pub(super) fn connection_timeout(raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::Transient,
        reason: ListingErrorReason::ConnectionTimedOut,
        provider: None,
        action_kind: None,
        retry_hint: true,
        raw_detail,
    }
}

pub(super) fn not_supported(raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::NeedsAction,
        reason: ListingErrorReason::NotSupported,
        provider: None,
        action_kind: None,
        retry_hint: false,
        raw_detail,
    }
}

/// `STATUS_DELETE_PENDING`: the file has been marked for deletion on the server
/// but at least one open handle is keeping it alive. The file disappears the
/// moment the last handle closes, so retry-after-a-moment is the right hint.
pub(super) fn delete_pending(path_display: &str, raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::Transient,
        reason: ListingErrorReason::DeletePending {
            path: path_display.to_string(),
        },
        provider: None,
        action_kind: None,
        retry_hint: true,
        raw_detail,
    }
}

pub(super) fn io_serious(path_display: &str, message: &str, raw_detail: String) -> ListingError {
    ListingError {
        category: ErrorCategory::Serious,
        reason: ListingErrorReason::IoSerious {
            path: path_display.to_string(),
            os_message: message.to_string(),
        },
        provider: None,
        action_kind: None,
        retry_hint: true,
        raw_detail,
    }
}
