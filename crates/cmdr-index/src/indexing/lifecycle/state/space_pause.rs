//! Pausing a volume's index while the disk it lives on is out of room, and bringing
//! it back once there's space again.
//!
//! The writer stops on `SQLITE_FULL` or on the host's low-disk-space answer
//! (`writer/space_guard.rs`), and the supervisor lands here instead of in
//! `fail_index`. The database is fine, so this is a STOP that keeps it (the same
//! non-sticky stop an eject does, so per-drive intent survives untouched), plus a
//! waiter that restarts the volume exactly as it was started once the host says
//! there's room. The restart is an ordinary start: a journaled volume replays from
//! the position its last committed chunk stored, so nothing is lost and nothing
//! is applied twice (`watch/DETAILS.md` § "A replay commits in chunks").
//!
//! Why a stop rather than a writer that waits in place: a parked writer backs its
//! bounded channel up into every sender, and a replay or a live loop blocked on a
//! send stops draining FSEvents, whose unbounded queue then grows toward the RAM
//! guard's forced full scan, the one thing a nearly full disk can least afford.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::teardown::stop_indexing;
use super::{INDEX_REGISTRY, StartRequest, resolved_index_db_path};
use crate::indexing::host::policy;
use crate::indexing::lifecycle::failure::SpaceStop;
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

/// Stop `volume_id`'s index, keeping its database, and restart it once there's room.
///
/// Runs the blocking drain, so call it off an async worker (the supervisor uses
/// `spawn_blocking`).
pub(crate) fn pause_for_space(volume_id: &str, why: SpaceStop) {
    let Some(request) = INDEX_REGISTRY
        .lock_ignore_poison()
        .get(volume_id)
        .map(|instance| instance.started_as.clone())
    else {
        // Already stopped or cleared by somebody else: their word stands.
        return;
    };
    let db_path = match resolved_index_db_path(volume_id) {
        Ok(path) => path,
        Err(e) => {
            log::warn!(
                "Indexing for '{volume_id}' is out of disk space, and its database can't be resolved ({e}); stopping it"
            );
            let _ = stop_indexing(volume_id);
            return;
        }
    };
    if let Err(e) = stop_indexing(volume_id) {
        log::warn!("Pausing '{volume_id}' for disk space: the stop finished with {e}");
    }
    let first_try = first_try_after(note_a_pause(volume_id, Instant::now()));
    log::warn!(
        "Indexing paused for '{volume_id}' ({why:?}): the disk the index lives on is nearly full. \
         It picks up where it left off once there's room (first check in {}s).",
        first_try.as_secs()
    );
    if !WAITING.lock_ignore_poison().insert(volume_id.to_string()) {
        return;
    }
    let volume_id = volume_id.to_string();
    crate::indexing::host::runtime::spawn(async move {
        wait_for_room(&volume_id, request, &db_path, first_try).await;
        WAITING.lock_ignore_poison().remove(&volume_id);
    });
}

/// Wait until the host says there's room, then restart the volume, unless
/// something else decided its fate meanwhile.
async fn wait_for_room(volume_id: &str, request: StartRequest, db_path: &Path, first_try: Duration) {
    let mut wait = first_try;
    loop {
        tokio::time::sleep(wait).await;
        wait = RECHECK_EVERY;
        let decision = decide(Facts {
            registered: INDEX_REGISTRY.lock_ignore_poison().contains_key(volume_id),
            database_on_disk: db_path.exists(),
            vetoed: IndexStore::user_disabled(db_path),
            storage_low: policy::current().index_storage_low(),
        });
        match decision {
            Decision::KeepWaiting => continue,
            Decision::Leave => {
                log::info!(
                    "'{volume_id}' no longer waits for disk space: it was started, stopped, or cleared meanwhile"
                );
                return;
            }
            Decision::Restart => {
                log::info!("There's room on disk again; restarting the index for '{volume_id}'");
                // `start` goes through `start_indexing_for`, so the master switch
                // still gates it.
                let volume_id = volume_id.to_string();
                let request = request.clone();
                let started = crate::indexing::host::runtime::spawn_blocking(move || request.start(&volume_id)).await;
                if let Ok(Err(e)) = started {
                    log::warn!("Restarting an index paused for disk space didn't take: {e}");
                }
                return;
            }
        }
    }
}

/// What a paused volume's waiter knows at one recheck.
#[derive(Debug, Clone, Copy)]
struct Facts {
    /// Something started the volume again already (the user, a remount).
    registered: bool,
    /// Its database is still there; a clear or a forget took it otherwise.
    database_on_disk: bool,
    /// The user turned this drive off meanwhile.
    vetoed: bool,
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
    if facts.registered || !facts.database_on_disk || facts.vetoed {
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
        registered: false,
        database_on_disk: true,
        vetoed: false,
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
                "started again",
                Facts {
                    registered: true,
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
