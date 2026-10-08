//! Telling a refused TLS handshake from any other failed connect, for the HTTP
//! backends (WebDAV, S3), whose connect probes word a self-signed server
//! differently from an unreachable one.
//!
//! `tokio-rustls` wraps every handshake refusal in an `io::Error` of kind
//! `InvalidData`, somewhere down the client error's source chain. ❗ Judged by
//! that typed kind, ❌ never by the message.

use std::error::Error;

/// Whether an `io::Error` of kind `InvalidData` sits anywhere under `err`.
pub fn has_tls_refusal(err: &(dyn Error + 'static)) -> bool {
    let mut source = err.source();
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
#[path = "tls_test.rs"]
mod tls_test;
