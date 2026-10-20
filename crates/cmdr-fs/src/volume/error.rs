//! [`VolumeError`], the one error union every `Volume` method fails with, and
//! its `io::Error` conversion contract.
//!
//! It lives apart from the value types in `types.rs` because it is the IPC-facing
//! vocabulary: the frontend renders every word from these variants, so renaming
//! one moves a TypeScript union member too. `mod.rs` re-exports it as
//! `volume::VolumeError`.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::error_detail::ErrorDetail;
use crate::log_error::{ErrorLogText, ErrorText, write_plain};
use crate::log_private::LogPrivate;
use crate::volume::git_error::FriendlyGitError;

/// Error type for volume operations, and the value the frontend renders words
/// from.
///
/// ❗ **This crosses IPC as a discriminated union and the frontend owns 100% of
/// the prose.** Adjacently tagged (`{ "type": "notFound", "data": "/path" }`)
/// rather than the internally-tagged shape the rest of the app uses, because
/// most variants carry a positional payload; converting all of them to named
/// fields would touch ~380 call sites and buy only a flatter JSON object.
/// Rename a variant and the TS union member has to move with it, the same
/// contract `ListingErrorReason` and `FriendlyGitErrorKind` carry.
///
/// The `String`s in here are LOG AND TECHNICAL-DETAIL text, never a sentence
/// shown on its own: the path-carrying variants carry a path (see
/// [`from_io_at`](Self::from_io_at)), and the diagnostic-carrying ones carry an
/// [`ErrorDetail`]: whatever the backend said, for the technical-details
/// disclosure, or the typed error behind it, so a log line marks its paths.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum VolumeError {
    /// No such path. Carries the path.
    NotFound(String),
    /// The OS or the backend refused access.
    PermissionDenied {
        /// The path that was refused. A backend with no path in hand carries
        /// what it has (the volume id, its own message).
        path: String,
        /// The errno behind the refusal, when there was one. It is what tells
        /// `EACCES` (the folder's permissions: an administrator could) from
        /// `EPERM` (macOS itself: a locked file, SIP), which the transfer layer
        /// turns into different advice. `None` for a backend that words its own
        /// refusals (SMB, MTP), which keeps the generic advice.
        raw_os_error: Option<i32>,
    },
    /// The destination already exists. Carries the path.
    AlreadyExists(String),
    /// Not supported by this volume type.
    NotSupported,
    /// Device went away mid-operation.
    DeviceDisconnected(ErrorDetail),
    /// The device or server is listed, but nothing has connected to it yet, so
    /// no volume answers for it. Carries the path.
    ///
    /// ❌ Never `DeviceDisconnected`, which tells the user a session dropped
    /// mid-operation, and ❌ never `NotFound`, which the frontend reads as "this
    /// folder was deleted" and walks the pane off the device. Opening it in a
    /// pane is what connects it.
    NotConnected(String),
    /// The device's session died mid-operation but the device itself is still
    /// attached, and a reopen is already running in the background (MTP: a PTP
    /// `DeviceReset`, typically after a cancelled or timed-out transfer). The
    /// operation that tripped it is lost, but retrying in a few seconds works —
    /// ❌ never map this to `DeviceDisconnected`, which would tear a live device
    /// out of the sidebar. MTP-only today. See `mtp/connection/DETAILS.md`
    /// § "Session reset is not a disconnect".
    DeviceSessionReset(ErrorDetail),
    /// Device or volume is read-only.
    ReadOnly(String),
    /// Device storage is full.
    StorageFull {
        /// What the backend reported, for the technical-details panel.
        message: ErrorDetail,
    },
    /// Connection timed out.
    ConnectionTimeout(ErrorDetail),
    /// Operation was cancelled by the user (progress callback returned Break).
    Cancelled(ErrorDetail),
    /// The path is a directory, not a file (for example, SMB STATUS_FILE_IS_A_DIRECTORY).
    IsADirectory(String),
    /// Something that isn't a directory sits where a directory has to be: a file,
    /// or a link that leads to anything but a folder. Carries the path of the
    /// thing IN THE WAY, which for a `mkdir -p` is often an ancestor of the path
    /// that was asked for.
    ///
    /// [`Volume::create_directory_all`](super::Volume::create_directory_all)
    /// raises it, on every backend. ❌ Never [`AlreadyExists`](Self::AlreadyExists),
    /// which callers of a `mkdir -p` read as "the folder is there, carry on", and
    /// ❌ never [`NotFound`](Self::NotFound), which names a folder the user asked
    /// Cmdr to CREATE as the thing that's missing. A link that leads to a folder
    /// is a folder here: see `mkdir_all` § "A link to a folder is a folder".
    NotADirectory(String),
    /// The destination can't hold this name, whatever it's asked to do with it.
    ///
    /// Distinct from [`NotFound`](Self::NotFound): the backend never got as far as
    /// looking, so retrying the same name can only fail the same way. The only fix
    /// is a different name, which is why it can't ride as a generic
    /// [`IoError`](Self::IoError) — that one offers a retry.
    ///
    /// SMB raises it from `STATUS_OBJECT_NAME_INVALID`. smb2 maps the characters
    /// SMB2 forbids outright (`"`, `*`, `:`, `<`, `>`, `?`, `\`, `|`, the control
    /// characters, and a trailing space or period) into the Unicode private-use
    /// area, so those copy through fine and what reaches here is a reserved Windows
    /// device name (`CON`, `NUL`, `LPT1`), a name past the server's own length
    /// limit, or a character the server's filesystem can't store. Carries what the
    /// backend reported, for the technical-details panel.
    InvalidName(ErrorDetail),
    /// The file is in `STATUS_DELETE_PENDING`: a delete has been requested on the server
    /// but at least one open handle is keeping the file alive. The file will disappear
    /// once the last handle closes; any new `Create` (stat, open, write) on the path
    /// fails with this status in the meantime. SMB-only today.
    DeletePending(String),
    /// A path names more than one stored entry, so the backend refused to pick
    /// one. Carries the path as it was given.
    ///
    /// SMB's [`Volume::find_stored_spelling`](super::Volume::find_stored_spelling)
    /// raises it for a path that matches several entries once Unicode form and
    /// case are set aside and none exactly (ERR-VETBX). Its device twin is
    /// [`DuplicateName`](Self::DuplicateName).
    /// ❌ Never resolve it by choosing a candidate: on a delete or an overwrite the
    /// wrong twin is someone's file.
    AmbiguousName(String),
    /// Two or more siblings carry EXACTLY this name, so the backend refused to
    /// pick one. Carries the path as it was given.
    ///
    /// MTP raises it for any op on a name two siblings share (PTP lets a phone or
    /// camera hold same-named files in one folder, #523). Handled exactly like
    /// [`AmbiguousName`](Self::AmbiguousName); its own variant only because the
    /// person reads a different cause and fix (rename one on the device).
    /// ❌ Never resolve it by choosing a twin.
    DuplicateName(String),
    /// The destination folder's cached handle was stale and the backend rejected
    /// a write into it (MTP: the device re-keyed its object handles since the
    /// folder was last listed). The backend has already refreshed its cache, so
    /// the transfer engine retries the write once with a fresh source stream.
    /// Carries the destination folder path for a destination-correct message if
    /// the retry also fails. MTP-only today.
    StaleDestinationHandle(String),
    /// The file's bytes sit in a cold storage class and can't be read until
    /// someone restores them (S3 Glacier Flexible Retrieval and Deep Archive,
    /// and Intelligent-Tiering's archive tiers, answer `InvalidObjectState`).
    /// Carries the path.
    ///
    /// The UI calls such a file "archived"; the internals say "cold storage"
    /// because "archive" already means a zip or tar here (`is_archive`,
    /// `NeedsPassword`). Retrying can only fail the same way until a restore
    /// lands, so it's typed rather than an [`IoError`](Self::IoError), which
    /// offers a Retry. S3-only today.
    ColdStorage(String),
    /// The source changed while a copy read it, so the copy stopped before
    /// publishing anything that might mix two versions. Carries the source's
    /// path. ❗ The source is the NEW version now: a move must never delete it.
    ///
    /// Typed rather than an [`IoError`](Self::IoError) so the user is told
    /// what happened; a retry copies the new version. Raised by an S3
    /// server-side copy in parts (`cmdr-s3` `server_copy.rs`), and by the copy
    /// engine for a source stream that delivered more or fewer bytes than its
    /// `StreamLength::Known` (`cmdr-operations` `transfer/volume/source_reads.rs`).
    SourceChanged(String),
    /// A reply on a signed session carried a bad or missing signature, so the backend refused
    /// it rather than trust what it said. Carries what the backend reported, for the
    /// technical-details panel.
    ///
    /// Typed rather than an [`IoError`](Self::IoError): it's a possible sign of tampering
    /// between the Mac and the server (or a broken network device), not a transport blip, so
    /// it's never retried and the host records it (`SecurityRefusal`). SMB-only today.
    SignatureInvalid(ErrorDetail),
    /// Anything the backend couldn't classify further. The classifier
    /// re-dispatches on `raw_os_error` when one is present.
    IoError {
        /// What the OS or backend reported, for the technical-details panel.
        message: ErrorDetail,
        /// The errno behind it, when there was one.
        raw_os_error: Option<i32>,
    },
    /// A password-protected archive needs a password to browse (header-encrypted
    /// 7z) or extract (any encrypted entry). `wrong_attempt` is `false` when no
    /// password has been tried and `true` when the supplied one was rejected, so
    /// the frontend can prompt afresh vs. say "that password didn't work". The
    /// archive backend raises this; the frontend supplies a per-archive password
    /// via `set_archive_password` and retries. Carries no path — the failing path
    /// is the one the caller was reading.
    NeedsPassword {
        /// `false` when no password has been tried yet, `true` when the supplied
        /// one was rejected.
        wrong_attempt: bool,
    },
    /// Structured git-layer failure.
    ///
    /// Carries the full `FriendlyGitError` (kind + path + optional raw detail)
    /// so the listing pipeline's `listing_error_from_volume_error` ships the
    /// typed git kind to `ErrorPane` as the `Git` reason (category from the
    /// kind, no baked prose) without parsing strings; the FE renders the
    /// git-specific copy. Built by the volume hooks in `file_system::git::mod`
    /// (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`).
    FriendlyGit(FriendlyGitError),
}

