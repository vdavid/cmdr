//! Shared transfer-error plumbing for volume-aware copy and move.
//!
//! `WriteFailure` carries the typed `WriteOperationError` the FE renders from,
//! and `map_volume_error` / `write_error_event_from` translate an originating
//! `VolumeError` into that typed shape and the outgoing `WriteErrorEvent`. Kept
//! in its own module so both `volume::copy` and `volume::r#move` depend on it
//! rather than on each other.

use std::path::{Path, PathBuf};

use super::super::super::host::{EngineHost, UnregisteredVolume};
use super::super::super::state::WriteOperationState;
use super::super::super::types::{
    PermissionSide, ReadOnlySide, WriteErrorEvent, WriteOperationError, WriteOperationType,
};
use super::super::recovered_name::{FinalizeFailure, LandingAftermath};
use super::super::volume_outage::{self, LostConnection, OutageFailure};
use cmdr_fs::volume::VolumeError;

/// Which side of a transfer a failing path belongs to.
///
/// A `VolumeError::NotFound` carries no clue about this, and the two sides mean
/// opposite things to the user: a missing SOURCE says "your file is gone", a
/// missing DESTINATION says "there was nowhere to put it". Only the call site
/// knows which volume it asked, so the role travels WITH the path from there —
/// ❌ never inferred downstream from the path's shape, which is how a NAS
/// destination once got reported as a vanished source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathRole {
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
pub struct WriteFailure {
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

/// Turns a failed landing into the typed error the user reads, for a caller
/// outside the transfer engine (fresh compression's publish).
///
/// The rename that failed is entirely the DESTINATION's, so `dest_path` names
/// the destination entry: the source is fully read and untouched by then, and
/// pointing the user at it would send them looking at an intact file.
///
/// When the landing rescued the new bytes (or couldn't, and left them under
/// their temp name), the answer is `NewDataKeptAt` and it carries that path: the
/// user's complete new file lives ONLY there, so an error that doesn't name it
/// leaves them hunting. A landing that refused to clear the way because its
/// record couldn't reach the disk is `LandingNotRecorded`. Any other landing
/// that cleared nothing rescued nothing and lost nothing, so it maps like any
/// other destination failure.
pub(crate) fn map_finalize_failure(dest_path: &Path, failure: FinalizeFailure) -> WriteOperationError {
    WriteFailure::from(PathedVolumeError::from_landing(failure, dest_path)).error
}

/// Turns a failure at `path`, on `role`'s volume, into the typed error the user
/// reads, for a caller outside the transfer engine whose outage wait ended the
/// work (the volume delete). What the wait learned about the connection
/// (`lost`) outranks the errno, exactly as for a transfer.
pub(crate) fn map_failure_after_wait(
    path: &Path,
    role: PathRole,
    error: VolumeError,
    lost: Option<LostConnection>,
) -> WriteOperationError {
    WriteFailure::from(PathedVolumeError::new(role, path, error).with_lost_connection(lost)).error
}

/// A `VolumeError` plus the path that actually produced it, and which side's
/// call that was.
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
/// Visible beyond `volume/` because `transfer/volume_outage.rs` carries it
/// through an outage wait.
#[derive(Debug, Clone)]
pub(crate) struct PathedVolumeError {
    /// The item that failed, as deep in the tree as the walker got, on the
    /// volume `role` names: a refused nested mkdir carries the DESTINATION
    /// folder, a failed read the SOURCE file.
    pub path: PathBuf,
    /// Which volume's call failed. Set by the constructor the failing call site
    /// picked (`AtPath::at_source` / `at_destination`, `from_landing`), ❌ never
    /// defaulted: a walker frame holds both paths, and only the call that failed
    /// says which one the user has to look at.
    pub role: PathRole,
    pub error: VolumeError,
    /// Set only by a failed landing that changes what the user is told: one
    /// that already cleared the way (a swap's deleted original, a claimed name)
    /// says where the complete new bytes are now
    /// (`WriteOperationError::NewDataKeptAt`), and one that refused before
    /// clearing anything says the file was left alone
    /// (`WriteOperationError::LandingNotRecorded`). It travels with the error
    /// rather than being reconstructed anywhere.
    pub aftermath: Option<LandingAftermath>,
    /// What the failure learned beyond its errno that the user needs instead.
    /// Boxed: it's rare, and every transfer `Result` carries this type by value
    /// (`clippy::result_large_err` holds it under 128 bytes).
    pub cause: Option<Box<FailureCause>>,
}

/// A cause that outranks a failure's errno in what the user is told.
#[derive(Debug, Clone)]
pub(crate) enum FailureCause {
    /// An outage wait ENDED the item's work on a server whose connection is
    /// gone (`volume_outage::lost_connection`): the connection is the cause the
    /// user needs (`WriteOperationError::ServerConnectionLost`), and only the
    /// wait knows it.
    LostConnection(LostConnection),
    /// A refusal made before any volume call failed (a drive mounted inside a
    /// folder an Overwrite would set aside, `PathedVolumeError::refused`): it
    /// already is the user's answer, and it outranks everything else here.
    Refused(WriteOperationError),
}

/// Maps with the side the failure carries. Both directions have cost a user a
/// wrong diagnosis: a source delete's `EPERM` blamed on the destination
/// (cmdr-reports#17), and a share refusing a nested folder read as the user's
/// local source being locked (cmdr-reports#62). A landing's `aftermath` turns it
/// into `NewDataKeptAt` instead (the user needs to be told where their new file
/// ended up rather than which errno the rename answered), or into
/// `LandingNotRecorded` (Cmdr's own refusal, with no errno at all).
/// A lost connection (`FailureCause::LostConnection`) turns it into
/// `ServerConnectionLost`: the server's connection is the cause, and the errno
/// says nothing a person can act on. What the landing did outranks both, and a
/// `FailureCause::Refused` (made before any volume call) outranks everything:
/// it already is the answer.
impl From<PathedVolumeError> for WriteFailure {
    fn from(e: PathedVolumeError) -> Self {
        let lost = match e.cause.map(|cause| *cause) {
            Some(FailureCause::Refused(refusal)) => return Self { error: refusal },
            Some(FailureCause::LostConnection(lost)) => Some(lost),
            None => None,
        };
        let path = e.path.display().to_string();
        let error = match (e.aftermath, lost) {
            (Some(LandingAftermath::NewDataKeptAt(kept_at)), _) => WriteOperationError::NewDataKeptAt {
                path,
                kept_at: kept_at.display().to_string(),
                message: e.error.to_string(),
            },
            (Some(LandingAftermath::NotRecorded), _) => WriteOperationError::LandingNotRecorded { path },
            (None, Some(lost)) => {
                let LostConnection {
                    volume_name,
                    gave_up_after,
                } = lost;
                WriteOperationError::ServerConnectionLost {
                    path,
                    volume_name,
                    gave_up_after_secs: gave_up_after.map(|waited| waited.as_secs()),
                }
            }
            (None, None) => map_volume_error(&path, e.role, e.error),
        };
        Self { error }
    }
}

/// Keeps the path and side through an outage wait, so a walk's listing that
/// waited out a dropped share still names the side that failed.
impl OutageFailure for PathedVolumeError {
    fn volume_error(&self) -> &VolumeError {
        &self.error
    }

