//! What can go wrong reaching an SFTP server, and how the protocol's answers map
//! onto Cmdr's vocabulary.
//!
//! ❌ Nothing here is prose a user reads. The `String`s are log diagnostics, the
//! same way `cmdr-smb`'s `ShareListError` carries one; the host renders every
//! human word from the typed variant.

use cmdr_fs::volume::VolumeError;
use openssh_sftp_client::Error as SftpError;
use openssh_sftp_client::error::SftpErrorKind;

/// Why a connection attempt didn't produce a working session.
///
/// A typed value rather than a message, because the app branches on it: three of
/// these put a different thing in front of the user, and ❌ recovering that from
/// a string is what `error-string-match` forbids.
#[derive(Debug)]
pub enum SftpConnectError {
    /// The server couldn't be reached: no route, refused, DNS, or the TCP
    /// connect never completed.
    Unreachable(String),
    /// The handshake didn't finish inside the connect budget.
    TimedOut,
    /// The server's host key is explicitly revoked in `~/.ssh/known_hosts`. ❌
    /// Not approvable: a revocation says the key is known to be compromised.
    HostKeyRevoked {
        /// The SSH key-type name the server presented.
        algorithm: String,
        /// Its OpenSSH `SHA256:…` fingerprint, for the record the user checks.
        fingerprint: String,
    },
    /// Every rung of the auth ladder was refused. Retrying with the same secret
    /// can only fail again, and on some servers it locks the account.
    AuthenticationRejected,
    /// A rung that needs a secret had none, so nothing could be tried. Only the
    /// user moves this forward.
    NeedsCredentials,
    /// The SSH transport or the SFTP subsystem itself refused.
    Transport(String),
}

/// Turns an [`SftpError`] into the `Volume` vocabulary.
///
/// ⚠️ SFTP v3 collapses most of errno into `SSH_FX_FAILURE`
/// ([`SftpErrorKind::Failure`]), so a `Failure` here is genuinely
/// unclassified rather than lazily mapped: telling `EEXIST` from `ENOTEMPTY`
/// takes a stat probe, which belongs on the write path where there's something
/// to probe. Reading paths only need the four codes the protocol does
/// distinguish.
pub fn map_sftp_error(err: &SftpError) -> VolumeError {
    match err {
        SftpError::SftpError(kind, message) => classify(*kind, &message.to_string()),
        // The channel under the engine died: the session is gone, and every
        // operation on it fails fast rather than hanging.
        SftpError::IOError(io) => VolumeError::DeviceDisconnected(io.to_string()),
        // ⚠️ The engine's read or flush task exited, which it does on a
        // deserialization failure as well as on a dead channel. Either way every
        // later request on this session answers the same thing, so the honest
        // report is a lost connection. See `volume/query.rs::listing_error` for
        // the filename case that reaches here.
        SftpError::BackgroundTaskFailure(what) => VolumeError::DeviceDisconnected((*what).to_string()),
        other => VolumeError::IoError {
            message: other.to_string(),
            raw_os_error: None,
        },
    }
}

/// The status-code half of the mapping, split out so the table is testable
/// without a server: the error type's message field has no public constructor.
fn classify(kind: SftpErrorKind, message: &str) -> VolumeError {
    match kind {
        SftpErrorKind::NoSuchFile => VolumeError::NotFound(message.to_string()),
        SftpErrorKind::PermDenied => VolumeError::PermissionDenied(message.to_string()),
        SftpErrorKind::OpUnsupported => VolumeError::NotSupported,
        _ => VolumeError::IoError {
            message: message.to_string(),
            raw_os_error: None,
        },
    }
}

#[cfg(test)]
#[path = "errors_test.rs"]
mod errors_test;
