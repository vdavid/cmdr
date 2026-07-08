//! The pure importance scorer: values in, score out, no I/O.
//!
//! [`score`] maps a folder's [`FolderSignals`] (weighted by [`Weights`], with
//! the availability [`SignalSet`] driving missing-signal redistribution) to a
//! `0.0..=1.0` [`Score`]. [`explain`] returns the same number plus the per-signal
//! [`SignalContribution`] breakdown that sums to it. Both are pure functions — no
//! `rusqlite`, no `Volume`, no filesystem, no clocks; "now" is passed in as a
//! value so recency is deterministic in tests.
//!
//! See the [`importance` module docs](../DETAILS.md) for the signal catalog and
//! the tunable-weights rationale.

pub mod types;
pub mod weights;

pub use types::{
    Explanation, FolderSignals, PathClass, Score, SignalContribution, SignalKind, SignalSet, extension_count,
};
pub use weights::Weights;

/// Scores a folder. See the [module docs](self) for the contract.
pub fn score(inputs: &FolderSignals, available: &SignalSet, weights: &Weights, now_secs: u64) -> Score {
    explain(inputs, available, weights, now_secs).score
}

/// Scores a folder AND returns the per-signal contribution breakdown.
///
/// The single source of truth for the formula: [`score`] delegates here and
/// returns only the scalar. The returned contributions sum (then clamp) to the
/// score whenever no floor override fires; when the denylist or the hidden/system
/// flag floors the score, [`Explanation::floored`] is `true` and the additive
/// terms are reported at the values they *would* have contributed, so a tuner can
/// still see the underlying signal shape.
pub fn explain(inputs: &FolderSignals, available: &SignalSet, weights: &Weights, now_secs: u64) -> Explanation {
    let contributions = per_signal_contributions(inputs, available, weights, now_secs);

    let additive: f64 = contributions.iter().map(|c| c.contribution).sum();

    // FLOOR overrides: a denylisted name or a hidden/system folder caps the score
    // at the floor regardless of its other signals (plan Decision 3). These are
    // hard caps, not additive terms, so they live outside the weighted sum.
    let floored = inputs.name_denylisted || inputs.hidden_or_system;
    let score = if floored {
        Score::FLOOR
    } else {
        Score::clamped(additive)
    };

    Explanation {
        score,
        contributions,
        floored,
    }
}

/// Computes each signal's contribution, applying missing-signal redistribution.
///
/// Weight for an unavailable optional signal ([`SignalSet`]) is spread
/// proportionally across the available signals, so a folder is never penalized
/// for a signal its backend can't produce (plan Decision 3). The `Visibility`
/// term reflects the SOFT (non-floor) side of hidden/system: even when not
/// floored, being visible contributes; the hard floor is applied in [`explain`].
fn per_signal_contributions(
    inputs: &FolderSignals,
    available: &SignalSet,
    weights: &Weights,
    now_secs: u64,
) -> Vec<SignalContribution> {
    let effective = effective_weights(available, weights);

    SignalKind::ALL
        .iter()
        .map(|&signal| {
            let weight = effective.weight_of(signal);
            let raw = raw_signal_value(signal, inputs, weights, now_secs);
            SignalContribution {
                signal,
                weight,
                raw,
                contribution: weight * raw,
            }
        })
        .collect()
}

/// The per-signal effective weights after redistributing unavailable ones.
struct EffectiveWeights {
    extension_diversity: f64,
    mtime_recency: f64,
    project_marker: f64,
    path_class: f64,
    visibility: f64,
    visit_activity: f64,
    last_used: f64,
}

