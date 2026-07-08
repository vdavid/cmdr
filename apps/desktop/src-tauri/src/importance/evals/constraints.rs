//! The constraint vocabulary and its pure evaluation arithmetic.
//!
//! A scenario carries two tiers of expectations about how its folders should
//! rank once scored (see [`super`]):
//!
//! - **Hard constraints** are ordering facts that must ALWAYS hold (a
//!   `node_modules` in the bottom decile, a project root above every cache). The
//!   harness turns each into an ordinary `#[test]` assertion, so a violation fails
//!   CI.
//! - **Soft constraints** are a larger set of desirable-but-not-sacred orderings.
//!   The harness counts the satisfied fraction as a scalar quality score, so a
//!   weight change becomes a measurable delta instead of a vibe.
//!
//! Both tiers speak the SAME [`Constraint`] vocabulary — the only difference is
//! how a caller treats a violation (fail vs. count). Every constraint evaluates
//! against a [`Ranking`]: the folders sorted by score, which this module also owns
//! (stable ordering, tie handling, decile math), so the semantics are defined once
//! and unit-tested here rather than re-derived per scenario.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A folder's scored position within a scenario, the unit constraints evaluate
/// against. Built by [`Ranking::from_scores`] from `(path, score)` pairs.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedFolder {
    /// The folder's absolute path (its identity within the scenario).
    pub path: String,
    /// Its importance score, `0.0..=1.0`.
    pub score: f64,
}

/// The folders of a scenario sorted by score descending, with the tie rule and
/// the rank/decile lookups every constraint needs. Owning the ordering here keeps
/// "rank", "top N", and "decile" defined once.
///
/// **Tie rule.** Sort is score DESC, then path ASC — the SAME stable order the
/// read API's `top_n` uses (`read::read_ordered`), so a scenario's ranking matches
/// what a consumer would see. Ties therefore resolve deterministically by path,
/// never by input order.
#[derive(Debug, Clone)]
pub struct Ranking {
    /// Folders, sorted best-first (score DESC, path ASC).
    ordered: Vec<RankedFolder>,
    /// `path → 0-based rank` (index into `ordered`), for O(1) constraint lookups.
    rank_of: HashMap<String, usize>,
}

impl Ranking {
    /// Build a ranking from `(path, score)` pairs. Sorts score DESC then path ASC
    /// (the read API's stable order), so ties break by path deterministically.
    pub fn from_scores(scores: impl IntoIterator<Item = (String, f64)>) -> Self {
        let mut ordered: Vec<RankedFolder> = scores
            .into_iter()
            .map(|(path, score)| RankedFolder { path, score })
            .collect();
        // score DESC, then path ASC — matches `read::read_ordered`'s ORDER BY so a
        // scenario ranks folders exactly as a live consumer would.
        ordered.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path))
        });
        let rank_of = ordered.iter().enumerate().map(|(i, f)| (f.path.clone(), i)).collect();
        Self { ordered, rank_of }
    }

    /// The number of folders ranked.
    pub fn len(&self) -> usize {
        self.ordered.len()
    }

    /// Whether the ranking is empty.
    pub fn is_empty(&self) -> bool {
        self.ordered.is_empty()
    }

    /// The folders in ranked order (best first).
    pub fn ordered(&self) -> &[RankedFolder] {
        &self.ordered
    }

    /// A folder's 0-based rank (0 is the most important), or `None` if the path
    /// isn't in the scenario.
    pub fn rank_of(&self, path: &str) -> Option<usize> {
        self.rank_of.get(path).copied()
    }

    /// Which decile (1..=10) a folder falls in, 1 being the most important tenth
    /// and 10 the least. `None` if the path isn't ranked.
    ///
    /// Decile math: a folder at 0-based `rank` out of `n` sits at fractional
    /// position `rank / n` in `[0, 1)`, so its decile is `floor(rank * 10 / n) + 1`,
    /// clamped to `10` (the last folder of a full list lands in decile 10, never an
    /// out-of-range 11). Integer arithmetic, so it's exact and tie-free.
    pub fn decile_of(&self, path: &str) -> Option<u8> {
        let rank = self.rank_of(path)?;
        Some(decile(rank, self.ordered.len()))
    }
}

/// The decile (1..=10) of a 0-based `rank` within `n` items. Pulled out so the
/// boundary math is unit-testable directly. `n == 0` is meaningless (no items);
/// it returns decile 1 defensively, but callers never rank an empty set.
pub fn decile(rank: usize, n: usize) -> u8 {
    if n == 0 {
        return 1;
    }
    let d = (rank * 10) / n + 1;
    d.min(10) as u8
}

