//! Index retention: bounded accumulation of external-volume index DBs.
//!
//! Local disk has exactly one index (`index-root.db`); SMB shares and MTP
//! devices each spawn their own `index-{volume_id}.db`, so over time the data
//! dir can accumulate one DB per share/phone-storage the user ever connected.
//! This module caps that accumulation with a simple, SAFE LRU eviction of the
//! least-recently-used **offline** (not currently indexed) external DBs.
//!
//! ## Safety invariants (never break these)
//!
//! - **Never evict a live volume's index.** Only DBs whose volume id is *not*
//!   in the registry are eviction candidates. A `Running`/`Initializing` (or
//!   even `ShuttingDown`) volume's DB is off-limits — deleting it out from
//!   under its writer would corrupt an in-flight scan. The registry is the
//!   single source of truth for "live"; we pass its snapshot in.
//! - **Never evict `root`.** The local-disk index is the search-feeding volume
//!   and is always wanted; it's excluded from candidates regardless of mtime.
//! - **Cap by COUNT, not running connections.** We only ever delete files for
//!   volumes with no registry instance, so there's no index writer to drain.
//! - **An evicted volume is a forgotten one.** Its files go through
//!   `volume_files::remove`, the same door `state::clear_index` uses, so the
//!   stores beside the index go with it and whatever still holds one open lets go
//!   first. ❌ Never unlink an `index-{id}.db` here by hand.
//!
//! ## Policy (intentionally simple)
//!
//! Keep at most [`MAX_EXTERNAL_INDEX_DBS`] external (non-root) index DBs. When
//! over the cap, evict the oldest-by-mtime offline ones until back at the cap.
//! mtime is a cheap LRU proxy: a DB is rewritten on every scan and live write,
//! so the least-recently-touched DB is the least-recently-used volume. This is
//! deliberately not a size budget or an access-time LRU; if a fancier policy is
//! ever needed, see the TODO at [`select_evictions`].

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::indexing::lifecycle::state;
use crate::indexing::volume::ROOT_VOLUME_ID;
use crate::volume_files::{self, Removal, StoreDirs, VolumeStore};

/// Maximum number of external (non-root) index DBs to retain. Beyond this, the
/// least-recently-used offline ones are evicted. Sized generously: a heavy user
/// with a dozen NAS shares and a few phones stays well under it, so eviction
/// only ever reclaims long-abandoned drives.
pub(crate) const MAX_EXTERNAL_INDEX_DBS: usize = 32;

/// One external index DB on disk: its volume id (parsed from the filename) and
/// last-modified time (the LRU key).
#[derive(Debug, Clone)]
pub(crate) struct IndexDbFile {
    pub(crate) volume_id: String,
    pub(crate) path: PathBuf,
    pub(crate) modified: SystemTime,
}

/// Decide which external index DBs to evict to get back under `cap`.
///
/// Pure and filesystem-free so the LRU + safety logic is unit-testable. Given
/// every on-disk external DB (`candidates`) and the set of currently-registered
/// (live) volume ids, returns the ones to evict, oldest-mtime first.
///
/// SAFETY: a candidate whose `volume_id` is in `registered` is dropped before
/// any eviction decision, so a live volume's DB is never returned no matter how
/// old its mtime. `root` is assumed already excluded by the caller's enumeration
/// (it's not an external DB), but we defensively skip it here too.
///
/// TODO(retention): if abandoned-drive accumulation ever proves to need a real
/// budget, replace the count cap with a total-bytes cap and/or an access-time
/// LRU (touch on read, not just write). The COUNT cap is the simple, safe v1.
pub(crate) fn select_evictions<'a>(
    candidates: &'a [IndexDbFile],
    registered: &[String],
    cap: usize,
) -> Vec<&'a IndexDbFile> {
    // Offline candidates only: a registered (live) volume's DB is never evicted.
    let mut offline: Vec<&IndexDbFile> = candidates
        .iter()
        .filter(|c| c.volume_id != ROOT_VOLUME_ID && !registered.iter().any(|r| r == &c.volume_id))
        .collect();

    // Total kept = live (registered, non-root, on-disk) + offline. We can only
    // shed offline ones, so evict down to `cap` total where possible. Count the
    // on-disk live externals toward the cap so a machine pinned at the cap by
    // live volumes simply evicts every offline DB (the safe outcome).
    let live_on_disk = candidates
        .iter()
        .filter(|c| c.volume_id != ROOT_VOLUME_ID && registered.iter().any(|r| r == &c.volume_id))
        .count();

    let total = live_on_disk + offline.len();
    if total <= cap {
        return Vec::new();
    }
    let to_evict = total - cap;

    // Oldest first (LRU): least-recently-modified DB is the least-recently-used.
    offline.sort_by_key(|c| c.modified);
    offline.into_iter().take(to_evict).collect()
}

