//! Where a volume's device stands, in this backend's own states, and the
//! hello that brings it back.
//!
//! The reporting rules (transitions, never states; a retired volume stays
//! silent) belong to `cmdr_fs::volume::ConnectionReporter`, which every
//! connecting backend shares.
//!
//! ❗ **A failing call marks the device gone only for the generation it started
//! on.** There is no session here (one socket per operation), so the generation
//! counts the times a reconnect's hello brought the device BACK
//! (`cmdr_fs::volume::ConnectionCell::revive`): a late error from an operation
//! that started before the device came back can't mark it gone again
//! ([`AdbVolume::note_lost_session`]).

use std::sync::atomic::Ordering;

use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::host::events::VolumeConnection;
use cmdr_fs::volume::{CellState, ReportedState, Retirement, Retires};

use super::{AdbVolume, AdbVolumeInner};

/// How this volume's device stands, kept in a `ConnectionCell` beside the
/// generation.
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

impl CellState for ConnectionState {
    const LIVE: Self = Self::Connected;
    const LOST: Self = Self::Disconnected;

    fn to_byte(self) -> u8 {
        self as u8
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Connected,
            _ => Self::Disconnected,
        }
    }
}

/// Widens the backend's own state into the enum every connecting backend reports
/// in. ❌ Deliberately not the frontend's wire enum: `events::volume_mapping`
/// owns that translation.
impl ReportedState for ConnectionState {
    fn reported(self) -> VolumeConnection {
        match self {
            ConnectionState::Connected => VolumeConnection::Connected,
            ConnectionState::Disconnected => VolumeConnection::Disconnected,
        }
    }
}

impl Retires for AdbVolumeInner {
    fn retirement(&self) -> &Retirement {
        self.connection.retirement()
    }
}

impl AdbVolumeInner {
    /// Re-runs the hello, single-flight, and reports the outcome as a
    /// transition.
    pub(super) async fn do_attempt_reconnect(&self) -> Result<(), VolumeError> {
        let _guard = self.reconnect_lock.lock().await;
        if self.unmounted.load(Ordering::Relaxed) || self.connection.is_retired() {
            return Err(VolumeError::DeviceDisconnected(self.volume_id.clone().into()));
        }
        match self.hello().await {
            Ok(()) => {
                // The next generation only if the device was down: a hello that
                // finds it already up brought nothing back.
                self.connection.revive();
                Ok(())
            }
            Err(e) => {
                self.connection.emit_if_changed(ConnectionState::Disconnected);
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
    /// something. ❗ Only for the generation the operation started on
    /// (`ran_on`, read before it ran): one that started before a reconnect
    /// brought the device back is old news, and marking the device gone on it
    /// would hand every operation since an outage it never had.
    pub(super) fn note_lost_session(&self, error: &VolumeError, ran_on: u64) {
        if matches!(
            error,
            VolumeError::DeviceDisconnected(_) | VolumeError::ConnectionTimeout(_)
        ) {
            self.inner.connection.lose(ran_on);
        }
    }

    /// The device tracker saw this serial leave the list.
    ///
    /// Published so the app can retire the volume the moment `track-devices`
    /// says so, rather than waiting for the next operation to fail on it.
    pub fn note_device_gone(&self) {
        self.inner.connection.emit_if_changed(ConnectionState::Disconnected);
    }
}