    fn into_cancelled(mut self) -> Self {
        self.error = volume_outage::cancelled();
        self
    }

    fn with_lost_connection(mut self, lost: Option<LostConnection>) -> Self {
        if let Some(lost) = lost {
            self.cause = Some(Box::new(FailureCause::LostConnection(lost)));
        }
        self
    }
}

/// The ways a `PathedVolumeError` comes to be, besides [`AtPath`].
///
/// ❗ `from_landing` is inherent to `PathedVolumeError`, the type it PRODUCES,
/// and not a method on `FinalizeFailure`. `cargo-modules` attributes an `impl`
/// to the module defining the type, so `impl FinalizeFailure` written here would
/// print as `recovered_name → volume::transfer_error`, welding the two into a
/// module cycle that reads backwards from the code. ❌ Don't move it onto
/// `FinalizeFailure` for the nicer call syntax. `scripts/check/checks/DETAILS.md`
/// § "Rust module cycles", trap 4.
impl PathedVolumeError {
    /// A failure of a call made on `role`'s volume, at `path` on it.
    pub(super) fn new(role: PathRole, path: &Path, error: VolumeError) -> Self {
        Self {
            path: path.to_path_buf(),
            role,
            error,
            aftermath: None,
            cause: None,
        }
    }

    /// Labels a failed landing (a staged write's rename, a safe-replace's
    /// swap) with the destination name the new bytes were meant to take. It
    /// is entirely the destination's, and its `aftermath` says where the bytes
    /// went when it rescued them, or that it refused before clearing anything.
    pub(super) fn from_landing(failure: FinalizeFailure, dest_path: &Path) -> Self {
        Self {
            path: dest_path.to_path_buf(),
            role: PathRole::Destination,
            error: failure.error,
            aftermath: failure.aftermath,
            cause: None,
        }
    }

