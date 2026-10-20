//! Trusting a NAS's self-signed certificate once a person pinned it, and
//! telling an approvable refusal from a broken handshake.
//!
//! ❗ **`rustls` is confined to this file.** `transport.rs` hands its
//! [`client_config`] to `reqwest`, and `errors.rs` asks [`tls_refusal`] what a
//! failed connect was; nothing else names a TLS type.
//!
//! **The verifier.** [`PinningVerifier`] wraps the platform verifier reqwest
//! itself would use (`rustls-platform-verifier`, so macOS's trust store and its
//! rules), and only reads a REFUSAL: a certificate the OS trusts is trusted as
//! ever, pin or no pin. When the OS refuses, the leaf's SHA-256 is compared
//! with the server's pin; equal is trusted, anything else is refused with an
//! [`ApprovableRefusal`] carrying the fingerprint and whether a different one
//! was pinned, so the connect probe can ask a person. `DETAILS.md` § "Trusting
//! a self-signed certificate" has why this and not `add_root_certificate`.
//!
//! **Approvable is narrow on purpose.** Only `verify_server_cert` refuses with
//! [`ApprovableRefusal`], and only for a certificate the OS judged on trust
//! (an unknown issuer, a name or date it didn't like): ❌ never a revoked, a
//! malformed, or a badly signed one, and ❌ never anything past that point in
//! the handshake. A server that presents a pinnable certificate but can't
//! prove it holds the key fails in `verify_tls1x_signature`, which this
//! delegates untouched, so it's a broken handshake and no prompt offers to
//! trust it.

use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

use cmdr_fs::volume::tls::causes;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, OtherError, SignatureScheme};
use sha2::{Digest, Sha256};

/// The TLS configuration every client of this backend uses: the OS's trust,
/// plus `pin` (a fingerprint in [`fingerprint`]'s spelling) for a certificate
/// the OS refuses.
///
/// ❗ ALPN offers `h2` and `http/1.1`, which is what reqwest offers on its own
/// with `http2` on: a preconfigured config is taken whole, ALPN included, so
/// leaving it empty would quietly pin every server to HTTP/1.1.
pub(crate) fn client_config(pin: Option<String>) -> Result<rustls::ClientConfig, rustls::Error> {
    // The provider reqwest falls back to, so both halves agree on one.
    let provider = rustls::crypto::CryptoProvider::get_default()
        .cloned()
        .unwrap_or_else(|| Arc::new(rustls::crypto::aws_lc_rs::default_provider()));
    let platform = rustls_platform_verifier::Verifier::new(Arc::clone(&provider))?;
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinningVerifier {
            platform: Arc::new(platform),
            pin,
        }))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

/// A certificate's SHA-256, as uppercase hex pairs joined by colons: the form
/// Keychain Access and `openssl x509 -fingerprint -sha256` show.
pub(crate) fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// The OS's verdict, with one pinned certificate let through.
#[derive(Debug)]
struct PinningVerifier {
    /// The platform verifier in the app; a scripted one in the unit cells.
    platform: Arc<dyn ServerCertVerifier>,
    pin: Option<String>,
}

impl ServerCertVerifier for PinningVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let refusal = match self
            .platform
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
        {
            Ok(verified) => return Ok(verified),
            Err(refusal) => refusal,
        };
        if !approvable(&refusal) {
            return Err(refusal);
        }
        let presented = fingerprint(end_entity.as_ref());
        match &self.pin {
            Some(pin) if *pin == presented => Ok(ServerCertVerified::assertion()),
            pin => Err(rustls::Error::InvalidCertificate(CertificateError::Other(OtherError(
                Arc::new(ApprovableRefusal {
                    fingerprint: presented,
                    changed: pin.is_some(),
                    platform: refusal,
                }),
            )))),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.platform.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.platform.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.platform.supported_verify_schemes()
    }
}

/// Whether the OS refused a certificate for a reason pinning that exact
/// certificate answers: who issued it, the name it carries, its dates, or a
/// platform verdict with no closer variant (macOS reports a self-signed leaf
/// as `Other`).
///
/// ❌ Revoked, unreadable, or badly signed certificates are never approvable,
/// and neither is anything that isn't a certificate verdict at all.
fn approvable(refusal: &rustls::Error) -> bool {
    match refusal {
        rustls::Error::InvalidCertificate(reason) => !matches!(
            reason,
            CertificateError::Revoked | CertificateError::BadEncoding | CertificateError::BadSignature
        ),
        _ => false,
    }
}

/// A certificate the OS refused on trust, which a person may pin.
#[derive(Debug)]
pub(crate) struct ApprovableRefusal {
    /// The leaf's [`fingerprint`].
    pub(crate) fingerprint: String,
    /// Whether a DIFFERENT certificate is pinned for this server.
    pub(crate) changed: bool,
    /// What the OS said, for the log.
    platform: rustls::Error,
}

impl fmt::Display for ApprovableRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the certificate isn't trusted ({}) and isn't the pinned one",
            self.platform
        )
    }
}

impl StdError for ApprovableRefusal {}

/// What a TLS failure under a connect error was.
#[derive(Debug)]
pub(crate) enum TlsRefusal {
    /// A certificate a person may pin.
    Approvable {
        /// Its [`fingerprint`].
        fingerprint: String,
        /// Whether a different one is pinned.
        changed: bool,
    },
    /// A certificate refused for a reason no pin answers (revoked, malformed,
    /// badly signed).
    Certificate,
    /// A handshake that broke past the certificate, or never reached it: an
    /// alert, a protocol mismatch, a key the server couldn't prove.
    Handshake,
    /// No TLS failure anywhere in the chain.
    None,
}

/// Reads the TLS failure under `err`, by type: the `rustls::Error` the
/// handshake's `io::Error` carries. ❌ Never by a message.
///
/// ❗ Through `cmdr_fs::volume::tls::causes`, which descends INTO an
/// `io::Error`: its own `source()` skips the error it wraps, so a plain walk
/// never meets the `rustls::Error`.
pub(crate) fn tls_refusal(err: &(dyn StdError + 'static)) -> TlsRefusal {
    causes(err)
        .find_map(|cause| cause.downcast_ref::<rustls::Error>())
        .map_or(TlsRefusal::None, classify)
}

/// One `rustls::Error`, in [`TlsRefusal`]'s terms.
fn classify(refusal: &rustls::Error) -> TlsRefusal {
    match refusal {
        rustls::Error::InvalidCertificate(CertificateError::Other(other)) => {
            match other.0.downcast_ref::<ApprovableRefusal>() {
                Some(approvable) => TlsRefusal::Approvable {
                    fingerprint: approvable.fingerprint.clone(),
                    changed: approvable.changed,
                },
                None => TlsRefusal::Certificate,
            }
        }
        // ❗ `BadSignature` here may be the handshake's own signature check
        // rather than the certificate's: either way it is not approvable, and
        // the certificate wording is the honest one for both.
        rustls::Error::InvalidCertificate(_) => TlsRefusal::Certificate,
        _ => TlsRefusal::Handshake,
    }
}

#[cfg(test)]
#[path = "tls_trust_test.rs"]
mod tls_trust_test;
