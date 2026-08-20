//! The correctness harness behind the scoped incremental walk: run BOTH walks over
//! the same real index and difference the rows they would write.
//!
//! Its own module rather than part of [`super::recompute`] because it is a
//! measurement path, not a production one — nothing in the app calls it; the
//! `importance-diff` dev bin does. Depth: `DETAILS.md` § The scoped walk.

use std::collections::HashMap;

use super::recompute::{RescoreScope, ScoringInputs, rescore_rows, rescore_subset, sanitize_incremental_batch};
use super::scoped_walk::plan_incremental_batch;
use super::walk::walk_index_folders;
use crate::importance::scorer::{SignalSet, Weights};
use crate::importance::writer::WeightRow;
use crate::indexing::store::IndexStore;

// ── The scoped-vs-full differential (the correctness harness) ──────────────

/// What one origin's scoped walk cost, and whether it agreed with the full walk.
pub struct OriginComparison {
    /// Folders the scoped walk read for this origin (the subtree's size).
    pub scoped_folders: usize,
    /// Rows the scoped walk would write.
    pub scoped_rows: usize,
    /// Rows the FULL walk would write for the same subtree — the oracle.
    pub oracle_rows: usize,
    /// Rows present in one side and not the other, by path.
    pub missing_or_extra: usize,
    /// Rows present in both whose score differs.
    pub score_mismatches: usize,
    /// Rows present in both whose signal blob differs.
    pub signal_mismatches: usize,
    /// `true` when the scoped walk declined (too big, or the marker guard fired), so
    /// this origin exercised the full-walk fallback instead of a comparison.
    pub fell_back: bool,
    /// `true` when the origin's subtree was past the budget, so the pass rescored it
    /// ALONE. The oracle is narrowed to match: the full walk's row for that one
    /// folder, since every row beneath it is left untouched by design.
    pub demoted: bool,
    /// Wall clock for the scoped walk alone.
    pub scoped_walk: std::time::Duration,
}

impl OriginComparison {
    /// Whether the two walks agreed on every row.
    pub fn agrees(&self) -> bool {
        self.missing_or_extra == 0 && self.score_mismatches == 0 && self.signal_mismatches == 0
    }
}

/// Compare the SCOPED walk against the full walk over a real index, read-only, for
/// each of `origins`.
///
/// The differential the scoped walk's correctness rests on: for the same index, the
/// same home, and a fixed `now_secs`, the rows a scoped walk would write for a
/// subtree must be exactly the rows the full walk would write for that same subtree
/// — same paths, same scores, same signal blobs. Scores drift with the wall clock
/// (the recency signal), which is why `now_secs` is an argument rather than read
/// here.
///
/// The full walk runs ONCE and serves as every origin's oracle. Each origin's guard
/// input is taken FROM that full walk, so the comparison isolates the walk itself
/// from the marker guard (which its own unit tests cover); an origin whose subtree is
/// too large still falls back, and reports that instead of a comparison.
pub fn compare_walks_for_incremental(
    index_db: &std::path::Path,
    home: &str,
    origins: &[String],
    now_secs: u64,
) -> Result<Vec<OriginComparison>, String> {
    let conn = IndexStore::open_read_connection(index_db).map_err(|e| e.to_string())?;
    let mut full = walk_index_folders(&conn, home)?;

    // Each origin's marker presence AS THE FULL WALK SEES IT, so the guard never
    // fires on a difference the comparison is meant to expose.
    let mut markers: HashMap<String, bool> = HashMap::new();
    full.for_each(|folder, path| {
        if origins.iter().any(|o| o == path) {
            markers.insert(
                path.to_string(),
                folder.children.has_direct_marker || folder.has_marker_below,
            );
        }
    });

    let weights = Weights::default();
    let available = SignalSet::listing_only();
    let visits = HashMap::new();
    let inputs = ScoringInputs {
        weights: &weights,
        home,
        now_secs,
        available,
        visits: &visits,
    };

    let mut out = Vec::new();
    for origin in origins {
        let one = std::slice::from_ref(origin);
        let started = std::time::Instant::now();
        let plan = plan_incremental_batch(&conn, one)?;
        // What the pass will act on: an over-budget origin is rescored alone, so both
        // sides of the comparison have to be narrowed to that one folder.
        let (cleared, demoted) = plan.lists_for(RescoreScope::ChangedSubtreesOnly);
        let outcome = crate::importance::scheduler::scoped_walk::try_scoped_walk(&conn, home, &plan, &markers)?;
        let scoped_walk = started.elapsed();
        let mut scoped = match outcome {
            crate::importance::scheduler::scoped_walk::ScopedWalkOutcome::Scoped(folders) => folders,
            crate::importance::scheduler::scoped_walk::ScopedWalkOutcome::FullWalkNeeded(_) => {
                out.push(OriginComparison {
                    scoped_folders: 0,
                    scoped_rows: 0,
                    oracle_rows: 0,
                    missing_or_extra: 0,
                    score_mismatches: 0,
                    signal_mismatches: 0,
                    fell_back: true,
                    demoted: !demoted.is_empty(),
                    scoped_walk,
                });
                continue;
            }
        };

        let scoped_folders = scoped.len();
        let scoped_rows = rescore_rows(
            &inputs,
            &rescore_subset(&mut scoped, &cleared, RescoreScope::ChangedSubtreesOnly, &demoted),
            &HashMap::new(),
        );
        let oracle_rows = rescore_rows(
            &inputs,
            &rescore_subset(&mut full, &cleared, RescoreScope::ChangedSubtreesOnly, &demoted),
            &HashMap::new(),
        );
        let mut comparison = diff_rows(scoped_folders, scoped_rows, oracle_rows, scoped_walk);
        comparison.demoted = !demoted.is_empty();
        out.push(comparison);
    }
    Ok(out)
}

