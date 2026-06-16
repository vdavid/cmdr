//! Error CLASSIFICATION: turns raw errors + path into a TYPED, word-free
//! classification the frontend renders.
//!
//! The backend emits a `ListingError` (a `category`, a semantic `reason` with
//! structured params, an optional detected `provider`, an optional `actionKind`,
//! a `retryHint`, and a technical `rawDetail`) and ZERO user-facing prose. The
//! frontend owns 100% of the titles/explanations/suggestions, rendered from this
//! typed data (`src/lib/errors/`).
//!
//! Sources that produce a `ListingError`, each in its own sibling module:
//! - `volume_error`: `VolumeError` (used by listing-error path; richest, dispatches to errno on raw
//!   `IoError`)
//! - `errno`: raw macOS errnos with a non-macOS fallback (called from `volume_error` when an
//!   `IoError` carries a `raw_os_error`)
//! - `empty_root`: TCC-restricted volume root hint (a single special case)
//!
//! The write-error path (`write-error` events) ships only the typed
//! `WriteOperationError`; the frontend renders its copy and classification.
//!
//! `enrich_with_provider` (in submodule `provider`) detects the cloud/mount
//! provider from the path and SETS the typed `provider` field. The frontend then
//! overlays the provider-specific suggestion (the words live in
//! `src/lib/errors/provider-error-messages.ts`).

mod empty_root;
mod errno;
mod kinds;
mod provider;
mod volume_error;

use serde::{Deserialize, Serialize};

use crate::file_system::git::friendly::FriendlyGitErrorKind;

// Public API re-exports: keep the `volume::friendly_error::*` import surface
// unchanged for callers regardless of how the module is split internally.
pub use empty_root::listing_error_for_restricted_empty_root;
pub use provider::{Provider, enrich_with_provider};
pub use volume_error::listing_error_from_volume_error;

// ============================================================================
// Data model
// ============================================================================

/// Typed action the frontend should offer alongside the error message.
///
/// Only set when a specific, platform-resolvable action is known. Defaults to `None`
/// for all other errors. The frontend uses this to render an action button without
/// substring-matching the title.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorActionKind {
    /// User should grant Full Disk Access in macOS System Settings → Privacy & Security.
    OpenPrivacySettings,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Might work if you retry (timeouts, temporary resource issues).
    Transient,
    /// User must do something (permission denied, disk full, device disconnected).
    NeedsAction,
    /// Something is genuinely broken (I/O hardware issues, corrupted data).
    Serious,
}

/// The typed, word-free classification of a listing/empty-root/git failure.
///
/// Carries everything the frontend needs to render the message itself: the
/// `category` (drives styling), the semantic `reason` (the FE switches on it to
/// pick the message factory; variant-carried params keep impossible param
/// combinations unrepresentable), the detected `provider` (FE overlays the
/// provider suggestion), the `action_kind` (drives the "Open System Settings"
/// button), the `retry_hint`, and the technical `raw_detail` (rendered as plain
/// text in the disclosure).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListingError {
    pub category: ErrorCategory,
    pub reason: ListingErrorReason,
    /// Detected cloud/mount provider, if any. The FE replaces the base reason's
    /// suggestion with the provider-specific one.
    pub provider: Option<Provider>,
    /// Typed action the frontend should offer. Drives the "Open System Settings"
    /// button without substring-matching the title.
    pub action_kind: Option<ErrorActionKind>,
    /// FE shows a "Try again" button when true.
    pub retry_hint: bool,
    /// For the technical-details disclosure, for example "ETIMEDOUT (os error 60)".
    /// Plain text, never markdown.
    pub raw_detail: String,
}

