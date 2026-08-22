//! Where a volume's connection stands, and the rule that keeps the frontend from
//! flickering.
//!
//! **A backend reports transitions, never states.** A server that is down fails
//! every operation aimed at it, and reporting each failure as news would flood a
//! frontend that acted on the first one. So every report goes through
//! [`SftpVolumeInner::emit_if_changed`], which swaps an atomic and stays quiet
//! when the value didn't move.
//!
//! ❗ **A retired volume stays silent.** Once a newer instance owns the id, this
//! one's news would describe a volume that no longer exists under that name, and
//! the frontend would show a healthy server as dropped.

use std::sync::atomic::Ordering;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::host::events::VolumeConnection;
use cmdr_fs::volume::{Retirement, Retires, SelfHandle};

use super::SftpVolumeInner;
use crate::auth::AuthRungUsed;

/// How this volume's session stands.
///
/// Stored as an `AtomicU8` for lock-free reads from any thread. Three values
/// rather than SMB's two, because [`NeedsCredentials`](Self::NeedsCredentials) is
/// a state this backend RESTS in: a passphrase-protected key can't come back on
/// its own, so it waits there until a human signs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    /// The session is up and serving.
    Connected = 0,
    /// The session is gone and something is expected to retry.
    Disconnected = 1,
    /// Only the user can move this forward: retrying costs an authentication
    /// attempt and buys nothing.
    NeedsCredentials = 2,
}

impl ConnectionState {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Connected,
            2 => Self::NeedsCredentials,
            _ => Self::Disconnected,
        }
    }
}

/// Widens the backend's own state into the enum every connecting backend reports
/// in.
///
/// ❌ Deliberately not the frontend's wire enum: `events::volume_mapping` owns
/// that translation for every backend, and producing it here would weld this
/// crate to the app.
impl From<ConnectionState> for VolumeConnection {
    fn from(state: ConnectionState) -> Self {
        match state {
            ConnectionState::Connected => Self::Connected,
            ConnectionState::Disconnected => Self::Disconnected,
            ConnectionState::NeedsCredentials => Self::NeedsCredentials,
        }
    }
}

/// The connection-scoped half carries the flag, which is what the reconnect loop
/// reaches back through.
impl Retires for SftpVolumeInner {
    fn retirement(&self) -> &Retirement {
        &self.retirement
    }
}

impl SftpVolumeInner {
    /// This volume's own handle, for the background work that has to keep asking
    /// whether the registry still serves it.
    pub(super) fn self_handle(&self) -> SelfHandle<SftpVolumeInner> {
        SelfHandle::new(self.me.clone())
    }

    /// Whether this volume has stopped owning its id: superseded by a newer
    /// instance, or removed from the registry outright.
    pub(super) fn is_retired(&self) -> bool {
        self.retirement.is_retired()
    }

    /// Which credential built the live session.
    pub(super) fn auth_rung(&self) -> AuthRungUsed {
        *self.rung.lock_ignore_poison()
    }

    /// Records the rung a fresh session came up on.
    pub(super) fn set_auth_rung(&self, rung: AuthRungUsed) {
        *self.rung.lock_ignore_poison() = rung;
    }

    /// Where the connection stands right now.
    pub(super) fn connection_state(&self) -> ConnectionState {
        ConnectionState::from_u8(self.state.load(Ordering::Relaxed))
    }

    /// Moves to `next` and reports it, ❗ only if that is a change.
    ///
    /// Returns whether anything moved, which is what lets the caller act once on
    /// a transition several threads noticed at the same moment.
    pub(super) fn emit_if_changed(&self, next: ConnectionState) -> bool {
        let previous = self.state.swap(next as u8, Ordering::Relaxed);
        if previous == next as u8 {
            return false;
        }
        // A retired volume still tracks its own state for whoever holds it, but
        // the id belongs to somebody else now.
        if !self.is_retired() {
            self.host.events().connection_changed(&self.volume_id, next.into());
        }
        true
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
