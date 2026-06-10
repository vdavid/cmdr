//! SMB utility functions.
//!
//! Contains error classification and share type conversion utilities.

use crate::network::smb_types::{ShareInfo, ShareListError};
use smb2::ErrorKind;

/// Checks if an error is an authentication error (including signing requirement).
pub fn is_auth_error(err: &smb2::Error) -> bool {
    matches!(
        err.kind(),
        ErrorKind::AuthRequired | ErrorKind::AccessDenied | ErrorKind::SigningRequired
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
    match err.kind() {
        ErrorKind::AuthRequired => ShareListError::AuthRequired { message },
        ErrorKind::AccessDenied => ShareListError::AuthFailed { message },
        ErrorKind::SigningRequired => ShareListError::SigningRequired { message },
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
// Consumed by the macOS arm of `smb_client` today (Linux's authenticated fallback goes
// through smbclient); the classifier itself is platform-agnostic.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "consumed by the macOS arm of smb_client; Linux goes through smbclient"
    )
)]
pub fn classify_authenticated_error(err: &smb2::Error) -> ShareListError {
    match err.kind() {
        ErrorKind::AuthRequired | ErrorKind::AccessDenied => ShareListError::AuthFailed {
            message: "Invalid username or password".to_string(),
        },
        _ => classify_error(err),
    }
}

/// Converts smb2 share info to Cmdr's ShareInfo type.
/// smb2's `list_shares()` already filters to disk shares and strips `$` shares.
pub fn convert_shares(shares: Vec<smb2::ShareInfo>) -> Vec<ShareInfo> {
    shares
        .into_iter()
        .map(|share| ShareInfo {
            name: share.name,
            is_disk: true,
            comment: if share.comment.is_empty() {
                None
            } else {
                Some(share.comment)
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_auth_error() {
        assert!(is_auth_error(&smb2::Error::Auth {
            message: "Logon failure".to_string(),
        }));
        assert!(!is_auth_error(&smb2::Error::Timeout));
        assert!(!is_auth_error(&smb2::Error::Disconnected));
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

    #[test]
    fn test_convert_shares() {
        let smb2_shares = vec![
            smb2::ShareInfo {
                name: "Documents".to_string(),
                share_type: 0,
                comment: "My documents".to_string(),
            },
            smb2::ShareInfo {
                name: "Public".to_string(),
                share_type: 0,
                comment: String::new(),
            },
        ];

        let result = convert_shares(smb2_shares);
        assert_eq!(result.len(), 2);

        assert_eq!(result[0].name, "Documents");
        assert!(result[0].is_disk);
        assert_eq!(result[0].comment.as_deref(), Some("My documents"));

        assert_eq!(result[1].name, "Public");
        assert!(result[1].is_disk);
        assert!(result[1].comment.is_none());
    }
}
