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

// TODO(stage 3): typed-mapping tests (errno/variant/git-kind → reason, category,
// retry, action_kind, provider, and populated params). The old prose-assertion
// tests were removed in stage 2 (they asserted on the deleted `FriendlyError`'s
// `.title`/`.explanation`/`.suggestion`); stage 3 recreates them as typed tests.
