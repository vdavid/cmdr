//! Connection-state enum plus the SmbVolume state-transition and query methods.

use super::SmbVolume;
use super::events::emit_state_change;
use crate::network::VolumeConnection;
use std::sync::atomic::Ordering;

/// Connection health states for an SmbVolume.
///
/// Stored as `AtomicU8` for lock-free reads from any thread. The internal state
/// machine is binary (`Direct ⇄ Disconnected`). The "OS mount" fallback the
/// frontend shows lives at the outer `SmbConnectionState` layer (see
/// `enrich_from_volume_registry` in `volumes/smb.rs`) and never reaches
/// this atomic on the smb2 hot path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    /// smb2 session is active. All ops go through smb2 (fast path).
    Direct = 0,
    /// smb2 session is down. Return errors immediately.
    Disconnected = 2,
}

impl ConnectionState {
    pub(super) fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Direct,
            2 => Self::Disconnected,
            _ => Self::Disconnected,
        }
    }
}

/// Widens the internal state machine into the wire enum the frontend reads.
///
/// One-directional on purpose: `VolumeConnection::NeedsCredentials` has no
/// `ConnectionState` counterpart. It's emitted straight from the reconnect
/// give-up path (`reconnect.rs`), never from a state the volume rests in.
impl From<ConnectionState> for VolumeConnection {
    fn from(state: ConnectionState) -> Self {
        match state {
            ConnectionState::Direct => Self::Connected,
            ConnectionState::Disconnected => Self::Disconnected,
        }
    }
}

impl SmbVolume {
    /// Returns the current connection state.
    pub fn connection_state(&self) -> ConnectionState {
        ConnectionState::from_u8(self.inner.state.load(Ordering::Relaxed))
    }

    /// Whether a newer instance owns this volume's id (see `on_superseded`).
    ///
    /// A retired instance keeps serving the holders that already have it, but
    /// it must stay silent about the id: the state the frontend, the index, and
    /// the reconnect machinery care about is the SUCCESSOR's. Emitting
    /// `volume-connection-changed` from here would tell the app a healthy volume
    /// just disconnected.
    pub(super) fn is_superseded(&self) -> bool {
        self.inner.superseded.load(Ordering::Relaxed)
    }

    /// `emit_state_change` for this volume, suppressed once superseded.
    pub(super) fn emit_state_change_for_id(&self, state: VolumeConnection) {
        if self.is_superseded() {
            return;
        }
        emit_state_change(&self.inner.volume_id, state);
    }

    /// Snapshot the smb2 client's diagnostics tree.
    ///
    /// Returns `None` while the client is disconnected (no `SmbClient`
    /// is held). Otherwise grabs the client mutex briefly, calls
    /// `client.diagnostics()` (cheap atomic loads + short critical
    /// sections inside smb2 — no I/O), and releases the lock before
    /// returning.
    ///
    /// Used by the debug-window SMB diagnostics dashboard. Safe to call
    /// at 1 Hz; cheap even at higher rates.
    pub async fn diagnostics(&self) -> Option<smb2::Diagnostics> {
        let guard = self.inner.client.lock().await;
        guard.as_ref().map(|c| c.diagnostics())
    }

    /// Flips state to `Disconnected` and emits `volume-connection-changed` if the
    /// previous state was something else (silent if we were already Disconnected,
    /// to avoid event spam when several in-flight ops all see the same broken
    /// session).
    pub(super) fn transition_to_disconnected(&self) {
        let prev = self
            .inner
            .state
            .swap(ConnectionState::Disconnected as u8, Ordering::Relaxed);
        if prev != ConnectionState::Disconnected as u8 {
            self.emit_state_change_for_id(ConnectionState::Disconnected.into());
        }
    }

    /// Flips state to `Direct` and emits `volume-connection-changed` if the previous
    /// state was something else. Called by `attempt_reconnect` after a successful
    /// session rebuild.
    pub(super) fn transition_to_direct(&self) {
        let prev = self.inner.state.swap(ConnectionState::Direct as u8, Ordering::Relaxed);
        if prev != ConnectionState::Direct as u8 {
            self.emit_state_change_for_id(ConnectionState::Direct.into());
        }
    }
}