/// Count how two row sets differ, by path presence, score, and signal blob. Reports
/// COUNTS only — the dev tool this feeds never prints a folder name.
fn diff_rows(
    scoped_folders: usize,
    scoped: Vec<WeightRow>,
    oracle: Vec<WeightRow>,
    scoped_walk: std::time::Duration,
) -> OriginComparison {
    let by_path: HashMap<&str, &WeightRow> = oracle.iter().map(|r| (r.path.as_str(), r)).collect();
    let mut missing_or_extra = oracle.len().saturating_sub(scoped.len());
    let mut score_mismatches = 0;
    let mut signal_mismatches = 0;
    for row in &scoped {
        match by_path.get(row.path.as_str()) {
            None => missing_or_extra += 1,
            Some(expected) => {
                if expected.score.to_bits() != row.score.to_bits() {
                    score_mismatches += 1;
                }
                if expected.signals_json != row.signals_json {
                    signal_mismatches += 1;
                }
            }
        }
    }
    OriginComparison {
        scoped_folders,
        scoped_rows: scoped.len(),
        oracle_rows: oracle.len(),
        missing_or_extra,
        score_mismatches,
        signal_mismatches,
        fell_back: false,
        demoted: false,
        scoped_walk,
    }
}

/// Sample up to `max` non-flooring directory paths spread across a real index, for
/// the differential to use as origins.
///
/// Strided over the id space (which is scan order, so roughly tree order) rather than
/// randomized, so a re-run compares the same subtrees.
pub fn sample_origins(index_db: &std::path::Path, home: &str, max: usize) -> Result<Vec<String>, String> {
    let conn = IndexStore::open_read_connection(index_db).map_err(|e| e.to_string())?;
    let total = IndexStore::get_dir_count(&conn).map_err(|e| e.to_string())?;
    let stride = (total / (max.max(1) as u64 * 4)).max(1) as i64;
    let mut stmt = conn
        .prepare("SELECT id FROM entries WHERE is_directory = 1 AND (id % ?1) = 0 LIMIT ?2")
        .map_err(|e| e.to_string())?;
    let ids: Vec<i64> = stmt
        .query_map(rusqlite::params![stride, (max * 4) as i64], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();
    let paths: Vec<String> = ids
        .into_iter()
        .filter_map(|id| IndexStore::reconstruct_path(&conn, id).ok())
        .collect();
    // The live path never sees a floored origin, so neither should the differential.
    Ok(sanitize_incremental_batch(&paths, home).into_iter().take(max).collect())
}
