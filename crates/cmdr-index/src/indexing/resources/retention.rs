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
//!   volumes with no registry instance, so there's no writer to drain and the
//!   delete is a plain unlink of the DB + WAL/SHM sidecars (mirrors the file
//!   deletion in `state::clear_index`).
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

/// Parse the volume id out of an `index-{volume_id}.db` filename. Returns `None`
/// for anything that isn't an index DB (so WAL/SHM sidecars and unrelated files
/// are ignored). A volume id may itself contain `-` (e.g. an MTP serial), so we
/// strip the fixed `index-` prefix and `.db` suffix rather than splitting.
pub(crate) fn volume_id_from_db_filename(file_name: &str) -> Option<&str> {
    file_name.strip_prefix("index-")?.strip_suffix(".db")
}

/// Decide which external index DBs to evict to get back under `cap`.
///
/// Pure and filesystem-free so the LRU + safety logic is unit-testable. Given
/// every on-disk external DB (`candidates`) and the set of currently-registered
/// (live) volume ids, returns the paths to delete, oldest-mtime first.
///
/// SAFETY: a candidate whose `volume_id` is in `registered` is dropped before
/// any eviction decision, so a live volume's DB is never returned no matter how
/// old its mtime. `root` is assumed already excluded by the caller's enumeration
/// (it's not an external DB), but we defensively skip it here too.
///
/// TODO(retention): if abandoned-drive accumulation ever proves to need a real
/// budget, replace the count cap with a total-bytes cap and/or an access-time
/// LRU (touch on read, not just write). The COUNT cap is the simple, safe v1.
pub(crate) fn select_evictions(candidates: &[IndexDbFile], registered: &[String], cap: usize) -> Vec<PathBuf> {
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
    offline.into_iter().take(to_evict).map(|c| c.path.clone()).collect()
}

/// Delete an index DB and its WAL/SHM sidecars from disk. Mirrors the sidecar
/// deletion in `state::clear_index`; used by eviction (the volume has no live
/// instance, so there's nothing to drain first). Best-effort: a missing sidecar
/// is fine, a failed delete is logged but doesn't abort the sweep.
fn delete_index_db_files(db_path: &Path) {
    for path in [
        db_path.to_path_buf(),
        db_path.with_extension("db-wal"),
        db_path.with_extension("db-shm"),
    ] {
        if path.exists()
            && let Err(e) = std::fs::remove_file(&path)
        {
            log::warn!(
                target: "indexing::retention",
                "failed to delete evicted index file {}: {e}",
                path.display()
            );
        }
    }
}

/// Enumerate every `index-*.db` in `data_dir` (excluding `root`), pairing each
/// with its mtime. Skips entries we can't stat (logged) and non-index files.
fn enumerate_external_index_dbs(data_dir: &Path) -> Vec<IndexDbFile> {
    let mut dbs = enumerate_index_dbs(data_dir);
    dbs.retain(|db| db.volume_id != ROOT_VOLUME_ID);
    dbs
}

