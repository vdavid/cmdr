//! Pausing a volume's index while the disk it lives on is out of room, and bringing
//! it back once there's space again.
//!
//! The writer stops on `SQLITE_FULL` or on the host's low-disk-space answer
//! (`writer/space_guard.rs`), and the supervisor lands here instead of in
//! `fail_index`. The database is fine, so the volume drains like a stop but stays
//! registered as [`IndexPhase::Paused`] with its READ handles still installed:
//! listings keep their last-known sizes (as stale) while nothing writes, and the
//! badge says paused (`Freshness::Paused`). Per-drive intent is untouched.
//!
//! A waiter then rechecks the host and restarts the volume once there's room. The
//! restart is an ordinary start that takes the paused slot over: a journaled
//! volume replays from the position its last committed chunk stored, so nothing is
//! lost and nothing is applied twice (`watch/DETAILS.md` § "A replay commits in
//! chunks"); a local volume Cmdr can't replay (an external drive) walks itself
//! whole, reconciling in place; a share or a phone, with no event history at all,
//! comes back `Stale` for the user to rescan.
//!
//! Why a stop rather than a writer that waits in place: a parked writer backs its
//! bounded channel up into every sender, and a replay or a live loop blocked on a
//! send stops draining FSEvents, whose unbounded queue then grows toward the RAM
//! guard's forced full scan, the one thing a nearly full disk can least afford.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::teardown::{drop_what_a_stopped_writer_is_owed, retire_after_the_drain, withdraw_from_the_read_path};
use super::{INDEX_REGISTRY, IndexPhase, PersistDisable, TeardownClaim, apply_freshness_event};
use crate::indexing::host::policy;
use crate::indexing::lifecycle::failure::SpaceStop;
use crate::indexing::lifecycle::freshness::FreshnessEvent;
use crate::indexing::lifecycle::manager::IndexManager;
use crate::indexing::resources::memory_watchdog;
use crate::indexing::store::IndexStore;
use crate::indexing::volume::VolumeId;

/// How often a paused volume asks whether there's room again. The host's reading
/// moves every couple of seconds; a restart is a replay or a walk, so there's no
/// reason to be quicker than this.
#[cfg(not(test))]
pub(crate) const RECHECK_EVERY: Duration = Duration::from_secs(30);
/// Short in tests, so a pause-and-resume runs in well under a second.
#[cfg(test)]
pub(crate) const RECHECK_EVERY: Duration = Duration::from_millis(50);

/// The longest a volume that keeps filling the disk waits before its next try.
pub(crate) const LONGEST_BACKOFF: Duration = Duration::from_secs(30 * 60);

/// How long a pause is remembered for the backoff. A volume that paused more than
/// this long ago starts over at [`RECHECK_EVERY`].
const STRIKE_MEMORY: Duration = Duration::from_secs(60 * 60);

/// Recent pauses per volume, for the backoff: how many, and when the last was.
static STRIKES: LazyLock<Mutex<HashMap<VolumeId, (u32, Instant)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Volumes with a waiter running, so a second pause doesn't start a second one.
static WAITING: LazyLock<Mutex<HashSet<VolumeId>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Pause `volume_id`'s index for disk space: drain its manager, keep its reads,
/// and restart it once there's room.
///
/// Same take-it-out-then-drain discipline as `fail_index`: the manager comes out
/// under a transient `ShuttingDown` that says it's a pause (`paused_for`), the
/// drain runs off the lock, and [`finish_pausing`] installs `Paused` on the far
/// side. A trip landing while a scan start has the manager out is CLAIMED and
/// finished at the handback, exactly like a failure: the signal is one-shot.
///
/// Runs the blocking drain, so call it off an async worker (the supervisor uses
/// `spawn_blocking`).
pub(crate) fn pause_for_space(volume_id: &str, why: SpaceStop) {
    let mgr = {
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        // Already stopped or cleared by somebody else: their word stands.
        let Some(instance) = reg.get_mut(volume_id) else {
            return;
        };
        if instance.phase.claim_the_teardown(TeardownClaim::Paused(why)) {
            log::info!("Pausing '{volume_id}' for disk space lands as its scan start hands the manager back");
            return;
        }
        match std::mem::replace(
            &mut instance.phase,
            IndexPhase::ShuttingDown {
                restart: None,
                then_clear: None,
                paused_for: Some(why),
            },
        ) {
            IndexPhase::Running(mgr) => mgr,
            other => {
                // Not running (stopped, cleared, or failed elsewhere): their word
                // stands, and the writer already exited.
                instance.phase = other;
                log::debug!("pause_for_space('{volume_id}'): not Running, nothing to pause");
                return;
            }
        }
    };
    finish_pausing(volume_id, mgr, why);
}

