//! SMB types and error definitions.
//!
//! Contains the core data structures for SMB share listing operations.

use serde::{Deserialize, Serialize};

/// Information about a discovered share.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareInfo {
    /// Name of the share (for example, "Documents", "Media").
    pub name: String,
    /// Whether this is a disk share (true) or other type like printer/IPC.
    pub is_disk: bool,
    /// Optional description/comment for the share.
    pub comment: Option<String>,
}

/// Authentication mode detected for a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// Guest access works for this host.
    GuestAllowed,
    /// Authentication is required (guest access failed).
    CredsRequired,
    /// Haven't checked yet or check failed.
    Unknown,
}

/// Result of a share listing operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareListResult {
    /// Shares found on the host (already filtered to disk shares only).
    pub shares: Vec<ShareInfo>,
    /// Authentication mode detected.
    pub auth_mode: AuthMode,
    /// Whether this result came from cache.
    pub from_cache: bool,
}

/// Error types for share listing operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
pub enum ShareListError {
    /// Host is not reachable.
    HostUnreachable(String),
    /// Connection timed out.
    Timeout(String),
    /// Authentication required but no credentials provided.
    AuthRequired(String),
    /// Server requires SMB signing - guest access won't work.
    SigningRequired(String),
    /// Authentication failed with provided credentials.
    AuthFailed(String),
    /// Other SMB protocol error.
    ProtocolError(String),
    /// DNS/hostname resolution failed.
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
