//! Tunable coefficients for the importance formula.
//!
//! The formula must iterate against real directory trees (agent-spec §18.3), so
//! the coefficients are data, not hardcoded constants: [`Weights::default`] gives
//! a reasonable starting point, and the M3 dev-tuning surface will override them
//! to tune against David's home directory. Keeping them here, serde-serializable,
//! also lets a future per-consumer weighting profile ship its own set.

use super::types::SignalKind;
use serde::{Deserialize, Serialize};

/// The named coefficients the scorer applies to each signal.
///
/// The seven additive weights should sum to `1.0` at their defaults so a folder
/// that maxes every signal (and hits no floor) reaches `Score::CEILING`. The
/// scorer does NOT require them to sum to one at runtime — a tuner can set any
/// values — but the redistribution logic and the `explain`-sums-to-`score`
/// invariant hold regardless.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Weights {
    /// Weight on extension diversity (mixed folders over monocultures).
    pub extension_diversity: f64,
    /// Weight on modification recency.
    pub mtime_recency: f64,
    /// Weight on a project marker raising the folder.
    pub project_marker: f64,
    /// Weight on the path-class prior.
    pub path_class: f64,
    /// Weight on visibility (not hidden / not system).
    pub visibility: f64,
    /// Weight on navigation-visit activity (optional signal).
    pub visit_activity: f64,
    /// Weight on Spotlight last-used recency (optional signal).
    pub last_used: f64,

    /// Half-life in seconds for the mtime-recency decay: a folder modified this
    /// long ago scores half of a just-modified folder on that signal. Defaults to
    /// 30 days. Not an additive weight — a shape parameter for the recency curve.
    pub mtime_half_life_secs: f64,
    /// Half-life in seconds for the last-used-recency decay. Defaults to 14 days.
    pub last_used_half_life_secs: f64,
    /// Visit count that saturates the visit-activity signal to `1.0`. Below it the
    /// signal scales linearly. Defaults to 10 visits.
    pub visit_saturation_count: f64,
}

impl Default for Weights {
    fn default() -> Self {
        // The seven additive weights sum to 1.0. These are a STARTING POINT to be
        // tuned against real trees (agent-spec §18.3, plan open-question 1); do
        // not treat them as validated. The largest weights sit on the signals
        // that most cleanly separate "matters" from "machine output": path class
        // and project markers.
        Self {
            extension_diversity: 0.15,
            mtime_recency: 0.15,
            project_marker: 0.20,
            path_class: 0.25,
            visibility: 0.10,
            visit_activity: 0.10,
            last_used: 0.05,

            mtime_half_life_secs: 30.0 * 24.0 * 60.0 * 60.0,
            last_used_half_life_secs: 14.0 * 24.0 * 60.0 * 60.0,
            visit_saturation_count: 10.0,
        }
    }
}

impl Weights {
    /// The default additive coefficient for one signal, before redistribution.
    pub fn additive_weight(&self, signal: SignalKind) -> f64 {
        match signal {
            SignalKind::ExtensionDiversity => self.extension_diversity,
            SignalKind::MtimeRecency => self.mtime_recency,
            SignalKind::ProjectMarker => self.project_marker,
            SignalKind::PathClass => self.path_class,
            SignalKind::Visibility => self.visibility,
            SignalKind::VisitActivity => self.visit_activity,
            SignalKind::LastUsed => self.last_used,
        }
    }

    /// The sum of the seven additive coefficients. `1.0` at the defaults.
    pub fn additive_total(&self) -> f64 {
        SignalKind::ALL.iter().map(|&s| self.additive_weight(s)).sum()
    }
}
