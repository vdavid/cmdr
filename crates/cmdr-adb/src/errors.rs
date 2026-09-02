//! What can go wrong talking to the ADB server, and how the device's answers map
//! onto Cmdr's vocabulary.
//!
//! ❌ Nothing here is prose a user reads. The `String`s are log diagnostics, the
//! same way `cmdr-sftp`'s `SftpConnectError` carries one; the host renders every
//! human word from the typed variant.

use std::io::ErrorKind;

use cmdr_fs::volume::VolumeError;

/// A transport-level failure on an open ADB socket.
///
/// Typed on the SHAPE of the failure, never on the server's wording: the one
/// variant that carries the server's text ([`AdbError::Refused`]) is a log
/// diagnostic, and ❌ branching on it is what `error-string-match` forbids.
#[derive(Debug)]
pub enum AdbError {
    /// The socket itself broke in a way that isn't one of the shapes below.
    Io(std::io::Error),
    /// The server answered `FAIL`. The payload is its message, verbatim, for
    /// the log.
    Refused(String),
    /// The server said something the protocol doesn't allow here (a status
    /// that's neither `OKAY` nor `FAIL`, a non-hex length, an unknown sync id).
    Protocol(String),
    /// The socket closed under us: the device unplugged, `adbd` restarted, or
    /// the server went away.
    DeviceGone,
    /// A read or connect ran past its budget.
    Timeout,
    /// The caller called the operation off.
    Cancelled,
}

impl std::fmt::Display for AdbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "adb socket: {err}"),
            Self::Refused(msg) => write!(f, "adb refused: {msg}"),
            Self::Protocol(msg) => write!(f, "adb protocol: {msg}"),
            Self::DeviceGone => f.write_str("adb device gone"),
            Self::Timeout => f.write_str("adb timed out"),
            Self::Cancelled => f.write_str("adb cancelled"),
        }
    }
}

impl std::error::Error for AdbError {}

impl From<std::io::Error> for AdbError {
    /// Sorts the socket's own failure shapes into the typed variants: a peer
    /// that hung up is [`AdbError::DeviceGone`], a budget that ran out is
    /// [`AdbError::Timeout`]. Everything else stays an [`AdbError::Io`].
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            ErrorKind::UnexpectedEof
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe => Self::DeviceGone,
            ErrorKind::TimedOut => Self::Timeout,
            _ => Self::Io(err),
        }
    }
}

/// Why a connect didn't produce a volume.
///
/// A typed value rather than a message, because the app branches on it: an
/// absent binary, an unauthorized phone, and a device too old for `shell_v2`
/// each put a different thing in front of the user.
#[derive(Debug)]
pub enum AdbConnectError {
    /// No `adb` binary anywhere [`crate::server::locate_adb_binary`] looks, so
    /// an absent server can't be started.
    AdbNotInstalled,
    /// The server socket couldn't be reached, even after a start attempt.
    ServerUnreachable(String),
    /// The device with this serial isn't attached (or vanished mid-connect).
    DeviceGone(String),
    /// The device is attached but hasn't accepted this computer's RSA key. The
    /// user has to tap "Allow" on the phone.
    Unauthorized(String),
    /// The device's ADB predates `shell_v2` (Android 7, 2016). The legacy
    /// `shell:` service carries no exit code, so nothing above can be honest
    /// about whether a mutation happened; refused up front.
    DeviceTooOld {
        /// The device's serial.
        serial: String,
    },
    /// The connect ran past its budget.
    TimedOut,
    /// The user called the connect off.
    Cancelled,
    /// The transport refused or broke in a way none of the above names.
    Transport(String),
}

impl std::fmt::Display for AdbConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AdbNotInstalled => f.write_str("adb binary not found"),
            Self::ServerUnreachable(msg) => write!(f, "adb server unreachable: {msg}"),
            Self::DeviceGone(serial) => write!(f, "adb device gone: {serial}"),
            Self::Unauthorized(serial) => write!(f, "adb device unauthorized: {serial}"),
            Self::DeviceTooOld { serial } => write!(f, "adb device too old (no shell_v2): {serial}"),
            Self::TimedOut => f.write_str("adb connect timed out"),
            Self::Cancelled => f.write_str("adb connect cancelled"),
            Self::Transport(msg) => write!(f, "adb transport: {msg}"),
        }
    }
}

impl std::error::Error for AdbConnectError {}

impl From<AdbError> for AdbConnectError {
    /// The transport's failure, seen from a connect that didn't finish.
    ///
    /// ❗ [`AdbError::DeviceGone`] arrives without a serial (the transport
    /// doesn't know one), so it becomes a [`AdbConnectError::DeviceGone`] with
    /// an EMPTY serial; [`AdbConnectError::for_device`] fills it in where the
    /// serial is in scope.
    fn from(err: AdbError) -> Self {
        match err {
            AdbError::Io(io) if io.kind() == ErrorKind::ConnectionRefused => Self::ServerUnreachable(io.to_string()),
            AdbError::Io(io) => Self::Transport(io.to_string()),
            AdbError::Refused(msg) | AdbError::Protocol(msg) => Self::Transport(msg),
            AdbError::DeviceGone => Self::DeviceGone(String::new()),
            AdbError::Timeout => Self::TimedOut,
            AdbError::Cancelled => Self::Cancelled,
        }
    }
}

