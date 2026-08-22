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

/// `ENOTEMPTY`, which POSIX numbers differently per platform.
///
/// The number, not the wording, is what the app renders "this folder still has
/// something in it" from, and every other backend reports the host platform's —
/// so a refusal from a Linux server reaching a Mac has to arrive as the Mac's,
/// or the same refusal reads differently depending on which backend produced it.
#[cfg(target_os = "linux")]
pub(crate) const ENOTEMPTY: i32 = 39;
/// `ENOTEMPTY` on everything else Cmdr builds for.
#[cfg(not(target_os = "linux"))]
pub(crate) const ENOTEMPTY: i32 = 66;

/// What the server says is at a path, asked only AFTER an operation on it
/// already failed.
///
/// ❗ The timing is the whole safety argument. As a PRE-flight guard this
/// question is a TOCTOU window: anything can happen between the answer and the
/// operation, and on a server with `posix-rename@openssh.com` what happens is a
/// silent overwrite. Asked afterwards it decides nothing that hasn't already
/// happened, so it can only make the report more accurate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WhatIsThere {
    /// Nothing, or the probe itself couldn't get an answer.
    Nothing,
    /// A file, a symlink, a socket: anything that isn't a directory.
    NotADirectory,
    /// A directory.
    Directory,
}

/// Which promise the operation that just failed owes an answer to.
///
/// SFTP v3 has no `SSH_FX_FILE_ALREADY_EXISTS` (that arrived in v4), and OpenSSH
/// folds `EEXIST`, `ENOTEMPTY`, and most of the rest of errno into the one
/// catch-all `SSH_FX_FAILURE`. The code alone therefore can't say which contract
/// was broken; what the operation was TRYING to do, plus what is at the path
/// afterwards, can.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attempted {
    /// Taking a name nothing was supposed to hold: `create_file`'s exclusive
    /// open, `create_directory`, and a no-clobber rename's destination claim.
    /// Owes `AlreadyExists`, which five conformance assertions and the
    /// folder-merge walker branch on.
    TakingAName,
    /// Removing one node, which the trait requires to refuse a directory that
    /// still holds something.
    RemovingANode,
}

/// Turns the catch-all a v3 server answers with into the variant the caller's
/// contract needs.
///
/// The table, and the reasoning behind each cell:
/// `crates/cmdr-sftp/DETAILS.md` § "The error policy".
pub(crate) fn resolve_ambiguity(err: &SftpError, path: &str, attempted: Attempted, found: WhatIsThere) -> VolumeError {
    match err {
        SftpError::SftpError(kind, message) => resolve(*kind, &message.to_string(), path, attempted, found),
        other => map_sftp_error(other),
    }
}

/// The decision itself, split from the error type so the whole table is testable
/// without a server: `SftpError`'s message field has no public constructor.
fn resolve(kind: SftpErrorKind, message: &str, path: &str, attempted: Attempted, found: WhatIsThere) -> VolumeError {
    // ❗ Only the catch-all is up for interpretation. A server that answered
    // `SSH_FX_NO_SUCH_FILE` was precise, and re-reading it through a probe taken
    // afterwards would build a lie out of a stale answer.
    if !matches!(kind, SftpErrorKind::Failure) {
        return classify(kind, message);
    }
    match (attempted, found) {
        (Attempted::TakingAName, WhatIsThere::NotADirectory | WhatIsThere::Directory) => {
            VolumeError::AlreadyExists(path.to_string())
        }
        // The rmdir failed and the directory is still there. OpenSSH answers
        // `EACCES` and `EPERM` with `SSH_FX_PERMISSION_DENIED`, so a permission
        // refusal wouldn't have arrived as the catch-all; what's left is a
        // directory that still holds something (`sftp-server.c`'s
        // `errno_to_portable`, OpenSSH 9.8, read 2026-08-22).
        (Attempted::RemovingANode, WhatIsThere::Directory) => VolumeError::IoError {
            message: message.to_string(),
            raw_os_error: Some(ENOTEMPTY),
        },
        // Nothing at the path (or nothing the probe could see) says the failure
        // was about something else entirely: a full disk, a read-only export, a
        // quota. ❗ The probe may only make a report MORE accurate, never
        // invent one.
        (Attempted::TakingAName | Attempted::RemovingANode, _) => classify(kind, message),
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