/// One expectation about how folders should rank. The shared vocabulary for both
/// the hard tier (must always hold) and the soft tier (counted into the quality
/// score) — a constraint doesn't know which tier it's in; the caller decides how
/// to treat a violation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Constraint {
    /// `above` must rank strictly higher (better) than `below`. The workhorse
    /// pairwise ordering fact ("the project root outranks its `node_modules`").
    Above { above: String, below: String },
    /// `path` must land within the top `n` folders (0-based rank `< n`). Captures
    /// "this is one of the few folders that really matter".
    TopN { path: String, n: usize },
    /// `path` must fall within the bottom decile (decile 10). The canonical
    /// "machine output is ignorable" expectation for caches, logs, `node_modules`.
    BottomDecile { path: String },
    /// `path` must sit in decile `at_most` or better (decile `<= at_most`, where 1
    /// is best). A softer "belongs near the top" than [`Self::TopN`].
    DecileAtMost { path: String, at_most: u8 },
    /// `path`'s score must be at or below `max` — the denylist/hidden floor is `0.0`,
    /// so this pins "a floored folder really scored 0", which pure ordering can't.
    ScoreAtMost { path: String, max: f64 },
}

/// Whether a constraint holds against a ranking, and why if it doesn't.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintOutcome {
    /// The constraint holds.
    Satisfied,
    /// The constraint is violated; the string explains it (for a hard-tier test
    /// failure message).
    Violated(String),
    /// A path the constraint names isn't in the scenario's ranking, so the
    /// constraint can't be judged. Treated as a violation for scoring (an
    /// expectation about a missing folder is not satisfied), but reported
    /// distinctly so a mistyped scenario path is obvious.
    Unknown(String),
}

impl ConstraintOutcome {
    /// Whether this outcome counts as satisfied for the soft quality score. Only
    /// [`Self::Satisfied`] does; a violation or an unknown path does not.
    pub fn is_satisfied(&self) -> bool {
        matches!(self, ConstraintOutcome::Satisfied)
    }
}

impl Constraint {
    /// Evaluate this constraint against a ranking.
    pub fn evaluate(&self, ranking: &Ranking) -> ConstraintOutcome {
        match self {
            Constraint::Above { above, below } => {
                let (Some(ra), Some(rb)) = (ranking.rank_of(above), ranking.rank_of(below)) else {
                    return ConstraintOutcome::Unknown(format!(
                        "'above' expectation names a folder not in the scenario: {above} above {below}"
                    ));
                };
                // Lower rank index = more important. `above` must be strictly higher.
                if ra < rb {
                    ConstraintOutcome::Satisfied
                } else {
                    ConstraintOutcome::Violated(format!(
                        "expected {above} (rank {ra}) above {below} (rank {rb}), but it isn't"
                    ))
                }
            }
            Constraint::TopN { path, n } => match ranking.rank_of(path) {
                None => ConstraintOutcome::Unknown(format!("top-{n} expectation names a missing folder: {path}")),
                Some(rank) if rank < *n => ConstraintOutcome::Satisfied,
                Some(rank) => ConstraintOutcome::Violated(format!(
                    "expected {path} in the top {n}, but it ranks {rank} (0-based)"
                )),
            },
            Constraint::BottomDecile { path } => match ranking.decile_of(path) {
                None => ConstraintOutcome::Unknown(format!("bottom-decile expectation names a missing folder: {path}")),
                Some(10) => ConstraintOutcome::Satisfied,
                Some(d) => ConstraintOutcome::Violated(format!(
                    "expected {path} in the bottom decile (10), but it's in decile {d}"
                )),
            },
            Constraint::DecileAtMost { path, at_most } => match ranking.decile_of(path) {
                None => ConstraintOutcome::Unknown(format!("decile expectation names a missing folder: {path}")),
                Some(d) if d <= *at_most => ConstraintOutcome::Satisfied,
                Some(d) => ConstraintOutcome::Violated(format!(
                    "expected {path} in decile {at_most} or better, but it's in decile {d}"
                )),
            },
            Constraint::ScoreAtMost { path, max } => match ranking.ordered.iter().find(|f| &f.path == path) {
                None => ConstraintOutcome::Unknown(format!("score expectation names a missing folder: {path}")),
                Some(f) if f.score <= *max + f64::EPSILON => ConstraintOutcome::Satisfied,
                Some(f) => ConstraintOutcome::Violated(format!(
                    "expected {path} to score at most {max}, but it scored {}",
                    f.score
                )),
            },
        }
    }
}

/// The satisfied fraction of a set of constraints against a ranking: the count of
/// [`ConstraintOutcome::Satisfied`] over the total. `0.0..=1.0`. An empty set
/// scores `1.0` (vacuously — nothing to violate), so a scenario with no soft
/// constraints doesn't drag an aggregate down.
pub fn satisfied_fraction(constraints: &[Constraint], ranking: &Ranking) -> f64 {
    if constraints.is_empty() {
        return 1.0;
    }
    let satisfied = constraints
        .iter()
        .filter(|c| c.evaluate(ranking).is_satisfied())
        .count();
    satisfied as f64 / constraints.len() as f64
}

#[cfg(test)]
mod tests;