/// The semantic reason for a listing/empty-root/git failure. One variant per
/// currently-distinct message (errnos that share identical copy collapse to one
/// reason). Variant names AND field names match the TS `ListingErrorReason`
/// union (plus the wire-only `git` variant) member-for-member.
///
/// The frontend NEVER sees raw errno numbers: Rust maps errno → semantic reason,
/// the FE switches on the reason. Git rides as the `git` variant carrying its own
/// typed kind; the FE routes it to its parallel git factory.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "reason", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ListingErrorReason {
    // ── errno: transient ──
    Interrupted,
    NotEnoughMemory,
    ResourceBusy { path: String },
    TemporarilyUnavailable,
    NetworkDown,
    NetworkConnectionDropped,
    ConnectionDropped,
    ConnectionReset,
    ConnectionTimedOutErrno,
    HostDown,
    StaleConnection,
    LockUnavailable,
    CancelledErrno,
    // ── errno: needs-action ──
    NotPermitted { path: String },
    PathNotFoundErrno { path: String },
    NoPermissionErrno { path: String },
    AlreadyExistsErrno { path: String },
    CrossDeviceOperation,
    NotAFolder { path: String },
    IsAFolderErrno { path: String },
    DiskFullErrno,
    ReadOnlyVolumeErrno,
    NotSupportedErrno,
    NetworkUnreachable,
    ConnectionRefused,
    SymlinkLoopErrno { path: String },
    NameTooLongErrno,
    HostUnreachable,
    FolderNotEmpty { path: String },
    QuotaExceeded,
    AuthRequiredEauth,
    AuthRequiredEneedauth,
    DevicePoweredOff,
    AttributeNotFound,
    // ── errno: serious ──
    DiskReadProblem { path: String },
    UnexpectedSystemResponse,
    DeviceProblem,
    CouldntReadUnknown { path: String },
    // ── typed VolumeError variants (shared "kinds") ──
    NotFound { path: String },
    TccRestricted { path: String },
    PermissionDenied { path: String },
    AlreadyExists { path: String },
    Cancelled,
    DeviceDisconnected { path: String },
    ReadOnly,
    StorageFull,
    ConnectionTimedOut,
    NotSupported,
    DeletePending { path: String },
    IoSerious { path: String, os_message: String },
    IsADirectory { path: String },
    // ── empty-root hint ──
    EmptyRootICloud,
    // ── git (wire-only; FE routes to its parallel git factory) ──
    Git { kind: FriendlyGitErrorKind },
}

