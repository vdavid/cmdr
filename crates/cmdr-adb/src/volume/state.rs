//! Where a volume's connection stands, and the rule that keeps the frontend from
//! flickering.
//!
//! **A backend reports transitions, never states.** A device that is gone fails
//! every operation aimed at it, and reporting each failure as news would flood a
//! frontend that acted on the first one. So every report goes through
//! [`AdbVolumeInner::emit_if_changed`], which swaps an atomic and stays quiet
//! when the value didn't move.
//!
//! ❗ **A retired volume stays silent.** Once a newer instance owns the id, this
//! one's news would describe a volume that no longer exists under that name.

use std::sync::atomic::Ordering;

use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::host::events::VolumeConnection;
use cmdr_fs::volume::{Retirement, Retires};

use super::{AdbVolume, AdbVolumeInner};

/// How this volume's device stands.
///
/// Two values: there is no credential to ask for on this backend (the device
/// authorizes the HOST, once, on its own screen), so nothing rests between
/// "up" and "gone".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    /// The device answers sync requests.
    Connected = 0,
    /// The device is gone or not answering, and something is expected to retry.
    Disconnected = 1,
}

impl ConnectionState {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Connected,
            _ => Self::Disconnected,
        }
    }
}

/// Widens the backend's own state into the enum every connecting backend reports
/// in. ❌ Deliberately not the frontend's wire enum: `events::volume_mapping`
/// owns that translation.
impl From<ConnectionState> for VolumeConnection {
    fn from(state: ConnectionState) -> Self {
        match state {
            ConnectionState::Connected => Self::Connected,
            ConnectionState::Disconnected => Self::Disconnected,
        }
    }
}

impl Retires for AdbVolumeInner {
    fn retirement(&self) -> &Retirement {
        &self.retirement
    }
}

impl AdbVolumeInner {
    /// Where the connection stands right now.
    pub(super) fn connection_state(&self) -> ConnectionState {
        ConnectionState::from_u8(self.state.load(Ordering::Relaxed))
    }

    /// Records that the device is gone ❗ without reporting it, for the paths
    /// where the volume is LEAVING: the frontend learns through
    /// `volumes-changed`, and a `disconnected` alongside it would race that into
    /// a banner for a volume no longer in the sidebar.
    pub(super) fn mark_gone_silently(&self) {
        self.state.store(ConnectionState::Disconnected as u8, Ordering::Relaxed);
    }

    /// Moves to `next` and reports it, ❗ only if that is a change.
    pub(super) fn emit_if_changed(&self, next: ConnectionState) -> bool {
        let previous = self.state.swap(next as u8, Ordering::Relaxed);
        if previous == next as u8 {
            return false;
        }
        if !self.retirement.is_retired() {
            self.host.events().connection_changed(&self.volume_id, next.into());
        }
        true
    }

    /// Re-runs the hello, single-flight, and reports the outcome as a
    /// transition.
    pub(super) async fn do_attempt_reconnect(&self) -> Result<(), VolumeError> {
        let _guard = self.reconnect_lock.lock().await;
        if self.unmounted.load(Ordering::Relaxed) || self.retirement.is_retired() {
            return Err(VolumeError::DeviceDisconnected(self.volume_id.clone()));
        }
        match self.hello().await {
            Ok(()) => {
                self.emit_if_changed(ConnectionState::Connected);
                Ok(())
            }
            Err(e) => {
                self.emit_if_changed(ConnectionState::Disconnected);
                Err(self.map_adb_error(e, "/"))
            }
        }
    }
}

impl AdbVolume {
    /// Reads a failed operation's answer for "the device is gone" and reports
    /// the transition once.
    ///
    /// Operations ARE the detector on this backend: there is no watcher, so a
    /// device that went away is invisible until something asks it for
    /// something.
    pub(super) fn note_lost_session(&self, error: &VolumeError) {
        if matches!(
            error,
            VolumeError::DeviceDisconnected(_) | VolumeError::ConnectionTimeout(_)
        ) {
            self.inner.emit_if_changed(ConnectionState::Disconnected);
        }
    }

    /// The device tracker saw this serial leave the list.
    ///
    /// Published so the app can retire the volume the moment `track-devices`
    /// says so, rather than waiting for the next operation to fail on it.
    pub fn note_device_gone(&self) {
        self.inner.emit_if_changed(ConnectionState::Disconnected);
    }
}
