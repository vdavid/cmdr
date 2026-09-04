//! Shared transfer-error plumbing for volume-aware copy and move.
//!
//! `WriteFailure` carries the typed `WriteOperationError` the FE renders from,
//! and `map_volume_error` / `write_error_event_from` translate an originating
//! `VolumeError` into that typed shape and the outgoing `WriteErrorEvent`. Kept
//! in its own module so both `volume::copy` and `volume::r#move` depend on it
//! rather than on each other.

use std::path::{Path, PathBuf};

use super::super::super::types::{WriteErrorEvent, WriteOperationError, WriteOperationType};
use crate::file_system::volume::VolumeError;

/// Which side of a transfer a failing path belongs to.
///
/// A `VolumeError::NotFound` carries no clue about this, and the two sides mean
/// opposite things to the user: a missing SOURCE says "your file is gone", a
/// missing DESTINATION says "there was nowhere to put it". Only the call site
/// knows which volume it asked, so the role travels WITH the path from there —
/// ❌ never inferred downstream from the path's shape, which is how a NAS
/// destination once got reported as a vanished source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::file_system::write_operations) enum PathRole {
    /// The path came from the volume being read (or from a walk over it).
    Source,
    /// The path came from the volume being written to.
    Destination,
}

/// A write-operation failure carrying the typed `WriteOperationError` the FE renders
/// from. The two volume-aware constructors map an originating `VolumeError + path`
/// into the typed error; `synthetic` wraps an already-typed error (cancellation,
/// validation, synthetic IoError).
#[derive(Debug, Clone)]
pub(crate) struct WriteFailure {
    pub error: WriteOperationError,
}

impl WriteFailure {
    /// Construct a `WriteFailure` from an originating `VolumeError + path`, mapping it
    /// to a `WriteOperationError`. One spot to map, replacing per-call-site boilerplate.
    /// `role` says which volume the path came from (see `PathRole`).
    pub(super) fn from_volume(path: &Path, role: PathRole, e: VolumeError) -> Self {
        let error = map_volume_error(&path.display().to_string(), role, e);
        Self { error }
    }

    /// Construct a `WriteFailure` from a synthetic `WriteOperationError` (no volume
    /// context). Used for cancellation, validation errors, etc.
    pub(super) fn synthetic(error: WriteOperationError) -> Self {
        Self { error }
    }
}

/// Convenience: take a captured `(VolumeError, PathBuf)` and build the `WriteFailure`
/// from it. Used inside loops where we cloned the path for logging.
impl From<(VolumeError, PathBuf)> for WriteFailure {
    fn from(ctx: (VolumeError, PathBuf)) -> Self {
        let (volume_error, path) = ctx;
        let error = map_volume_error(&path.display().to_string(), PathRole::Source, volume_error);
        Self { error }
    }
}

/// A `VolumeError` plus the path that actually produced it.
///
/// One `copy_single_path` or `remove_tree` call can descend a
/// whole subtree, so the failure a caller sees may come from a file thousands of
/// entries below the top-level item the user selected. Only the walker knows
/// which one it was, and once the error leaves it that knowledge is gone for
/// good. Carrying the originating
/// path out WITH the error is what lets the reported message name the file that
/// failed instead of the folder that happens to contain it. ❌ Don't "simplify"
/// this back to a bare `VolumeError`: the caller cannot reconstruct the path,
/// and its only honest fallback is the top-level item, which is the wrong
/// answer for every directory transfer.
/// Visible across `write_operations` (not just `transfer`) because
/// `remove_tree` returns it and `archive_edit` calls that.
#[derive(Debug, Clone)]
pub(in crate::file_system::write_operations) struct PathedVolumeError {
    /// The item that failed, as deep in the tree as the walker got.
    pub path: PathBuf,
    pub error: VolumeError,
}

/// `PathRole::Source` isn't a default here, it's what the carried path IS: every
/// `at()` site labels the error with the SOURCE item the walker was on (that's the
/// type's whole purpose), so the path a `PathedVolumeError` holds is a source path
/// even when the failing call was a write. Naming it a destination would attach a
/// destination verdict to a source path, which is worse than the mismatch it fixes.
impl From<PathedVolumeError> for WriteFailure {
    fn from(e: PathedVolumeError) -> Self {
        Self {
            error: map_volume_error(&e.path.display().to_string(), PathRole::Source, e.error),
        }
    }
}

/// Attaches the failing path to a `Result<_, VolumeError>`.
///
/// Use it at the site that KNOWS the path (the loop holding `child_source`),
/// never higher: an `at()` applied one frame up re-labels the error with the
/// parent, which is the bug this whole type exists to prevent.
pub(super) trait AtPath<T> {
    fn at(self, path: &Path) -> Result<T, PathedVolumeError>;
}

impl<T> AtPath<T> for Result<T, VolumeError> {
    fn at(self, path: &Path) -> Result<T, PathedVolumeError> {
        self.map_err(|error| PathedVolumeError {
            path: path.to_path_buf(),
            error,
        })
    }
}

