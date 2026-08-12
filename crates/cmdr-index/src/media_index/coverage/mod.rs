//! What "covered" means, and how much of it is done: the coverage rule the settings
//! preview and the destructive reclaim prune both go through, plus the joined
//! per-folder read behind the folder badges.
//!
//! Two aggregates answer "how far along is this folder", and they have DIFFERENT
//! sources and update models, so each owns its own cache and its own file:
//!
//! - [`eligible`] is the DENOMINATOR: how many images the drive index says qualify per
//!   folder. Rebuilt from a whole-volume index walk each pass, so it reflects the live
//!   filesystem, and it's the half that reaches the walk.
//! - [`accounted`] is the NUMERATOR: how many of those the media index has a stored
//!   result for. Maintained incrementally by the ONE writer thread per volume, so it
//!   reflects what enrichment has stored, and it reaches nothing above it.
//!
//! ❌ Don't merge them back into one cache or one file. The walk-driven refill would
//! wipe the incrementally-maintained accounted counts, and the walk and the writer would
//! be back in one import cycle (which is what one shared file cost last time).
//!
//! The threshold is applied cheaply on top: intersect the importance `above_threshold`
//! folder set with the cached counts. The importance read behind it is NOT cheap — it
//! reads and sorts every scored folder — so it goes through the cache in [`scores`],
//! which is what keeps a debounced slider drag (and the per-visible-range badge query)
//! from re-reading the table each time. ❌ Never call `ImportanceIndex::above_threshold`
//! straight from a UI-driven path; take [`importance_scores`] or
//! [`importance_scores_above`].

// Visible to the rest of the subsystem (not to a host) because the ONE writer thread
// per volume mutates it directly: routing its ±1 deltas through this facade would put
// the writer and the walk back in one import cycle.
pub(super) mod accounted;
mod eligible;
mod rollup;
mod scores;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::Path;

use super::gate::IndexScope;
use super::paths::parent_dir;

pub use eligible::{FolderImageCounts, cached, get_or_build, invalidate};
pub use scores::{importance_scores, importance_scores_above};
#[cfg(any(test, feature = "testing"))]
pub use scores::clear_cache_for_test as clear_score_cache_for_test;
pub(crate) use eligible::{patch_touched_dirs, replace_from_entries};
// The walk-parity tests in `scheduler/enrich_tests.rs` are the only callers outside the
// eligible cache itself; production reaches them through `get_or_build`.
#[cfg(test)]
pub(crate) use eligible::{build_counts, count_qualifying_images};

/// One folder's coverage: the eligible denominator and accounted numerator, each a
/// subtree total over the folder and its descendants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderCoverageCounts {
    /// How many images under the folder qualify for enrichment at all.
    pub eligible: u64,
    /// How many of those the media index has a stored result for.
    pub accounted: u64,
}

/// The eligible + accounted subtree totals for `folders` on `volume_id`, one per input
/// folder in order — everything the folder-coverage badges need, in one call.
///
/// Both sides come from cached rollups, never a `media_status` scan per query: the
/// eligible side builds itself from the drive index on demand, and the accounted side is
/// seeded here from `data_dir`'s `media.db` if this volume's writer hasn't spawned yet
/// this session (the feature was just enabled, or the volume never enriched). Folder
/// paths are in the volume's INDEX-path space (== the OS path for a local volume),
/// matching the stored rows and the eligible cache.
pub fn folder_coverage(data_dir: &Path, volume_id: &str, folders: &[String]) -> Vec<FolderCoverageCounts> {
    accounted::ensure_seeded(volume_id, &super::store::media_db_path(data_dir, volume_id));
    let eligible = eligible::eligible_subtrees(volume_id, folders);
    let accounted = accounted::subtrees(volume_id, folders);
    eligible
        .into_iter()
        .zip(accounted)
        .map(|(eligible, accounted)| FolderCoverageCounts { eligible, accounted })
        .collect()
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
    /// (also the per-volume `keptCount`, the same rows framed as "still searchable").
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
    scope: IndexScope,
    is_override: &dyn Fn(&str) -> bool,
    is_excluded: &dyn Fn(&str) -> bool,
) -> StoredPartition {
    let mut surviving = 0u64;
    let mut doomed = Vec::new();
    for path in stored_paths {
        if stored_row_survives(path, folder_scores, threshold, scope, is_override, is_excluded) {
            surviving += 1;
        } else {
            doomed.push(path.clone());
        }
    }
    StoredPartition { surviving, doomed }
}