impl EffectiveWeights {
    fn weight_of(&self, signal: SignalKind) -> f64 {
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
}

/// Redistributes the weight of unavailable optional signals across the available
/// ones, preserving the total. If `visit`/`last_used` is unavailable, its default
/// coefficient is removed and the remaining coefficients are scaled up so they sum
/// to the same total as before (plan Decision 3: redistribute, never fabricate).
fn effective_weights(available: &SignalSet, weights: &Weights) -> EffectiveWeights {
    let total = weights.additive_total();

    // Sum of the coefficients whose signal is available.
    let mut available_sum = 0.0;
    for &signal in &SignalKind::ALL {
        if signal_available(signal, available) {
            available_sum += weights.additive_weight(signal);
        }
    }

    // Scale factor that lifts the available coefficients back up to `total`.
    // Guard the degenerate all-unavailable case (can't happen: the listing
    // signals are always available) so we never divide by zero.
    let scale = if available_sum > 0.0 {
        total / available_sum
    } else {
        0.0
    };

    let scaled = |signal: SignalKind| {
        if signal_available(signal, available) {
            weights.additive_weight(signal) * scale
        } else {
            0.0
        }
    };

    EffectiveWeights {
        extension_diversity: scaled(SignalKind::ExtensionDiversity),
        mtime_recency: scaled(SignalKind::MtimeRecency),
        project_marker: scaled(SignalKind::ProjectMarker),
        path_class: scaled(SignalKind::PathClass),
        visibility: scaled(SignalKind::Visibility),
        visit_activity: scaled(SignalKind::VisitActivity),
        last_used: scaled(SignalKind::LastUsed),
    }
}

/// Whether a signal is available. The five always-present listing signals are
/// always available; the two optional ones consult the [`SignalSet`].
fn signal_available(signal: SignalKind, available: &SignalSet) -> bool {
    match signal {
        SignalKind::VisitActivity => available.visit_available,
        SignalKind::LastUsed => available.last_used_available,
        _ => true,
    }
}

/// The normalized `0.0..=1.0` value of one signal for a folder.
fn raw_signal_value(signal: SignalKind, inputs: &FolderSignals, weights: &Weights, now_secs: u64) -> f64 {
    match signal {
        SignalKind::ExtensionDiversity => extension_diversity(inputs),
        SignalKind::MtimeRecency => recency(inputs.mtime_secs, now_secs, weights.mtime_half_life_secs),
        SignalKind::ProjectMarker => {
            if inputs.has_project_marker {
                1.0
            } else {
                0.0
            }
        }
        SignalKind::PathClass => path_class_prior(inputs.path_class),
        SignalKind::Visibility => {
            if inputs.hidden_or_system {
                0.0
            } else {
                1.0
            }
        }
        SignalKind::VisitActivity => visit_activity(inputs.visit_count, weights.visit_saturation_count),
        SignalKind::LastUsed => recency(inputs.last_used_secs, now_secs, weights.last_used_half_life_secs),
    }
}

/// Extension diversity, normalized `0.0..=1.0`.
///
/// A folder with no files is neutral (`0.0` — nothing to say). Otherwise the
/// value rises with the count of distinct extensions relative to the file count:
/// a monoculture (one extension over many files) sits near the floor, and a mix
/// of kinds sits high. Concretely, `distinct / min(file_count, cap)` clamped —
/// so three files of three types already reads as diverse, while 200 `.log` files
/// (one extension) reads as a monoculture.
fn extension_diversity(inputs: &FolderSignals) -> f64 {
    if inputs.file_count == 0 {
        return 0.0;
    }
    // Cap the denominator so a genuinely varied folder with a moderate file count
    // reaches near-1.0 without needing hundreds of distinct extensions.
    const DIVERSITY_CAP: f64 = 5.0;
    let denom = (inputs.file_count as f64).min(DIVERSITY_CAP);
    ((inputs.distinct_extension_count as f64) / denom).clamp(0.0, 1.0)
}

/// Exponential recency decay in `0.0..=1.0`: `1.0` at `now`, `0.5` at one
/// half-life ago, approaching `0.0` for very old timestamps. `None` (no
/// timestamp) is neutral (`0.0`). A timestamp in the future (clock skew) clamps
/// to `1.0`.
fn recency(ts_secs: Option<u64>, now_secs: u64, half_life_secs: f64) -> f64 {
    let Some(ts) = ts_secs else {
        return 0.0;
    };
    if ts >= now_secs {
        return 1.0;
    }
    if half_life_secs <= 0.0 {
        return 0.0;
    }
    let age = (now_secs - ts) as f64;
    // 0.5 ^ (age / half_life)
    2f64.powf(-age / half_life_secs)
}

/// The path-class prior in `0.0..=1.0`.
fn path_class_prior(class: PathClass) -> f64 {
    match class {
        PathClass::ProjectRoot => 1.0,
        PathClass::UserContent => 0.8,
        PathClass::Neutral => 0.4,
        PathClass::SystemOrCache => 0.0,
    }
}

/// Visit-activity signal in `0.0..=1.0`: linear up to the saturation count, then
/// flat at `1.0`. `None` (signal absent) is neutral (`0.0`).
fn visit_activity(visit_count: Option<u32>, saturation: f64) -> f64 {
    let Some(count) = visit_count else {
        return 0.0;
    };
    if saturation <= 0.0 {
        return 0.0;
    }
    ((count as f64) / saturation).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests;