/// Enumerate every `index-*.db` in `data_dir`, `root` included, pairing each with
/// its mtime. Skips entries we can't stat (logged) and non-index files.
fn enumerate_index_dbs(data_dir: &Path) -> Vec<IndexDbFile> {
    let read_dir = match std::fs::read_dir(data_dir) {
        Ok(rd) => rd,
        Err(e) => {
            log::warn!(target: "indexing::retention", "cannot read data dir {}: {e}", data_dir.display());
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(volume_id) = volume_id_from_db_filename(file_name) else {
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

/// How many bytes every index database on disk occupies right now, `root`
/// included and WAL/SHM sidecars counted, whether or not the volume has a live
/// instance.
///
/// The registry can't answer this: a database only a search's walk ever wrote is
/// on disk with nothing registered for it the moment the app restarts, and so is
/// a drive's index the user turned indexing off for. That's exactly the disk the
/// settings screen has to be able to show and reclaim, so this reads the files
/// rather than the pool. Best-effort: a database it can't stat counts as zero
/// instead of failing the whole answer.
pub(crate) fn total_index_db_bytes() -> u64 {
    let Some(data_dir) = data_dir_for_sweep("measure the index's disk use") else {
        return 0;
    };
    enumerate_index_dbs(&data_dir)
        .iter()
        .map(|db| index_db_bytes(&db.path))
        .sum()
}

/// The volume id of every index database on disk, `root` included and in no
/// particular order. What a "clear everything" sweep needs on top of the
/// registry, which only knows the volumes that are live right now.
pub(crate) fn volume_ids_on_disk() -> Vec<String> {
    let Some(data_dir) = data_dir_for_sweep("list the index databases on disk") else {
        return Vec::new();
    };
    enumerate_index_dbs(&data_dir)
        .into_iter()
        .map(|db| db.volume_id)
        .collect()
}

/// One index database's bytes on disk, sidecars included. Mirrors
/// `IndexStore::db_file_size`, which answers the same question for a database
/// that happens to be open.
fn index_db_bytes(db_path: &Path) -> u64 {
    [
        db_path.to_path_buf(),
        db_path.with_extension("db-wal"),
        db_path.with_extension("db-shm"),
    ]
    .iter()
    .map(|path| std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
    .sum()
}

/// The data dir, or `None` with one log line naming what couldn't be done.
fn data_dir_for_sweep(what: &str) -> Option<PathBuf> {
    match crate::indexing::host::config::data_dir() {
        Ok(dir) => Some(dir),
        Err(e) => {
            log::warn!(target: "indexing::retention", "cannot resolve the data dir to {what}: {e}");
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
    let Some(data_dir) = data_dir_for_sweep("enforce the index-database cap") else {
        return;
    };
    let candidates = enumerate_external_index_dbs(&data_dir);
    let registered = state::all_registered_volume_ids();
    let evictions = select_evictions(&candidates, &registered, MAX_EXTERNAL_INDEX_DBS);

    if evictions.is_empty() {
        return;
    }
    log::info!(
        target: "indexing::retention",
        "external index DB cap ({MAX_EXTERNAL_INDEX_DBS}) exceeded; evicting {} least-recently-used offline index DB(s)",
        evictions.len()
    );
    for path in evictions {
        log::info!(target: "indexing::retention", "evicting abandoned index DB {}", path.display());
        delete_index_db_files(&path);
    }
}

/// Delete the index databases keyed by a volume ID from the retired ID scheme.
///
/// Volume IDs are now identity-keyed (`cmdr_fs::volume::ids`), so a database
/// named by an ID of the old shape can never be opened again: nothing mints
/// those IDs, so nothing will ever look one up. Left alone they'd sit in the data
/// dir until the LRU cap happened to reach them, which for a user under the cap
/// is never.
///
/// Safe to run at any time and safe to fail: a legacy ID can't be live (nothing
/// can produce one), and these databases are disposable caches. Best-effort by
/// design, so a delete that doesn't work out is a log line, not an error path.
pub fn sweep_legacy_scheme_dbs() {
    let Some(data_dir) = data_dir_for_sweep("sweep index databases from the retired ID scheme") else {
        return;
    };
    let stale: Vec<IndexDbFile> = enumerate_index_dbs(&data_dir)
        .into_iter()
        .filter(|db| cmdr_fs::volume::is_legacy_volume_id(&db.volume_id))
        .collect();
    if stale.is_empty() {
        return;
    }
    log::info!(
        target: "indexing::retention",
        "deleting {} index database(s) stranded by the switch to identity-keyed volume IDs",
        stale.len()
    );
    for db in stale {
        // The two sidecar caches are named after the same volume ID and are just
        // as unreachable, so they go together (`importance/store`, `media_index/store`).
        delete_index_db_files(&db.path);
        delete_index_db_files(&crate::importance::store::importance_db_path(&data_dir, &db.volume_id));
        delete_index_db_files(&crate::media_index::store::media_db_path(&data_dir, &db.volume_id));
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

    #[test]
    fn parses_volume_id_from_filename() {
        assert_eq!(volume_id_from_db_filename("index-root.db"), Some("root"));
        assert_eq!(volume_id_from_db_filename("index-smb-nas.db"), Some("smb-nas"));
        // A volume id containing '-' (MTP serial) survives prefix/suffix strip.
        assert_eq!(
            volume_id_from_db_filename("index-mtp-AABBCC-1.db"),
            Some("mtp-AABBCC-1")
        );
        // Non-index files and sidecars are ignored.
        assert_eq!(volume_id_from_db_filename("index-root.db-wal"), None);
        assert_eq!(volume_id_from_db_filename("history.db"), None);
        assert_eq!(volume_id_from_db_filename("index-.db"), Some(""));
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
        let evicted = select_evictions(&candidates, &[], 2);
        assert_eq!(evicted, vec![PathBuf::from("/data/index-smb-old.db")]);
    }

    #[test]
    fn never_evicts_a_registered_live_volume() {
        // smb-live is the oldest BUT registered → must never be evicted even
        // though by mtime it's the LRU. cap=1, so we still need to shed one;
        // the oldest *offline* one (smb-old) goes instead.
        let candidates = vec![db("smb-live", 1), db("smb-old", 2), db("smb-new", 3)];
        let registered = vec!["smb-live".to_string()];
        let evicted = select_evictions(&candidates, &registered, 1);
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
        let evicted = select_evictions(&candidates, &[], 1);
        assert!(!evicted.iter().any(|p| p.to_string_lossy().contains("index-root.db")));
    }

    #[test]
    fn all_offline_evicted_when_live_volumes_fill_the_cap() {
        // 2 live externals already meet cap=2; every offline DB is then evicted.
        let candidates = vec![db("smb-live1", 10), db("smb-live2", 11), db("smb-cold", 1)];
        let registered = vec!["smb-live1".to_string(), "smb-live2".to_string()];
        let evicted = select_evictions(&candidates, &registered, 2);
        assert_eq!(evicted, vec![PathBuf::from("/data/index-smb-cold.db")]);
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
