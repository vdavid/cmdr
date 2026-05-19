//! Friendly error mapping: turns raw errors + path into user-facing error info.
//!
//! Three sources produce a `FriendlyError`, each in its own sibling module:
//! - `volume_error`: `VolumeError` (used by listing-error path; richest, dispatches to errno on raw
//!   `IoError`)
//! - `write_error`: `WriteOperationError` (used by `write-error` events; mirror of `volume_error`
//!   for the post-`map_volume_error` shape)
//! - `errno`: raw macOS errnos with a non-macOS fallback (called from `volume_error` when an
//!   `IoError` carries a `raw_os_error`)
//! - `empty_root`: TCC-restricted volume root hint (a single special case)
//!
//! `enrich_with_provider` (in sibling module `provider.rs`) layers
//! provider-specific suggestions on top: that's the second pass that turns
//! "Couldn't read this folder" into "This folder is managed by **MacDroid**…".

mod empty_root;
mod errno;
mod kinds;
mod markdown;
mod volume_error;
mod write_error;

use serde::{Deserialize, Serialize};

// Public API re-exports: keep the `volume::friendly_error::*` import surface
// unchanged for callers regardless of how the module is split internally.
pub use empty_root::friendly_error_for_restricted_empty_root;
pub use markdown::{Markdown, MarkdownArg};
pub use volume_error::friendly_error_from_volume_error;
pub use write_error::friendly_from_write_error;

