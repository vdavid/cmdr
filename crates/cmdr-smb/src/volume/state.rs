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
use std::sync::atomic::{AtomicU64, Ordering};

/// Connection health states for an SmbVolume.
///
/// Held in a [`SessionCell`], packed with the session's generation, for
/// lock-free reads from any thread. The internal state machine is binary
/// (`Direct ⇄ Disconnected`). The "OS mount" fallback the frontend shows lives at
/// the outer `cmdr_fs::volume::ConnectionState` layer (see
/// `enrich_from_volume_registry` in `volumes/platform/macos/smb.rs`) and never reaches this
/// cell on the smb2 hot path.
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

/// Which installed main session a call ran on. Every rebuild of the session
/// installs the next one, so an error from a call on an older session can be
/// told from the current session's own (`SessionCell::lose`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SessionGeneration(u64);

impl SessionGeneration {
    /// The session a volume is built with.
    pub(super) const FIRST: Self = Self(0);

    /// The counter itself, as `Volume::connection_generation` reports it.
    pub(super) fn count(self) -> u64 {
        self.0
    }
}

/// Which installed transfer session a call ran on (`transfer_session.rs`), so a
/// loss from one a reconnect already replaced moves nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TransferGeneration(u64);

impl TransferGeneration {
    pub(super) fn new(count: u64) -> Self {
        Self(count)
    }
}

/// Which session a call ran on: what the error funnel needs to mark the right
/// one lost (`SmbVolumeInner::note_error`). A transfer session's loss ❌ never
/// moves the main session's state: the share is still up, and the transfer's
/// retry rides the main session while the transfer session reconnects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RanOn {
    Main(SessionGeneration),
    Transfer(TransferGeneration),
}

impl From<SessionGeneration> for RanOn {
    fn from(generation: SessionGeneration) -> Self {
        Self::Main(generation)
    }
}

/// The share's connection state and the generation of its installed session, in
/// ONE atomic, so a transition can check which session it's about and act in the
/// same step.
///
/// The work that outlives a call (the streaming-read producer, a held-open
/// random reader) holds the share (`Arc<SmbVolumeInner>`) and reports a
/// mid-stream loss through its error funnel.
///
/// ❗ The only way to `Disconnected` for a lost session is [`lose`](Self::lose),
/// which acts only while the session the failing call ran on is still the
/// installed one. A late error from a session a reconnect already replaced must
/// never take the fresh one down: every sibling on it would take an outage it
/// never had.
pub(super) struct SessionCell(AtomicU64);

impl SessionCell {
    /// The low byte holds the state, the rest the generation.
    const STATE_BITS: u32 = 8;

    fn pack(state: ConnectionState, generation: SessionGeneration) -> u64 {
        (generation.0 << Self::STATE_BITS) | state as u64
    }

    fn unpack(packed: u64) -> (ConnectionState, SessionGeneration) {
        (
            ConnectionState::from_u8(packed as u8),
            SessionGeneration(packed >> Self::STATE_BITS),
        )
    }

    pub(super) fn new(state: ConnectionState, generation: SessionGeneration) -> Self {
        Self(AtomicU64::new(Self::pack(state, generation)))
    }

    fn load(&self) -> (ConnectionState, SessionGeneration) {
        Self::unpack(self.0.load(Ordering::Acquire))
    }

    pub(super) fn state(&self) -> ConnectionState {
        self.load().0
    }

    /// Moves to `next` from the current state and generation, atomically, and
    /// reports the state it moved from. `None` leaves the cell as it is.
    fn update(
        &self,
        next: impl Fn(ConnectionState, SessionGeneration) -> Option<(ConnectionState, SessionGeneration)>,
    ) -> Option<ConnectionState> {
        self.0
            .try_update(Ordering::AcqRel, Ordering::Acquire, |packed| {
                let (state, generation) = Self::unpack(packed);
                next(state, generation).map(|(state, generation)| Self::pack(state, generation))
            })
            .ok()
            .map(|packed| Self::unpack(packed).0)
    }

    /// The session `ran_on` is lost: flips to `Disconnected` if it's still the
    /// installed one and wasn't already marked lost. Whether this call flipped
    /// it, so exactly one of the calls meeting the same dead session announces
    /// it.
    pub(super) fn lose(&self, ran_on: SessionGeneration) -> bool {
        self.update(|state, generation| {
            (generation == ran_on && state != ConnectionState::Disconnected)
                .then_some((ConnectionState::Disconnected, generation))
        })
        .is_some()
    }

    /// The generation the session about to be installed gets. Runs in the same
    /// critical section that installs it, so a call can never hold the new
    /// session under the old generation. The state stays as it is (no call gets
    /// a lease until [`restore`](Self::restore) says `Direct`).
    pub(super) fn advance(&self) -> SessionGeneration {
        let before = self.0.fetch_add(1 << Self::STATE_BITS, Ordering::AcqRel);
        SessionGeneration(Self::unpack(before).1.0 + 1)
    }

