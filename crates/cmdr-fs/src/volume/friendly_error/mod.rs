//! Error CLASSIFICATION: turns raw errors + path into a TYPED, word-free
//! classification the frontend renders.
//!
//! The backend emits a `ListingError` (a `category`, a semantic `reason` with
//! structured params, an optional detected `provider`, an optional `actionKind`,
//! a `retryHint`, and a technical `rawDetail`) and ZERO user-facing prose. The
//! frontend owns 100% of the titles/explanations/suggestions, rendered from this
//! typed data (`src/lib/error-messages/`).
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
//! `src/lib/error-messages/provider-error-messages.ts`).

mod empty_root;
mod errno;
/// Git-specific error classification. It lives under `friendly_error` because
/// the two reference each other: a `FriendlyGitErrorKind` maps to an
/// [`ErrorCategory`], and `VolumeError::FriendlyGit` carries the whole thing.
pub mod git;
mod kinds;
mod provider;
mod volume_error;

use serde::{Deserialize, Serialize};

use crate::volume::friendly_error::git::FriendlyGitErrorKind;

// Public API re-exports: keep the `volume::friendly_error::*` import surface
// unchanged for callers regardless of how the module is split internally.
pub use empty_root::listing_error_for_restricted_empty_root;
pub use provider::{Provider, enrich_with_provider};
pub use volume_error::{
    archive_needs_password_listing_error, archive_unreadable_listing_error, listing_error_from_volume_error,
};

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