/// Whether a single stored row SURVIVES the current setting — the one canonical
/// survival rule, shared by [`partition_stored`] (which collects the doomed paths for a
/// prune) and the counts-only [`MediaScheduler::stored_coverage_counts`] (which the
/// volume-state poll calls without allocating a 200k-path list). A row survives when
/// it's NOT under an excluded folder AND (covered by an "always index" override OR, in
/// the automatic SCOPE only, its parent folder scores at or above `threshold`); the
/// score term keys on score-MAP MEMBERSHIP, so a floored folder (no row) is below any
/// threshold.
///
/// In the narrow scope the threshold term drops out entirely, matching the enrichment
/// gate ([`local_should_enrich`]) exactly — the destructive reclaim selection can never
/// propose deleting a row a pass would keep, or keep one a pass would never write.
///
/// [`MediaScheduler::stored_coverage_counts`]: crate::media_index::scheduler::MediaScheduler::stored_coverage_counts
/// [`local_should_enrich`]: crate::media_index::scheduler
pub(crate) fn stored_row_survives(
    path: &str,
    folder_scores: &HashMap<String, f64>,
    threshold: f64,
    scope: IndexScope,
    is_override: &dyn Fn(&str) -> bool,
    is_excluded: &dyn Fn(&str) -> bool,
) -> bool {
    if is_excluded(path) {
        return false;
    }
    if is_override(path) {
        return true;
    }
    scope.consults_importance() && folder_scores.get(parent_dir(path)).is_some_and(|s| *s >= threshold)
}

/// The chosen-folder counts for ONE volume: how many folders holding qualifying images
/// are covered by an "always index" override, and how many images they hold. The narrow
/// scope's counterpart to [`covered_for_volume`] — same two quantities, the other
/// coverage rule — so the settings preview and progress lines stay honest in both
/// scopes off the one cached [`FolderImageCounts`]. `is_override` takes the STORED
/// (index-relative) folder path; the caller wires the OS-mount mapping, as everywhere
/// else here.
pub fn chosen_for_volume(counts: &FolderImageCounts, is_override: &dyn Fn(&str) -> bool) -> (u64, u64) {
    let mut folders = 0u64;
    let mut images = 0u64;
    for (folder, count) in &counts.per_folder {
        if is_override(folder) {
            folders += 1;
            images += count;
        }
    }
    (folders, images)
}

/// The covered folder + image counts for one volume under the CURRENT scope: the
/// chosen folders alone, or those plus every folder at or above `threshold`. The one
/// dispatcher both the settings preview and the per-volume progress line go through, so
/// neither can drift from the enrichment gate.
pub fn covered_in_scope(
    counts: &FolderImageCounts,
    folder_scores: &HashMap<String, f64>,
    threshold: f64,
    scope: IndexScope,
    is_override: &dyn Fn(&str) -> bool,
) -> (u64, u64) {
    match scope {
        IndexScope::ChosenFolders => chosen_for_volume(counts, is_override),
        IndexScope::ByImportance => {
            // The automatic scope covers the above-threshold folders PLUS the chosen
            // ones; a chosen folder that scores below (or isn't scored at all) would
            // otherwise be missing from a count the enrichment gate does include.
            let (mut folders, mut images) = covered_for_volume(counts, folder_scores, threshold);
            for (folder, count) in &counts.per_folder {
                let scored_in = folder_scores.get(folder.as_str()).is_some_and(|s| *s >= threshold);
                if !scored_in && is_override(folder) {
                    folders += 1;
                    images += count;
                }
            }
            (folders, images)
        }
    }
}