/// ❗ **For logs and debugging only.** Nothing a user reads comes from here: the
/// frontend renders every word from the typed variant (`src/lib/error-messages/`
/// and `src/lib/file-operations/…`), through the message catalog, in ten locales.
/// A `to_string()` that reaches a UI surface is a bug, not a shortcut. A log line
/// prints it through `LogError`, which marks the path it carries.
impl std::fmt::Display for VolumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_plain(self, f)
    }
}

/// The one text behind `Display` and `LogError`. A path-carrying variant marks its
/// path as the kind it is (a stale handle names a folder, cold storage and a changed
/// source name a file); the diagnostic-carrying ones carry a backend's own words,
/// which stay text unless an `ErrorDetail` kept a typed cause, whose values it marks. `ReadOnly` is a path from `from_io_at` and some backends but a
/// message from others, so it's marked as a path: a message hashed whole loses a
/// triage clue, an unmarked path ships a name.
impl ErrorLogText for VolumeError {
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> std::fmt::Result {
        let (prefix, value) = match self {
            Self::NotFound(path) => ("Path not found: ", LogPrivate::path(path)),
            Self::PermissionDenied { path, .. } => ("Permission denied: ", LogPrivate::path(path)),
            Self::AlreadyExists(path) => ("Already exists: ", LogPrivate::path(path)),
            Self::NotConnected(path) => ("Not connected yet: ", LogPrivate::path(path)),
            Self::ReadOnly(path) => ("Read-only: ", LogPrivate::path(path)),
            Self::IsADirectory(path) => ("Is a directory: ", LogPrivate::dir_path(path)),
            Self::NotADirectory(path) => ("Not a directory: ", LogPrivate::path(path)),
            Self::DeletePending(path) => ("Delete pending: ", LogPrivate::path(path)),
            Self::AmbiguousName(path) => ("More than one stored name matches: ", LogPrivate::path(path)),
            Self::DuplicateName(path) => ("Same-named siblings: ", LogPrivate::path(path)),
            Self::StaleDestinationHandle(path) => ("Destination folder handle was stale: ", LogPrivate::dir_path(path)),
            Self::ColdStorage(path) => (
                "In cold storage, needs a restore before reading: ",
                LogPrivate::file_path(path),
            ),
            Self::SourceChanged(path) => ("The source changed during the copy: ", LogPrivate::file_path(path)),
            Self::NotSupported => return out.write_str("Operation not supported"),
            Self::DeviceDisconnected(msg) => return detail_line(out, "Device disconnected: ", msg),
            Self::DeviceSessionReset(msg) => return detail_line(out, "Device session restarted: ", msg),
            Self::StorageFull { message } => return detail_line(out, "Storage full: ", message),
            Self::ConnectionTimeout(msg) => return detail_line(out, "Connection timed out: ", msg),
            Self::Cancelled(msg) => return detail_line(out, "Cancelled: ", msg),
            Self::InvalidName(msg) => return detail_line(out, "Name not usable at the destination: ", msg),
            Self::SignatureInvalid(msg) => return detail_line(out, "Reply refused, its signature didn't hold: ", msg),
            Self::IoError { message, .. } => return detail_line(out, "I/O error: ", message),
            Self::NeedsPassword { wrong_attempt: true } => return out.write_str("Archive password is incorrect"),
            Self::NeedsPassword { wrong_attempt: false } => return out.write_str("Archive is password-protected"),
            Self::FriendlyGit(err) => {
                out.write_str("git: ")?;
                return out.error(err);
            }
        };
        out.write_str(prefix)?;
        out.value(value)
    }
}

