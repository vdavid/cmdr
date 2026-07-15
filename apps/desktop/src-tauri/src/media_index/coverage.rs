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

/// The reclaim partition of a volume's STORED media rows: the set that SURVIVES the
/// current setting, and the DOOMED set a reclaim prune would delete.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StoredPartition {
    /// How many stored rows fall INSIDE current coverage (they stay).
    pub surviving: u64,
    /// The stored paths OUTSIDE current coverage — the reclaim prune's doomed set
    /// (also M5's `keptCount`, the same rows framed as "still searchable").
    pub doomed: Vec<String>,
}

/// Partition a volume's stored media rows into surviving vs doomed by the SAME
/// precedence enrichment uses, so the destructive reclaim selection can't drift from
/// what the pass would keep. Pure over its inputs (no DB, no importance store, no app),
/// so it's unit-testable directly.
///
/// A stored path SURVIVES when it would still be enriched at `threshold`: NOT under an
/// excluded folder (a hard privacy veto) AND (covered by an "always index" override OR
/// its parent folder scores at or above `threshold`). `folder_scores` holds every
/// scored folder (`folder → score`); a folder ABSENT from it counts as below any
/// threshold — floored junk, or a folder scored away since enrichment — so its rows are
/// DOOMED (matching the enrichment gate, which keys on score-map membership, never a
/// `>= 0.0` on a defaulted 0.0). Everything not surviving is doomed, so
/// `surviving + doomed.len()` is exactly the stored-row count: no row lands in neither
/// bucket (the partition invariant the reclaim arithmetic leans on).
///
/// `is_override` / `is_excluded` take the STORED (index-relative) path; the caller wires
/// the OS-mount mapping (identity on a local volume, mount-root join on a network one),
/// keeping this core pure and shared across both volume kinds.
pub fn partition_stored(
    stored_paths: &[String],
    folder_scores: &HashMap<String, f64>,
    threshold: f64,
    is_override: &dyn Fn(&str) -> bool,
    is_excluded: &dyn Fn(&str) -> bool,
) -> StoredPartition {
    let mut surviving = 0u64;
    let mut doomed = Vec::new();
    for path in stored_paths {
        if stored_row_survives(path, folder_scores, threshold, is_override, is_excluded) {
            surviving += 1;
        } else {
            doomed.push(path.clone());
        }
    }
    StoredPartition { surviving, doomed }
}

/// Whether a single stored row SURVIVES the current setting — the one canonical
/// survival rule, shared by [`partition_stored`] (which collects the doomed paths for a
/// prune) and the counts-only [`MediaScheduler::stored_coverage_counts`] (which the M5
/// volume-state poll calls without allocating a 200k-path list). A row survives when
/// it's NOT under an excluded folder AND (covered by an "always index" override OR its
/// parent folder scores at or above `threshold`); keys on score-MAP MEMBERSHIP, so a
/// floored folder (no row) is below any threshold.
///
/// [`MediaScheduler::stored_coverage_counts`]: crate::media_index::scheduler::MediaScheduler::stored_coverage_counts
pub(crate) fn stored_row_survives(
    path: &str,
    folder_scores: &HashMap<String, f64>,
    threshold: f64,
    is_override: &dyn Fn(&str) -> bool,
    is_excluded: &dyn Fn(&str) -> bool,
) -> bool {
    !is_excluded(path) && (is_override(path) || folder_scores.get(parent_dir(path)).is_some_and(|s| *s >= threshold))
}

/// Convenience: read a volume's importance folder scores as a `folder → score` map, or
/// `None` when importance never scored it (offline / fresh). Mirrors the scheduler's
/// `folder_scores`, but returns EVERY scored folder (threshold applied by
/// [`covered_for_volume`] so one read serves any slider position during a debounced
/// drag).
pub fn importance_scores(data_dir: &Path, volume_id: &str) -> Option<HashMap<String, f64>> {
    use crate::importance::{ImportanceIndex, SignalSet};
    let index = ImportanceIndex::open(data_dir, volume_id, SignalSet::all());
    if !importance_scored(&index) {
        return None;
    }
    match index.above_threshold(0.0) {
        Ok(weights) => Some(weights.into_iter().map(|w| (w.path, w.score.value())).collect()),
        Err(_) => None,
    }
}

