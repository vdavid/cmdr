//! Why rebuilding a network share's session didn't work, as a value rather than
//! a sentence.
//!
//! ❌ Nothing here is prose a user reads. The reconnect surfaces speak for
//! themselves (the reconnecting view, the gave-up banner, the sign-in form's
//! own copy); this type is what the frontend LOGS and what a future surface
//! would render from. The volume's own refusal rides through
//! [`ReconnectError::Volume`] carrying the whole `VolumeError`, which is
//! already the wire type, so a backend that grows a variant reaches the
//! frontend without a second vocabulary to keep in step.

use cmdr_fs::volume::VolumeError;
use serde::{Deserialize, Serialize};

/// A typed refusal from `reconnect_smb_volume` /
/// `reconnect_smb_volume_with_credentials`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ReconnectError {
    /// The id isn't registered in `VolumeManager` any more (a race: the share
    /// was unmounted between the frontend's tick and the backend acting on it).
    VolumeNotFound {
        /// The id that no longer resolves.
        volume_id: String,
    },
    /// The volume refused, and said why in its own vocabulary. A non-SMB volume
    /// answers `VolumeError::NotSupported` here (the trait default); the
    /// frontend only ever calls this for known SMB volumes.
    Volume {
        /// The backend's typed answer.
        error: VolumeError,
    },
}

impl std::fmt::Display for ReconnectError {
    /// ❗ For logs and debugging only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VolumeNotFound { volume_id } => write!(f, "volume not found: {volume_id}"),
            Self::Volume { error } => write!(f, "volume: {error}"),
        }
    }
}

impl std::error::Error for ReconnectError {}

impl From<VolumeError> for ReconnectError {
    fn from(error: VolumeError) -> Self {
        Self::Volume { error }
    }
}
