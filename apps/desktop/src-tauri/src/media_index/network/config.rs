//! The durable-enough, live-applied network-enrichment policy state: the per-volume
//! SMB opt-in, the "always index" overrides, and the transient paused-volume set.
//!
//! **Why a settings-seeded global, not a fourth per-volume store.** The opt-in and
//! the overrides are user config (a handful of volumes/folders), not per-image data,
//! so they ride the existing sparse settings store (`mediaIndex.*` keys, FE-owned)
//! rather than a new SQLite DB with its own writer thread. `media_index::scheduler`
//! runs off the IPC thread and consults this on every pass, so the values live in a
//! process-global seeded from `load_settings` at startup and live-applied through the
//! `media_index_set_*` commands (the standard backend-affecting-setting pattern). The
//! paused set is purely runtime (a disconnect marker), never persisted.

use std::collections::HashSet;
use std::sync::LazyLock;
use std::sync::RwLock;

use crate::ignore_poison::RwLockIgnorePoison;

/// The user-set network-enrichment policy: which volumes are opted into background
/// SMB enrichment, and which volumes/folders are "always index" overrides.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NetworkEnrichConfig {
    /// Volume ids the user opted into background network enrichment (off by default;
    /// turning on the master toggle does NOT auto-enrich network volumes).
    pub opted_in_volumes: HashSet<String>,
    /// Volume ids marked "always index": enrich regardless of the importance
    /// threshold (a NAS archive scores low on navigation-based importance, so without
    /// this its photos defer forever — plan Decision 6).
    pub always_index_volumes: HashSet<String>,
    /// Absolute folder paths (OS-mount form, e.g. `/Volumes/naspi/Photos`) marked
    /// "always index": every image at or under one enriches regardless of importance.
    pub always_index_folders: HashSet<String>,
    /// Absolute folder paths the user EXCLUDED from photo-search indexing (the privacy
    /// complement to the opt-in — a sensitive high-importance folder like
    /// `~/Documents/IDs`). A hard veto: an image at or under an excluded folder never
    /// enriches, even under an "always index" override. Plan M2 § Privacy.
    pub excluded_folders: HashSet<String>,
}

impl NetworkEnrichConfig {
    /// Whether `volume_id` is opted into background network enrichment.
    pub fn is_opted_in(&self, volume_id: &str) -> bool {
        self.opted_in_volumes.contains(volume_id)
    }

    /// Whether an image at OS path `os_path` on `volume_id` is covered by an
    /// "always index" override (the whole volume, or an ancestor folder).
    pub fn covers(&self, volume_id: &str, os_path: &str) -> bool {
        self.always_index_volumes.contains(volume_id)
            || self.always_index_folders.iter().any(|f| path_is_within(os_path, f))
    }

    /// Whether an image at OS path `os_path` is under a user-excluded folder — a hard
    /// veto that beats any "always index" override (the privacy complement).
    pub fn is_excluded(&self, os_path: &str) -> bool {
        self.excluded_folders.iter().any(|f| path_is_within(os_path, f))
    }
}

/// Whether `path` is `ancestor` itself or lives under it. Pure path-prefix arithmetic
/// (a trailing-slash-safe prefix, so `/Photos2` isn't "within" `/Photos`).
pub fn path_is_within(path: &str, ancestor: &str) -> bool {
    let ancestor = ancestor.trim_end_matches('/');
    if ancestor.is_empty() {
        return true; // "/" (or empty) is an ancestor of everything
    }
    path == ancestor || path.strip_prefix(ancestor).is_some_and(|rest| rest.starts_with('/'))
}

/// The process-global config, seeded from settings at startup and live-applied.
static CONFIG: LazyLock<RwLock<NetworkEnrichConfig>> = LazyLock::new(|| RwLock::new(NetworkEnrichConfig::default()));

/// Volume ids whose enrichment paused on a disconnect (unmount). Purely runtime: a
/// paused volume keeps every completed row and resumes on reconnect (never GC'd, never
/// marked failed — that's the disconnect data-safety line).
static PAUSED: LazyLock<RwLock<HashSet<String>>> = LazyLock::new(|| RwLock::new(HashSet::new()));

/// Replace the whole config (startup seed + live-apply of a bulk change).
pub fn set_config(config: NetworkEnrichConfig) {
    *CONFIG.write_ignore_poison() = config;
}

