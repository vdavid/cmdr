//! Where a volume's connection stands, and the rule that keeps the frontend
//! from flickering.
//!
//! **A backend reports transitions, never states.** A server that is down
//! fails every request aimed at it, and reporting each failure would flood a
//! frontend that acted on the first one. So every report goes through
//! [`WebdavVolumeInner::emit_if_changed`], which swaps an atomic and stays
//! quiet when the value didn't move. ❗ A retired volume stays silent: its id
//! belongs to a newer instance now.

use std::sync::atomic::Ordering;

use cmdr_fs::volume::host::events::VolumeConnection;
use cmdr_fs::volume::{Retirement, Retires, SelfHandle};

use super::WebdavVolumeInner;

/// How this volume's connection stands. HTTP has no session, so "connected"
/// means the last request that reached the wire came back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    /// Requests are being answered.
    Connected = 0,
    /// The last request found the server gone; something is expected to retry.
    Disconnected = 1,
    /// The server refused the stored secret. Only the user moves this forward:
    /// retrying costs an authentication attempt and buys nothing.
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

impl From<ConnectionState> for VolumeConnection {
    fn from(state: ConnectionState) -> Self {
        match state {
            ConnectionState::Connected => Self::Connected,
            ConnectionState::Disconnected => Self::Disconnected,
            ConnectionState::NeedsCredentials => Self::NeedsCredentials,
        }
    }
}

impl Retires for WebdavVolumeInner {
    fn retirement(&self) -> &Retirement {
        &self.retirement
    }
}

impl WebdavVolumeInner {
    /// This volume's own handle, for the background work that has to keep
    /// asking whether the registry still serves it.
    pub(super) fn self_handle(&self) -> SelfHandle<WebdavVolumeInner> {
        SelfHandle::new(self.me.clone())
    }

    /// Whether this volume has stopped owning its id.
    pub(super) fn is_retired(&self) -> bool {
        self.retirement.is_retired()
    }

    /// Where the connection stands right now.
    pub(super) fn connection_state(&self) -> ConnectionState {
        ConnectionState::from_u8(self.state.load(Ordering::Relaxed))
    }

    /// Records that the connection is gone ❗ without reporting it, for the
    /// paths where the volume is LEAVING: the frontend learns through
    /// `volumes-changed`, and a `disconnected` alongside it would race that
    /// into a banner for a volume no longer in the sidebar.
    pub(super) fn mark_gone_silently(&self) {
        self.state.store(ConnectionState::Disconnected as u8, Ordering::Relaxed);
    }

    /// Moves to `next` and reports it, ❗ only if that is a change. Returns
    /// whether anything moved, which lets the caller act once on a transition
    /// several tasks noticed at the same moment.
    pub(super) fn emit_if_changed(&self, next: ConnectionState) -> bool {
        let previous = self.state.swap(next as u8, Ordering::Relaxed);
        if previous == next as u8 {
            return false;
        }
        if !self.is_retired() {
            self.host.events().connection_changed(&self.volume_id, next.into());
        }
        true
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
