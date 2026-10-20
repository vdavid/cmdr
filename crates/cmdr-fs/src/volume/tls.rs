//! Telling a refused TLS handshake from any other failed connect, for the HTTP
//! backends (WebDAV, S3), whose connect probes word a self-signed server
//! differently from an unreachable one.
//!
//! `tokio-rustls` wraps every handshake refusal in an `io::Error` of kind
//! `InvalidData`, somewhere down the client error's source chain. ❗ Judged by
//! that typed kind, ❌ never by the message.
//!
//! Also the vocabulary of trusting a certificate macOS doesn't
//! ([`CertificatePrompt`]): the backend that verifies says WHICH certificate,
//! the app asks a person and keeps the pin (`host::certificates`).

use std::error::Error;

use serde::{Deserialize, Serialize};

/// A server certificate the OS refused and a person may trust anyway, and what
/// to say about it.
///
/// The TLS twin of an SSH host-key prompt: crosses IPC as-is, and the
/// approval hands `host`, `port`, and `fingerprint` straight back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CertificatePrompt {
    /// The server, as addressed.
    pub host: String,
    /// Its port. Part of the identity: two services on one NAS can serve two
    /// certificates.
    pub port: u16,
    /// The leaf certificate's SHA-256, as uppercase hex pairs joined by colons
    /// (`AB:CD:…`, 32 pairs): the form Keychain Access, browsers, and
    /// `openssl x509 -fingerprint -sha256` show, so a person can compare it.
    pub fingerprint: String,
    /// Whether this is first contact or a certificate that CHANGED.
    pub kind: CertificatePromptKind,
}

/// Which of the two moments a certificate prompt is for.
///
/// ❗ Two variants rather than one flag because they must never share a path: a
/// first-seen certificate is routine, and a changed one is the shape a
/// man-in-the-middle takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CertificatePromptKind {
    /// Nothing is pinned for this server.
    Unknown,
    /// A different certificate is pinned for this server.
    Changed,
}

/// Whether an `io::Error` of kind `InvalidData` sits anywhere under `err`.
pub fn has_tls_refusal(err: &(dyn Error + 'static)) -> bool {
    causes(err).any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::InvalidData)
    })
}

/// Every error under `err`, nearest first, ❗ descending INTO each `io::Error`'s
/// wrapped error.
///
/// `io::Error::source` skips the error it wraps and answers that error's own
/// source, so a plain `source()` walk never sees it. reqwest 0.13.5 nests the
/// handshake's `io::Error` (kind `InvalidData`, holding the `rustls::Error`)
/// inside another of kind `Other` (printed off `webdav-fixture-tls`,
/// 2026-10-20), which a plain walk passes straight over.
pub fn causes<'a>(err: &'a (dyn Error + 'static)) -> impl Iterator<Item = &'a (dyn Error + 'static)> {
    std::iter::successors(cause_of(err), |cause| cause_of(*cause))
}

/// The error directly under `err`: an `io::Error`'s wrapped one, else `source()`.
fn cause_of<'a>(err: &'a (dyn Error + 'static)) -> Option<&'a (dyn Error + 'static)> {
    if let Some(wrapped) = err.downcast_ref::<std::io::Error>().and_then(std::io::Error::get_ref) {
        return Some(wrapped);
    }
    err.source()
}

#[cfg(test)]
#[path = "tls_test.rs"]
mod tls_test;
