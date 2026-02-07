//! SMB types and error definitions.
//!
//! Contains the core data structures for SMB share listing operations.

use serde::{Deserialize, Serialize};

/// Information about a discovered share.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareInfo {
    pub name: String,
    /// False for printer/IPC shares.
    pub is_disk: bool,
    pub comment: Option<String>,
}

/// Authentication mode detected for a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    GuestAllowed,
    CredsRequired,
    /// Not yet checked or check failed.
    Unknown,
}

/// Result of a share listing operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareListResult {
    /// Already filtered to disk shares only.
    pub shares: Vec<ShareInfo>,
    pub auth_mode: AuthMode,
    pub from_cache: bool,
}

/// Error types for share listing operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
pub enum ShareListError {
    HostUnreachable(String),
    Timeout(String),
    AuthRequired(String),
    /// Guest access won't work.
    SigningRequired(String),
    AuthFailed(String),
    ProtocolError(String),
    ResolutionFailed(String),
}

impl std::fmt::Display for ShareListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostUnreachable(msg) => write!(f, "Host unreachable: {}", msg),
            Self::Timeout(msg) => write!(f, "Timeout: {}", msg),
            Self::AuthRequired(msg) => write!(f, "Authentication required: {}", msg),
            Self::SigningRequired(msg) => write!(f, "SMB signing required: {}", msg),
            Self::AuthFailed(msg) => write!(f, "Authentication failed: {}", msg),
            Self::ProtocolError(msg) => write!(f, "Protocol error: {}", msg),
            Self::ResolutionFailed(msg) => write!(f, "Resolution failed: {}", msg),
        }
    }
}