/// Drain a paused volume's manager and register it as `Paused`, read handles
/// kept, then start the waiter. The half of [`pause_for_space`] that runs OFF the
/// lock, shared with the claimed path so a trip that landed mid-scan-start ends
/// the volume in the same place.
///
/// ⚠️ **Only if the drain marker still says pause.** A stop or a clear that
/// landed in the drain published a fresh marker without `paused_for`, and a start
/// recorded a `restart` on it: either way somebody else's word outranks the
/// pause, and the volume ends exactly where that stop would have ended it.
pub(super) fn finish_pausing(volume_id: &str, mut mgr: Box<IndexManager>, why: SpaceStop) {
    // The reads stay; everything else a stopped writer was owed goes.
    drop_what_a_stopped_writer_is_owed(volume_id);
    let db_path = mgr.db_path().to_path_buf();
    let events = Arc::clone(&mgr.events);
    mgr.shutdown();
    // The manager carries a share of the volume's hold, which a clear that met the
    // drain waits on.
    drop(mgr);

    let paused = {
        let mut reg = INDEX_REGISTRY.lock_ignore_poison();
        match reg.get_mut(volume_id) {
            Some(instance)
                if matches!(
                    instance.phase,
                    IndexPhase::ShuttingDown {
                        restart: None,
                        then_clear: None,
                        paused_for: Some(_),
                    }
                ) =>
            {
                instance.phase = IndexPhase::Paused {
                    db_path: db_path.clone(),
                    why,
                };
                true
            }
            _ => false,
        }
    };
    if !paused {
        log::info!("Pausing '{volume_id}' for disk space: a stop, clear, or start landed meanwhile, and it wins");
        withdraw_from_the_read_path(volume_id);
        retire_after_the_drain(volume_id, &db_path, PersistDisable::No);
        return;
    }

    crate::indexing::events::set_phase_for(
        events.as_ref(),
        volume_id,
        crate::indexing::events::ActivityPhase::Idle,
        "paused for disk space",
    );
    apply_freshness_event(volume_id, FreshnessEvent::SpacePaused);

    let first_try = first_try_after(note_a_pause(volume_id, Instant::now()));
    log::warn!(
        "Indexing paused for '{volume_id}' ({why:?}): the disk the index lives on is nearly full. \
         Its sizes keep serving as they were, and it picks up where it left off once there's room \
         (first check in {}s).",
        first_try.as_secs()
    );
    if !WAITING.lock_ignore_poison().insert(volume_id.to_string()) {
        return;
    }
    let volume_id = volume_id.to_string();
    let watchdog_stops = memory_watchdog::stop_generation();
    crate::indexing::host::runtime::spawn(async move {
        wait_for_room(&volume_id, &db_path, first_try, watchdog_stops).await;
        WAITING.lock_ignore_poison().remove(&volume_id);
    });
}

/// Whether the registry holds `volume_id` paused right now.
fn is_paused(volume_id: &str) -> bool {
    matches!(
        INDEX_REGISTRY.lock_ignore_poison().get(volume_id).map(|i| &i.phase),
        Some(IndexPhase::Paused { .. })
    )
}

/// Wait until the host says there's room, then restart the volume, unless
/// something else decided its fate meanwhile.
async fn wait_for_room(volume_id: &str, db_path: &Path, first_try: Duration, watchdog_stops: u64) {
    let mut wait = first_try;
    loop {
        tokio::time::sleep(wait).await;
        wait = RECHECK_EVERY;
        // Off the async worker: the facts read the database file and the host may take
        // a fresh free-space reading, both blocking calls.
        let facts = {
            let volume_id = volume_id.to_string();
            let db_path = db_path.to_path_buf();
            crate::indexing::host::runtime::spawn_blocking(move || Facts {
                still_paused: is_paused(&volume_id),
                database_on_disk: db_path.exists(),
                vetoed: IndexStore::user_disabled(&db_path),
                watchdog_stopped: memory_watchdog::stop_generation() != watchdog_stops,
                storage_low: policy::current().index_storage_low(),
            })
            .await
        };
        let Ok(facts) = facts else {
            // The blocking pool is shutting down with the app: nothing to restart into.
            return;
        };
        let decision = decide(facts);
        match decision {
            Decision::KeepWaiting => continue,
            Decision::Leave => {
                log::info!(
                    "'{volume_id}' no longer waits for disk space: it was started, stopped, or cleared meanwhile, \
                     or the memory watchdog stopped indexing"
                );
                return;
            }
            Decision::Restart => {
                log::info!("There's room on disk again; restarting the index for '{volume_id}'");
                let volume_id = volume_id.to_string();
                let restarted =
                    crate::indexing::host::runtime::spawn_blocking(move || restart_after_the_pause(&volume_id)).await;
                if let Ok(Err(e)) = restarted {
                    log::warn!("Restarting an index paused for disk space didn't take: {e}");
                }
                return;
            }
        }
    }
}

/// Start a paused volume again exactly as it was started (at the root it has now,
/// if a rename moved it meanwhile), marked as a pause's restart: the reservation
/// takes the paused slot over, and a volume with no journal keeps its rows without
/// a walk. Goes through `start_indexing_for`, so the master switch still gates it.
/// A volume that isn't paused any more is somebody else's to start.
fn restart_after_the_pause(volume_id: &str) -> Result<(), String> {
    let request = {
        let reg = INDEX_REGISTRY.lock_ignore_poison();
        match reg.get(volume_id) {
            Some(instance) if matches!(instance.phase, IndexPhase::Paused { .. }) => instance.started_as.clone(),
            _ => return Ok(()),
        }
    };
    request.after_space_pause().start(volume_id)
}

