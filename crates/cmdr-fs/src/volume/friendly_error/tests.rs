//! Tests live here because they exercise the public API (`listing_error_from_volume_error`,
//! `listing_error_for_restricted_empty_root`) which dispatches across the sibling
//! modules. The errno-arm tests build a `VolumeError::IoError { raw_os_error: Some(_) }`
//! so the macOS arms in `errno` get exercised end-to-end.
//!
//! These assert only the typed shape (category, retry, action_kind, reason,
//! provider) — the user-facing words live on the frontend and are
//! behavior-preservation-checked by the frozen FE golden in
//! `src/lib/error-messages/__fixtures__/friendly_error_golden.json`.

use std::path::Path;

use super::*;
use crate::volume::VolumeError;
use crate::volume::friendly_error::git::{FriendlyGitError, FriendlyGitErrorKind};

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
            // ❌ Deliberately NOT the `DeviceDisconnected` classification: the
            // device is still attached and a reopen is already running, so the
            // user needs to wait and retry, not go re-plug anything.
            VolumeError::DeviceSessionReset("x".into()),
            ErrorCategory::Transient,
            true,
            |r| matches!(r, ListingErrorReason::DeviceReconnecting { .. }),
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
            // The destination can't hold this name, so retrying it can only fail
            // again: NeedsAction with ❌ no retry hint. Renaming is the only fix.
            VolumeError::InvalidName("x".into()),
            ErrorCategory::NeedsAction,
            false,
            |r| matches!(r, ListingErrorReason::InvalidName { .. }),
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
        (
            // A HEADER-encrypted archive fails browsing with `NeedsPassword`
            // (its metadata is encrypted); the listing arm surfaces the dedicated
            // needs-password reason the FE renders as a prompt, NeedsAction so
            // it's recoverable by supplying the password.
            VolumeError::NeedsPassword { wrong_attempt: false },
            ErrorCategory::NeedsAction,
            false,
            |r| matches!(r, ListingErrorReason::ArchiveNeedsPassword { wrong_attempt: false }),
        ),
        (
            // The retry case carries `wrong_attempt: true` through to the FE copy.
            VolumeError::NeedsPassword { wrong_attempt: true },
            ErrorCategory::NeedsAction,
            false,
            |r| matches!(r, ListingErrorReason::ArchiveNeedsPassword { wrong_attempt: true }),
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
    // The whole point of this message is naming the file the user has to rename,
    // so the path param is load-bearing rather than decorative.
    match listing_error_from_volume_error(&VolumeError::InvalidName("x".into()), path).reason {
        ListingErrorReason::InvalidName { path } => assert_eq!(path, want),
        other => panic!("InvalidName should carry a path, got {other:?}"),
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

/// A permission-denied ON a shut TCC gate surfaces the dedicated `TccRestricted`
/// reason (two escape hatches) AND the privacy-settings action. A plain path
/// falls through to the generic `PermissionDenied`.
///
/// macOS-only: `tcc_denial_is_plausible` returns `false` on other platforms, so
/// the TCC branch can't be exercised off-macOS.
#[cfg(target_os = "macos")]
#[test]
fn permission_denied_on_a_tcc_gate_uses_tcc_restricted_reason() {
    // `~/Downloads` is TCC-classified purely by path, and being its own anchor it
    // needs no probe of a folder this test can't control. Both keep it stable in CI.
    let home = dirs::home_dir().expect("home dir");
    let gate = home.join("Downloads");
    assert!(
        crate::tcc_paths::tcc_anchor(&gate).as_deref() == Some(gate.as_path()),
        "~/Downloads must be its own TCC anchor for this test to be meaningful"
    );
    let listing = listing_error_from_volume_error(&VolumeError::PermissionDenied("x".into()), &gate);
    assert!(
        matches!(listing.reason, ListingErrorReason::TccRestricted { .. }),
        "a denied TCC gate should use TccRestricted, got {:?}",
        listing.reason
    );
    assert_eq!(listing.category, ErrorCategory::NeedsAction);
    assert_eq!(listing.action_kind, Some(ErrorActionKind::OpenPrivacySettings));

    // A path that is neither TCC-classified nor a network volume falls
    // through to the generic permission-denied reason.
    let plain_path = Path::new("/tmp/cmdr-not-tcc/folder");
    assert!(
        !crate::tcc_paths::is_potentially_tcc_restricted(plain_path),
        "the plain path must NOT be TCC-classified"
    );
    let listing = listing_error_from_volume_error(&VolumeError::PermissionDenied("x".into()), plain_path);
    assert!(
        matches!(listing.reason, ListingErrorReason::PermissionDenied { .. }),
        "plain path should use the generic PermissionDenied, got {:?}",
        listing.reason
    );
}

/// A denial below a TCC gate that OPENS fine isn't TCC's doing: the grant is already
/// there and something below it refused on its own. Sending the user to System
/// Settings would have them hunt for a permission they hold, so the privacy-settings
/// action must not ride along.
///
/// `~/Library/CloudStorage` anchors per FileProvider domain, so a made-up domain name
/// gives a probe target that reliably does not exist (and therefore isn't "shut")
/// without this test touching a real folder.
#[cfg(target_os = "macos")]
#[test]
fn permission_denied_below_an_open_tcc_gate_is_not_tcc_restricted() {
    let home = dirs::home_dir().expect("home dir");
    let path = home.join("Library/CloudStorage/CmdrNoSuchProvider-test/folder");
    assert!(
        crate::tcc_paths::is_potentially_tcc_restricted(&path),
        "the path must be TCC-classified, so the denial reaches the TCC branch"
    );
    let listing = listing_error_from_volume_error(&VolumeError::PermissionDenied("x".into()), &path);
    assert!(
        matches!(listing.reason, ListingErrorReason::PermissionDenied { .. }),
        "an open gate should fall through to the generic PermissionDenied, got {:?}",
        listing.reason
    );
}

/// A share's own permissions are the file server's business, so the remote reason
/// offers no privacy-settings action: there is nothing on this Mac to grant.
///
/// Driven through the constructor rather than `listing_error_from_volume_error`,
/// because reaching the branch needs a live `smbfs`/`afpfs`/`nfs` mount that CI has
/// no way to provide. `scripts/soak-smb.sh` covers the end-to-end path.
#[test]
fn remote_permission_denied_offers_no_privacy_settings_action() {
    let listing = kinds::remote_permission_denied("/Volumes/share/lost+found", "denied".into());
    match &listing.reason {
        ListingErrorReason::RemotePermissionDenied { path } => assert_eq!(path, "/Volumes/share/lost+found"),
        other => panic!("expected RemotePermissionDenied, got {other:?}"),
    }
    assert_eq!(listing.category, ErrorCategory::NeedsAction);
    assert_eq!(
        listing.action_kind, None,
        "System Settings holds no grant for a server-side denial"
    );
    assert!(!listing.retry_hint, "retrying reruns the same refused request");
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

/// The constructor keeps both halves a caller reads back: which kind of trouble
/// it is, and the path it happened on.
#[test]
fn friendly_git_error_carries_kind_and_path() {
    let err = FriendlyGitError::new(FriendlyGitErrorKind::NotARepo, "/tmp/foo");
    assert_eq!(err.kind, FriendlyGitErrorKind::NotARepo);
    assert_eq!(err.path, "/tmp/foo");
}

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
