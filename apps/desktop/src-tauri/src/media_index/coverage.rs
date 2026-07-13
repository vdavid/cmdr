//! The covered-count machinery behind the importance settings slider's live preview:
//! given a folder-importance threshold, how many folders and qualifying images
//! would be image-indexed on the ENABLED volumes.
//!
//! The qualifying-image count per folder comes from the drive index (the same
//! image-qualification predicate the scheduler enriches by), which is an O(entries)
//! walk — too heavy to run per slider-drag frame. So the per-folder counts are cached
//! per volume ([`FolderImageCounts`]) and invalidated when a pass runs (the only time
//! the qualifying set changes). The threshold is then applied cheaply: intersect the
//! importance `above_threshold` folder set with the cached counts. The importance read
//! itself is a single indexed query, so a debounced drag stays cheap.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

use crate::ignore_poison::IgnorePoison;

use super::scheduler::enrich::{parent_dir, walk_image_entries};

/// The qualifying-image counts for one volume: how many images each folder holds, and
/// the volume total. Built from one index walk, cached until the next pass.
#[derive(Debug, Default, Clone)]
pub struct FolderImageCounts {
    /// `folder path → qualifying image count` for every folder with at least one.
    pub per_folder: HashMap<String, u64>,
    /// The total qualifying images across the volume.
    pub total: u64,
}

/// The process-global per-volume counts cache.
static COUNTS: LazyLock<Mutex<HashMap<String, Arc<FolderImageCounts>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Get the cached folder image counts for `volume_id`, building them from the drive
/// index on first use (or after an [`invalidate`]). `None` when the volume's index
/// isn't registered (offline / never scanned) — the caller reports that volume as
/// still pending rather than counting a wrong number.
pub fn get_or_build(volume_id: &str) -> Option<Arc<FolderImageCounts>> {
    if let Some(counts) = COUNTS.lock_ignore_poison().get(volume_id) {
        return Some(Arc::clone(counts));
    }
    let pool = crate::indexing::get_read_pool_for(volume_id)?;
    let images = pool.with_conn(walk_image_entries).ok()?.ok()?;
    let mut per_folder: HashMap<String, u64> = HashMap::new();
    for image in &images {
        *per_folder.entry(parent_dir(&image.path).to_string()).or_default() += 1;
    }
    let counts = Arc::new(FolderImageCounts {
        total: images.len() as u64,
        per_folder,
    });
    COUNTS
        .lock_ignore_poison()
        .insert(volume_id.to_string(), Arc::clone(&counts));
    Some(counts)
}

/// Drop a volume's cached counts (its qualifying set may have changed after a pass /
/// a rescan). The next preview rebuilds them.
pub fn invalidate(volume_id: &str) {
    COUNTS.lock_ignore_poison().remove(volume_id);
}

/// The covered folder + image counts for ONE volume at `threshold`, given its cached
/// image counts and its importance folder scores. `folder_scores` is `Some(map)` of
/// `folder → score` (importance ≥ some floor); `None` means importance hasn't scored
/// this volume yet (the caller reports it pending). Pure, so the threshold arithmetic
/// is unit-testable without an index or importance DB.
pub fn covered_for_volume(
    counts: &FolderImageCounts,
    folder_scores: &HashMap<String, f64>,
    threshold: f64,
) -> (u64, u64) {
    let mut folders = 0u64;
    let mut images = 0u64;
    for (folder, score) in folder_scores {
        if *score >= threshold {
            folders += 1;
            images += counts.per_folder.get(folder).copied().unwrap_or(0);
        }
    }
    (folders, images)
}

/// Convenience: read a volume's importance folder scores as a `folder → score` map, or
/// `None` when importance never scored it (offline / fresh). Mirrors the scheduler's
/// `folder_scores`, but returns EVERY scored folder (threshold applied by
/// [`covered_for_volume`] so one read serves any slider position during a debounced
/// drag).
pub fn importance_scores(data_dir: &Path, volume_id: &str) -> Option<HashMap<String, f64>> {
    use crate::importance::{ImportanceIndex, SignalSet};
    let index = ImportanceIndex::open(data_dir, volume_id, SignalSet::all());
    if index.recompute_generation().unwrap_or(0) == 0 {
        return None;
    }
    match index.above_threshold(0.0) {
        Ok(weights) => Some(weights.into_iter().map(|w| (w.path, w.score.value())).collect()),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covered_counts_folders_and_images_above_threshold() {
        let counts = FolderImageCounts {
            per_folder: [
                ("/high".to_string(), 100u64),
                ("/mid".to_string(), 40),
                ("/low".to_string(), 5),
            ]
            .into_iter()
            .collect(),
            total: 145,
        };
        let scores: HashMap<String, f64> = [
            ("/high".to_string(), 0.9),
            ("/mid".to_string(), 0.5),
            ("/low".to_string(), 0.1),
        ]
        .into_iter()
        .collect();

        // Threshold 0.5: /high and /mid qualify ⇒ 2 folders, 140 images.
        assert_eq!(covered_for_volume(&counts, &scores, 0.5), (2, 140));
        // Threshold 0.95: nothing qualifies.
        assert_eq!(covered_for_volume(&counts, &scores, 0.95), (0, 0));
        // Threshold 0.0: all three ⇒ 3 folders, 145 images.
        assert_eq!(covered_for_volume(&counts, &scores, 0.0), (3, 145));
    }

    #[test]
    fn covered_ignores_a_scored_folder_with_no_qualifying_images() {
        // A folder importance scored but holding no images contributes a folder, zero
        // images (honest: the count never over-promises images).
        let counts = FolderImageCounts {
            per_folder: [("/photos".to_string(), 10u64)].into_iter().collect(),
            total: 10,
        };
        let scores: HashMap<String, f64> = [("/photos".to_string(), 0.8), ("/empty".to_string(), 0.8)]
            .into_iter()
            .collect();
        assert_eq!(covered_for_volume(&counts, &scores, 0.5), (2, 10));
    }
}
