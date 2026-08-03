//! The wire format for what a storage backend reports about its connection.
//!
//! A backend says `VolumeConnection::Disconnected`; this decides that the
//! frontend hears `smb-connection-changed` with `"disconnected"` in it. Keeping
//! the split here is what lets a backend crate carry no `tauri`, no
//! `tauri_specta`, and no English: the payload struct
//! (`network::SmbConnectionChanged`), its derives, and the three wire values are
//! all on this side.
//!
//! **The payload is named for the only backend that had one.** A second
//! connecting backend wants a generic event, which is a frontend-visible rename
//! (the reconnect manager subscribes by name) rather than something this adapter
//! can paper over. Until then every connection transition rides the SMB event.

use tauri::AppHandle;
use tauri_specta::Event;

use cmdr_fs::volume::host::events::{VolumeConnection, VolumeEventSink};

use crate::network::SmbConnectionChanged;

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

/// What the frontend's reconnect manager matches on. ❌ These three strings are a
/// contract with `src/lib/network/`, not labels: changing one silently strands
/// the banner and the sign-in prompt.
fn wire_state(connection: VolumeConnection) -> &'static str {
    match connection {
        VolumeConnection::Connected => "direct",
        VolumeConnection::Disconnected => "disconnected",
        VolumeConnection::NeedsCredentials => "needs_auth",
    }
}

impl VolumeEventSink for TauriVolumeEvents {
    fn connection_changed(&self, volume_id: &str, connection: VolumeConnection) {
        if let Err(e) = (SmbConnectionChanged {
            volume_id: volume_id.to_string(),
            state: wire_state(connection).to_string(),
        })
        .emit(&self.app)
        {
            log::warn!(target: "volume", "connection change for `{volume_id}` never reached the frontend: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mapping is the whole adapter, and the frontend's three cases depend on
    /// these exact strings.
    #[test]
    fn every_transition_has_the_wire_value_the_frontend_matches_on() {
        assert_eq!(wire_state(VolumeConnection::Connected), "direct");
        assert_eq!(wire_state(VolumeConnection::Disconnected), "disconnected");
        assert_eq!(wire_state(VolumeConnection::NeedsCredentials), "needs_auth");
    }
}
