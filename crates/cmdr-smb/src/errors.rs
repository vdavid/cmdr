//! Turning an [`smb2::Error`] into something a caller can act on.
//!
//! Two questions, and they're not the same one: [`is_auth_error`] answers "would
//! credentials help?", which every retry and fallback path asks; the `classify_*`
//! pair turns the error into the typed [`ShareListError`] a host renders.

use crate::types::ShareListError;
use smb2::ErrorKind;

/// Checks if an error is an authentication error (including signing requirement, and a named
/// login answered with a guest session, which is how a Samba server set to
/// `map to guest = bad user` answers a wrong password).
///
/// It doesn't say WHICH step refused. Access denied at SessionSetup and at
/// TreeConnect both count, and they mean different things: the second is a share
/// turning away an identity the server signed in. A caller that words or routes a
/// refusal reads the `Protocol` error's `command` too.
pub fn is_auth_error(err: &smb2::Error) -> bool {
    matches!(
        err.kind(),
        ErrorKind::AuthRequired | ErrorKind::AccessDenied | ErrorKind::SigningRequired | ErrorKind::GuestSessionRefused
    )
}

/// Classifies an smb2 error into a ShareListError.
pub fn classify_error(err: &smb2::Error) -> ShareListError {
    let message = err.to_string();
    // A refused/unreachable TCP connect means the server is offline, not that its RPC is
    // incompatible: classify as HostUnreachable so callers don't fall back to
    // smbutil/smbclient against the same dead port. Other Io kinds (a transport hiccup
    // mid-session) keep falling through to ProtocolError below.
    if let smb2::Error::Io(io_err) = err
        && matches!(
            io_err.kind(),
            std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::HostUnreachable
                | std::io::ErrorKind::NetworkUnreachable
        )
    {
        return ShareListError::HostUnreachable { message };
    }
    // By variant: `EncryptionRequired` shares its `Unsupported` kind and means something else
    // (Cmdr required encryption the server can't give), but Cmdr never sets
    // `ClientConfig::require_encryption`, so it can't arrive yet.
    if let smb2::Error::EncryptionUnavailable { .. } = err {
        return ShareListError::EncryptionUnavailable { message };
    }
    match err.kind() {
        ErrorKind::AuthRequired => ShareListError::AuthRequired { message },
        ErrorKind::AccessDenied => ShareListError::AuthFailed { message },
        ErrorKind::SigningRequired => ShareListError::SigningRequired { message },
        // Only a named login gets here, so the credentials are what's wrong.
        ErrorKind::GuestSessionRefused => ShareListError::AuthFailed { message },
        ErrorKind::SignatureInvalid => ShareListError::SignatureInvalid { message },
        ErrorKind::NegotiateValidationFailed => ShareListError::NegotiationInvalid { message },
        ErrorKind::NotFound => ShareListError::ProtocolError { message },
        ErrorKind::ConnectionLost => ShareListError::HostUnreachable { message },
        ErrorKind::TimedOut => ShareListError::Timeout { message },
        _ => ShareListError::ProtocolError { message },
    }
}