    /// A refusal made before any volume call, at `path` on the destination.
    /// `error` is what a walker that inspects it sees: a plain I/O failure,
    /// neither a cancel nor an outage, so nothing retries or waits on it.
    pub(super) fn refused(path: &Path, refusal: WriteOperationError) -> Self {
        Self {
            path: path.to_path_buf(),
            role: PathRole::Destination,
            error: VolumeError::IoError {
                message: format!("{refusal:?}").into(),
                raw_os_error: None,
            },
            aftermath: None,
            cause: Some(Box::new(FailureCause::Refused(refusal))),
        }
    }

    /// For a caller that speaks `WriteOperationError` (a driver's resolve
    /// closure, the rename merge), so a pathed failure reaches it mapped the
    /// one way, with its side and what an outage wait learned.
    ///
    /// ❗ A method, ❌ not `From<PathedVolumeError> for WriteOperationError`:
    /// a `From` for a type defined in `types::errors` prints as
    /// `types::errors → volume::transfer_error` (trap 3 of the section named
    /// above), which welds the `types` floor to most of the crate.
    pub(crate) fn into_write_error(self) -> WriteOperationError {
        WriteFailure::from(self).error
    }
}

/// Attaches the failing path, and the side it's on, to a
/// `Result<_, VolumeError>`.
///
/// Use it at the site that KNOWS the path (the loop holding `child_source`),
/// never higher: one applied a frame up re-labels the error with the parent,
/// which is the bug this whole type exists to prevent. Pick the method by WHICH
/// VOLUME the failing call went to, and pass that volume's path: a frame holding
/// both `child_source` and `child_dest` reports a destination write at
/// `child_dest`.
pub(super) trait AtPath<T> {
    /// The call went to the source volume, at `path` on it.
    fn at_source(self, path: &Path) -> Result<T, PathedVolumeError>;
    /// The call went to the destination volume, at `path` on it.
    fn at_destination(self, path: &Path) -> Result<T, PathedVolumeError>;
}

impl<T> AtPath<T> for Result<T, VolumeError> {
    fn at_source(self, path: &Path) -> Result<T, PathedVolumeError> {
        self.map_err(|error| PathedVolumeError::new(PathRole::Source, path, error))
    }

