//! Connection-state enum plus the share's state-transition and query methods.
//!
//! They live on [`SmbVolumeInner`], the share-scoped half: connection health,
//! retirement, and the events they emit belong to the SHARE, not to one of the
//! mount roots it is reachable through. [`SmbVolume`] passes the two public ones
//! through.

use super::{SmbVolume, SmbVolumeInner};
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::events::VolumeConnection;
use cmdr_fs::volume::{Retirement, Retires, SelfHandle};
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

/// Widens the internal state machine into the backend-facing enum every
/// connecting backend reports in (`cmdr_fs::volume::host::events`).
///
/// Deliberately NOT the frontend's wire enum: `events::volume_mapping` owns that
/// translation for every backend, and converting straight to it here would weld
/// the SMB backend to the app's `network` module in a cycle (a `From` impl is
/// attributed to the module defining the type it produces).
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

/// The share carries the flag, so a promotion (which builds another instance over
/// the SAME inner) can't retire a watcher that is still the live one.
impl Retires for SmbVolumeInner {
    fn retirement(&self) -> &Retirement {
        &self.retirement
    }
}

impl SmbVolumeInner {
    /// Everything this share asks the app around it. Shared with the background
    /// work that outlives a single call: the watcher reports listing changes and
    /// watch gaps through the same host the share was built with.
    pub(super) fn host(&self) -> &VolumeHost {
        &self.host
    }

    /// This share's own handle, for the background work (the watcher, the
    /// watcher-death reconnect loop) that has to keep asking whether the registry
    /// still serves it. See `cmdr_fs::volume::SelfHandle`.
    pub(super) fn self_handle(&self) -> SelfHandle<SmbVolumeInner> {
        SelfHandle::new(self.me.clone())
    }

    /// Returns the current connection state.
    pub(super) fn connection_state(&self) -> ConnectionState {
        ConnectionState::from_u8(self.state.load(Ordering::Relaxed))
    }

    /// Whether this share has stopped owning its volume id: superseded by a newer
    /// instance, or removed from the registry outright.
    ///
    /// A retired share keeps serving the holders that already have it, but it
    /// must stay silent about the id: the state the frontend, the index, and the
    /// reconnect machinery care about belongs to somebody else. Emitting
    /// `volume-connection-changed` from here would tell the app a healthy volume
    /// just disconnected.
    pub(super) fn is_retired(&self) -> bool {
        self.retirement.is_retired()
    }

    /// Reports a session-state transition for this volume, suppressed once
    /// retired.
    ///
    /// A retired share still tracks its own state for whoever holds it, but the
    /// id belongs to somebody else now: announcing a disconnect under it would
    /// tell the frontend a healthy volume just went down.
    pub(super) fn emit_state_change_for_id(&self, state: VolumeConnection) {
        if self.is_retired() {
            return;
        }
        self.host.events().connection_changed(&self.volume_id, state);
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
    pub(super) async fn diagnostics(&self) -> Option<smb2::Diagnostics> {
        let guard = self.client.lock().await;
        guard.as_ref().map(|c| c.diagnostics())
    }

    /// Flips state to `Disconnected` and emits `volume-connection-changed` if the
    /// previous state was something else (silent if we were already Disconnected,
    /// to avoid event spam when several in-flight ops all see the same broken
    /// session).
    pub(super) fn transition_to_disconnected(&self) {
        let prev = self.state.swap(ConnectionState::Disconnected as u8, Ordering::Relaxed);
        if prev != ConnectionState::Disconnected as u8 {
            self.emit_state_change_for_id(ConnectionState::Disconnected.into());
        }
    }

    /// Flips state to `Direct` and emits `volume-connection-changed` if the previous
    /// state was something else. Called by `attempt_reconnect` after a successful
    /// session rebuild.
    pub(super) fn transition_to_direct(&self) {
        let prev = self.state.swap(ConnectionState::Direct as u8, Ordering::Relaxed);
        if prev != ConnectionState::Direct as u8 {
            self.emit_state_change_for_id(ConnectionState::Direct.into());
        }
    }
}

/// Instance-level pass-throughs to the share, for the callers that hold an
/// `SmbVolume` rather than its inner state: the connection-quality indicator and
/// the debug window's diagnostics dashboard.
impl SmbVolume {
    /// Returns the current connection state.
    pub fn connection_state(&self) -> ConnectionState {
        self.inner.connection_state()
    }

    /// Snapshot the smb2 client's diagnostics tree.
    pub async fn diagnostics(&self) -> Option<smb2::Diagnostics> {
        self.inner.diagnostics().await
    }

    /// The client's current SMB credit count, or `None` while disconnected.
    ///
    /// A single number off the same brief client-mutex hold `diagnostics` takes.
    /// The soak suite samples it between iterations, where a credit leak shows up
    /// as a slow bleed long before it stalls a read.
    #[cfg(any(test, feature = "testing"))]
    pub(super) async fn session_credits(&self) -> Option<u16> {
        let guard = self.inner.client.lock().await;
        guard.as_ref().map(|c| c.credits())
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
