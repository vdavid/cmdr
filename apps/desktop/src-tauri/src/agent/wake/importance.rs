//! How much the folder a rollup describes matters, cached.
//!
//! ⚠️ **This runs on the WRITER thread, after the channel — ❌ never in `route()`.** `lookup` is
//! SQLite behind a shared cache, and `route()` runs on the live loop, which may touch neither a
//! lock nor a database.
//!
//! `ImportanceIndex::open` is already cheap (the connection is lazy and thread-local), so the
//! cost worth avoiding is the per-folder `lookup`, not the open. Folders repeat heavily across
//! batches, so a small map with a short TTL removes almost all of them, and a stale weight only
//! misprices one wake.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use cmdr_index::importance::{ImportanceIndex, WeightLookup};

use super::FolderImportance;

/// How long a looked-up weight stays good. A guess in the same class as the interest
/// thresholds: long enough that a burst in one folder costs one lookup, short enough that a
/// freshly scored folder starts counting within a minute.
const TTL: Duration = Duration::from_secs(60);

/// How many folders the cache remembers. Past this it is emptied rather than evicted one by
/// one: the entries expire together anyway, and an LRU here would be machinery for a map that
/// exists to skip a cheap query.
const MAX_ENTRIES: usize = 1_024;

const LOG_TARGET: &str = "agent::wake";

/// A per-volume, per-folder importance lookup with a short TTL. Owned by the writer thread, so
/// it needs no lock of its own.
pub struct ImportanceCache {
    data_dir: PathBuf,
    entries: HashMap<(String, String), (FolderImportance, Instant)>,
}

impl ImportanceCache {
    pub fn new(data_dir: PathBuf) -> Self {
        ImportanceCache {
            data_dir,
            entries: HashMap::new(),
        }
    }

    /// What the importance scorer says about `folder` on `volume_id`, from the cache when it
    /// can be and from the index otherwise.
    pub fn lookup(&mut self, volume_id: &str, folder: &str, now: Instant) -> FolderImportance {
        let key = (volume_id.to_string(), folder.to_string());
        if let Some((importance, at)) = self.entries.get(&key)
            && now.duration_since(*at) < TTL
        {
            return *importance;
        }
        let importance = read_importance(&self.data_dir, volume_id, folder);
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.clear();
        }
        self.entries.insert(key, (importance, now));
        importance
    }
}

/// One uncached lookup.
///
/// ⚠️ Opened with `available_for(volume_id)`, ❌ not `SignalSet::all()`: a network volume
/// degrades its signal set, and reading it with the full mask would redistribute the weights
/// differently from the recompute that wrote them.
fn read_importance(data_dir: &std::path::Path, volume_id: &str, folder: &str) -> FolderImportance {
    let index = ImportanceIndex::open(data_dir, volume_id, crate::mcp::resources::importance::available_for(volume_id));
    match index.lookup(folder) {
        Ok(lookup) => from_weight_lookup(&lookup),
        Err(e) => {
            log::debug!(target: LOG_TARGET, "importance lookup for a wake rollup failed: {e}");
            // Unknown, not floored: an unreadable index says nothing about the folder, and
            // floored would silently mute a folder the scorer may rate highly.
            FolderImportance::Unknown
        }
    }
}

/// Map the index's answer variant for variant.
///
/// ⚠️ ❌ **Never through `score()`**, which collapses `Floored` and `Unscored` into the same
/// `0.0`. That collapse is right for ranking folders and wrong here: a project cloned five
/// minutes ago would then rank exactly like `node_modules`, and the agent would ignore every
/// new folder on the disk.
fn from_weight_lookup(lookup: &WeightLookup) -> FolderImportance {
    match lookup {
        WeightLookup::Scored(weight) => FolderImportance::Scored(weight.score.value()),
        WeightLookup::Floored(_) => FolderImportance::Floored,
        WeightLookup::Unscored => FolderImportance::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠️ The mapping that must not go through `score()`. `Unscored` is a folder the scorer has
    /// not REACHED; zero is a folder it looked at and rated junk, and the two want opposite
    /// behaviour from the agent.
    #[test]
    fn an_unscored_folder_is_unknown_rather_than_zero() {
        assert_eq!(from_weight_lookup(&WeightLookup::Unscored), FolderImportance::Unknown);
        assert_eq!(
            from_weight_lookup(&WeightLookup::Floored(
                cmdr_index::importance::FloorReason::NameDenylisted
            )),
            FolderImportance::Floored,
            "and a denylisted folder stays distinguishable from an unreached one"
        );
    }

    /// A folder that repeats across a burst of batches costs one lookup, not one per batch.
    #[test]
    fn a_repeated_folder_is_answered_from_the_cache_inside_the_ttl() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut cache = ImportanceCache::new(dir.path().to_path_buf());
        let now = Instant::now();

        let first = cache.lookup("root", "/Users/someone/Downloads", now);
        let again = cache.lookup("root", "/Users/someone/Downloads", now + TTL / 2);

        assert_eq!(first, again);
        assert_eq!(cache.entries.len(), 1, "one entry, however many times it was asked");
    }

    /// Past the TTL the answer is taken fresh, so a volume that finishes scanning starts
    /// counting within the minute rather than at the next restart.
    #[test]
    fn an_expired_entry_is_looked_up_again() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut cache = ImportanceCache::new(dir.path().to_path_buf());
        let now = Instant::now();

        cache.lookup("root", "/Users/someone/Downloads", now);
        let stamped_at = cache
            .entries
            .get(&("root".to_string(), "/Users/someone/Downloads".to_string()))
            .expect("cached")
            .1;
        cache.lookup("root", "/Users/someone/Downloads", now + TTL + Duration::from_secs(1));
        let restamped_at = cache
            .entries
            .get(&("root".to_string(), "/Users/someone/Downloads".to_string()))
            .expect("cached")
            .1;

        assert!(restamped_at > stamped_at, "the entry was refreshed, not served stale");
    }
}
