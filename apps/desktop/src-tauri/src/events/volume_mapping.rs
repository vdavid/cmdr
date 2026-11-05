//! The wire format for what a storage backend reports about its connection.
//!
//! A backend says `VolumeConnection::Disconnected`; this decides that the
//! frontend hears `volume-connection-changed` carrying that transition. Keeping
//! the split here is what lets a backend crate carry no `tauri`, no
//! `tauri_specta`, and no English: the payload struct
//! (`network::VolumeConnectionChanged`), its derives, and the wire enum all live
//! on this side.
//!
//! **The event is backend-neutral on purpose.** SMB is the only emitter today;
//! the next connecting backend (FTP, S3, SFTP) rides this same channel and
//! inherits the reconnect cycle rather than adding a parallel, backend-named
//! event. See `network/DETAILS.md`.

use tauri::AppHandle;
use tauri_specta::Event;

use cmdr_fs::volume::host::events::{SecurityRefusal, VolumeConnection, VolumeEventSink};

use crate::network::{VolumeConnection as WireConnection, VolumeConnectionChanged};

/// Turns a backend's typed connection transitions into the frontend's event.
pub struct TauriVolumeEvents {
    app: AppHandle,
}

impl TauriVolumeEvents {
    /// Wires the sink to the app it emits through.
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

/// What the frontend's reconnect manager branches on. The two enums stay
/// separate on purpose: the backend-facing one ships in `cmdr-fs` with no serde
/// and no `specta`, the wire one in `network` with both. This match is the only
/// place they meet, so widening either end is a compile error here rather than a
/// silently stranded banner or sign-in prompt.
fn wire_state(connection: VolumeConnection) -> WireConnection {
    match connection {
        VolumeConnection::Connected => WireConnection::Connected,
        VolumeConnection::Disconnected => WireConnection::Disconnected,
        VolumeConnection::NeedsCredentials => WireConnection::NeedsCredentials,
        VolumeConnection::NeedsHostKeyApproval => WireConnection::NeedsHostKeyApproval,
        VolumeConnection::GuestSessionRefused => WireConnection::GuestSessionRefused,
        VolumeConnection::EncryptionRequired => WireConnection::EncryptionRequired,
        VolumeConnection::NegotiationInvalid => WireConnection::NegotiationInvalid,
        VolumeConnection::EncryptionUnavailable => WireConnection::EncryptionUnavailable,
    }
}

impl VolumeEventSink for TauriVolumeEvents {
    fn connection_changed(&self, volume_id: &str, connection: VolumeConnection) {
        if let Err(e) = (VolumeConnectionChanged {
            volume_id: volume_id.to_string(),
            state: wire_state(connection),
        })
        .emit(&self.app)
        {
            log::warn!(target: "volume", "connection change for `{volume_id}` never reached the frontend: {e}");
        }
    }

    fn security_refusal(&self, volume_id: &str, refusal: SecurityRefusal) {
        record_security_refusal(volume_id, refusal);
    }
}

/// Where a backend's security refusal goes: the security event log, coalesced per episode.
fn record_security_refusal(volume_id: &str, refusal: SecurityRefusal) {
    // `SecurityRefusal` is `#[non_exhaustive]`: a refusal this build doesn't know yet records nothing.
    match refusal {
        SecurityRefusal::ReplySignatureInvalid => crate::security_log::note_smb_signature_rejection(
            crate::security_log::SmbScope::Volume(volume_id.to_string()),
        ),
        SecurityRefusal::NegotiationInvalid => crate::security_log::note_smb_negotiation_rejection(
            crate::security_log::SmbScope::Volume(volume_id.to_string()),
        ),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mapping is the whole adapter: every backend-facing transition has to
    /// reach the frontend as the matching wire variant, never a neighboring one.
    #[test]
    fn a_refused_signature_reaches_the_security_log_once_per_episode() {
        use crate::security_log::{SecurityEvent, SmbScope, testing};
        let capture = testing::capture_events();
        for _ in 0..3 {
            record_security_refusal("smb-nas-share", SecurityRefusal::ReplySignatureInvalid);
        }
        assert_eq!(
            capture.events(),
            vec![SecurityEvent::SmbSignatureRejectionsStarted {
                scope: SmbScope::Volume("smb-nas-share".into())
            }]
        );
    }

    #[test]
    fn every_transition_maps_to_the_wire_variant_the_frontend_branches_on() {
        assert_eq!(wire_state(VolumeConnection::Connected), WireConnection::Connected);
        assert_eq!(wire_state(VolumeConnection::Disconnected), WireConnection::Disconnected);
        assert_eq!(
            wire_state(VolumeConnection::NeedsCredentials),
            WireConnection::NeedsCredentials
        );
        assert_eq!(
            wire_state(VolumeConnection::NeedsHostKeyApproval),
            WireConnection::NeedsHostKeyApproval
        );
        assert_eq!(
            wire_state(VolumeConnection::GuestSessionRefused),
            WireConnection::GuestSessionRefused
        );
        assert_eq!(
            wire_state(VolumeConnection::EncryptionRequired),
            WireConnection::EncryptionRequired
        );
        assert_eq!(
            wire_state(VolumeConnection::NegotiationInvalid),
            WireConnection::NegotiationInvalid
        );
        assert_eq!(
            wire_state(VolumeConnection::EncryptionUnavailable),
            WireConnection::EncryptionUnavailable
        );
    }

    /// An unconfirmed negotiation is its own episode, apart from refused signatures on the same
    /// volume, so the record says which signal it was (#931).
    #[test]
    fn an_unconfirmed_negotiation_reaches_the_security_log_once_per_episode() {
        use crate::security_log::{SecurityEvent, SmbScope, testing};
        let capture = testing::capture_events();
        for _ in 0..3 {
            record_security_refusal("smb-nas-share", SecurityRefusal::NegotiationInvalid);
        }
        assert_eq!(
            capture.events(),
            vec![SecurityEvent::SmbNegotiationRejectionsStarted {
                scope: SmbScope::Volume("smb-nas-share".into())
            }]
        );
    }
}
