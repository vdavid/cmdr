//! A refused handshake's verdict, read off the `rustls::Error` under it (#1111): a certificate
//! that's only untrusted may be added to the trust store, one that's revoked or broken must not.

use std::io;

use rustls::CertificateError;

use super::tls_refusal_verdict;
use crate::refusal::S3ConnectError;

/// The shape reqwest hands back: the `rustls::Error` inside an `InvalidData` `io::Error`, inside
/// another of kind `Other` (`cmdr_fs::volume::tls::causes`).
fn handshake_refused_with(refusal: rustls::Error) -> io::Error {
    io::Error::other(io::Error::new(io::ErrorKind::InvalidData, refusal))
}

#[test]
fn a_self_signed_or_unknown_issuer_certificate_is_untrusted() {
    for reason in [
        CertificateError::UnknownIssuer,
        CertificateError::Expired,
        CertificateError::NotValidForName,
    ] {
        let err = handshake_refused_with(rustls::Error::InvalidCertificate(reason.clone()));
        assert!(
            matches!(tls_refusal_verdict(&err), S3ConnectError::CertificateUntrusted),
            "{reason:?}"
        );
    }
}

#[test]
fn a_revoked_malformed_or_badly_signed_certificate_is_rejected() {
    for reason in [
        CertificateError::Revoked,
        CertificateError::BadEncoding,
        CertificateError::BadSignature,
    ] {
        let err = handshake_refused_with(rustls::Error::InvalidCertificate(reason.clone()));
        assert!(
            matches!(tls_refusal_verdict(&err), S3ConnectError::CertificateRejected),
            "{reason:?}"
        );
    }
}

#[test]
fn a_refusal_with_no_rustls_error_under_it_stays_untrusted() {
    let err = io::Error::other(io::Error::new(io::ErrorKind::InvalidData, "invalid peer certificate"));
    assert!(matches!(
        tls_refusal_verdict(&err),
        S3ConnectError::CertificateUntrusted
    ));
}
