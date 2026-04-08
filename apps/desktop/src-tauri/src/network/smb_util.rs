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