/// Enumerate every `index-*.db` in `drive_index_dir` (excluding `root`), pairing each
/// with its mtime. Skips entries we can't stat (logged) and non-index files.
fn enumerate_external_index_dbs(drive_index_dir: &Path) -> Vec<IndexDbFile> {
    let mut dbs = enumerate_index_dbs(drive_index_dir);
    dbs.retain(|db| db.volume_id != ROOT_VOLUME_ID);
    dbs
}

/// Enumerate every `index-*.db` in `drive_index_dir`, `root` included, pairing each with
/// its mtime. Skips entries we can't stat (logged) and non-index files.
fn enumerate_index_dbs(drive_index_dir: &Path) -> Vec<IndexDbFile> {
    let read_dir = match std::fs::read_dir(drive_index_dir) {
        Ok(rd) => rd,
        Err(e) => {
            log::warn!(target: "indexing::retention", "cannot read {}: {e}", drive_index_dir.display());
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(volume_id) = VolumeStore::Index.volume_id_of(file_name) else {
            continue;
        };
        let modified = match entry.metadata().and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(e) => {
                log::warn!(target: "indexing::retention", "cannot stat {}: {e}", path.display());
                // Treat un-stattable as epoch (most-evictable) rather than skip,
                // so a broken file can still be reclaimed.
                SystemTime::UNIX_EPOCH
            }
        };
        out.push(IndexDbFile {
            volume_id: volume_id.to_string(),
            path,
            modified,
        });
    }
    out
}

/// How many bytes forgetting every volume would give back right now: every index
/// database on disk and the stores that go with one, `root` included and WAL/SHM
/// sidecars counted, whether or not the volume has a live instance.
///
/// The registry can't answer this: a database only a search's walk ever wrote is
/// on disk with nothing registered for it the moment the app restarts, and so is
/// a drive's index the user turned indexing off for. That's exactly the disk the
/// settings screen has to be able to show and reclaim, so this reads the files
/// rather than the pool. It is the same set [`volume_ids_on_disk`] hands the
/// clear, so the number shown is the number a clear takes to zero. Best-effort: a
/// file it can't stat counts as zero instead of failing the whole answer.
pub(crate) fn total_index_db_bytes() -> u64 {
    let Some(dirs) = dirs_for_sweep("measure the index's disk use") else {
        return 0;
    };
    volume_files::volume_ids_on_disk(&dirs, Removal::Forgotten)
        .iter()
        .map(|volume_id| volume_files::bytes_on_disk(&dirs, volume_id, Removal::Forgotten))
        .sum()
}

/// The id of every volume a "clear everything" sweep would take something from,
/// `root` included and in no particular order. What the sweep needs on top of
/// the registry, which only knows the volumes that are live right now.
pub(crate) fn volume_ids_on_disk() -> Vec<String> {
    let Some(dirs) = dirs_for_sweep("list the index databases on disk") else {
        return Vec::new();
    };
    volume_files::volume_ids_on_disk(&dirs, Removal::Forgotten)
}

/// The stores' folders, or `None` with one log line naming what couldn't be done.
fn dirs_for_sweep(what: &str) -> Option<StoreDirs> {
    match StoreDirs::configured() {
        Ok(dirs) => Some(dirs),
        Err(e) => {
            log::warn!(target: "indexing::retention", "cannot resolve the index's folders to {what}: {e}");
            None
        }
    }
}