/// A message variant's line: its words, then the detail, which marks a typed cause's values and
/// writes a backend's own words as foreign text.
fn detail_line(out: &mut ErrorText<'_>, prefix: &str, detail: &ErrorDetail) -> std::fmt::Result {
    out.write_str(prefix)?;
    out.error(detail)
}

impl std::error::Error for VolumeError {}

// Here, not in `child_name.rs`: the module-cycle check files an impl under the type it
// produces, so beside `ChildName` it reads as `error` ↔ `child_name`.
impl From<super::NotAChildName> for VolumeError {
    fn from(err: super::NotAChildName) -> Self {
        VolumeError::InvalidName(err.to_string().into())
    }
}

impl VolumeError {
    /// Classifies a [`std::io::Error`] that happened at a KNOWN path.
    ///
    /// ❗ **The path is the payload, not context.** [`NotFound`](Self::NotFound),
    /// [`PermissionDenied`](Self::PermissionDenied), and
    /// [`AlreadyExists`](Self::AlreadyExists) are defined to carry the path, and
    /// the transfer layer takes that literally: `map_volume_error` forwards the
    /// string straight into `SourceNotFound { path }`, which the frontend renders
    /// as the name of the file the user is missing. A bare `io::Error` has no path
    /// inside it, so there is deliberately no `From<io::Error>` to reach for; use
    /// this at every site that knows which path it was touching, and
    /// [`from_io_without_path`](Self::from_io_without_path) only where none exists.
    ///
    /// `assert_not_found_carries_the_path` holds every backend to it.
    ///
    /// A kind with a typed home lands in it, the way a remote backend says the
    /// same thing (`cmdr-adb`'s `volume_error_from_errno` is the twin): a
    /// read-only filesystem is [`ReadOnly`](Self::ReadOnly), a full disk or spent
    /// quota [`StorageFull`](Self::StorageFull), a name the filesystem can't hold
    /// [`InvalidName`](Self::InvalidName). As an `IoError` each reached the user
    /// as a generic failure with a Retry that could only fail again.
    ///
    /// ❗ Everything else stays an `IoError` carrying its errno, which classifiers
    /// dispatch on: `note_root_failure` promotes a mount on `ETIMEDOUT` /
    /// `ENOTCONN` / `ESTALE`, transfer retry treats those as a blip, and every
    /// backend spells a non-empty folder `ENOTEMPTY`. ❌ Never lift one of those
    /// into a typed variant here.
    pub fn from_io_at(err: &std::io::Error, path: impl AsRef<std::path::Path>) -> Self {
        use std::io::ErrorKind;

        let located = || path.as_ref().to_string_lossy().into_owned();
        match err.kind() {
            ErrorKind::NotFound => Self::NotFound(located()),
            ErrorKind::PermissionDenied => Self::PermissionDenied {
                path: located(),
                raw_os_error: err.raw_os_error(),
            },
            ErrorKind::AlreadyExists => Self::AlreadyExists(located()),
            ErrorKind::ReadOnlyFilesystem => Self::ReadOnly(located()),
            ErrorKind::IsADirectory => Self::IsADirectory(located()),
            // What the OS said, ❌ never the path: a message variant logs as plain text, and
            // every caller names the item from its own context.
            ErrorKind::StorageFull | ErrorKind::QuotaExceeded => Self::StorageFull {
                message: err.to_string().into(),
            },
            ErrorKind::InvalidFilename => Self::InvalidName(err.to_string().into()),
            _ => Self::from_io_without_path(err),
        }
    }