/// Classifies an error from an *authenticated* listing attempt.
///
/// The caller supplied explicit credentials, so an auth-class rejection
/// (`AuthRequired` / `AccessDenied`, for example `STATUS_LOGON_FAILURE`) means the
/// credentials are wrong (`AuthFailed`), not that authentication is required.
/// `SigningRequired` is deliberately NOT remapped: it's a protocol capability mismatch,
/// not a credential problem. Everything else falls through to [`classify_error`].
///
/// Only the macOS host calls this today (Linux's authenticated fallback goes through
/// `smbclient`); the classifier itself is platform-agnostic.
pub fn classify_authenticated_error(err: &smb2::Error) -> ShareListError {
    match err.kind() {
        ErrorKind::AuthRequired | ErrorKind::AccessDenied | ErrorKind::GuestSessionRefused => {
            ShareListError::AuthFailed {
                message: "Invalid username or password".to_string(),
            }
        }
        _ => classify_error(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guest_offered() -> smb2::Error {
        smb2::Error::GuestSessionRefused {
            username: "david".to_string(),
            anonymous: false,
        }
    }

    fn signature_refused() -> smb2::Error {
        smb2::Error::SignatureInvalid {
            command: smb2::types::Command::SessionSetup,
            message_id: smb2::types::MessageId(3),
            reason: smb2::error::SignatureFailure::Missing,
        }
    }

    /// A guest session offered for a named login is Samba's `map to guest = bad user` answer
    /// to a wrong password, so it takes the credentials path like any other wrong password.
    #[test]
    fn a_refused_guest_session_reads_as_wrong_credentials() {
        assert!(is_auth_error(&guest_offered()));
        assert!(matches!(
            classify_error(&guest_offered()),
            ShareListError::AuthFailed { .. }
        ));
        assert!(matches!(
            classify_authenticated_error(&guest_offered()),
            ShareListError::AuthFailed { .. }
        ));
    }

    /// A refused signature is its own typed answer: not a credentials problem, and not the
    /// protocol error that sends a listing down the CLI fallback.
    #[test]
    fn a_refused_signature_is_typed_and_not_an_auth_error() {
        assert!(!is_auth_error(&signature_refused()));
        assert!(matches!(
            classify_error(&signature_refused()),
            ShareListError::SignatureInvalid { .. }
        ));
        assert!(matches!(
            classify_authenticated_error(&signature_refused()),
            ShareListError::SignatureInvalid { .. }
        ));
    }

    /// A negotiation the server didn't confirm (smb2 0.41) is a possible sign of tampering, like a
    /// refused signature: typed, not auth, and never the `ProtocolError` that sends a listing down
    /// the unverified CLI fallback.
    #[test]
    fn an_unconfirmed_negotiation_is_typed_and_not_an_auth_error() {
        let err = smb2::Error::NegotiateValidationFailed {
            reason: "the server answered a different dialect".to_string(),
        };
        assert!(!is_auth_error(&err));
        assert!(matches!(
            classify_error(&err),
            ShareListError::NegotiationInvalid { .. }
        ));
        assert!(matches!(
            classify_authenticated_error(&err),
            ShareListError::NegotiationInvalid { .. }
        ));
    }

    /// A server that only accepts encrypted connections, on a connection that agreed on no cipher.
    /// Its own answer, ❌ never `ProtocolError`: the CLI fallback lists unencrypted.
    #[test]
    fn a_server_requiring_encryption_it_did_not_negotiate_is_typed() {
        let err = smb2::Error::EncryptionUnavailable { share: None };
        assert!(!is_auth_error(&err));
        assert!(matches!(
            classify_error(&err),
            ShareListError::EncryptionUnavailable { .. }
        ));
    }

    #[test]
    fn test_is_auth_error() {
        assert!(is_auth_error(&smb2::Error::Auth {
            message: "Logon failure".to_string(),
        }));
        assert!(!is_auth_error(&smb2::Error::Timeout));
        assert!(!is_auth_error(&smb2::Error::Disconnected));
    }

    /// macOS smbd rejects a guest/anonymous SessionSetup with
    /// `STATUS_ACCOUNT_RESTRICTION` (0xC000006E). smb2 0.13.1+ classifies the
    /// whole logon-rejection family as `ErrorKind::AuthRequired`; on 0.12 this
    /// status fell through to `Other`, so the guest attempt against a
    /// credentials-requiring server (like the user's own Mac) surfaced as
    /// `ProtocolError` — a misleading "falling back to smbutil/smbclient" WARN
    /// plus a pointless CLI-fallback subprocess instead of the credentials path.
    #[test]
    fn test_guest_rejection_status_is_auth_error() {
        let err = smb2::Error::Protocol {
            status: smb2::types::status::NtStatus::ACCOUNT_RESTRICTION,
            command: smb2::types::Command::SessionSetup,
        };
        assert!(is_auth_error(&err));
        match classify_error(&err) {
            ShareListError::AuthRequired { .. } => {}
            e => panic!("Expected AuthRequired for STATUS_ACCOUNT_RESTRICTION, got {:?}", e),
        }
    }

    /// When the caller supplied explicit credentials, an auth-class rejection means the
    /// credentials are WRONG, not that authentication is required. Without the
    /// context-aware classifier, a wrong password surfaced as "Authentication required.
    /// Please enter your credentials." in the login form (observed in QA against the
    /// Naspolya NAS) instead of "Invalid username or password."
    #[test]
    fn test_classify_authenticated_error_maps_rejection_to_auth_failed() {
        match classify_authenticated_error(&smb2::Error::Auth {
            message: "STATUS_LOGON_FAILURE during SessionSetup".to_string(),
        }) {
            ShareListError::AuthFailed { .. } => {}
            e => panic!("Expected AuthFailed for a rejected authenticated session, got {:?}", e),
        }

        // Non-auth errors keep their regular classification.
        match classify_authenticated_error(&smb2::Error::Timeout) {
            ShareListError::Timeout { .. } => {}
            e => panic!("Expected Timeout, got {:?}", e),
        }
    }

    /// A refused/unreachable TCP connect means the server is offline, not that its RPC is
    /// incompatible. Pre-fix this classified as ProtocolError, which (a) triggered a
    /// smbutil/smbclient fallback against the same dead port and (b) logged a warn for the
    /// routine case of a known server being offline.
    #[test]
    fn test_classify_error_connection_refused_is_host_unreachable() {
        for io_kind in [
            std::io::ErrorKind::ConnectionRefused,
            std::io::ErrorKind::HostUnreachable,
            std::io::ErrorKind::NetworkUnreachable,
        ] {
            match classify_error(&smb2::Error::Io(std::io::Error::from(io_kind))) {
                ShareListError::HostUnreachable { .. } => {}
                e => panic!("Expected HostUnreachable for {io_kind:?}, got {:?}", e),
            }
        }

        // Other Io errors stay ProtocolError (transport hiccup mid-session, not an offline host).
        match classify_error(&smb2::Error::Io(std::io::Error::from(std::io::ErrorKind::BrokenPipe))) {
            ShareListError::ProtocolError { .. } => {}
            e => panic!("Expected ProtocolError for BrokenPipe, got {:?}", e),
        }
    }

    #[test]
    fn test_classify_error_timeout() {
        match classify_error(&smb2::Error::Timeout) {
            ShareListError::Timeout { .. } => {}
            e => panic!("Expected Timeout, got {:?}", e),
        }
    }

    #[test]
    fn test_classify_error_disconnected() {
        match classify_error(&smb2::Error::Disconnected) {
            ShareListError::HostUnreachable { .. } => {}
            e => panic!("Expected HostUnreachable, got {:?}", e),
        }
    }

    #[test]
    fn test_classify_error_auth() {
        match classify_error(&smb2::Error::Auth {
            message: "bad password".to_string(),
        }) {
            ShareListError::AuthRequired { .. } => {}
            e => panic!("Expected AuthRequired, got {:?}", e),
        }
    }
}
