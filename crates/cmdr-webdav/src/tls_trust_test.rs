//! The pin's decision table and the refusal reader, with a scripted platform
//! verifier and no server. The real handshake against a self-signed server is
//! `volume/tls_trust_test.rs`.

use std::fmt;
use std::io;
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{AlertDescription, CertificateError, DigitallySignedStruct, OtherError, SignatureScheme};

use super::{ApprovableRefusal, PinningVerifier, TlsRefusal, approvable, fingerprint, tls_refusal};

/// A platform verifier that answers one verdict for every certificate.
#[derive(Debug)]
struct Scripted(Option<CertificateError>);

impl ServerCertVerifier for Scripted {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match &self.0 {
            None => Ok(ServerCertVerified::assertion()),
            Some(reason) => Err(rustls::Error::InvalidCertificate(reason.clone())),
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        Vec::new()
    }
}

/// The bytes a server presents, as far as the pin cares: it never parses them.
const LEAF: &[u8] = b"a self-signed NAS certificate";

fn verify(platform: Option<CertificateError>, pin: Option<&str>) -> Result<ServerCertVerified, rustls::Error> {
    let verifier = PinningVerifier {
        platform: Arc::new(Scripted(platform)),
        pin: pin.map(str::to_string),
    };
    verifier.verify_server_cert(
        &CertificateDer::from(LEAF),
        &[],
        &ServerName::try_from("nas.local").expect("a valid name"),
        &[],
        UnixTime::now(),
    )
}

/// What a refusal says once it has travelled the way `tokio-rustls` sends it.
fn read_back(refusal: rustls::Error) -> TlsRefusal {
    tls_refusal(&Wrapping(Box::new(io::Error::new(io::ErrorKind::InvalidData, refusal))))
}

/// An error wrapping `source`, the way a client error wraps its cause.
#[derive(Debug)]
struct Wrapping(Box<dyn std::error::Error + Send + Sync + 'static>);

impl fmt::Display for Wrapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wrapping")
    }
}

impl std::error::Error for Wrapping {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

#[test]
fn a_fingerprint_is_uppercase_hex_pairs_joined_by_colons() {
    // SHA-256 of "abc", FIPS 180-2 appendix B.1.
    assert_eq!(
        fingerprint(b"abc"),
        "BA:78:16:BF:8F:01:CF:EA:41:41:40:DE:5D:AE:22:23:B0:03:61:A3:96:17:7A:9C:B4:10:FF:61:F2:00:15:AD"
    );
}

#[test]
fn a_certificate_the_os_trusts_is_trusted_whatever_is_pinned() {
    assert!(verify(None, None).is_ok());
    assert!(
        verify(None, Some("not-this-one")).is_ok(),
        "a pin never takes trust away"
    );
}

#[test]
fn an_untrusted_certificate_matching_its_pin_is_trusted() {
    assert!(verify(Some(CertificateError::UnknownIssuer), Some(&fingerprint(LEAF))).is_ok());
}

#[test]
fn an_untrusted_certificate_with_no_pin_is_first_contact() {
    let refused = verify(Some(CertificateError::UnknownIssuer), None).expect_err("nothing pinned");
    match read_back(refused) {
        TlsRefusal::Approvable {
            fingerprint: shown,
            changed,
        } => {
            assert_eq!(shown, fingerprint(LEAF));
            assert!(!changed);
        }
        other => panic!("expected an approvable refusal, got {other:?}"),
    }
}

#[test]
fn an_untrusted_certificate_under_a_different_pin_has_changed() {
    let refused = verify(Some(CertificateError::UnknownIssuer), Some("AB:CD")).expect_err("another pin");
    assert!(matches!(
        read_back(refused),
        TlsRefusal::Approvable { changed: true, .. }
    ));
}

#[test]
fn a_revoked_certificate_is_never_approvable_even_when_pinned() {
    let refused = verify(Some(CertificateError::Revoked), Some(&fingerprint(LEAF))).expect_err("revoked");
    assert!(matches!(read_back(refused), TlsRefusal::Certificate));
}

#[test]
fn only_trust_verdicts_are_approvable() {
    for reason in [
        CertificateError::UnknownIssuer,
        CertificateError::NotValidForName,
        CertificateError::Expired,
        CertificateError::NotValidYet,
        // macOS reports a self-signed leaf as a platform `Other`.
        CertificateError::Other(OtherError(Arc::new(io::Error::other("not trusted")))),
    ] {
        assert!(
            approvable(&rustls::Error::InvalidCertificate(reason.clone())),
            "{reason:?}"
        );
    }
    for reason in [
        CertificateError::Revoked,
        CertificateError::BadEncoding,
        CertificateError::BadSignature,
    ] {
        assert!(
            !approvable(&rustls::Error::InvalidCertificate(reason.clone())),
            "{reason:?}"
        );
    }
    assert!(!approvable(&rustls::Error::AlertReceived(
        AlertDescription::HandshakeFailure
    )));
}

#[test]
fn a_broken_handshake_reads_as_a_handshake_failure_not_a_certificate() {
    assert!(matches!(
        read_back(rustls::Error::AlertReceived(AlertDescription::ProtocolVersion)),
        TlsRefusal::Handshake
    ));
    // A server presenting a pinnable certificate but failing to prove its key
    // fails here, past `verify_server_cert`, and must never reach a prompt.
    assert!(matches!(
        read_back(rustls::Error::InvalidCertificate(CertificateError::BadSignature)),
        TlsRefusal::Certificate
    ));
}

#[test]
fn a_platform_other_that_this_verifier_didnt_wrap_is_not_approvable() {
    let refusal = rustls::Error::InvalidCertificate(CertificateError::Other(OtherError(Arc::new(io::Error::other(
        "something the OS said",
    )))));
    assert!(matches!(read_back(refusal), TlsRefusal::Certificate));
}

#[test]
fn no_tls_error_in_the_chain_is_none() {
    let refused = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused");
    assert!(matches!(tls_refusal(&Wrapping(Box::new(refused))), TlsRefusal::None));
}

#[test]
fn an_approvable_refusal_says_what_the_os_said_for_the_log() {
    let refusal = ApprovableRefusal {
        fingerprint: "AB".to_string(),
        changed: false,
        platform: rustls::Error::InvalidCertificate(CertificateError::UnknownIssuer),
    };
    assert!(refusal.to_string().contains("UnknownIssuer"), "{refusal}");
}
