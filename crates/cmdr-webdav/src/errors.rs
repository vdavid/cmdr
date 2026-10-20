//! What can go wrong reaching a WebDAV server, and how HTTP's answers map onto
//! Cmdr's vocabulary.
//!
//! ❌ Nothing here is prose a user reads. The `String`s are log diagnostics; the
//! host renders every human word from the typed variant. ❌ Nothing here reads
//! a message: every decision is on a status code, a method, one of
//! `reqwest::Error`'s typed predicates, or the `rustls::Error` under it.

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::tls::{CertificatePrompt, CertificatePromptKind};
use log::debug;
use reqwest::StatusCode;

use crate::tls_trust::{TlsRefusal, tls_refusal};

/// Why a connection attempt didn't produce a working volume.
#[derive(Debug)]
pub enum WebdavConnectError {
    /// The server couldn't be reached: DNS, refused, no route, or a TLS
    /// handshake that broke anywhere but the certificate (`tls_trust.rs`).
    Unreachable(String),
    /// The probe didn't finish inside the connect budget.
    TimedOut,
    /// The OS refused the server's certificate on trust and a person may pin it:
    /// a self-signed NAS on first contact, or a pinned one that CHANGED
    /// (`tls_trust.rs`). ❗ No client is kept across the prompt; pinning is
    /// followed by dialing again.
    CertificateNeedsApproval(CertificatePrompt),
    /// The certificate was refused for a reason no pin answers: revoked,
    /// malformed, or badly signed. ❌ Not approvable.
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
            debug!(
                "WebDAV path={path:?}: backend=webdav, error_kind=permission_denied, code={status}",
                path = LogPrivate::path(path)
            );
            VolumeError::PermissionDenied {
                path: path.to_string(),
                raw_os_error: None,
            }
        }
        StatusCode::NOT_FOUND => {
            debug!(
                "WebDAV path={path:?}: backend=webdav, error_kind=not_found",
                path = LogPrivate::path(path)
            );
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
                message: format!("HTTP {status}").into(),
                raw_os_error: None,
            },
        },
        StatusCode::LOCKED => VolumeError::IoError {
            message: format!("HTTP {status}").into(),
            raw_os_error: Some(EBUSY),
        },
        StatusCode::INSUFFICIENT_STORAGE => VolumeError::StorageFull {
            message: format!("HTTP {status}").into(),
        },
        StatusCode::NOT_IMPLEMENTED => VolumeError::NotSupported,
        other => VolumeError::IoError {
            message: format!("HTTP {other}").into(),
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
        return VolumeError::ConnectionTimeout(path.to_string().into());
    }
    if err.is_connect() || err.is_request() {
        debug!(
            "WebDAV path={path:?}: source=backend, backend=webdav, error_kind=disconnected, detail={:?}",
            cmdr_fs::log_detail::LogDetail(&err.to_string()),
            path = LogPrivate::path(path)
        );
        return VolumeError::DeviceDisconnected(volume_id.to_string().into());
    }
    VolumeError::IoError {
        message: err.to_string().into(),
        raw_os_error: None,
    }
}

/// Classifies a `reqwest` failure on the CONNECT probe to `(host, port)`.
///
/// A TLS refusal is read off the `rustls::Error` under it (`tls_trust.rs`), by
/// type: a certificate the OS refused on trust is one a person may pin, one
/// refused for any other reason is `CertificateUntrusted`, and a handshake that
/// broke anywhere else is the server being unreachable over TLS, ❗ so no prompt
/// ever offers to trust it. ❌ Never by the message.
pub(crate) fn classify_connect_error(err: &reqwest::Error, host: &str, port: u16) -> WebdavConnectError {
    if err.is_timeout() {
        return WebdavConnectError::TimedOut;
    }
    match tls_refusal(err) {
        TlsRefusal::Approvable { fingerprint, changed } => {
            return WebdavConnectError::CertificateNeedsApproval(CertificatePrompt {
                host: host.to_string(),
                port,
                fingerprint,
                kind: if changed {
                    CertificatePromptKind::Changed
                } else {
                    CertificatePromptKind::Unknown
                },
            });
        }
        TlsRefusal::Certificate => return WebdavConnectError::CertificateUntrusted,
        TlsRefusal::Handshake => return WebdavConnectError::Unreachable(err.to_string()),
        TlsRefusal::None => {}
    }
    if err.is_connect() || err.is_request() {
        return WebdavConnectError::Unreachable(err.to_string());
    }
    WebdavConnectError::Transport(err.to_string())
}

#[cfg(test)]
#[path = "errors_test.rs"]
mod errors_test;