    fn at_destination(self, path: &Path) -> Result<T, PathedVolumeError> {
        self.map_err(|error| PathedVolumeError::new(PathRole::Destination, path, error))
    }
}

/// Builds a `WriteErrorEvent` from a `WriteFailure`, marking the row it names
/// in the operation's transfer tree. The FE renders all copy and classification
/// from the typed `error`. Shared by `volume::r#move`, `volume::move_same`, and
/// `volume::copy`.
pub(super) fn write_error_event_from(
    state: &WriteOperationState,
    operation_id: String,
    operation_type: WriteOperationType,
    failure: WriteFailure,
) -> WriteErrorEvent {
    state.transfer_tree.note_failure(&failure.error);
    WriteErrorEvent::new(operation_id, operation_type, failure.error)
}

/// The refusal for an operation whose volume isn't registered, asked before
/// anything is read or written. `path` is what the caller sent for that side: the
/// destination folder, or the first source.
///
/// A listed phone or a saved server nothing has connected is
/// `SourceNotConnected` / `DestinationNotConnected`, so the user hears that opening
/// it is the way through. Any other id is a volume that left the registry: as a
/// source, `SourceNoLongerConnected` (a phone unplugged under a search-results
/// pane is how a person gets here); as a destination, an unmount race, an
/// `IoError` naming the id. The classification itself: `OperationVolumes::unregistered`.
pub(crate) async fn unregistered_volume_error(
    host: &EngineHost,
    volume_id: &str,
    path: &str,
    role: PathRole,
) -> WriteOperationError {
    match (host.volumes().unregistered(volume_id).await, role) {
        (UnregisteredVolume::NotConnected, role) => not_connected(path, role),
        (UnregisteredVolume::Gone, PathRole::Source) => {
            WriteOperationError::SourceNoLongerConnected { path: path.to_string() }
        }
        // (Log and technical-details text, not rendered prose.)
        (UnregisteredVolume::Gone, PathRole::Destination) => WriteOperationError::IoError {
            path: volume_id.to_string(),
            message: format!("Destination volume '{volume_id}' not found"),
        },
    }
}

impl crate::Operations {
    /// [`unregistered_volume_error`] for a SOURCE, for a caller outside the engine that
    /// checks the volume before it hands anything over (Ask Cmdr's approval bridge), so its
    /// refusal is worded exactly as a clicked copy's would be.
    pub async fn unregistered_source_error(&self, volume_id: &str, path: &str) -> WriteOperationError {
        unregistered_volume_error(self.host(), volume_id, path, PathRole::Source).await
    }
}

/// A move whose copy LANDED at `landed_at` but whose source `refused` to go. ❗ Not a failed
/// move: the item is now in both places, and the bare refusal ("you don't have permission to
/// move files here") made a user conclude nothing had happened (cmdr-reports#17). The refusal
/// rides along as `cause` for its own advice.
pub(super) fn source_not_removed(refused: &Path, landed_at: &Path, e: VolumeError) -> WriteOperationError {
    let path = refused.display().to_string();
    WriteOperationError::SourceNotRemoved {
        landed_at: landed_at.display().to_string(),
        cause: Box::new(map_volume_error(&path, PathRole::Source, e)),
        path,
    }
}

fn not_connected(path: &str, role: PathRole) -> WriteOperationError {
    let path = path.to_string();
    match role {
        PathRole::Source => WriteOperationError::SourceNotConnected { path },
        PathRole::Destination => WriteOperationError::DestinationNotConnected { path },
    }
}

/// Maps VolumeError to WriteOperationError, attaching path context where the original error lacks
/// one.
///
/// `role` decides what a `NotFound` means: the error itself doesn't say which
/// volume answered, and only the caller knows. ❌ Never guess it here.
pub(crate) fn map_volume_error(context_path: &str, role: PathRole, e: VolumeError) -> WriteOperationError {
    match e {
        // The same errno, two different stories for the user. Reporting a
        // destination that couldn't be addressed as a missing SOURCE is what sent
        // a NAS user hunting for a file that had never moved.
        VolumeError::NotFound(path) => match role {
            PathRole::Source => WriteOperationError::SourceNotFound { path },
            PathRole::Destination => WriteOperationError::DestinationNotFound { path },
        },
        // The errno rides through, so `EPERM` (a locked file, macOS itself) and
        // `EACCES` (the folder's permissions) get their own advice; a backend that
        // words its own refusals has none and keeps the generic sentence. `message`
        // is for the details block, which already shows the path: the OS's
        // sentence when there's an errno, else whatever the backend carried if
        // it isn't that same path (MTP carries its own words). No folder: ❌
        // naming one we didn't prove with `access(W_OK)` is what the local path
        // exists to avoid.
        VolumeError::PermissionDenied { path, raw_os_error } => {
            let message = match raw_os_error {
                Some(errno) => std::io::Error::from_raw_os_error(errno).to_string(),
                None if path != context_path => path,
                None => "Permission denied".to_string(),
            };
            let side = Some(match role {
                PathRole::Source => PermissionSide::Source,
                PathRole::Destination => PermissionSide::Destination,
            });
            // An S3 refusal is the account's (its keys, or a provider that paused
            // it over a usage cap or billing), read off the app path's own
            // scheme, ❌ never the message.
            if raw_os_error.is_none()
                && cmdr_fs::volume::server_of_path(context_path)
                    .is_some_and(|server| server.kind == cmdr_fs::volume::BackendKind::S3)
            {
                return WriteOperationError::object_store_refused(context_path.to_string(), message, side);
            }
            WriteOperationError::permission_denied(context_path.to_string(), message, raw_os_error, None, side)
        }
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
        // A backend session that dropped (MTP, SMB), which has no mount table
        // and no captured sides behind it, so the copy stays the
        // volume-agnostic one. A LOCAL drive that leaves is named at the
        // operation's boundary instead (`transfer_sides.rs`).
        VolumeError::DeviceDisconnected(_) => WriteOperationError::DeviceDisconnected {
            path: context_path.to_string(),
            side: None,
        },
        // Names the half that asked, the same way an unregistered volume does
        // before a transfer starts (`unregistered_volume_error`). ❌ Never
        // `DeviceDisconnected`, which would tell the user a session dropped
        // mid-copy.
        VolumeError::NotConnected(_) => not_connected(context_path, role),
        // The backend refused a WRITE, so it is the destination that is read-only.
        VolumeError::ReadOnly(_) => WriteOperationError::ReadOnlyDevice {
            path: context_path.to_string(),
            device_name: None,
            side: ReadOnlySide::Destination,
        },
        // The destination refused a write for lack of room. Nothing measured it,
        // so it's the sizeless variant; `InsufficientSpace` is for a pre-flight
        // that has both numbers.
        VolumeError::StorageFull { .. } => WriteOperationError::DestinationFull {
            path: context_path.to_string(),
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
            message: message.to_string(),
        },
        // A reply refused for its signature stops the transfer with its own stopped state; the
        // person also hears about the server once per session (`crate::security_notices`).
        VolumeError::SignatureInvalid(message) => WriteOperationError::ReplySignatureRefused {
            path: context_path.to_string(),
            message: message.to_string(),
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
        // A file where the destination folder, or one above it, should be. The
        // volume names the thing in the way, and that path is the one fact the
        // user can act on, so it rides through typed: as an `IoError` the dialog
        // offered a Retry that could only meet the same file again.
        VolumeError::NotADirectory(path) => WriteOperationError::DestinationNotAFolder { path },
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
        // Only a source can be archived: nothing writes into cold storage. Typed
        // to the dialog, because the fix is a restore and a Retry can't be one.
        VolumeError::ColdStorage(_) => WriteOperationError::SourceInColdStorage {
            path: context_path.to_string(),
        },
        // A server-side copy saw its source replaced mid-copy, or a source
        // stream broke its declared length (`source_reads.rs`), and nothing
        // landed. Named plainly: the user decides whether the new version is
        // the one to copy.
        VolumeError::SourceChanged(_) => WriteOperationError::SourceChanged {
            path: context_path.to_string(),
        },
        // On the DESTINATION the folder holds two look-alikes of the name and
        // neither spelled as asked (`landing.rs`), or two same-named files on a
        // device, so it's taken and no answer picks which: "already exists" is
        // the truth, with no Retry that would ask the same question again. On the
        // SOURCE it's an MTP folder holding two same-named files, and the backend
        // won't pick one to read (#523): nothing is taken anywhere, so it stays a
        // refusal of this item.
        VolumeError::AmbiguousName(path) | VolumeError::DuplicateName(path) => match role {
            PathRole::Destination => WriteOperationError::DestinationExists { path },
            PathRole::Source => WriteOperationError::IoError {
                path,
                message: "More than one item on the device answers to this name, so Cmdr didn't pick one to read"
                    .to_string(),
            },
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

/// What tier 2 reports when it ends a wait.
///
/// A `Cancelled`, deliberately, and it decides three things at once: `retry.rs`
/// never re-runs a cancel, the post-loop keys `write-cancelled` off a
/// `Cancelled`-shaped error (so an abort closes the dialog instead of logging a
/// failed transfer), and no caller mistakes it for a transport fault worth
/// reporting to the user.
pub(super) fn hard_abort_error(path: &Path) -> VolumeError {
    VolumeError::Cancelled(format!("stopped waiting for {} so the app can quit", path.display()).into())
}

#[cfg(test)]
#[path = "transfer_error_tests.rs"]
mod tests;