/// Enforce the external-index-DB cap: evict the least-recently-used OFFLINE
/// (not currently registered) external index DBs until back under
/// [`MAX_EXTERNAL_INDEX_DBS`]. A no-op when under the cap. Logs what it evicts.
///
/// Call after enabling a new external (SMB/MTP) index, so the cap is checked
/// exactly when accumulation can grow. Never evicts a live volume's DB (see the
/// module safety invariants) nor `root`.
pub(crate) fn enforce_external_index_cap() {
    let Some(dirs) = dirs_for_sweep("enforce the index-database cap") else {
        return;
    };
    evict_over_cap(&dirs, &state::all_registered_volume_ids(), MAX_EXTERNAL_INDEX_DBS);
}

/// [`enforce_external_index_cap`] over a data dir, a registry snapshot, and a cap
/// passed in, so a test can drive an eviction without 33 databases.
fn evict_over_cap(dirs: &StoreDirs, registered: &[String], cap: usize) {
    let candidates = enumerate_external_index_dbs(&dirs.drive_index);
    let evictions = select_evictions(&candidates, registered, cap);

    if evictions.is_empty() {
        return;
    }
    log::info!(
        target: "indexing::retention",
        "external index DB cap ({cap}) exceeded; evicting {} least-recently-used offline index DB(s)",
        evictions.len()
    );
    for db in evictions {
        log::info!(target: "indexing::retention", "evicting abandoned index DB {}", db.path.display());
        if let Err(e) = volume_files::remove(dirs, &db.volume_id, Removal::Forgotten) {
            log::warn!(target: "indexing::retention", "evicting '{}' left files behind: {e}", db.volume_id);
        }
    }
}

/// Delete every store's files keyed by a volume ID from the retired ID scheme.
///
/// Volume IDs are now identity-keyed (`cmdr_fs::volume::ids`), so a database
/// named by an ID of the old shape can never be opened again: nothing mints
/// those IDs, so nothing will ever look one up. Left alone they'd sit in the data
/// dir until the LRU cap happened to reach them, which for a user under the cap
/// is never. The ids are read from every store's files, so a sibling whose index
/// an earlier forget already took is found too.
///
/// Runs once per launch, from `Index::start_root_at_launch`, off-thread. Safe to
/// fail: these are disposable caches, so a delete that doesn't work out is a log
/// line rather than an error path.
///
/// Carries the same safety invariant as the eviction path even though a legacy ID
/// shouldn't be able to be live (nothing can mint one): a registered volume's
/// database is never deleted, whatever its ID looks like.
pub(crate) fn sweep_legacy_scheme_dbs() {
    let Some(dirs) = dirs_for_sweep("sweep index databases from the retired ID scheme") else {
        return;
    };
    sweep_legacy_scheme_dbs_in(&dirs, &state::all_registered_volume_ids());
}

