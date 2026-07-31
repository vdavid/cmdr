//! Ranking-quality evals for the importance scorer — the measurement instrument
//! for tuning [`Weights`].
//!
//! The scorer ships with deliberately-unvalidated default weights (see
//! `weights.rs`). This module makes "did a weight change help?" a measurable
//! question instead of a vibe: a [`Scenario`] (folders + their signals + ranking
//! expectations) scores through the pure scorer, and its expectations are checked
//! against the resulting ranking.
//!
//! ## The two tiers
//!
//! - **Hard constraints** are ordering facts that must ALWAYS hold. The test
//!   module ([`tests`]) asserts each as an ordinary `#[test]`, so a violation fails
//!   CI — a regression that lets `node_modules` climb out of the bottom decile
//!   breaks the build.
//! - **Soft constraints** are a larger set of desirable orderings. [`score_scenario`]
//!   returns the satisfied fraction as a scalar quality score, and the aggregate
//!   across scenarios is pinned to a floor constant ([`tests::SOFT_SCORE_FLOOR`]) —
//!   a fixed floor, NOT a self-updating ratchet. A change that drops quality below
//!   the floor fails; when tuning improves quality, the floor is consciously raised.
//!
//! ## The fitness function
//!
//! [`score_scenario`] is pure and fast: `(scenario, weights) -> f64`. Nothing
//! about it touches I/O, a clock (the scenario carries its own `now`), or global
//! state, so a future tuner can grid-search or hill-climb over `Weights` by calling
//! it in a loop. [`aggregate_score`] averages per-scenario scores into one number.
//!
//! ## Scenarios
//!
//! Synthetic scenarios ([`scenarios`]) are committed and cover varied homes
//! (dev-home, media-home, downloads-heavy, an SMB/NAS). Real-derived corpus
//! scenarios are loaded at test time from a gitignored dir (the corpus tool
//! exports them, David labels them); [`tests`] auto-includes any it finds, so the
//! committed suite is fully green with zero corpus files present.

pub mod constraints;
pub mod corpus;
pub mod scenario;
pub mod scenarios;

pub use constraints::{Constraint, ConstraintOutcome, Ranking, satisfied_fraction};
pub use scenario::{Availability, Scenario, ScenarioFolder};

use crate::importance::scorer::{Weights, score};

/// Score every folder in a scenario under `weights` and return the ranking. The
/// bridge from the scenario's stored signals to a [`Ranking`] the constraints
/// evaluate against — pure, so it's the inner loop of the fitness function.
pub fn rank_scenario(scenario: &Scenario, weights: &Weights) -> Ranking {
    let available = scenario.availability.signal_set();
    Ranking::from_scores(scenario.folders.iter().map(|f| {
        let s = score(&f.signals, &available, weights, scenario.now_secs);
        (f.path.clone(), s.value())
    }))
}

/// The soft-constraint quality score for one scenario under `weights`: the
/// satisfied fraction of its soft constraints against the ranking, `0.0..=1.0`.
/// The pure fitness function a tuner optimizes — higher is better.
///
/// Hard constraints are NOT counted here; they're all-or-nothing and enforced as
/// `#[test]`s. This scalar is only the soft tier, so a tuner sees smooth movement
/// (a knob turn that satisfies one more soft ordering nudges the score up) rather
/// than a step function.
pub fn score_scenario(scenario: &Scenario, weights: &Weights) -> f64 {
    let ranking = rank_scenario(scenario, weights);
    satisfied_fraction(&scenario.soft, &ranking)
}

/// The aggregate soft-score across scenarios: the unweighted mean of each
/// scenario's [`score_scenario`]. One number to compare weight sets by, and the
/// value pinned against the floor. An empty set scores `1.0` (vacuous).
pub fn aggregate_score(scenarios: &[Scenario], weights: &Weights) -> f64 {
    if scenarios.is_empty() {
        return 1.0;
    }
    let sum: f64 = scenarios.iter().map(|s| score_scenario(s, weights)).sum();
    sum / scenarios.len() as f64
}

#[cfg(test)]
mod tests;