/// How serious a failure is, and therefore how the frontend styles it.
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
    /// How serious this is. Drives the frontend's icon and severity color.
    pub category: ErrorCategory,
    /// What went wrong, semantically. The frontend switches on it to pick the
    /// message factory.
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
    /// A signal interrupted the syscall before it did anything.
    Interrupted,
    /// The system was out of memory for the operation.
    NotEnoughMemory,
    /// The path is in use by something that holds it exclusively.
    ResourceBusy {
        /// The path the OS named.
        path: String,
    },
    /// The resource would have blocked; worth another try.
    TemporarilyUnavailable,
    /// The network itself is down.
    NetworkDown,
    /// The network reset, dropping the connection with it.
    NetworkConnectionDropped,
    /// The connection aborted mid-operation.
    ConnectionDropped,
    /// The peer reset the connection.
    ConnectionReset,
    /// The connection timed out at the OS level.
    ConnectionTimedOutErrno,
    /// The host is reachable on the network but not answering.
    HostDown,
    /// A network file handle went stale (the mount moved out from under it).
    StaleConnection,
    /// No file locks were available.
    LockUnavailable,
    /// The OS reported the operation as cancelled.
    CancelledErrno,
    // ── errno: needs-action ──
    /// The operation isn't permitted for this user on this path.
    NotPermitted {
        /// The path the failure was about.
        path: String,
    },
    /// The path doesn't exist.
    PathNotFoundErrno {
        /// The path the failure was about.
        path: String,
    },
    /// The OS denied access to the path.
    NoPermissionErrno {
        /// The path the failure was about.
        path: String,
    },
    /// Something already exists at the destination.
    AlreadyExistsErrno {
        /// The path the failure was about.
        path: String,
    },
    /// A link or rename crossed a device boundary, which the OS refuses.
    CrossDeviceOperation,
    /// A path component that had to be a directory is a file.
    NotAFolder {
        /// The path the failure was about.
        path: String,
    },
    /// The path is a directory where a file was needed.
    IsAFolderErrno {
        /// The path the failure was about.
        path: String,
    },
    /// The destination volume is out of space.
    DiskFullErrno,
    /// The volume is mounted read-only.
    ReadOnlyVolumeErrno,
    /// The filesystem doesn't support the operation.
    NotSupportedErrno,
    /// No route to the network.
    NetworkUnreachable,
    /// The host actively refused the connection.
    ConnectionRefused,
    /// Following symlinks went in a circle.
    SymlinkLoopErrno {
        /// The path the failure was about.
        path: String,
    },
    /// The name exceeds the filesystem's length limit.
    NameTooLongErrno,
    /// No route to the host.
    HostUnreachable,
    /// A directory had to be empty to be removed, and was not.
    FolderNotEmpty {
        /// The path the failure was about.
        path: String,
    },
    /// The user's disk quota is used up.
    QuotaExceeded,
    /// The server rejected the credentials.
    AuthRequiredEauth,
    /// The server wants credentials that were never supplied.
    AuthRequiredEneedauth,
    /// The device is powered off.
    DevicePoweredOff,
    /// The requested extended attribute is absent.
    AttributeNotFound,
    // ── errno: serious ──
    /// A hardware-level read failed. Usually a failing disk or a dropped mount.
    DiskReadProblem {
        /// The path the failure was about.
        path: String,
    },
    /// The OS rejected the arguments, which means we built the call wrong.
    UnexpectedSystemResponse,
    /// The device reported a fault of its own.
    DeviceProblem,
    /// An errno we don't classify. The raw code rides in `raw_detail`.
    CouldntReadUnknown {
        /// The path the failure was about.
        path: String,
    },
    // ── typed VolumeError variants (shared "kinds") ──
    /// `VolumeError::NotFound`: the backend has no such path.
    NotFound {
        /// The path the failure was about.
        path: String,
    },
    /// macOS TCC is guarding the path; the user has to grant access.
    TccRestricted {
        /// The path the failure was about.
        path: String,
    },
    /// `VolumeError::PermissionDenied`: the backend refused access.
    PermissionDenied {
        /// The path the failure was about.
        path: String,
    },
    /// A file server refused access to a path on a mounted network share. macOS already
    /// grants the mount, so there is nothing to change on this Mac.
    RemotePermissionDenied {
        /// The path the failure was about.
        path: String,
    },
    /// `VolumeError::AlreadyExists`: the destination is taken.
    AlreadyExists {
        /// The path the failure was about.
        path: String,
    },
    /// The user cancelled the operation.
    Cancelled,
    /// The device went away mid-operation.
    DeviceDisconnected {
        /// The path the failure was about.
        path: String,
    },
    /// The device's session died but the device is still attached, and a reopen is already running.
    DeviceReconnecting {
        /// The path the failure was about.
        path: String,
    },
    /// The volume or device is read-only.
    ReadOnly,
    /// The device is out of storage.
    StorageFull,
    /// The backend timed out waiting for the device or server.
    ConnectionTimedOut,
    /// The backend doesn't implement this operation.
    NotSupported,
    /// A delete is pending on the path and an open handle is keeping it alive.
    DeletePending {
        /// The path the failure was about.
        path: String,
    },
    /// An I/O failure the backend couldn't classify further.
    IoSerious {
        /// The path the failure was about.
        path: String,
        /// What the OS said, for the technical-details disclosure.
        os_message: String,
    },
    /// The path is a directory where a file was needed.
    IsADirectory {
        /// The path the failure was about.
        path: String,
    },
    // ── archive (browsing a `.zip` that can't be read) ──
    /// Browsing an archive failed because the archive itself is unreadable:
    /// damaged/truncated, encrypted, an unsupported format, or a file that carries
    /// an archive extension but isn't really an archive. The `ArchiveVolume`
    /// collapses the integrity family to `NotSupported`/`IoError`, so this is
    /// classified at the listing seam from the path + error kind, not from a
    /// dedicated `VolumeError`.
    ArchiveUnreadable,
    /// Browsing a HEADER-encrypted archive (a `-mhe=on` 7z) failed because its
    /// metadata is itself encrypted, so even listing needs the password. The FE
    /// renders the password prompt (not an error pane); `wrong_attempt` swaps the
    /// copy to "that password didn't work" after a rejected try. Distinct from
    /// `ArchiveUnreadable` (which is unrecoverable) — this one is retried by
    /// supplying the password and re-navigating. (Content-encrypted archives list
    /// fine and prompt only on extract, via the transfer path.)
    ArchiveNeedsPassword {
        /// `true` once a supplied password has been rejected.
        wrong_attempt: bool,
    },
    // ── empty-root hint ──
    /// An iCloud Drive root that looks empty because TCC is hiding it, not because it is.
    EmptyRootICloud,
    // ── git (wire-only; FE routes to its parallel git factory) ──
    /// A git-layer failure, carrying its own typed kind for the frontend to route.
    Git {
        /// Which git failure it was.
        kind: FriendlyGitErrorKind,
    },
}

#[cfg(test)]
mod tests;
