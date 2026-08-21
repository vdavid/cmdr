//! The share-listing vocabulary: what a server said it offers, and how the
//! attempt went.
//!
//! Every type here crosses IPC, so it carries serde and `specta::Type`. The
//! `#[serde(tag = "type")]` on [`ShareListError`] is load-bearing: each variant
//! carries different fields while the JSON stays flat.

use serde::{Deserialize, Serialize};

/// Information about a discovered share.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareInfo {
    /// The share name as the server spells it (`archive`, never `//nas/archive`).
    pub name: String,
    /// False for printer/IPC shares.
    pub is_disk: bool,
    /// The server's own description of the share, when it set one.
    pub comment: Option<String>,
}

/// Authentication mode detected for a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// The server let an anonymous session list its shares.
    GuestAllowed,
    /// The server turned the anonymous session away; a sign-in is needed.
    CredsRequired,
    /// Not yet checked or check failed.
    Unknown,
}

/// Result of a share listing operation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareListResult {
    /// Already filtered to disk shares only.
    pub shares: Vec<ShareInfo>,
    /// What the listing attempt learned about the server's auth stance.
    pub auth_mode: AuthMode,
    /// True when this answer came from the in-memory cache rather than the wire.
    pub from_cache: bool,
}

/// Error types for share listing operations.
///
/// Uses internally tagged representation so each variant can carry different fields
/// while keeping a flat JSON shape (`{ "type": "...", "message": "..." }`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ShareListError {
    /// The server is offline, or nothing is listening on the SMB port.
    HostUnreachable {
        /// Diagnostic detail, for logs.
        message: String,
    },
    /// The server accepted the connection and then went quiet.
    Timeout {
        /// Diagnostic detail, for logs.
        message: String,
    },
    /// The server turned an anonymous session away; credentials are needed.
    AuthRequired {
        /// Diagnostic detail, for logs.
        message: String,
    },
    /// Guest access won't work.
    SigningRequired {
        /// Diagnostic detail, for logs.
        message: String,
    },
    /// The credentials the caller supplied were rejected.
    AuthFailed {
        /// Diagnostic detail, for logs.
        message: String,
    },
    /// The exchange broke down in a way none of the other variants describes.
    ProtocolError {
        /// Diagnostic detail, for logs.
        message: String,
    },
    /// The hostname never resolved to an address.
    ResolutionFailed {
        /// Diagnostic detail, for logs.
        message: String,
    },
    /// A required CLI tool is not installed.
    MissingDependency {
        /// Diagnostic detail, for logs.
        message: String,
        /// What the user would run to install it, when there's a one-liner.
        #[serde(rename = "installCommand")]
        install_command: Option<String>,
    },
}

impl std::fmt::Display for ShareListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostUnreachable { message } => write!(f, "Host unreachable: {}", message),
            Self::Timeout { message } => write!(f, "Timeout: {}", message),
            Self::AuthRequired { message } => write!(f, "Authentication required: {}", message),
            Self::SigningRequired { message } => write!(f, "SMB signing required: {}", message),
            Self::AuthFailed { message } => write!(f, "Authentication failed: {}", message),
            Self::ProtocolError { message } => write!(f, "Protocol error: {}", message),
            Self::ResolutionFailed { message } => write!(f, "Resolution failed: {}", message),
            Self::MissingDependency { message, .. } => write!(f, "Missing dependency: {}", message),
        }
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