    /// Classifies a [`std::io::Error`] with no path behind it: a pipe, a socket, a
    /// channel, a subprocess.
    ///
    /// Always [`IoError`](Self::IoError), because the three path-carrying variants
    /// would have nothing honest to carry. Prefer
    /// [`from_io_at`](Self::from_io_at) wherever a path is in scope.
    pub fn from_io_without_path(err: &std::io::Error) -> Self {
        Self::IoError {
            message: err.to_string().into(),
            raw_os_error: err.raw_os_error(),
        }
    }

    /// The errno this error carries, if its variant keeps one. For diagnostics: EACCES and
    /// EPERM read the same to a user but point at different fixes (POSIX/ACL versus TCC).
    #[must_use]
    pub fn raw_os_error(&self) -> Option<i32> {
        match self {
            Self::PermissionDenied { raw_os_error, .. } | Self::IoError { raw_os_error, .. } => *raw_os_error,
            _ => None,
        }
    }

    /// `, errno=13` for a log line when the variant carries an errno, and nothing otherwise.
    #[must_use]
    pub fn errno_field(&self) -> ErrnoField {
        ErrnoField(self.raw_os_error())
    }
}

/// [`VolumeError::errno_field`]'s rendering: the bare number, or no field at all.
#[derive(Debug, Clone, Copy)]
pub struct ErrnoField(Option<i32>);

impl std::fmt::Display for ErrnoField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(errno) => write!(f, ", errno={errno}"),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "from_io_at_test.rs"]
mod from_io_at_test;