impl AdbConnectError {
    /// The same error with `serial` filled into the variants that carry one
    /// and arrived without it (see the `From<AdbError>` impl).
    #[must_use]
    pub fn for_device(self, serial: &str) -> Self {
        match self {
            Self::DeviceGone(s) if s.is_empty() => Self::DeviceGone(serial.to_string()),
            Self::Unauthorized(s) if s.is_empty() => Self::Unauthorized(serial.to_string()),
            other => other,
        }
    }
}

// The device is Linux whatever the host is, so the errno numbers `STA2`/`DNT2`
// carry are Linux's. Named here rather than pulled from `libc`, which would tie
// them to the HOST platform's table.

/// Linux `EPERM`.
pub const EPERM: i32 = 1;
/// Linux `ENOENT`.
pub const ENOENT: i32 = 2;
/// Linux `EACCES`.
pub const EACCES: i32 = 13;
/// Linux `EEXIST`.
pub const EEXIST: i32 = 17;
/// Linux `ENOTDIR`. Only the fake device in `testing` names it: a real one
/// falls through `volume_error_from_errno` to `IoError` carrying the number,
/// which is all the app's classifier needs, so it is gated like its one user.
#[cfg(any(test, feature = "testing"))]
pub const ENOTDIR: i32 = 20;
/// Linux `EISDIR`.
pub const EISDIR: i32 = 21;
/// Linux `ENOSPC`.
pub const ENOSPC: i32 = 28;
/// Linux `EROFS`.
pub const EROFS: i32 = 30;
/// Linux `ENAMETOOLONG`.
pub const ENAMETOOLONG: i32 = 36;
/// Linux `ENOTEMPTY`, as the DEVICE numbers it.
pub const ENOTEMPTY_DEVICE: i32 = 39;

/// `ENOTEMPTY` as the HOST numbers it, which is what the app's classifier
/// re-dispatches `raw_os_error` against (`cmdr-sftp` makes the same translation).
#[cfg(target_os = "linux")]
const ENOTEMPTY_HOST: i32 = 39;
/// `ENOTEMPTY` on everything else Cmdr builds for.
#[cfg(not(target_os = "linux"))]
const ENOTEMPTY_HOST: i32 = 66;

/// Turns a device errno (from `STA2`/`DNT2`, or a shell probe) into the
/// `Volume` vocabulary, for an operation on `path`.
///
/// ❗ **`path` is not context, it's the payload.** [`VolumeError::NotFound`] and
/// [`VolumeError::PermissionDenied`] are DEFINED to carry the path
/// (`cmdr-fs/src/volume/types.rs`), and the transfer layer forwards that string
/// straight into `SourceNotFound { path }`, which the frontend renders as the
/// name of the missing file. It goes in VERBATIM.
///
/// Anything unlisted is an [`VolumeError::IoError`] carrying the number, so the
/// app's classifier can still dispatch on it; `ENOTEMPTY` is translated to the
/// host's number on the way (the device is Linux, the host may not be).
pub fn volume_error_from_errno(errno: i32, path: &str) -> VolumeError {
    match errno {
        ENOENT => VolumeError::NotFound(path.to_string()),
        EACCES | EPERM => VolumeError::PermissionDenied(path.to_string()),
        EEXIST => VolumeError::AlreadyExists(path.to_string()),
        EROFS => VolumeError::ReadOnly(path.to_string()),
        ENOSPC => VolumeError::StorageFull {
            message: format!("ENOSPC on {path}"),
        },
        EISDIR => VolumeError::IsADirectory(path.to_string()),
        ENAMETOOLONG => VolumeError::InvalidName(path.to_string()),
        ENOTEMPTY_DEVICE => VolumeError::IoError {
            message: format!("ENOTEMPTY on {path}"),
            raw_os_error: Some(ENOTEMPTY_HOST),
        },
        other => VolumeError::IoError {
            message: format!("errno {other} on {path}"),
            raw_os_error: Some(other),
        },
    }
}

/// Turns a transport failure during an operation on `path` into the `Volume`
/// vocabulary.
///
/// A [`AdbError::Refused`] is the device's `FAIL` text, which the sync service
/// produces without an errno; it lands as an unclassified
/// [`VolumeError::IoError`] with the text in `message` for the log. Callers that
/// can do better (a shell probe, an `STA2` afterwards) resolve it before coming
/// here.
pub fn volume_error_from_adb(error: AdbError, path: &str) -> VolumeError {
    match error {
        AdbError::DeviceGone => VolumeError::DeviceDisconnected(path.to_string()),
        AdbError::Timeout => VolumeError::ConnectionTimeout(path.to_string()),
        AdbError::Cancelled => VolumeError::Cancelled(path.to_string()),
        AdbError::Io(io) => VolumeError::IoError {
            message: io.to_string(),
            raw_os_error: None,
        },
        AdbError::Refused(msg) | AdbError::Protocol(msg) => VolumeError::IoError {
            message: msg,
            raw_os_error: None,
        },
    }
}

#[cfg(test)]
#[path = "errors_test.rs"]
mod errors_test;
