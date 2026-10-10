//! The steps every network backend's connect wiring owes, whatever protocol it
//! speaks: a table of attempts a user can still call off, a landing that refuses
//! a dial whose place moved while it was out, and installing the finished volume
//! while retiring whoever held its id.
//!
//! ❗ **A backend never registers itself.** Each `*_volume_wiring.rs` knows both
//! its backend and the volume registry, and neither of those knows the wiring;
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
    /// The place ids it dials, so a move can call it off by place.
    places: Vec<String>,
}

/// The connect attempts of one backend, by the id their caller made up.
///
/// ❗ The id is the CALLER's, and that is the whole point: a connect can hold for
/// half a minute, so a sign-in dialog has to arm its cancel button before the
/// command answers, and an id the backend handed back would only arrive once
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
        self.register_dialing(attempt_id, Vec::new())
    }

    /// [`Self::register`] for a dial to a saved server's `places` (its volume
    /// id, and for S3 the account's too), which a move can call off
    /// ([`Self::cancel_dials_to`]) and which lands through
    /// [`AttemptGuard::land`].
    pub fn register_dialing(&'static self, attempt_id: &str, places: Vec<String>) -> (CancellationToken, AttemptGuard) {
        let cancel = CancellationToken::new();
        let serial = self.next_serial.fetch_add(1, Ordering::Relaxed);
        // Before the entry is filed: a move that lands from here on is one this
        // dial set out before, so its landing must see it.
        let set_out_at = MOVE_GENERATION.load(Ordering::SeqCst);
        self.entries.lock_ignore_poison().insert(
            attempt_id.to_string(),
            Attempt {
                serial,
                cancel: cancel.clone(),
                places: places.clone(),
            },
        );
        (
            cancel,
            AttemptGuard {
                table: self,
                id: attempt_id.to_string(),
                serial,
                places,
                set_out_at,
            },
        )
    }

    /// Calls off every connect dialing `place`, answering how many were running.
    /// A move's courtesy to a dial at the old address: it stops a connect that
    /// can only end in a refused landing. ❗ Not the guarantee: a dial already
    /// past its last cancel check lands anyway, and [`AttemptGuard::land`] is
    /// what refuses it.
    pub fn cancel_dials_to(&self, place: &str) -> usize {
        let tokens: Vec<CancellationToken> = self
            .entries
            .lock_ignore_poison()
            .values()
            .filter(|attempt| attempt.places.iter().any(|p| p == place))
            .map(|attempt| attempt.cancel.clone())
            .collect();
        for token in &tokens {
            token.cancel();
        }
        if !tokens.is_empty() {
            log::info!(target: "volume", "{} connect to a place that moved was called off", self.backend);
        }
        tokens.len()
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

    /// Whether an attempt is filed under `attempt_id`, without touching it: how a
    /// cell waits for a spawned connect to be cancelable when calling it off
    /// would change what it is testing.
    #[cfg(test)]
    pub fn is_filed(&self, attempt_id: &str) -> bool {
        self.entries.lock_ignore_poison().contains_key(attempt_id)
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
    /// The place ids this attempt dials (empty for a dial nothing can move).
    places: Vec<String>,
    /// [`MOVE_GENERATION`] when it set out.
    set_out_at: u64,
}

impl AttemptGuard {
    /// Holds the landing open for a dial that just succeeded, or `None` when one
    /// of its places moved to a new address while it was out.
    ///
    /// ❗ **This is the guarantee, ❌ cancellation isn't.** A dial to the old
    /// address that set out before a Save and lands after it would `remember`
    /// the old entry again, a second saved server beside the moved one. The
    /// check and the move can't interleave: a move holds [`PlaceMove`] across
    /// the store write AND the session drop, and the landing holds its read side
    /// across install and remember, so a dial lands wholly before a move (which
    /// then takes it along) or wholly after (and is refused here).
    ///
    /// On `None`, let the volume go ([`let_go`]) and answer `Cancelled`: the
    /// pane that asked followed the place to its new address and dials it there.
    pub async fn land(&self) -> Option<Landing> {
        let moved = MOVED_AWAY.read().await;
        let moved_since = self
            .places
            .iter()
            .any(|place| moved.get(place).is_some_and(|&at| at > self.set_out_at));
        if moved_since {
            log::info!(target: "volume", "{} connect landed after its place moved; letting it go", self.table.backend);
            return None;
        }
        Some(Landing { _moved: moved })
    }
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

// ============================================================================
// A place that moved while a dial to it was out
// ============================================================================

/// Bumped once per place a move takes away from its address. A dial notes it
/// when it sets out ([`AttemptTable::register_dialing`]).
static MOVE_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Each place id a move took away from its address, with the generation it
/// moved at. One entry per move for the life of the process, so it stays tiny.
static MOVED_AWAY: tokio::sync::RwLock<BTreeMap<String, u64>> = tokio::sync::RwLock::const_new(BTreeMap::new());

/// A dial's landing, held across installing the volume and remembering the
/// server. A move waits for it ([`AttemptGuard::land`]).
pub struct Landing {
    _moved: tokio::sync::RwLockReadGuard<'static, BTreeMap<String, u64>>,
}

/// A move in progress, from before its secret is copied to after its old
/// session is dropped. Every dial landing meanwhile waits for it.
pub struct PlaceMove {
    moved: tokio::sync::RwLockWriteGuard<'static, BTreeMap<String, u64>>,
}

/// Starts a move: waits for the dials landing right now, and holds every later
/// one until the returned guard drops.
pub async fn start_move() -> PlaceMove {
    PlaceMove {
        moved: MOVED_AWAY.write().await,
    }
}

impl PlaceMove {
    /// Marks `place` as moved away from its address: a dial to it that set out
    /// before now can no longer land ([`AttemptGuard::land`]). Call it once the
    /// store holds the new address, ❌ never before: a move the store refused
    /// moved nothing.
    pub fn moved_away(&mut self, place: &str) {
        let at = MOVE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        self.moved.insert(place.to_string(), at);
    }
}

/// Lets go of a volume a refused landing dialed but never registered: the
/// session closes, and nothing else ever saw it.
pub async fn let_go(volume: Arc<dyn Volume>) {
    let _ = tokio::task::spawn_blocking(move || volume.on_unmount()).await;
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