/// A snapshot of the current config (cheap clone; consulted once per pass).
pub fn snapshot() -> NetworkEnrichConfig {
    CONFIG.read_ignore_poison().clone()
}

/// Set or clear a volume's background-enrichment opt-in (live-applied).
pub fn set_opted_in(volume_id: &str, opted_in: bool) {
    let mut cfg = CONFIG.write_ignore_poison();
    if opted_in {
        cfg.opted_in_volumes.insert(volume_id.to_string());
    } else {
        cfg.opted_in_volumes.remove(volume_id);
    }
}

/// Whether `volume_id` is opted into background network enrichment.
pub fn is_opted_in(volume_id: &str) -> bool {
    CONFIG.read_ignore_poison().is_opted_in(volume_id)
}

/// Set or clear a whole-volume "always index" override (live-applied).
pub fn set_always_index_volume(volume_id: &str, always: bool) {
    let mut cfg = CONFIG.write_ignore_poison();
    if always {
        cfg.always_index_volumes.insert(volume_id.to_string());
    } else {
        cfg.always_index_volumes.remove(volume_id);
    }
}

/// Set or clear a folder "always index" override (live-applied). `folder` is an
/// absolute OS-mount path.
pub fn set_always_index_folder(folder: &str, always: bool) {
    let mut cfg = CONFIG.write_ignore_poison();
    if always {
        cfg.always_index_folders.insert(folder.to_string());
    } else {
        cfg.always_index_folders.remove(folder);
    }
}

/// Set or clear a folder photo-search EXCLUSION (live-applied). `folder` is an
/// absolute path; every image at or under it is skipped (privacy veto).
pub fn set_excluded_folder(folder: &str, excluded: bool) {
    let mut cfg = CONFIG.write_ignore_poison();
    if excluded {
        cfg.excluded_folders.insert(folder.to_string());
    } else {
        cfg.excluded_folders.remove(folder);
    }
}

/// Whether an image at OS path `os_path` on `volume_id` is override-covered.
pub fn covers_override(volume_id: &str, os_path: &str) -> bool {
    CONFIG.read_ignore_poison().covers(volume_id, os_path)
}

/// Whether an image at OS path `os_path` is under a user-excluded folder (a hard
/// privacy veto, beats any override).
pub fn is_excluded(os_path: &str) -> bool {
    CONFIG.read_ignore_poison().is_excluded(os_path)
}

/// Mark a volume paused (its enrichment stopped on a disconnect; resumes on reconnect).
pub fn mark_paused(volume_id: &str) {
    PAUSED.write_ignore_poison().insert(volume_id.to_string());
}

/// Clear a volume's paused marker (it reconnected / completed a pass).
pub fn clear_paused(volume_id: &str) {
    PAUSED.write_ignore_poison().remove(volume_id);
}

/// Whether a volume is currently paused (disconnected mid-enrichment).
pub fn is_paused(volume_id: &str) -> bool {
    PAUSED.read_ignore_poison().contains(volume_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_within_is_trailing_slash_safe() {
        assert!(path_is_within("/Volumes/naspi/Photos", "/Volumes/naspi/Photos"));
        assert!(path_is_within("/Volumes/naspi/Photos/a.jpg", "/Volumes/naspi/Photos"));
        assert!(path_is_within("/Volumes/naspi/Photos/a.jpg", "/Volumes/naspi/Photos/"));
        // A sibling that shares a name prefix is NOT within.
        assert!(!path_is_within("/Volumes/naspi/Photos2/a.jpg", "/Volumes/naspi/Photos"));
        assert!(!path_is_within("/Volumes/other/a.jpg", "/Volumes/naspi/Photos"));
    }

    #[test]
    fn covers_matches_whole_volume_or_ancestor_folder() {
        let cfg = NetworkEnrichConfig {
            opted_in_volumes: HashSet::new(),
            always_index_volumes: ["smb-vol".to_string()].into_iter().collect(),
            always_index_folders: ["/Volumes/naspi/Photos".to_string()].into_iter().collect(),
            excluded_folders: HashSet::new(),
        };
        // Whole-volume override.
        assert!(cfg.covers("smb-vol", "/anything/x.jpg"));
        // Folder override on a different volume.
        assert!(cfg.covers("other", "/Volumes/naspi/Photos/2026/x.jpg"));
        assert!(!cfg.covers("other", "/Volumes/naspi/Docs/x.jpg"));
    }
}