/// What a paused volume's waiter knows at one recheck.
#[derive(Debug, Clone, Copy)]
struct Facts {
    /// The registry still holds the volume `Paused`. Anything that moved it off (a
    /// start, a stop, a clear, the master switch) decided its fate already.
    still_paused: bool,
    /// Its database is still there; a clear or a forget took it otherwise.
    database_on_disk: bool,
    /// The user turned this drive off meanwhile.
    vetoed: bool,
    /// The memory watchdog stopped all indexing since the pause. Nothing restarts
    /// indexing after a watchdog stop (the user does), so neither does this: a
    /// restart here would bring one volume back into the memory pressure that
    /// stopped every other one.
    watchdog_stopped: bool,
    /// The host still says the disk is low.
    storage_low: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    KeepWaiting,
    Restart,
    Leave,
}

/// The waiter's one decision, pure so every arm is pinned. Anything somebody else
/// decided meanwhile wins over a restart: the pause was ours, the stop or clear
/// was theirs.
fn decide(facts: Facts) -> Decision {
    if !facts.still_paused || !facts.database_on_disk || facts.vetoed || facts.watchdog_stopped {
        return Decision::Leave;
    }
    if facts.storage_low {
        return Decision::KeepWaiting;
    }
    Decision::Restart
}

/// Record a pause and answer how many this volume has had recently, this one
/// included.
fn note_a_pause(volume_id: &str, now: Instant) -> u32 {
    let mut strikes = STRIKES.lock_ignore_poison();
    let entry = strikes.entry(volume_id.to_string()).or_insert((0, now));
    if now.saturating_duration_since(entry.1) > STRIKE_MEMORY {
        entry.0 = 0;
    }
    entry.0 += 1;
    entry.1 = now;
    entry.0
}

/// How long the waiter sleeps before its first check, given how many recent pauses
/// this is. Doubles per repeat: a volume whose restart fills the disk straight
/// back up (the host's threshold set lower than what one chunk needs) would
/// otherwise churn a replay every 30 s.
fn first_try_after(strikes: u32) -> Duration {
    let doublings = strikes.saturating_sub(1).min(16);
    RECHECK_EVERY.saturating_mul(1 << doublings).min(LONGEST_BACKOFF)
}

/// Forget a volume's pause history, for tests that pause the same volume id twice.
#[cfg(test)]
fn forget_strikes(volume_id: &str) {
    STRIKES.lock_ignore_poison().remove(volume_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM: Facts = Facts {
        still_paused: true,
        database_on_disk: true,
        vetoed: false,
        watchdog_stopped: false,
        storage_low: false,
    };

    #[test]
    fn a_paused_volume_restarts_once_there_is_room() {
        assert_eq!(decide(ROOM), Decision::Restart);
        assert_eq!(
            decide(Facts {
                storage_low: true,
                ..ROOM
            }),
            Decision::KeepWaiting
        );
    }

    #[test]
    fn whatever_happened_to_the_volume_meanwhile_wins_over_a_restart() {
        for (what, facts) in [
            (
                "started, stopped, or cleared meanwhile",
                Facts {
                    still_paused: false,
                    ..ROOM
                },
            ),
            (
                "cleared or forgotten",
                Facts {
                    database_on_disk: false,
                    ..ROOM
                },
            ),
            ("turned off", Facts { vetoed: true, ..ROOM }),
            (
                "the memory watchdog stopped indexing",
                Facts {
                    watchdog_stopped: true,
                    ..ROOM
                },
            ),
            (
                "turned off while still low",
                Facts {
                    vetoed: true,
                    storage_low: true,
                    ..ROOM
                },
            ),
        ] {
            assert_eq!(decide(facts), Decision::Leave, "{what}");
        }
    }

    #[test]
    fn a_volume_that_keeps_filling_the_disk_backs_off() {
        assert_eq!(first_try_after(1), RECHECK_EVERY);
        assert_eq!(first_try_after(2), RECHECK_EVERY * 2);
        assert_eq!(first_try_after(3), RECHECK_EVERY * 4);
        assert_eq!(first_try_after(40), LONGEST_BACKOFF, "capped, and no overflow");
    }

    #[test]
    fn the_backoff_forgets_a_pause_from_long_ago() {
        let volume = "space-pause-strikes";
        forget_strikes(volume);
        let start = Instant::now();
        assert_eq!(note_a_pause(volume, start), 1);
        assert_eq!(note_a_pause(volume, start + Duration::from_secs(60)), 2);
        assert_eq!(
            note_a_pause(
                volume,
                start + Duration::from_secs(60) + STRIKE_MEMORY + Duration::from_secs(1)
            ),
            1,
            "an hour of quiet starts the backoff over"
        );
        forget_strikes(volume);
    }
}
