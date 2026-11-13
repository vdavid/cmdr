//! The rows the drive index answers: a folder's whole size and how much it
//! matters. Both read databases only, ❌ never the disk, so a folder of any size
//! answers in milliseconds and a mount is never touched.

use std::path::PathBuf;

use cmdr_about::fact::{Fact, UnknownWhy};
use cmdr_about::payload::{Importance, ImportanceBand, RecursiveSize, SizeCoverage};
use cmdr_index::IndexStatusResponse;
use cmdr_index::importance::{
    Explanation, FloorReason, FolderSignals, ImportanceIndex, PathClass, SignalKind, SignalSet,
};
use cmdr_index::store::DirStats;

use crate::index_host::index;
use crate::tools::resources::importance::{PathImportance, snapshot_path_on};

/// Where an item sits in the index and the importance store.
#[derive(Debug, Clone)]
pub(crate) struct IndexPlace {
    /// The index that holds its rows. ❗ Not always its volume's own id: the boot
    /// disk's index walks a cloud drive (`location::resolve::IndexSources`).
    pub(crate) index_id: String,
    /// The item's app path, as the index is asked.
    pub(crate) path: String,
    /// The importance store's volume, and the path in that store's own space.
    /// `None`: the store has no path for it, so the card has no importance row.
    pub(crate) importance: Option<(String, String)>,
    /// The app's data dir, where the importance stores live. `None` likewise.
    pub(crate) data_dir: Option<PathBuf>,
}

impl IndexPlace {
    /// Whether the importance row can be asked at all.
    pub(crate) fn scores(&self) -> bool {
        self.importance.is_some() && self.data_dir.is_some()
    }
}

/// How many signals the row names behind a score.
const MAX_REASONS: usize = 3;

/// The folder's whole size, with the index's coverage flags verbatim.
pub(crate) fn recursive_size_fact(place: &IndexPlace) -> Fact<RecursiveSize> {
    match index().dir_stats(&place.index_id, &place.path) {
        Ok(Some(stats)) => {
            let walked = index()
                .status(&place.index_id)
                .map(|status| status.walked_roots)
                .unwrap_or_default();
            let walk_moves_it = IndexStatusResponse::walk_affects(&walked, &place.path);
            Fact::Known(recursive_size_of(&stats, walk_moves_it))
        }
        Ok(None) => Fact::unknown(UnknownWhy::NotIndexed),
        Err(error) => {
            log::warn!(target: "about", "couldn't read a folder's size from the index: {error}");
            Fact::unknown(UnknownWhy::Unparseable)
        }
    }
}

/// [`RecursiveSize`] from the index's numbers. `walk_moves_it` is the volume's
/// half of "still moving" (a walk on, above, or below the folder); the stats
/// carry the folder's own (writes still draining). Reading only the second calls
/// a folder settled through the very walk rewriting it.
pub(crate) fn recursive_size_of(stats: &DirStats, walk_moves_it: bool) -> RecursiveSize {
    let coverage = if !stats.recursive_size_complete {
        SizeCoverage::LowerBound
    } else if stats.recursive_size_stale {
        SizeCoverage::ExactButStale
    } else {
        SizeCoverage::Exact
    };
    RecursiveSize {
        logical: stats.recursive_size,
        physical: stats.recursive_physical_size,
        files: stats.recursive_file_count,
        folders: stats.recursive_dir_count,
        coverage,
        updating: stats.recursive_size_pending || walk_moves_it,
        has_symlinks: stats.recursive_has_symlinks,
    }
}

/// How much the folder matters, by its stored score.
pub(crate) fn importance_fact(place: &IndexPlace, now_secs: u64) -> Fact<Importance> {
    let (Some((volume_id, store_path)), Some(data_dir)) = (&place.importance, &place.data_dir) else {
        return Fact::unknown(UnknownWhy::NotIndexed);
    };
    match snapshot_path_on(data_dir, volume_id, store_path, now_secs) {
        PathImportance::Scored {
            weight, explanation, ..
        } => {
            // The availability mask doesn't touch the generation read.
            let recompute = ImportanceIndex::open(data_dir, volume_id, SignalSet::all())
                .recompute_generation()
                .unwrap_or(0);
            Fact::Known(Importance {
                band: ImportanceBand::of_score(weight.score.value()),
                stale: weight.as_of_generation < recompute,
                reasons: explanation
                    .map(|explanation| reasons(&explanation, &weight.signals))
                    .unwrap_or_default(),
                visits: weight.signals.visit_count.map(u64::from),
            })
        }
        PathImportance::Floored { reason } => Fact::Known(Importance {
            band: ImportanceBand::Floored,
            stale: false,
            reasons: vec![floor_token(reason).to_string()],
            visits: None,
        }),
        PathImportance::Unscored => Fact::unknown(UnknownWhy::NotIndexed),
    }
}