/// Whether importance genuinely has data for this volume — the "has it scored?"
/// check both the scheduler's `folder_scores` and [`importance_scores`] gate on.
///
/// Keys on live weight rows, NOT solely the `recompute_generation` stamp: a store
/// maintained only by INCREMENTAL rescores carries hundreds of thousands of weight
/// rows but no generation (the incremental path deliberately never bumps it), and a
/// schema-recreated store starts at generation 0 until its first FULL pass stamps
/// one. Gating on the generation alone reads such a volume as "never scored" forever
/// and reports "0 covered" at every threshold, even though the weights are perfectly
/// usable (`importance/DETAILS.md` § Generation-stamp semantics). So: scored when a
/// full pass stamped a generation OR any weight row exists. Reuses the cheap
/// `scored_folder_count` probe (a `COUNT(*)`, short-circuits to 0 for a missing DB) —
/// don't add a second method.
pub(crate) fn importance_scored(index: &crate::importance::ImportanceIndex) -> bool {
    index.recompute_generation().unwrap_or(0) > 0 || index.scored_folder_count().unwrap_or(0) > 0
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

    // ── The reclaim partition (M4): stored rows inside vs outside coverage ──────

    fn scores(entries: &[(&str, f64)]) -> HashMap<String, f64> {
        entries.iter().map(|(p, s)| (p.to_string(), *s)).collect()
    }

    #[test]
    fn partition_splits_stored_rows_at_the_threshold_boundary() {
        // A folder scoring exactly AT the threshold survives; one below is doomed; one
        // with no score row at all is doomed (floored / scored away → score 0.0).
        let stored = vec![
            "/at/a.jpg".to_string(),
            "/below/b.jpg".to_string(),
            "/floored/c.jpg".to_string(),
        ];
        let folder_scores = scores(&[("/at", 0.4), ("/below", 0.2)]);
        let no = |_: &str| false;
        let part = partition_stored(&stored, &folder_scores, 0.4, &no, &no);
        assert_eq!(part.surviving, 1, "the at-threshold folder survives");
        assert_eq!(
            part.doomed,
            vec!["/below/b.jpg".to_string(), "/floored/c.jpg".to_string()],
            "the below-threshold and the no-score-row folders are doomed"
        );
        // The partition invariant: every stored row lands in exactly one bucket.
        assert_eq!(part.surviving as usize + part.doomed.len(), stored.len());
    }

    #[test]
    fn partition_keeps_an_override_covered_row_below_threshold() {
        // An "always index" override survives even when its folder scores below the
        // threshold (or isn't scored at all) — same precedence as enrichment.
        let stored = vec!["/archive/a.jpg".to_string()];
        let folder_scores = scores(&[]);
        let is_override = |p: &str| p.starts_with("/archive/");
        let no = |_: &str| false;
        let part = partition_stored(&stored, &folder_scores, 0.8, &is_override, &no);
        assert_eq!(part.surviving, 1, "an override-covered row survives");
        assert!(part.doomed.is_empty());
    }

    #[test]
    fn partition_dooms_an_excluded_row_even_when_covered() {
        // The privacy exclusion is a HARD veto: an excluded row is doomed even if an
        // override would otherwise cover it (exclusion beats coverage everywhere).
        let stored = vec!["/archive/secret.jpg".to_string()];
        let folder_scores = scores(&[]);
        let always = |_: &str| true; // override covers everything
        let is_excluded = |p: &str| p.starts_with("/archive/");
        let part = partition_stored(&stored, &folder_scores, 0.0, &always, &is_excluded);
        assert_eq!(part.surviving, 0, "an excluded row never survives");
        assert_eq!(part.doomed, vec!["/archive/secret.jpg".to_string()]);
    }

    #[test]
    fn partition_at_threshold_zero_still_dooms_a_floored_folder() {
        // At threshold 0.0 a SCORED folder survives, but a floored folder (no score row)
        // is still doomed — it keys on map membership, never a `>= 0.0` on a default 0.0.
        let stored = vec!["/scored/a.jpg".to_string(), "/floored/b.jpg".to_string()];
        let folder_scores = scores(&[("/scored", 0.0)]);
        let no = |_: &str| false;
        let part = partition_stored(&stored, &folder_scores, 0.0, &no, &no);
        assert_eq!(part.surviving, 1, "the scored folder survives at threshold 0");
        assert_eq!(
            part.doomed,
            vec!["/floored/b.jpg".to_string()],
            "the floored folder (no score row) is doomed even at threshold 0"
        );
    }
}
