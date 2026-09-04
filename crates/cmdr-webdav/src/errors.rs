//! What can go wrong reaching a WebDAV server, and how HTTP's answers map onto
//! Cmdr's vocabulary.
//!
//! ❌ Nothing here is prose a user reads. The `String`s are log diagnostics; the
//! host renders every human word from the typed variant. ❌ Nothing here reads
//! a message: every decision is on a status code, a method, or one of
//! `reqwest::Error`'s typed predicates.

use cmdr_fs::volume::VolumeError;
use log::debug;
use reqwest::StatusCode;

/// Why a connection attempt didn't produce a working volume.
#[derive(Debug)]
pub enum WebdavConnectError {
    /// The server couldn't be reached: DNS, refused, no route, or a TLS
    /// handshake that broke for a reason other than trust.
    Unreachable(String),
    /// The probe didn't finish inside the connect budget.
    TimedOut,
    /// The TLS handshake was refused on trust, which is what a self-signed NAS
    /// certificate produces. Typed so the frontend can say so; there is no
    /// pinning flow yet.
    CertificateUntrusted,
    /// The server refused the stored secret. Retrying with the same one can
    /// only fail again, and on some servers it locks the account.
    AuthenticationRejected,
    /// The server challenged with no scheme this backend speaks (a Digest-only
    /// server). ❗ Its own variant rather than `AuthenticationRejected`: the
    /// secret was never offered, so nothing about it is known to be wrong.
    AuthMethodUnsupported,
    /// The store held nothing to offer, so nothing was tried.
    NeedsCredentials,
    /// Something answered, but not with a WebDAV `multistatus`: an HTML page on
    /// the wrong path, a 405 on PROPFIND.
    NotAWebdavServer,
    /// Anything else: a 5xx on the probe, a body that wouldn't read.
    Transport(String),
    /// The user called the connect off. Nothing is wrong with the server or the
    /// credentials, so a frontend that got one has nothing to report.
    Cancelled,
}

/// Which promise the operation that just got a status owes an answer to.
///
/// HTTP's codes are about the request, not the contract: a 405 is "method not
/// allowed" on a MKCOL that hit an existing collection AND on a server that
/// doesn't do MKCOL at all, and a 412 is a broken precondition whatever the
/// precondition said. What the operation was TRYING to do is what turns a code
/// into the variant the caller branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attempted {
    /// Reading or removing something that should be there.
    Reaching,
    /// Taking a name nothing was supposed to hold: `create_file`'s
    /// `If-None-Match: *`, `create_directory`'s MKCOL, and a no-clobber MOVE's
    /// `Overwrite: F`. Owes `AlreadyExists`.
    TakingAName,
}

/// `EBUSY`, which is 16 on every platform Cmdr builds for (unlike `ENOTEMPTY`,
/// which `mutation.rs` has to split). The number is what the app renders
/// "something else is using this" from.
pub(crate) const EBUSY: i32 = 16;

/// Turns a non-success status into the `Volume` vocabulary, for an operation on
/// `path`.
///
/// ❗ **`path` is not context, it's the payload.** `NotFound` and
/// `PermissionDenied` are DEFINED to carry the path, and the transfer layer
/// renders it as the name of the file the user is missing.
pub(crate) fn map_status(status: StatusCode, path: &str, attempted: Attempted) -> VolumeError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            debug!("WebDAV {path}: the server refused access ({status})");
            VolumeError::PermissionDenied(path.to_string())
        }
        StatusCode::NOT_FOUND => {
            debug!("WebDAV {path}: the server reports nothing there");
            VolumeError::NotFound(path.to_string())
        }
        // A MKCOL on an occupied name (RFC 4918 § 9.3.1). Anywhere else it
        // means the server doesn't do the method.
        StatusCode::METHOD_NOT_ALLOWED => match attempted {
            Attempted::TakingAName => VolumeError::AlreadyExists(path.to_string()),
            Attempted::Reaching => VolumeError::NotSupported,
        },
        // A missing ancestor, on MKCOL, PUT, MOVE, and COPY alike (RFC 4918
        // § 9.3.1, § 9.8.5, § 9.9.4).
        StatusCode::CONFLICT => VolumeError::NotFound(path.to_string()),
        // The precondition this backend sets is always "nothing is there":
        // `If-None-Match: *` or `Overwrite: F`.
        StatusCode::PRECONDITION_FAILED => match attempted {
            Attempted::TakingAName => VolumeError::AlreadyExists(path.to_string()),
            Attempted::Reaching => VolumeError::IoError {
                message: format!("HTTP {status}"),
                raw_os_error: None,
            },
        },
        StatusCode::LOCKED => VolumeError::IoError {
            message: format!("HTTP {status}"),
            raw_os_error: Some(EBUSY),
        },
        StatusCode::INSUFFICIENT_STORAGE => VolumeError::StorageFull {
            message: format!("HTTP {status}"),
        },
        StatusCode::NOT_IMPLEMENTED => VolumeError::NotSupported,
        other => VolumeError::IoError {
            message: format!("HTTP {other}"),
            raw_os_error: None,
        },
    }
}

/// Turns a `reqwest` failure (no status came back) into the `Volume`
/// vocabulary, by its typed predicates.
///
/// A connection that couldn't be made or was cut mid-flight is the volume
/// being gone, which is what starts the reconnect loop; a timeout is its own
/// variant so the transfer layer can retry rather than remount.
pub(crate) fn map_transport_error(err: &reqwest::Error, volume_id: &str, path: &str) -> VolumeError {
    if err.is_timeout() {
        return VolumeError::ConnectionTimeout(path.to_string());
    }
    if err.is_connect() || err.is_request() {
        debug!("WebDAV {path}: the connection is gone ({err})");
        return VolumeError::DeviceDisconnected(volume_id.to_string());
    }
    VolumeError::IoError {
        message: err.to_string(),
        raw_os_error: None,
    }
}

/// Classifies a `reqwest` failure on the CONNECT probe.
///
/// A TLS refusal reaches here as a connect error whose source chain carries an
/// `io::Error` of kind `InvalidData`, which is how `tokio-rustls` wraps every
/// handshake refusal (trust included, and by far the commonest). ❗ Judged by
/// the typed `ErrorKind`, ❌ never by the message.
pub(crate) fn classify_connect_error(err: &reqwest::Error) -> WebdavConnectError {
    if err.is_timeout() {
        return WebdavConnectError::TimedOut;
    }
    if err.is_connect() {
        if has_tls_refusal(err) {
            return WebdavConnectError::CertificateUntrusted;
        }
        return WebdavConnectError::Unreachable(err.to_string());
    }
    if err.is_request() {
        return WebdavConnectError::Unreachable(err.to_string());
    }
    WebdavConnectError::Transport(err.to_string())
}

/// Whether an `io::Error` of kind `InvalidData` sits anywhere under `err`.
fn has_tls_refusal(err: &reqwest::Error) -> bool {
    let mut source = std::error::Error::source(err);
    while let Some(inner) = source {
        if let Some(io) = inner.downcast_ref::<std::io::Error>()
            && io.kind() == std::io::ErrorKind::InvalidData
        {
            return true;
        }
        source = inner.source();
    }
    false
}

#[cfg(test)]
#[path = "errors_test.rs"]
mod errors_test;
