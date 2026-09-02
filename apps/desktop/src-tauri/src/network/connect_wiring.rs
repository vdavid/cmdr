//! The two steps every network backend's connect wiring owes, whatever protocol
//! it speaks: a table of attempts a user can still call off, and installing the
//! finished volume while retiring whoever held its id.
//!
//! ❗ **A backend never registers itself.** Each `*_volume_wiring.rs` knows both
//! its backend and the volume registry, and neither of those knows the wiring —
//! `DETAILS.md` § "Backends never register themselves" has the rationale. What
//! lives HERE is only the part that is identical between them, so a fix to the
//! cancel race or the supersede order lands once instead of once per protocol.
//!
//! ❗ **Each backend owns its own [`AttemptTable`], ❌ never a shared one.** The
//! attempt ids are minted with per-backend prefixes on the frontend, and one
//! table would let a stray cancel from one sign-in dialog reach into another
//! backend's dial.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::Volume;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// One connect a user could still call off, and the serial that says WHICH
/// attempt holds the entry.
struct Attempt {
    serial: u64,
    cancel: CancellationToken,
}

/// The connect attempts of one backend, by the id their caller made up.
///
/// ❗ The id is the CALLER's, and that is the whole point: a connect can hold for
/// half a minute, so a sign-in dialog has to arm its cancel button before the
/// command answers — and an id the backend handed back would only arrive once
/// the connect was already over. The serial is what keeps a repeated id honest:
/// a finishing attempt only ever takes its OWN entry out.
pub struct AttemptTable {
    /// What a cancel log line calls this backend, so one table's messages can't
    /// be read as another's.
    backend: &'static str,
    entries: Mutex<BTreeMap<String, Attempt>>,
    next_serial: AtomicU64,
}

impl AttemptTable {
    /// An empty table for `backend`, const so a caller can hold it in a `static`
    /// without a lazy wrapper.
    pub const fn new(backend: &'static str) -> Self {
        Self {
            backend,
            entries: Mutex::new(BTreeMap::new()),
            next_serial: AtomicU64::new(0),
        }
    }

    /// Files `attempt_id` as cancelable and hands back the token the dial runs
    /// under, plus the guard that takes the entry out again.
    ///
    /// ❗ Hold the guard for the whole dial. Dropping it early leaves the
    /// connect running with nothing able to stop it.
    pub fn register(&'static self, attempt_id: &str) -> (CancellationToken, AttemptGuard) {
        let cancel = CancellationToken::new();
        let serial = self.next_serial.fetch_add(1, Ordering::Relaxed);
        self.entries.lock_ignore_poison().insert(
            attempt_id.to_string(),
            Attempt {
                serial,
                cancel: cancel.clone(),
            },
        );
        (
            cancel,
            AttemptGuard {
                table: self,
                id: attempt_id.to_string(),
                serial,
            },
        )
    }

    /// Calls off the connect filed under `attempt_id`, answering whether one was
    /// running.
    ///
    /// ❗ An id nobody is holding is a plain `false`: a cancel racing a connect
    /// that just finished is ordinary, and there is nothing wrong to report
    /// about it. The entry stays until the dial itself notices, so ❌ this never
    /// reports on what the attempt then did.
    pub fn cancel(&self, attempt_id: &str) -> bool {
        let Some(cancel) = self
            .entries
            .lock_ignore_poison()
            .get(attempt_id)
            .map(|attempt| attempt.cancel.clone())
        else {
            return false;
        };
        cancel.cancel();
        log::info!(target: "volume", "{} connect was called off", self.backend);
        true
    }
}

/// Takes one attempt's entry out of the table when its connect ends, however it
/// ends.
///
/// ❗ A guard rather than a call at each exit: a connect leaves through eight
/// arms, and the one that forgets is a token nobody ever collects.
pub struct AttemptGuard {
    table: &'static AttemptTable,
    id: String,
    serial: u64,
}

impl Drop for AttemptGuard {
    fn drop(&mut self) {
        let mut entries = self.table.entries.lock_ignore_poison();
        // Only if it's still ours: a second connect under the same id has
        // replaced the entry, and taking that one out would leave it
        // uncancelable.
        if entries
            .get(&self.id)
            .is_some_and(|attempt| attempt.serial == self.serial)
        {
            entries.remove(&self.id);
        }
    }
}

/// Installs `volume` under `volume_id`, retiring whoever held that id, and tells
/// the frontend the volume list moved.
///
/// ❗ `on_superseded`, ❌ never `on_unmount`: a running transfer, an open viewer
/// stream, and the indexer all hold an `Arc` across a re-registration, and
/// tearing the session down would kill every one of them on a connection that is
/// perfectly healthy.
pub async fn install_retiring_incumbent(volume_id: &str, volume: Arc<dyn Volume>) {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    // Asked BEFORE retiring anyone: a registry that keeps the incumbent would
    // otherwise leave the id pointing at a volume whose background work we just
    // stopped.
    let refused = manager.would_keep_incumbent(volume_id, volume.root());
    if !refused && let Some(previous) = manager.get(volume_id) {
        let _ = tokio::task::spawn_blocking(move || previous.on_superseded()).await;
    }
    manager.register(volume_id, volume);
    crate::volume_broadcast::emit_volumes_changed();
}

#[cfg(test)]
#[path = "connect_wiring_test.rs"]
mod connect_wiring_test;
