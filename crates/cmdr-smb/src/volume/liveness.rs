//! What this share's connection can say about itself without anyone asking the
//! server: whether it is still talking, and how many bytes it has received.
//!
//! Both answers are readings smb2 keeps on its own clocks (`Connection::liveness`
//! and `Connection::inbound`), so polling them costs a few atomic loads and
//! never touches the wire, the client mutex, or a request's deadline. The
//! transfer watchdog polls both once a second.

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::ConnectionLiveness;
use smb2::Liveness;
use smb2::client::Connection;
use std::sync::RwLock as StdRwLock;

/// The connection the share's CURRENT client owns, kept apart from the client
/// so a sync caller can read it without the async client mutex.
///
/// ❗ Every place that installs or drops the client installs or drops this in
/// the same breath (`SmbVolume::new`, `do_attempt_reconnect`, `on_unmount`). A
/// handle left over from a rebuilt client would answer for a connection nobody
/// uses any more: a torn-down one reads `Disconnected` forever, and one that
/// merely went quiet could keep a dead session's verdict alive after a
/// reconnect. The mutex can't be the source instead: a listing holds it across
/// its whole round trip, and a wedged one would hide the verdict exactly when
/// it matters.
pub(super) struct LiveConnection {
    current: StdRwLock<Option<Connection>>,
}

impl LiveConnection {
    pub(super) fn new(conn: Option<Connection>) -> Self {
        Self {
            current: StdRwLock::new(conn),
        }
    }

    /// Point at the connection a freshly installed client owns, or at nothing
    /// once the client is gone.
    pub(super) fn replace(&self, conn: Option<Connection>) {
        *self.current.write_ignore_poison() = conn;
    }

    /// The watchdog's liveness question, answered from the current connection.
    /// `None` while there is no client.
    pub(super) fn verdict(&self) -> Option<ConnectionLiveness> {
        self.current
            .read_ignore_poison()
            .as_ref()
            .and_then(|c| verdict_for(c.liveness()))
    }

    /// Payload bytes the current connection has received, a frame still
    /// arriving included. `None` while there is no client.
    pub(super) fn bytes_received(&self) -> Option<u64> {
        self.current
            .read_ignore_poison()
            .as_ref()
            .map(|c| c.inbound().bytes_received)
    }

    /// Asks the current connection something sync (no wire), or `None` while
    /// there is no client.
    pub(super) fn with_connection<T>(&self, ask: impl FnOnce(&Connection) -> T) -> Option<T> {
        self.current.read_ignore_poison().as_ref().map(ask)
    }

    #[cfg(test)]
    pub(super) fn current(&self) -> Option<Connection> {
        self.current.read_ignore_poison().clone()
    }
}

/// How smb2's reading maps onto the watchdog's three-valued answer.
///
/// - `Unresponsive` is `Dead`: the keepalive is armed, a request is out, and the
///   server has put NOTHING on the wire past the liveness window, probe replies
///   included. It's the reading `Error::ServerUnresponsive` rests on, read before
///   any request has paid for it. A slow response can't produce it (every byte
///   counts as it lands), but a NAS saturated enough to drop ECHOs for a whole
///   window can, which is why it's only half of the watchdog's gate.
/// - `Alive` is `Alive`: a byte arrived inside the window, mid-frame included.
/// - `Idle` and `Quiet` are `None`. Idle has nothing outstanding to be silent
///   about, and Quiet is silence that proves nothing yet (too short, or no
///   keepalive to tell slow from dead).
/// - `Disconnected` is `None`, deliberately. A torn-down connection has already
///   failed every request on it, so the task the watchdog would unstick has its
///   error in hand, and the per-file retry takes it from there. `Dead` would
///   arrive strictly after that error and add nothing.
///
/// ❌ Don't read this verdict alone: the watchdog ANDs it with its own stillness
/// window (`transfer_probe_watchdog.rs`), because `Unresponsive` flips back to `Alive` the
/// moment a byte lands.
pub(super) fn verdict_for(liveness: Liveness) -> Option<ConnectionLiveness> {
    match liveness {
        Liveness::Unresponsive { .. } => Some(ConnectionLiveness::Dead),
        Liveness::Alive => Some(ConnectionLiveness::Alive),
        // `Liveness` is `#[non_exhaustive]`: a reading added later says nothing
        // until someone decides what it means here.
        _ => None,
    }
}

#[cfg(test)]
#[path = "liveness_test.rs"]
mod liveness_test;