/// The strongest positive signals behind a score, as tokens for the frontend to
/// word. A neutral path class and the constant visibility term say nothing.
pub(crate) fn reasons(explanation: &Explanation, signals: &FolderSignals) -> Vec<String> {
    let mut contributions: Vec<_> = explanation
        .contributions
        .iter()
        .filter(|contribution| contribution.contribution > 0.0)
        .collect();
    contributions.sort_by(|a, b| b.contribution.total_cmp(&a.contribution));
    contributions
        .into_iter()
        .filter_map(|contribution| signal_token(contribution.signal, signals))
        .take(MAX_REASONS)
        .map(str::to_string)
        .collect()
}

fn signal_token(signal: SignalKind, signals: &FolderSignals) -> Option<&'static str> {
    match signal {
        SignalKind::PathClass => match signals.path_class {
            PathClass::ProjectRoot => Some("projectRoot"),
            PathClass::UserContent => Some("userContent"),
            PathClass::SystemOrCache | PathClass::Neutral => None,
        },
        SignalKind::ProjectMarker => Some("projectMarker"),
        SignalKind::MtimeRecency => Some("mtimeRecency"),
        SignalKind::VisitActivity => Some("visitActivity"),
        SignalKind::LastUsed => Some("lastUsed"),
        SignalKind::ExtensionDiversity => Some("extensionDiversity"),
        SignalKind::Visibility => None,
    }
}

fn floor_token(reason: FloorReason) -> &'static str {
    match reason {
        FloorReason::NameDenylisted => "nameDenylisted",
        FloorReason::HiddenOrSystem => "hiddenOrSystem",
        FloorReason::UnderFlooredAncestor => "underFlooredAncestor",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdr_index::importance::{Score, SignalContribution};

    fn stats(complete: bool, stale: bool, pending: bool) -> DirStats {
        DirStats {
            path: "/x".to_string(),
            recursive_size: 1_000,
            recursive_physical_size: 4_096,
            recursive_file_count: 3,
            recursive_dir_count: 1,
            recursive_has_symlinks: true,
            recursive_size_pending: pending,
            recursive_size_pending_changes_in: None,
            recursive_size_complete: complete,
            recursive_size_stale: stale,
        }
    }

    #[test]
    fn the_index_flags_pass_through_verbatim() {
        let exact = recursive_size_of(&stats(true, false, false), false);
        assert_eq!(exact.coverage, SizeCoverage::Exact);
        assert!(!exact.updating);
        assert!(exact.has_symlinks);
        assert_eq!(
            (exact.logical, exact.physical, exact.files, exact.folders),
            (1_000, 4_096, 3, 1)
        );
        assert_eq!(
            recursive_size_of(&stats(true, true, false), false).coverage,
            SizeCoverage::ExactButStale
        );
        // A lower bound stays one even when the stale flag is set beside it.
        assert_eq!(
            recursive_size_of(&stats(false, true, false), false).coverage,
            SizeCoverage::LowerBound
        );
    }

    #[test]
    fn a_walk_nearby_or_draining_writes_mark_the_size_as_moving() {
        assert!(recursive_size_of(&stats(true, false, false), true).updating);
        assert!(recursive_size_of(&stats(true, false, true), false).updating);
    }

    fn contribution(signal: SignalKind, points: f64) -> SignalContribution {
        SignalContribution {
            signal,
            weight: 1.0,
            raw: points,
            contribution: points,
        }
    }

    #[test]
    fn the_strongest_signals_come_first_and_silent_ones_say_nothing() {
        let signals = FolderSignals {
            path_class: PathClass::Neutral,
            ..FolderSignals::neutral()
        };
        let explanation = Explanation {
            score: Score(0.8),
            contributions: vec![
                contribution(SignalKind::Visibility, 0.5),
                contribution(SignalKind::MtimeRecency, 0.1),
                contribution(SignalKind::PathClass, 0.4),
                contribution(SignalKind::VisitActivity, 0.3),
                contribution(SignalKind::ProjectMarker, 0.2),
                contribution(SignalKind::LastUsed, 0.0),
            ],
            floored: false,
        };
        assert_eq!(
            reasons(&explanation, &signals),
            ["visitActivity", "projectMarker", "mtimeRecency"]
        );
    }
}
