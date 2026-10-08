//! [`VolumeError`], the one error union every `Volume` method fails with, and
//! its `io::Error` conversion contract.
//!
//! It lives apart from the value types in `types.rs` because it is the IPC-facing
//! vocabulary: the frontend renders every word from these variants, so renaming
//! one moves a TypeScript union member too. `mod.rs` re-exports it as
//! `volume::VolumeError`.

use serde::{Deserialize, Serialize};

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
/// [`from_io_at`](Self::from_io_at)), and the diagnostic-carrying ones carry
/// whatever the backend said, for the technical-details disclosure.
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
    DeviceDisconnected(String),
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
    DeviceSessionReset(String),
    /// Device or volume is read-only.
    ReadOnly(String),
    /// Device storage is full.
    StorageFull {
        /// What the backend reported, for the technical-details panel.
        message: String,
    },
    /// Connection timed out.
    ConnectionTimeout(String),
    /// Operation was cancelled by the user (progress callback returned Break).
    Cancelled(String),
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
    InvalidName(String),
    /// The file is in `STATUS_DELETE_PENDING`: a delete has been requested on the server
    /// but at least one open handle is keeping the file alive. The file will disappear
    /// once the last handle closes; any new `Create` (stat, open, write) on the path
    /// fails with this status in the meantime. SMB-only today.
    DeletePending(String),
    /// A path from outside this volume's own listings matches more than one
    /// stored entry once Unicode form and case are set aside, and none of them
    /// exactly, so the backend refused to pick one. Carries the path as it was
    /// given.
    ///
    /// Raised only by [`Volume::find_stored_spelling`](super::Volume::find_stored_spelling).
    /// ❌ Never resolve it by choosing a candidate: on a delete or an overwrite the
    /// wrong twin is someone's file. SMB-only today (ERR-VETBX).
    AmbiguousName(String),
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
    /// what happened; a retry copies the new version. S3-only today (a
    /// server-side copy in parts, `cmdr-s3` `server_copy.rs`).
    SourceChanged(String),
    /// Anything the backend couldn't classify further. The classifier
    /// re-dispatches on `raw_os_error` when one is present.
    IoError {
        /// What the OS or backend reported, for the technical-details panel.
        message: String,
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
    FriendlyGit(crate::volume::friendly_error::git::FriendlyGitError),
}

/// ❗ **For logs and debugging only.** Nothing a user reads comes from here: the
/// frontend renders every word from the typed variant (`src/lib/error-messages/`
/// and `src/lib/file-operations/…`), through the message catalog, in ten locales.
/// A `to_string()` that reaches a UI surface is a bug, not a shortcut.
impl std::fmt::Display for VolumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "Path not found: {}", path),
            Self::PermissionDenied { path, .. } => write!(f, "Permission denied: {}", path),
            Self::AlreadyExists(path) => write!(f, "Already exists: {}", path),
            Self::NotSupported => write!(f, "Operation not supported"),
            Self::DeviceDisconnected(msg) => write!(f, "Device disconnected: {}", msg),
            Self::NotConnected(path) => write!(f, "Not connected yet: {}", path),
            Self::DeviceSessionReset(msg) => write!(f, "Device session restarted: {}", msg),
            Self::ReadOnly(msg) => write!(f, "Read-only: {}", msg),
            Self::StorageFull { message } => write!(f, "Storage full: {}", message),
            Self::ConnectionTimeout(msg) => write!(f, "Connection timed out: {}", msg),
            Self::Cancelled(msg) => write!(f, "Cancelled: {}", msg),
            Self::IsADirectory(path) => write!(f, "Is a directory: {}", path),
            Self::NotADirectory(path) => write!(f, "Not a directory: {}", path),
            Self::InvalidName(msg) => write!(f, "Name not usable at the destination: {}", msg),
            Self::DeletePending(path) => write!(f, "Delete pending: {}", path),
            Self::AmbiguousName(path) => write!(f, "More than one stored name matches: {}", path),
            Self::StaleDestinationHandle(path) => write!(f, "Destination folder handle was stale: {}", path),
            Self::ColdStorage(path) => write!(f, "In cold storage, needs a restore before reading: {}", path),
            Self::SourceChanged(path) => write!(f, "The source changed during the copy: {}", path),
            Self::IoError { message, .. } => write!(f, "I/O error: {}", message),
            Self::NeedsPassword { wrong_attempt } => {
                if *wrong_attempt {
                    f.write_str("Archive password is incorrect")
                } else {
                    f.write_str("Archive is password-protected")
                }
            }
            Self::FriendlyGit(err) => write!(f, "git: {}", err),
        }
    }
}

impl std::error::Error for VolumeError {}

// Here, not in `child_name.rs`: the module-cycle check files an impl under the type it
// produces, so beside `ChildName` it reads as `error` ↔ `child_name`.
impl From<super::NotAChildName> for VolumeError {
    fn from(err: super::NotAChildName) -> Self {
        VolumeError::InvalidName(err.to_string())
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
            ErrorKind::StorageFull | ErrorKind::QuotaExceeded => Self::StorageFull {
                message: format!("{err}: {}", located()),
            },
            ErrorKind::InvalidFilename => Self::InvalidName(format!("{err}: {}", located())),
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
            message: err.to_string(),
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
