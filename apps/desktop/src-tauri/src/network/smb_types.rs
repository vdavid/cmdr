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
///
/// Uses internally tagged representation so each variant can carry different fields
/// while keeping a flat JSON shape (`{ "type": "...", "message": "..." }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ShareListError {
    HostUnreachable {
        message: String,
    },
    Timeout {
        message: String,
    },
    AuthRequired {
        message: String,
    },
    /// Guest access won't work.
    SigningRequired {
        message: String,
    },
    AuthFailed {
        message: String,
    },
    ProtocolError {
        message: String,
    },
    ResolutionFailed {
        message: String,
    },
    /// A required CLI tool is not installed.
    MissingDependency {
        message: String,
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