/// [`sweep_legacy_scheme_dbs`] over a data dir and a registry snapshot passed in.
fn sweep_legacy_scheme_dbs_in(dirs: &StoreDirs, registered: &[String]) {
    let stale: Vec<String> = volume_files::volume_ids_on_disk(dirs, Removal::Unreachable)
        .into_iter()
        .filter(|volume_id| cmdr_fs::volume::is_legacy_volume_id(volume_id))
        .filter(|volume_id| !registered.iter().any(|live| live == volume_id))
        .collect();
    if stale.is_empty() {
        return;
    }
    log::info!(
        target: "indexing::retention",
        "deleting the databases of {} stranded by the switch to identity-keyed volume IDs",
        cmdr_fs::pluralize::pluralize(stale.len() as u64, "volume")
    );
    for volume_id in stale {
        if let Err(e) = volume_files::remove(dirs, &volume_id, Removal::Unreachable) {
            log::warn!(target: "indexing::retention", "sweeping '{volume_id}' left files behind: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn db(volume_id: &str, mtime_secs: u64) -> IndexDbFile {
        IndexDbFile {
            volume_id: volume_id.to_string(),
            path: PathBuf::from(format!("/data/index-{volume_id}.db")),
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(mtime_secs),
        }
    }

    /// The paths of what a selection evicts, in its order.
    fn paths(evicted: &[&IndexDbFile]) -> Vec<PathBuf> {
        evicted.iter().map(|db| db.path.clone()).collect()
    }

    #[test]
    fn under_cap_evicts_nothing() {
        let candidates = vec![db("smb-a", 1), db("smb-b", 2)];
        assert!(select_evictions(&candidates, &[], 32).is_empty());
    }

    #[test]
    fn over_cap_evicts_oldest_offline_first() {
        // cap = 2, three offline DBs → evict the single oldest (smb-old).
        let candidates = vec![db("smb-new", 300), db("smb-old", 100), db("smb-mid", 200)];
        let evicted = paths(&select_evictions(&candidates, &[], 2));
        assert_eq!(evicted, vec![PathBuf::from("/data/index-smb-old.db")]);
    }

    #[test]
    fn never_evicts_a_registered_live_volume() {
        // smb-live is the oldest BUT registered → must never be evicted even
        // though by mtime it's the LRU. cap=1, so we still need to shed one;
        // the oldest *offline* one (smb-old) goes instead.
        let candidates = vec![db("smb-live", 1), db("smb-old", 2), db("smb-new", 3)];
        let registered = vec!["smb-live".to_string()];
        let evicted = paths(&select_evictions(&candidates, &registered, 1));
        assert!(
            !evicted.contains(&PathBuf::from("/data/index-smb-live.db")),
            "a live volume's DB must never be evicted"
        );
        // total on disk = 3 (1 live + 2 offline), cap 1 → evict 2 offline ones.
        assert_eq!(
            evicted,
            vec![
                PathBuf::from("/data/index-smb-old.db"),
                PathBuf::from("/data/index-smb-new.db"),
            ]
        );
    }

    #[test]
    fn never_evicts_root() {
        // root is excluded from candidates by enumeration, but defend in the
        // pure selector too: even if root slips in, it's never evicted.
        let candidates = vec![db("root", 1), db("smb-a", 2), db("smb-b", 3)];
        let evicted = paths(&select_evictions(&candidates, &[], 1));
        assert!(!evicted.iter().any(|p| p.to_string_lossy().contains("index-root.db")));
    }

    #[test]
    fn all_offline_evicted_when_live_volumes_fill_the_cap() {
        // 2 live externals already meet cap=2; every offline DB is then evicted.
        let candidates = vec![db("smb-live1", 10), db("smb-live2", 11), db("smb-cold", 1)];
        let registered = vec!["smb-live1".to_string(), "smb-live2".to_string()];
        let evicted = paths(&select_evictions(&candidates, &registered, 2));
        assert_eq!(evicted, vec![PathBuf::from("/data/index-smb-cold.db")]);
    }

    /// Write `name` into `dir` with its mtime `age_secs` in the past, so the LRU
    /// has an order to evict in.
    fn write_aged(dir: &Path, name: &str, age_secs: u64) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).expect("create");
        file.set_modified(SystemTime::now() - Duration::from_secs(age_secs))
            .expect("set mtime");
        path
    }

    /// The cap evicts a share's INDEX because nobody has used the share in a long
    /// time, and what it kept beside that index is just as abandoned. Pre-fix the
    /// eviction unlinked `index-{id}.db` alone, so the folder-importance database
    /// (tens of MB on a big share) outlived it with nothing left to collect it.
    #[test]
    fn evicting_a_share_takes_its_importance_database_with_its_index() {
        let dir = tempfile::tempdir().expect("temp dir");
        let old_index = write_aged(dir.path(), "index-smb-old.db", 300);
        let old_files = [
            "index-smb-old.db-wal",
            "importance-smb-old.db",
            "importance-smb-old.db-wal",
            "importance-smb-old.db-shm",
        ]
        .map(|name| write_aged(dir.path(), name, 300));
        let kept = ["index-smb-new.db", "importance-smb-new.db"].map(|name| write_aged(dir.path(), name, 10));

        evict_over_cap(&StoreDirs::single(dir.path()), &[], 1);

        assert!(!old_index.exists(), "the least recently used index is evicted");
        for file in &old_files {
            assert!(!file.exists(), "{} must go with it", file.display());
        }
        for file in &kept {
            assert!(file.exists(), "{} is under the cap and stays", file.display());
        }
    }

    /// A volume ID from the retired scheme keys state nothing can reach again, in
    /// EVERY store: the sweep takes the index, the importance database, the media
    /// database, and the vector index the media database carries beside it.
    /// Pre-fix the vector index stayed (it was never in the list), and so did a
    /// sibling whose index had already gone (the sweep only looked for indexes).
    #[test]
    fn the_legacy_sweep_takes_every_file_a_retired_id_keys() {
        const LEGACY: &str = "smb-naspolya-445-naspi";
        const ORPHANED: &str = "smb-oldnas-445-photos";
        const CURRENT: &str = "smb-naspolya-445-naspi-cca75cc9c09a6fb5";
        assert!(cmdr_fs::volume::is_legacy_volume_id(LEGACY) && cmdr_fs::volume::is_legacy_volume_id(ORPHANED));
        assert!(!cmdr_fs::volume::is_legacy_volume_id(CURRENT));

        let dir = tempfile::tempdir().expect("temp dir");
        let write = |name: String| {
            let path = dir.path().join(name);
            std::fs::write(&path, [0u8; 8]).expect("write");
            path
        };
        let swept = [
            format!("index-{LEGACY}.db"),
            format!("index-{LEGACY}.db-wal"),
            format!("importance-{LEGACY}.db"),
            format!("media-{LEGACY}.db"),
            format!("media-{LEGACY}.db-shm"),
            format!("media-{LEGACY}.clip.usearch"),
            format!("media-{LEGACY}.clip.usearch.meta"),
            format!("media-{LEGACY}.clip.usearch.dirty"),
            // No index beside these: an earlier forget already took it.
            format!("importance-{ORPHANED}.db"),
            format!("media-{ORPHANED}.clip.usearch"),
        ]
        .map(write);
        let kept = [
            format!("index-{CURRENT}.db"),
            format!("importance-{CURRENT}.db"),
            format!("media-{CURRENT}.db"),
            format!("media-{CURRENT}.clip.usearch"),
            "index-root.db".to_string(),
            "importance-root.db".to_string(),
            "operation-log.db".to_string(),
        ]
        .map(write);

        sweep_legacy_scheme_dbs_in(&StoreDirs::single(dir.path()), &[]);

        for file in &swept {
            assert!(
                !file.exists(),
                "{} is keyed by a retired id and must go",
                file.display()
            );
        }
        for file in &kept {
            assert!(file.exists(), "{} must stay", file.display());
        }
    }

    /// The settings screen's two questions, over files rather than the registry:
    /// how much disk is this, and which volumes is it. Both have to answer for a
    /// database nothing has registered — the shape a search's walk leaves behind
    /// on a machine that indexes nothing.
    #[test]
    fn the_footprint_counts_every_database_and_its_sidecars() {
        let _lock = crate::indexing::handle::test_lock();
        let dir = tempfile::tempdir().expect("temp dir");
        let _config = crate::indexing::host::config::install_data_dir_for_test(dir.path());

        std::fs::write(dir.path().join("index-root.db"), vec![0u8; 1000]).expect("write root db");
        std::fs::write(dir.path().join("index-root.db-wal"), vec![0u8; 500]).expect("write root wal");
        std::fs::write(dir.path().join("index-smb-nas.db"), vec![0u8; 300]).expect("write share db");
        // Not an index database, and never counted as one.
        std::fs::write(dir.path().join("history.db"), vec![0u8; 9999]).expect("write other db");

        assert_eq!(total_index_db_bytes(), 1800, "main + WAL, across every volume");

        let mut ids = volume_ids_on_disk();
        ids.sort();
        assert_eq!(ids, vec!["root".to_string(), "smb-nas".to_string()]);
    }
}