// Re-export `enrich_with_provider` so callers can keep importing from
// `friendly_error::enrich_with_provider`.
pub use crate::file_system::volume::provider::enrich_with_provider;

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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FriendlyError {
    pub category: ErrorCategory,
    pub title: String,
    /// Markdown (rendered by snarkdown on FE). Build with `md!(...)` so
    /// interpolated runtime strings get escaped.
    pub explanation: Markdown,
    /// Markdown rendered by snarkdown on the frontend. Build with `md!(...)`
    /// so interpolated runtime strings get escaped.
    pub suggestion: Markdown,
    /// For the technical details disclosure, for example "ETIMEDOUT (os error 60)".
    pub raw_detail: String,
    /// FE shows a "Try again" button when true.
    pub retry_hint: bool,
    /// Typed action the frontend should offer. Drives the "Open System Settings" button
    /// without substring-matching the title.
    pub action_kind: Option<ErrorActionKind>,
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

// ============================================================================
// Tests
// ============================================================================
//
// Tests live here because they exercise the public API (`friendly_error_from_volume_error`,
// `friendly_error_for_restricted_empty_root`) which dispatches across the sibling
// modules. The errno-arm tests build a `VolumeError::IoError { raw_os_error: Some(_) }`
// so the macOS arms in `errno` get exercised end-to-end.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::volume::VolumeError;
    use std::path::Path;

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
            (VolumeError::DeletePending("x".into()), ErrorCategory::Transient, true),
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
    fn restricted_empty_root_known_volume_returns_hint() {
        let path = Path::new("/Users/test/Library/Mobile Documents/com~apple~CloudDocs");
        let friendly = friendly_error_for_restricted_empty_root("cloud-icloud", path)
            .expect("iCloud volume should produce a hint");
        assert_eq!(friendly.category, ErrorCategory::NeedsAction);
        assert!(
            friendly.retry_hint,
            "User should be able to retry after granting access"
        );
        assert!(friendly.title.contains("iCloud"));
        assert!(friendly.suggestion.as_str().contains("Full Disk Access"));

        let lowered = format!("{} {} {}", friendly.title, friendly.explanation, friendly.suggestion).to_lowercase();
        for word in ["error", "failed", "just", "simple", "easy"] {
            assert!(
                !lowered
                    .split_whitespace()
                    .any(|w| w.trim_matches(|c: char| !c.is_alphabetic()) == word),
                "iCloud hint shouldn't contain trivializing word `{word}`",
            );
        }
    }

    #[test]
    fn restricted_empty_root_unknown_volume_returns_none() {
        let path = Path::new("/some/other/path");
        assert!(friendly_error_for_restricted_empty_root("root", path).is_none());
        assert!(friendly_error_for_restricted_empty_root("cloud-dropbox", path).is_none());
    }

    #[test]
    fn error_messages_never_contain_error_or_failed() {
        use crate::file_system::git::friendly::{FriendlyGitError, FriendlyGitErrorKind};

        let path = Path::new("/test/path");

        // Test a selection of variants and errnos, plus every git-friendly
        // kind so the M4 additions stay clean.
        let mut errors: Vec<VolumeError> = vec![
            VolumeError::NotFound("x".into()),
            VolumeError::PermissionDenied("x".into()),
            VolumeError::ConnectionTimeout("x".into()),
            VolumeError::DeletePending("x".into()),
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
        for kind in [
            FriendlyGitErrorKind::NotARepo,
            FriendlyGitErrorKind::OrphanedWorktree,
            FriendlyGitErrorKind::CorruptRepo,
            FriendlyGitErrorKind::IndexLocked,
            FriendlyGitErrorKind::PermissionDenied,
            FriendlyGitErrorKind::BareRepo,
            FriendlyGitErrorKind::BlobTooLarge,
            FriendlyGitErrorKind::ShallowBoundary,
            FriendlyGitErrorKind::MissingObject,
            FriendlyGitErrorKind::GitDirPermissionDenied,
        ] {
            errors.push(VolumeError::FriendlyGit(FriendlyGitError::new(kind, "/some/repo/.git")));
        }

        for err in &errors {
            let friendly = friendly_error_from_volume_error(err, path);

            // Check title, explanation, and suggestion (not raw_detail, which is technical)
            let title_lower = friendly.title.to_lowercase();
            let explanation_lower = friendly.explanation.as_str().to_lowercase();
            let suggestion_lower = friendly.suggestion.as_str().to_lowercase();

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

    /// Regression: the raw OS message `STATUS_DELETE_PENDING during Create` used
    /// to flow straight into `format!()` inside `kinds::io_serious`, which made
    /// snarkdown render `_DELETE_` as italics in the UI. With `Markdown` + `md!`,
    /// the runtime `message` arg is encoded as HTML entities so the wire format
    /// carries `STATUS&#95;DELETE&#95;PENDING`; snarkdown passes the entities
    /// through and the browser decodes them as plain underscores.
    #[test]
    fn io_serious_escapes_message_markdown_specials() {
        let path = Path::new("/Volumes/share/_todo_pics/photo.jpg");
        // io_serious is reached by IoError without a raw_os_error (or with an
        // unrecognized one on macOS; on non-macOS it's the only path).
        let err = VolumeError::IoError {
            message: "Protocol error: STATUS_DELETE_PENDING during Create".into(),
            raw_os_error: None,
        };
        let friendly = friendly_error_from_volume_error(&err, path);

        let exp = friendly.explanation.as_str();
        assert!(
            exp.contains("STATUS&#95;DELETE&#95;PENDING"),
            "explanation should HTML-encode underscores in runtime message, got: {exp:?}"
        );
        // Sanity: the raw unescaped form must NOT appear, or snarkdown would
        // render it as italics.
        assert!(
            !exp.contains("STATUS_DELETE_PENDING"),
            "explanation must not contain raw underscores from runtime message, got: {exp:?}"
        );
        // The path's literal underscores are also encoded.
        assert!(
            exp.contains("&#95;todo&#95;pics"),
            "explanation should HTML-encode underscores in the path, got: {exp:?}"
        );
    }

    #[test]
    fn delete_pending_uses_dedicated_copy() {
        let path = Path::new("/Volumes/share/photo.jpg");
        let err = VolumeError::DeletePending("Protocol error: STATUS_DELETE_PENDING during Create".into());
        let friendly = friendly_error_from_volume_error(&err, path);

        assert_eq!(friendly.category, ErrorCategory::Transient);
        assert!(
            friendly.retry_hint,
            "DeletePending is transient — user should see a retry hint"
        );
        assert!(
            friendly.title.contains("being removed"),
            "DeletePending title should say the file is being removed, got: {:?}",
            friendly.title,
        );
        // The path is interpolated into the explanation so the user knows which file.
        assert!(
            friendly.explanation.as_str().contains("photo.jpg"),
            "DeletePending explanation should include the path, got: {:?}",
            friendly.explanation,
        );
        // raw_detail preserves the underlying NTSTATUS for the technical-details disclosure.
        assert!(
            friendly.raw_detail.contains("DELETE_PENDING"),
            "raw_detail should preserve the NTSTATUS code, got: {:?}",
            friendly.raw_detail,
        );
    }

    // ── action_kind tests ───────────────────────────────────────────────

    #[cfg(target_os = "macos")]
    #[test]
    fn permission_denied_volume_error_has_open_privacy_settings() {
        let path = Path::new("/test/path");
        let err = VolumeError::PermissionDenied("denied".into());
        let friendly = friendly_error_from_volume_error(&err, path);
        assert_eq!(
            friendly.action_kind,
            Some(ErrorActionKind::OpenPrivacySettings),
            "PermissionDenied should set action_kind = OpenPrivacySettings"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn eperm_errno_has_open_privacy_settings() {
        let path = Path::new("/test/path");
        let err = make_io_error(1); // EPERM
        let friendly = friendly_error_from_volume_error(&err, path);
        assert_eq!(
            friendly.action_kind,
            Some(ErrorActionKind::OpenPrivacySettings),
            "EPERM (errno 1) should set action_kind = OpenPrivacySettings"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn eacces_errno_has_open_privacy_settings() {
        let path = Path::new("/test/path");
        let err = make_io_error(13); // EACCES
        let friendly = friendly_error_from_volume_error(&err, path);
        assert_eq!(
            friendly.action_kind,
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
            let friendly = friendly_error_from_volume_error(err, path);
            assert_eq!(
                friendly.action_kind, None,
                "VolumeError {:?} should have no action_kind",
                err
            );
        }
    }
}
