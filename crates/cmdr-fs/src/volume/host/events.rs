//! Typed events a backend reports, for the host to render.
//!
//! A backend says WHAT happened; the host decides what the user sees and in
//! which words. So the payload types that cross to the frontend, and their
//! `tauri_specta::Event` derives, stay app-side: the seam speaks in the enum
//! below and the app's adapter turns it into whatever the frontend subscribes to.
//!
//! That split is what keeps a backend crate free of `tauri` and free of English.
//! ❌ Never put a user-facing sentence in an event a backend emits.

/// How a volume's connection to its server stands right now.
///
/// A typed value rather than a string on purpose: the host maps each variant to
/// the wire value the frontend expects, so renaming what the UI subscribes to
/// can never silently change what a backend means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeConnection {
    /// The backend's own session is up and serving.
    Connected,
    /// The session is down. Operations fail immediately rather than hanging,
    /// and whoever owns recovery (the reconnect cycle, the user) takes over.
    Disconnected,
    /// The server rejected our credentials. Retrying with the same ones won't
    /// help and risks locking the account: only the user can move this forward.
    NeedsCredentials,
}

/// Where a backend's typed events go.
///
/// Cmdr answers this from the app's event layer, where the Tauri payload types
/// live; a test or a tool answers nothing ([`NoVolumeEvents`]).
pub trait VolumeEventSink: Send + Sync {
    /// A volume's connection state changed.
    ///
    /// Report only real transitions. Firing on every failed operation while a
    /// server is down floods the frontend with an event it already acted on, so
    /// a backend compares against its previous state first.
    ///
    /// A superseded instance (one whose volume id a newer instance now owns)
    /// must stay silent: its news would describe a volume that no longer exists
    /// under that id, and the frontend would show a healthy share as dropped.
    fn connection_changed(&self, volume_id: &str, connection: VolumeConnection);
}

/// Events go nowhere. There's no frontend in a bench, a tool, or most tests.
pub(super) struct NoVolumeEvents;

impl VolumeEventSink for NoVolumeEvents {
    fn connection_changed(&self, _volume_id: &str, _connection: VolumeConnection) {}
}

#[cfg(any(test, feature = "testing"))]
pub use recording::RecordingVolumeEvents;

#[cfg(any(test, feature = "testing"))]
mod recording {
    use std::sync::Mutex;

    use super::{VolumeConnection, VolumeEventSink};
    use crate::ignore_poison::IgnorePoison;

    /// A [`VolumeEventSink`] that remembers every
    /// transition, so a reconnect test can assert on the sequence a user would
    /// have seen.
    #[derive(Default)]
    pub struct RecordingVolumeEvents {
        transitions: Mutex<Vec<(String, VolumeConnection)>>,
    }

    impl RecordingVolumeEvents {
        /// A recorder with nothing seen yet.
        pub fn new() -> Self {
            Self::default()
        }

        /// Every transition reported so far, in order.
        pub fn transitions(&self) -> Vec<(String, VolumeConnection)> {
            self.transitions.lock_ignore_poison().clone()
        }
    }

    impl VolumeEventSink for RecordingVolumeEvents {
        fn connection_changed(&self, volume_id: &str, connection: VolumeConnection) {
            self.transitions
                .lock_ignore_poison()
                .push((volume_id.to_string(), connection));
        }
    }
}