/// Builds a `WriteErrorEvent` from a `WriteFailure`. The FE renders all copy and
/// classification from the typed `error`. Shared by `volume::r#move` and `volume::copy`.
pub(super) fn write_error_event_from(
    operation_id: String,
    operation_type: WriteOperationType,
    failure: WriteFailure,
) -> WriteErrorEvent {
    WriteErrorEvent::new(operation_id, operation_type, failure.error)
}

/// Maps VolumeError to WriteOperationError, attaching path context where the original error lacks
/// one.
///
/// `role` decides what a `NotFound` means: the error itself doesn't say which
/// volume answered, and only the caller knows. ❌ Never guess it here.
pub(in crate::file_system::write_operations) fn map_volume_error(
    context_path: &str,
    role: PathRole,
    e: VolumeError,
) -> WriteOperationError {
    match e {
        // The same errno, two different stories for the user. Reporting a
        // destination that couldn't be addressed as a missing SOURCE is what sent
        // a NAS user hunting for a file that had never moved.
        VolumeError::NotFound(path) => match role {
            PathRole::Source => WriteOperationError::SourceNotFound { path },
            PathRole::Destination => WriteOperationError::DestinationNotFound { path },
        },
        VolumeError::PermissionDenied(msg) => WriteOperationError::PermissionDenied {
            path: context_path.to_string(),
            message: msg,
        },
        VolumeError::AlreadyExists(path) => WriteOperationError::DestinationExists { path },
        // ❗ Name the ROLE. The bare wording said only "this volume type", so a
        // transfer that died here left a reader unable to tell which of the two
        // volumes refused, let alone which call. `role` is the one fact the
        // caller has and the error doesn't, and it costs nothing to spend it.
        // (This is a technical-details string for the log and the details panel,
        // not rendered prose: the FE renders its copy from the typed variant.)
        VolumeError::NotSupported => WriteOperationError::IoError {
            path: context_path.to_string(),
            message: format!(
                "The {} volume does not support this operation",
                match role {
                    PathRole::Source => "source",
                    PathRole::Destination => "destination",
                }
            ),
        },
        VolumeError::DeviceDisconnected(_) => WriteOperationError::DeviceDisconnected {
            path: context_path.to_string(),
        },
        VolumeError::ReadOnly(_) => WriteOperationError::ReadOnlyDevice {
            path: context_path.to_string(),
            device_name: None,
        },
        VolumeError::StorageFull { .. } => WriteOperationError::InsufficientSpace {
            required: 0,
            available: 0,
            volume_name: None,
        },
        VolumeError::ConnectionTimeout(_) => WriteOperationError::ConnectionInterrupted {
            path: context_path.to_string(),
        },
        // The device's session died mid-write but the device is still attached
        // and a reopen is already running (MTP session reset). "Connection
        // interrupted, try again" is exactly right, and ❌ it must never become
        // `DeviceDisconnected`, which tells the user to go re-plug a phone that
        // never left.
        VolumeError::DeviceSessionReset(_) => WriteOperationError::ConnectionInterrupted {
            path: context_path.to_string(),
        },
        VolumeError::Cancelled(_) => WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        },
        VolumeError::IoError { message, .. } => WriteOperationError::IoError {
            path: context_path.to_string(),
            message,
        },
        // Extracting from a password-protected archive: a typed signal the FE
        // prompts on (then retries via `set_archive_password`), never a generic
        // read error.
        VolumeError::NeedsPassword { wrong_attempt } => WriteOperationError::ArchiveNeedsPassword {
            path: context_path.to_string(),
            wrong_attempt,
        },
        VolumeError::FriendlyGit(git_err) => WriteOperationError::IoError {
            path: context_path.to_string(),
            message: git_err.to_string(),
        },
        VolumeError::IsADirectory(path) => WriteOperationError::IoError {
            path,
            message: "Is a directory".to_string(),
        },
        // The destination refused the name itself, so the transfer can only
        // succeed under a different one. It must stay typed all the way to the
        // dialog: as an `IoError` the user gets "couldn't copy the file" plus a
        // Retry button that re-runs the identical, still-impossible request.
        // `context_path` is the item the walker was on, so the message names the
        // file to rename rather than the folder it lives in.
        VolumeError::InvalidName(message) => WriteOperationError::InvalidName {
            path: context_path.to_string(),
            message,
        },
        VolumeError::DeletePending(_) => WriteOperationError::DeletePending {
            path: context_path.to_string(),
        },
        // Surfaced only when the transfer engine's one-shot retry on a stale
        // destination handle ALSO failed. The fault is the destination folder
        // (its handle couldn't be re-resolved), never the source, so attach the
        // dest folder path and a destination-write classification — never
        // `SourceNotFound`, which would point the user at an intact source file.
        VolumeError::StaleDestinationHandle(dest_folder) => WriteOperationError::WriteError {
            path: dest_folder,
            message: "The destination folder couldn't be found on the device. Open the folder again and retry."
                .to_string(),
        },
    }
}

#[cfg(test)]
#[path = "transfer_error_tests.rs"]
mod tests;
