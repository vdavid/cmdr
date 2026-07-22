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

use tauri::AppHandle;

use crate::indexing::state::ROOT_VOLUME_ID;

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
        if volume_id == ROOT_VOLUME_ID {
            continue;
        }
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

/// Enforce the external-index-DB cap: evict the least-recently-used OFFLINE
/// (not currently registered) external index DBs until back under
/// [`MAX_EXTERNAL_INDEX_DBS`]. A no-op when under the cap. Logs what it evicts.
///
/// Call after enabling a new external (SMB/MTP) index, so the cap is checked
/// exactly when accumulation can grow. Never evicts a live volume's DB (see the
/// module safety invariants) nor `root`.
pub(crate) fn enforce_external_index_cap(app: &AppHandle) {
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(d) => d,
        Err(e) => {
            log::warn!(target: "indexing::retention", "cannot resolve data dir for cap enforcement: {e}");
            return;
        }
    };
    let candidates = enumerate_external_index_dbs(&data_dir);
    let registered = crate::indexing::state::all_registered_volume_ids();
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
}
