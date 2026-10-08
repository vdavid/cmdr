//! The one AI refusal type: every surface that says "your organization said no" to an AI request
//! carries a `ManagedAiRefusal`, so the frontend has one copy map and a new surface can't spell
//! the reason differently.

use serde::{Deserialize, Serialize};

/// Why the policy refused an AI request. Produced only by [`super::ManagedPolicy::ai_destination`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ManagedAiRefusal {
    /// `DisableAI`: no AI at all, local included.
    AiOff,
    /// `DisableCloudAI`, or an empty `AllowedCloudAIHosts`: on-device only.
    CloudAiOff,
    /// The request's host isn't in `AllowedCloudAIHosts`.
    HostNotAllowed,
}

/// Where an AI request goes, which is all the policy needs to judge it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiDestination {
    /// Cmdr's own `llama-server` on this Mac.
    LocalServer,
    /// Any other endpoint, loopback included: `localhost` can be a tunnel to anywhere, so only the
    /// Cmdr-managed server counts as on-device.
    Remote(url::Url),
}