// ============================================================================
// Tests
// ============================================================================
//
// Tests live here because they exercise the public API (`listing_error_from_volume_error`,
// `listing_error_for_restricted_empty_root`) which dispatches across the sibling
// modules. The errno-arm tests build a `VolumeError::IoError { raw_os_error: Some(_) }`
// so the macOS arms in `errno` get exercised end-to-end.
//
// These assert only the typed shape (category, retry, action_kind, reason,
// provider) — the user-facing words live on the frontend and are
// behavior-preservation-checked by the frozen FE golden in
// `src/lib/errors/__fixtures__/friendly_error_golden.json`.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::file_system::git::friendly::{FriendlyGitError, FriendlyGitErrorKind};
    use crate::file_system::volume::VolumeError;

    // ── Errno category + reason tests ───────────────────────────────────
    //
    // Build an `IoError { raw_os_error: Some(_) }` so the macOS arms in `errno`
    // get exercised end-to-end via `listing_error_from_volume_error`.

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
        // EINTR, ENOMEM, EBUSY, EAGAIN, ENETDOWN, ENETRESET, ECONNABORTED,
        // ECONNRESET, ETIMEDOUT, EHOSTDOWN, ESTALE, ENOLCK, ECANCELED.
        let transient_errnos = [4, 12, 16, 35, 50, 52, 53, 54, 60, 64, 70, 77, 89];
        let path = Path::new("/test/path");

        for errno in transient_errnos {
            let err = make_io_error(errno);
            let listing = listing_error_from_volume_error(&err, path);
            assert_eq!(
                listing.category,
                ErrorCategory::Transient,
                "errno {errno} should be Transient, got {:?}",
                listing.category
            );
            assert!(listing.retry_hint, "errno {errno} should have retry_hint");
            assert_eq!(listing.action_kind, None, "transient errno {errno} has no action");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn needs_action_errnos_map_to_needs_action() {
        // EPERM, ENOENT, EACCES, EEXIST, EXDEV, ENOTDIR, EISDIR, ENOSPC, EROFS,
        // ENOTSUP, ENETUNREACH, ECONNREFUSED, ELOOP, ENAMETOOLONG, EHOSTUNREACH,
        // ENOTEMPTY, EDQUOT, EAUTH, ENEEDAUTH, EPWROFF, ENOATTR.
        let needs_action_errnos = [
            1, 2, 13, 17, 18, 20, 21, 28, 30, 45, 51, 61, 62, 63, 65, 66, 69, 80, 81, 82, 93,
        ];
        let path = Path::new("/test/path");

        for errno in needs_action_errnos {
            let err = make_io_error(errno);
            let listing = listing_error_from_volume_error(&err, path);
            assert_eq!(
                listing.category,
                ErrorCategory::NeedsAction,
                "errno {errno} should be NeedsAction, got {:?}",
                listing.category
            );
            assert!(!listing.retry_hint, "errno {errno} should not have retry_hint");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn serious_errnos_map_to_serious() {
        // EIO, EINVAL, EDEVERR.
        let serious_errnos = [5, 22, 83];
        let path = Path::new("/test/path");

        for errno in serious_errnos {
            let err = make_io_error(errno);
            let listing = listing_error_from_volume_error(&err, path);
            assert_eq!(
                listing.category,
                ErrorCategory::Serious,
                "errno {errno} should be Serious, got {:?}",
                listing.category
            );
            assert!(listing.retry_hint, "errno {errno} should have retry_hint");
        }
    }

    /// Spot-check that representative errnos map to the EXACT reason variant the
    /// FE switches on (not just the right category). Drift here breaks the FE
    /// parity test, so pin the wire contract.
    #[cfg(target_os = "macos")]
    #[test]
    fn errnos_map_to_their_specific_reason() {
        let path = Path::new("/test/path");
        // (errno, predicate on the reason)
        assert!(matches!(
            listing_error_from_volume_error(&make_io_error(4), path).reason,
            ListingErrorReason::Interrupted
        ));
        assert!(matches!(
            listing_error_from_volume_error(&make_io_error(60), path).reason,
            ListingErrorReason::ConnectionTimedOutErrno
        ));
        assert!(matches!(
            listing_error_from_volume_error(&make_io_error(2), path).reason,
            ListingErrorReason::PathNotFoundErrno { .. }
        ));
        assert!(matches!(
            listing_error_from_volume_error(&make_io_error(28), path).reason,
            ListingErrorReason::DiskFullErrno
        ));
        assert!(matches!(
            listing_error_from_volume_error(&make_io_error(5), path).reason,
            ListingErrorReason::DiskReadProblem { .. }
        ));
    }

    /// Path-carrying errno arms populate the `path` param so the FE can show it.
    #[cfg(target_os = "macos")]
    #[test]
    fn path_carrying_errnos_populate_path_param() {
        let path = Path::new("/test/some/folder");
        let want = "/test/some/folder";
        // EBUSY (16) carries a path on the transient side.
        match listing_error_from_volume_error(&make_io_error(16), path).reason {
            ListingErrorReason::ResourceBusy { path } => assert_eq!(path, want),
            other => panic!("EBUSY should be ResourceBusy with a path, got {other:?}"),
        }
        // ENOENT (2) carries a path on the needs-action side.
        match listing_error_from_volume_error(&make_io_error(2), path).reason {
            ListingErrorReason::PathNotFoundErrno { path } => assert_eq!(path, want),
            other => panic!("ENOENT should be PathNotFoundErrno with a path, got {other:?}"),
        }
        // EIO (5) carries a path on the serious side.
        match listing_error_from_volume_error(&make_io_error(5), path).reason {
            ListingErrorReason::DiskReadProblem { path } => assert_eq!(path, want),
            other => panic!("EIO should be DiskReadProblem with a path, got {other:?}"),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn unknown_errno_falls_back_to_serious_couldnt_read() {
        let err = make_io_error(9999);
        let path = Path::new("/test/path");
        let listing = listing_error_from_volume_error(&err, path);

        assert_eq!(listing.category, ErrorCategory::Serious);
        assert!(listing.retry_hint);
        assert!(
            matches!(listing.reason, ListingErrorReason::CouldntReadUnknown { .. }),
            "unknown errno should fall back to CouldntReadUnknown, got {:?}",
            listing.reason
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn errno_raw_detail_includes_name_and_code() {
        let err = make_io_error(60); // ETIMEDOUT
        let path = Path::new("/test/path");
        let listing = listing_error_from_volume_error(&err, path);

        assert!(
            listing.raw_detail.contains("ETIMEDOUT"),
            "raw_detail should include the errno name, got {:?}",
            listing.raw_detail
        );
        assert!(
            listing.raw_detail.contains("60"),
            "raw_detail should include the errno number, got {:?}",
            listing.raw_detail
        );
    }

    // ── Typed VolumeError variant tests ─────────────────────────────────

    /// Every non-git `VolumeError` variant maps to the right category, retry
    /// hint, and reason variant. The reason is the FE's wire contract, so assert
    /// it directly (not just the category).
    #[test]
    fn volume_error_variants_map_correctly() {
        let path = Path::new("/test/path");

        // Each case: the error, expected category, expected retry, and a
        // predicate that the reason matches the expected variant.
        #[allow(
            clippy::type_complexity,
            reason = "a flat tuple-with-predicate table is the clearest way to assert reason+category+retry per variant"
        )]
        let cases: Vec<(VolumeError, ErrorCategory, bool, fn(&ListingErrorReason) -> bool)> = vec![
            (
                VolumeError::NotFound("x".into()),
                ErrorCategory::NeedsAction,
                false,
                |r| matches!(r, ListingErrorReason::NotFound { .. }),
            ),
            (
                // A plain (non-TCC, non-network) path falls through to the
                // generic permission-denied reason.
                VolumeError::PermissionDenied("x".into()),
                ErrorCategory::NeedsAction,
                false,
                |r| matches!(r, ListingErrorReason::PermissionDenied { .. }),
            ),
            (
                VolumeError::AlreadyExists("x".into()),
                ErrorCategory::NeedsAction,
                false,
                |r| matches!(r, ListingErrorReason::AlreadyExists { .. }),
            ),
            (VolumeError::NotSupported, ErrorCategory::NeedsAction, false, |r| {
                matches!(r, ListingErrorReason::NotSupported)
            }),
            (
                VolumeError::DeviceDisconnected("x".into()),
                ErrorCategory::NeedsAction,
                false,
                |r| matches!(r, ListingErrorReason::DeviceDisconnected { .. }),
            ),
            (
                VolumeError::ReadOnly("x".into()),
                ErrorCategory::NeedsAction,
                false,
                |r| matches!(r, ListingErrorReason::ReadOnly),
            ),
            (
                VolumeError::StorageFull { message: "x".into() },
                ErrorCategory::NeedsAction,
                false,
                |r| matches!(r, ListingErrorReason::StorageFull),
            ),
            (
                VolumeError::ConnectionTimeout("x".into()),
                ErrorCategory::Transient,
                true,
                |r| matches!(r, ListingErrorReason::ConnectionTimedOut),
            ),
            (
                VolumeError::Cancelled("x".into()),
                ErrorCategory::Transient,
                true,
                |r| matches!(r, ListingErrorReason::Cancelled),
            ),
            (
                VolumeError::DeletePending("x".into()),
                ErrorCategory::Transient,
                true,
                |r| matches!(r, ListingErrorReason::DeletePending { .. }),
            ),
            (
                VolumeError::IsADirectory("x".into()),
                ErrorCategory::NeedsAction,
                false,
                |r| matches!(r, ListingErrorReason::IsADirectory { .. }),
            ),
            (
                VolumeError::IoError {
                    message: "x".into(),
                    raw_os_error: None,
                },
                ErrorCategory::Serious,
                true,
                |r| matches!(r, ListingErrorReason::IoSerious { .. }),
            ),
        ];

        for (err, expected_category, expected_retry, reason_matches) in cases {
            let listing = listing_error_from_volume_error(&err, path);
            assert_eq!(
                listing.category, expected_category,
                "VolumeError {err:?} should map to {expected_category:?}"
            );
            assert_eq!(
                listing.retry_hint, expected_retry,
                "VolumeError {err:?} retry_hint should be {expected_retry}"
            );
            assert!(
                reason_matches(&listing.reason),
                "VolumeError {err:?} produced unexpected reason {:?}",
                listing.reason
            );
        }
    }

    /// Path-carrying typed variants populate the `path` param.
    #[test]
    fn typed_variants_populate_path_param() {
        let path = Path::new("/test/some/file.txt");
        let want = "/test/some/file.txt";

        match listing_error_from_volume_error(&VolumeError::NotFound("x".into()), path).reason {
            ListingErrorReason::NotFound { path } => assert_eq!(path, want),
            other => panic!("NotFound should carry a path, got {other:?}"),
        }
        match listing_error_from_volume_error(&VolumeError::DeletePending("x".into()), path).reason {
            ListingErrorReason::DeletePending { path } => assert_eq!(path, want),
            other => panic!("DeletePending should carry a path, got {other:?}"),
        }
    }

    /// `IoSerious` carries both the path and the raw OS message as params so the
    /// FE can interpolate (and escape) them.
    #[test]
    fn io_serious_carries_path_and_os_message() {
        let path = Path::new("/Volumes/share/_todo_pics/photo.jpg");
        let os_msg = "Protocol error: STATUS_DELETE_PENDING during Create";
        let err = VolumeError::IoError {
            message: os_msg.into(),
            raw_os_error: None,
        };
        match listing_error_from_volume_error(&err, path).reason {
            ListingErrorReason::IoSerious { path: p, os_message } => {
                assert_eq!(p, "/Volumes/share/_todo_pics/photo.jpg");
                assert_eq!(os_message, os_msg);
            }
            other => panic!("IoError without errno should be IoSerious, got {other:?}"),
        }
    }

    // ── TCC-vs-permission branch ─────────────────────────────────────────

    /// A permission-denied on a TCC-guarded path surfaces the dedicated
    /// `TccRestricted` reason (two escape hatches) AND the privacy-settings
    /// action. A plain path falls through to the generic `PermissionDenied`.
    ///
    /// macOS-only: `is_potentially_tcc_restricted` returns `false` on other
    /// platforms (and `is_network_volume_path` would need a live network mount),
    /// so the TCC branch can't be exercised off-macOS.
    #[cfg(target_os = "macos")]
    #[test]
    fn permission_denied_tcc_path_uses_tcc_restricted_reason() {
        // A path under `~/Downloads` is TCC-classified purely by path (no
        // statfs / live mount needed), so this is stable in CI.
        let home = dirs::home_dir().expect("home dir");
        let tcc_path = home.join("Downloads/some-folder");
        assert!(
            crate::restricted_paths::tcc_paths::is_potentially_tcc_restricted(&tcc_path),
            "~/Downloads must be TCC-classified for this test to be meaningful"
        );
        let listing = listing_error_from_volume_error(&VolumeError::PermissionDenied("x".into()), &tcc_path);
        assert!(
            matches!(listing.reason, ListingErrorReason::TccRestricted { .. }),
            "TCC path should use TccRestricted, got {:?}",
            listing.reason
        );
        assert_eq!(listing.category, ErrorCategory::NeedsAction);
        assert_eq!(listing.action_kind, Some(ErrorActionKind::OpenPrivacySettings));

        // A path that is neither TCC-classified nor a network volume falls
        // through to the generic permission-denied reason.
        let plain_path = Path::new("/tmp/cmdr-not-tcc/folder");
        assert!(
            !crate::restricted_paths::tcc_paths::is_potentially_tcc_restricted(plain_path)
                && !crate::restricted_paths::tcc_paths::is_network_volume_path(plain_path),
            "the plain path must NOT be TCC-classified"
        );
        let listing = listing_error_from_volume_error(&VolumeError::PermissionDenied("x".into()), plain_path);
        assert!(
            matches!(listing.reason, ListingErrorReason::PermissionDenied { .. }),
            "plain path should use the generic PermissionDenied, got {:?}",
            listing.reason
        );
    }

    // ── action_kind tests ────────────────────────────────────────────────

    #[test]
    fn permission_denied_volume_error_has_open_privacy_settings() {
        let path = Path::new("/test/path");
        let listing = listing_error_from_volume_error(&VolumeError::PermissionDenied("denied".into()), path);
        assert_eq!(
            listing.action_kind,
            Some(ErrorActionKind::OpenPrivacySettings),
            "PermissionDenied should set action_kind = OpenPrivacySettings"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn eperm_errno_has_open_privacy_settings() {
        let path = Path::new("/test/path");
        let listing = listing_error_from_volume_error(&make_io_error(1), path); // EPERM
        assert_eq!(
            listing.action_kind,
            Some(ErrorActionKind::OpenPrivacySettings),
            "EPERM (errno 1) should set action_kind = OpenPrivacySettings"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn eacces_errno_has_open_privacy_settings() {
        let path = Path::new("/test/path");
        let listing = listing_error_from_volume_error(&make_io_error(13), path); // EACCES
        assert_eq!(
            listing.action_kind,
            Some(ErrorActionKind::OpenPrivacySettings),
            "EACCES (errno 13) should set action_kind = OpenPrivacySettings"
        );
    }

    #[test]
    fn non_permission_errors_have_no_action_kind() {
        let path = Path::new("/test/path");
        let cases = vec![
            VolumeError::NotFound("x".into()),
            VolumeError::ConnectionTimeout("x".into()),
            VolumeError::IoError {
                message: "x".into(),
                raw_os_error: None,
            },
        ];
        for err in &cases {
            let listing = listing_error_from_volume_error(err, path);
            assert_eq!(
                listing.action_kind, None,
                "VolumeError {err:?} should have no action_kind"
            );
        }
    }

    // ── Git pass-through (Layer 0) ───────────────────────────────────────

    /// `FriendlyGit` rides as the `Git` reason carrying the typed kind, with the
    /// category derived from the kind and retry on only for transient kinds.
    #[test]
    fn friendly_git_rides_as_git_reason_with_kind_category() {
        let path = Path::new("/some/repo/.git");
        // (kind, expected category, expected retry)
        let cases = [
            (FriendlyGitErrorKind::NotARepo, ErrorCategory::NeedsAction, false),
            (FriendlyGitErrorKind::IndexLocked, ErrorCategory::Transient, true),
            (FriendlyGitErrorKind::CorruptRepo, ErrorCategory::Serious, false),
            (FriendlyGitErrorKind::MissingObject, ErrorCategory::Serious, false),
            (FriendlyGitErrorKind::BareRepo, ErrorCategory::NeedsAction, false),
        ];
        for (kind, expected_category, expected_retry) in cases {
            let err = VolumeError::FriendlyGit(FriendlyGitError::new(kind, "/some/repo/.git"));
            let listing = listing_error_from_volume_error(&err, path);
            match listing.reason {
                ListingErrorReason::Git { kind: got } => assert_eq!(got, kind, "git kind should ride through"),
                other => panic!("FriendlyGit should produce the Git reason, got {other:?}"),
            }
            assert_eq!(listing.category, expected_category, "git {kind:?} category");
            assert_eq!(listing.retry_hint, expected_retry, "git {kind:?} retry");
            // Git is a Layer-0 pass-through: never provider-enriched, no action.
            assert_eq!(listing.provider, None, "git errors are not provider-enriched");
            assert_eq!(listing.action_kind, None, "git errors carry no action_kind");
        }
    }

    // ── Empty-root iCloud hint ───────────────────────────────────────────

    #[test]
    fn restricted_empty_root_known_volume_returns_hint() {
        let path = Path::new("/Users/test/Library/Mobile Documents/com~apple~CloudDocs");
        let listing =
            listing_error_for_restricted_empty_root("cloud-icloud", path).expect("iCloud volume should produce a hint");
        assert!(
            matches!(listing.reason, ListingErrorReason::EmptyRootICloud),
            "iCloud empty root should use the EmptyRootICloud reason, got {:?}",
            listing.reason
        );
        assert_eq!(listing.category, ErrorCategory::NeedsAction);
        assert!(listing.retry_hint, "user can retry after granting access");
        assert_eq!(listing.action_kind, Some(ErrorActionKind::OpenPrivacySettings));
        // raw_detail carries the diagnostic context (no prose).
        assert!(
            listing.raw_detail.contains("cloud-icloud"),
            "raw_detail should record the volume id, got {:?}",
            listing.raw_detail
        );
    }

    #[test]
    fn restricted_empty_root_unknown_volume_returns_none() {
        let path = Path::new("/some/other/path");
        assert!(listing_error_for_restricted_empty_root("root", path).is_none());
        assert!(listing_error_for_restricted_empty_root("cloud-dropbox", path).is_none());
    }
}