    /// Session `generation` is up: flips to `Direct` if it's still the
    /// installed one. Whether this call flipped it.
    pub(super) fn restore(&self, generation: SessionGeneration) -> bool {
        self.update(|state, current| {
            (current == generation && state != ConnectionState::Direct).then_some((ConnectionState::Direct, current))
        })
        .is_some()
    }

    /// The volume is gone for good (`on_unmount`): `Disconnected` whatever
    /// session is installed, since none of them will serve again.
    pub(super) fn shut_down(&self) {
        self.update(|_, generation| Some((ConnectionState::Disconnected, generation)));
    }

    /// The installed session's generation: what `Volume::connection_generation`
    /// answers, so a transfer can tell a late error from a replaced session
    /// (`crates/cmdr-operations/src/transfer/volume_outage.rs`). ❌ An error
    /// funnel reports the generation its lease carried, never this: a fresh
    /// read would let a replaced session's loss mark its successor.
    pub(super) fn generation(&self) -> SessionGeneration {
        self.load().1
    }

    /// Puts the cell in `state` without a transition or an event, for the
    /// suites that start from a connected volume.
    #[cfg(test)]
    pub(super) fn set_for_test(&self, state: ConnectionState) {
        self.update(|_, generation| Some((state, generation)));
    }
}

/// Whether `err` means the session that produced it can't serve again: THE
/// session-loss predicate, for the main session's funnel
/// ([`SmbVolumeInner::note_smb_error`]), the scan pool's members, and a write's
/// date stamp alike.
///
/// By [`smb2::Error::kind`] alone. Since smb2 0.32.0 the request whose own send
/// broke gets `SendFailed` (kind `ConnectionLost`) and `Io` on an established
/// connection means a LOCAL failure, so ❌ no errno arm: an `EACCES` or `ENOSPC`
/// must never end the session. A refused signature joins the lost connection: it
/// taints the session, whichever call saw it.
pub(super) fn ends_the_session(err: &smb2::Error) -> bool {
    matches!(
        err.kind(),
        smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired | smb2::ErrorKind::SignatureInvalid
    )
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
        self.state.state()
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
        let guard = self.session.lock().await;
        guard.as_ref().map(|s| s.client.diagnostics())
    }

    /// THE funnel every smb2 error passes, whichever call saw it: tells the host
    /// about a refused signature, and, when the error ends the session
    /// ([`ends_the_session`]), marks session `ran_on` lost. Returns whether the
    /// error ends the session, for the caller's log line.
    ///
    /// `ran_on` is the generation of the session the failing call RAN on (its
    /// [`SessionLease`](super::session::SessionLease)), ❌ never a fresh read of
    /// the current one, which would let a late error from a replaced session
    /// take down its successor.
    pub(super) fn note_smb_error(&self, ran_on: SessionGeneration, err: &smb2::Error) -> bool {
        super::session::report_security_refusal(&self.host, &self.volume_id, err);
        let ends = ends_the_session(err);
        if ends {
            self.transition_to_disconnected(ran_on);
        }
        ends
    }

    /// [`note_smb_error`](Self::note_smb_error) for a call on EITHER session: a
    /// transfer session's loss marks only that one lost (`transfer_session.rs`),
    /// and the share stays `Direct`.
    pub(super) fn note_error(&self, ran_on: RanOn, err: &smb2::Error) -> bool {
        match ran_on {
            RanOn::Main(generation) => self.note_smb_error(generation, err),
            RanOn::Transfer(generation) => {
                super::session::report_security_refusal(&self.host, &self.volume_id, err);
                let ends = ends_the_session(err);
                if ends {
                    self.transfer.lose(generation);
                }
                ends
            }
        }
    }

    /// Marks session `ran_on` lost, and emits `volume-connection-changed` if
    /// this call is the one that flipped it: silent when it was already
    /// `Disconnected` (several in-flight ops all meet the same broken session)
    /// and when a reconnect already installed a newer session.
    pub(super) fn transition_to_disconnected(&self, ran_on: SessionGeneration) {
        if self.state.lose(ran_on) {
            self.emit_state_change_for_id(ConnectionState::Disconnected.into());
        }
    }

    /// Flips state to `Direct` for the session `do_attempt_reconnect` just
    /// installed as `generation`, and emits `volume-connection-changed` if this
    /// call flipped it.
    pub(super) fn transition_to_direct(&self, generation: SessionGeneration) {
        if self.state.restore(generation) {
            self.emit_state_change_for_id(ConnectionState::Direct.into());
        }
    }
}

/// Instance-level pass-throughs to the share, for the callers that hold an
/// `SmbVolume` rather than its inner state: the connection-quality indicator and
/// the debug window's diagnostics dashboard.
impl SmbVolume {
    /// The share's own connection state.
    ///
    /// Named apart from [`Volume::connection_state`](cmdr_fs::volume::Volume::connection_state),
    /// which this type also implements: an inherent method shadows a trait one,
    /// so two same-named accessors returning two different enums would resolve by
    /// receiver type and silently hand a caller the wrong one.
    pub fn session_state(&self) -> ConnectionState {
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
        let guard = self.inner.session.lock().await;
        guard.as_ref().map(|s| s.client.credits())
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
