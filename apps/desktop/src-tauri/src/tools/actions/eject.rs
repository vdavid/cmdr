//! The `eject` tool: detaches a volume by id, whichever kind it is.
//!
//! Thin adapter over the typed `file_system::volume::eject::eject` (smart backend
//! / thin frontend). It dispatches no FE action and invents no ack — it calls the
//! backend directly and returns OK (the `connect_to_server` / `indexing` / `queue`
//! precedent, so there is no FE action to ack). It has parity with the one-click Eject button, and
//! the backend refuses honestly while a write op touches the volume (`Busy`) or
//! when the volume can't be detached, surfaced as errors rather than false OKs.
//!
//! ❗ **Every answer is typed, both ways.** Success carries which teardown ran and
//! a refusal carries WHY as `data.outcome`, because a sentence is the one thing an
//! agent may not branch on (`no-error-string-match` applies to an agent parsing us
//! too). The message beside it is for a human reading a transcript.

use serde_json::{Value, json};

use super::{ToolError, ToolResult};

pub async fn execute_eject(params: &Value) -> ToolResult {
    let volume_id = params
        .get("volumeId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'volumeId' parameter"))?;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use crate::file_system::volume::eject::{EjectError, EjectScope};

        let scope = match params.get("scope").and_then(|v| v.as_str()) {
            None | Some("volume") => EjectScope::Volume,
            Some("disk") => EjectScope::Disk,
            Some(other) => {
                return Err(ToolError::invalid_params(format!(
                    "Unknown scope '{other}': use 'volume' or 'disk'"
                )));
            }
        };
        match crate::file_system::volume::eject::eject_scoped(volume_id, scope).await {
            Ok(outcome) => Ok(json!({
                "outcome": outcome_tag(outcome),
                "volumeId": volume_id,
                "message": success_message(outcome, volume_id),
            })),
            Err(error) => {
                let mut data = json!({ "outcome": refusal_tag(&error), "volumeId": volume_id });
                // ❗ An absent key is "we couldn't tell you", ❌ never an empty answer,
                // which is why a failed conversion omits it rather than writing a null.
                // The holders carry that same distinction in their own tag.
                match &error {
                    EjectError::UnmountRefused { holders, .. } | EjectError::HeldBeforeEject { holders } => {
                        if let Ok(holders) = serde_json::to_value(holders) {
                            data["holders"] = holders;
                        }
                    }
                    EjectError::DeviceDisconnectRefused { provider, .. } => data["provider"] = json!(provider),
                    // Which step stalled decides whether a retry is worth anything.
                    EjectError::NotResponding { step } => {
                        if let Ok(step) = serde_json::to_value(step) {
                            data["step"] = step;
                        }
                    }
                    _ => {}
                }
                Err(ToolError::internal(format!("Couldn't detach {volume_id}: {error}")).with_data(data))
            }
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = volume_id;
        Err(ToolError::internal("Eject isn't supported on this platform"))
    }
}

/// What a teardown did, as the tag an agent branches on.
///
/// Exhaustive on purpose: a new outcome doesn't compile until it answers.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn outcome_tag(outcome: crate::file_system::volume::eject::EjectOutcome) -> &'static str {
    use crate::file_system::volume::eject::EjectOutcome;

    match outcome {
        EjectOutcome::DeviceDisconnected => "deviceDisconnected",
        EjectOutcome::RemoteDisconnected => "remoteDisconnected",
        EjectOutcome::Unmounted => "unmounted",
        EjectOutcome::DiskEjected => "diskEjected",
        EjectOutcome::AlreadyGone => "alreadyGone",
        EjectOutcome::VolumeEjectedDiskStays => "volumeEjectedDiskStays",
    }
}

/// The human half of a successful answer, for someone reading the transcript.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn success_message(outcome: crate::file_system::volume::eject::EjectOutcome, volume_id: &str) -> String {
    use crate::file_system::volume::eject::EjectOutcome;

    match outcome {
        EjectOutcome::DeviceDisconnected => format!("Disconnected the device behind {volume_id}."),
        EjectOutcome::RemoteDisconnected => format!("Closed the connection to {volume_id}."),
        EjectOutcome::Unmounted => format!("Unmounting {volume_id}. It disappears once teardown completes."),
        EjectOutcome::DiskEjected => {
            format!("Ejecting the whole disk under {volume_id}, every volume on it included.")
        }
        EjectOutcome::AlreadyGone => format!("{volume_id} was already gone, so there was nothing to detach."),
        EjectOutcome::VolumeEjectedDiskStays => {
            format!("Unmounted {volume_id}. Its disk stays attached, since other volumes on it are still mounted.")
        }
    }
}

/// Why a detach didn't happen, as the tag an agent branches on.
///
/// The tags match `EjectError`'s own serde names, so this side and the wire type
/// stay one vocabulary. Exhaustive on purpose: a new refusal doesn't compile
/// until it answers.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn refusal_tag(error: &crate::file_system::volume::eject::EjectError) -> &'static str {
    use crate::file_system::volume::eject::EjectError;

    match error {
        EjectError::Busy => "busy",
        EjectError::VolumeNotFound { .. } => "volumeNotFound",
        EjectError::NotEjectable { .. } => "notEjectable",
        EjectError::NotAnSmbVolume { .. } => "notAnSmbVolume",
        EjectError::RemoteNotConnected { .. } => "remoteNotConnected",
        EjectError::DeviceDisconnectRefused { .. } => "deviceDisconnectRefused",
        EjectError::UnmountRefused { .. } => "unmountRefused",
        EjectError::HeldBeforeEject { .. } => "heldBeforeEject",
        EjectError::TimedOut => "timedOut",
        EjectError::NotResponding { .. } => "notResponding",
        EjectError::Unexpected { .. } => "unexpected",
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "eject_test.rs"]
mod eject_test;
