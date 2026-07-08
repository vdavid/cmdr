//! Pure result ranking: blend a result's match quality with its parent folder's
//! importance weight, so interesting files rise toward the top without ever
//! letting importance override a clearly-better name match.
//!
//! This is a pure module (no I/O, no DB): [`engine`](super::engine) hands it the
//! matched entries plus an importance lookup (a closure over a per-volume weight
//! map, built in `commands/search.rs`), and it returns the ranked order. Keeping
//! it separate from the hot scan loop makes the ranking policy directly testable
//! and keeps `engine.rs` focused.
//!
//! ## The blend (why it's constructed this way)
//!
//! Two design requirements pull against each other:
//!
//! 1. Match quality must DOMINATE: an exact filename match in a boring folder has
//!    to beat a weaker match in an important folder, always.
//! 2. Importance should still help: among matches of the SAME quality, the more
//!    important folder's file should rank higher.
//!
//! We satisfy both by construction with a lexicographic sort: results are grouped
//! into match-quality BANDS ([`MatchQuality`]), and importance only ever reorders
//! WITHIN a band — it can never lift a result across a band boundary. So (1) holds
//! no matter how large a weight is, and (2) is the within-band tiebreak/boost.
//!
//! Within a band the sort key is `recency * (1 + IMPORTANCE_BLEND_COEFF * weight)`:
//! a modest multiplicative nudge on the existing recency ordering. With weight
//! `0.0` (no importance data, a floored folder, or an unscored volume) the
//! multiplier is exactly `1.0`, so the within-band order collapses to pure recency
//! — byte-for-byte today's behavior. That's the degradation contract: absent
//! importance, ranking equals what it was before this feature.

use super::index::SearchIndex;
use super::types::{PatternType, SearchQuery};

/// How well a result's name matches the user's search pattern, as a coarse band.
///
/// Ranking sorts by this FIRST (higher variant wins), so importance — applied only
/// within a band — can never lift a weaker match above a stronger one. The bands
/// are deliberately few: the goal is the dominance property ("exact beats fuzzy"),
/// not a fine-grained relevance score.
///
/// Ordered worst-to-best so the derived `Ord` ranks a higher band as greater.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MatchQuality {
    /// The pattern matched somewhere in the name, but not as a prefix or the whole
    /// name (a mid-string substring), or the pattern carries wildcards/regex so no
    /// meaningful name-quality gradient applies. This is the neutral band that
    /// preserves today's recency ordering.
    Other,
    /// The name starts with the query stem (a prefix match).
    Prefix,
    /// The name equals the query stem exactly (case-normalized). The strongest band.
    Exact,
}

/// How strongly a parent folder's importance weight boosts a result WITHIN its
/// match-quality band. A conservative default: at the maximum weight of `1.0`, a
/// result's recency key is scaled by `1 + 0.5 = 1.5`, enough to lift an important
/// folder's file over a same-quality match roughly half a "recency generation"
/// newer, but nowhere near enough to matter across bands (bands are compared
/// first). A future tunable — raise it to make importance more assertive among
/// equal-quality matches. Not yet validated against real trees (the importance
/// weights themselves are a starting point; see `importance/scorer/weights.rs`).
pub(crate) const IMPORTANCE_BLEND_COEFF: f64 = 0.5;

/// The wildcard-free query stem used for match-quality classification, or an empty
/// string when the pattern carries a wildcard or is regex.
///
/// Only a plain glob substring (`report`, auto-wrapped to `*report*` for matching)
/// has a meaningful exact-vs-prefix-vs-substring gradient. A wildcard glob
/// (`report*`, `*.pdf`) or a regex has no such gradient, so it returns `""` and the
/// whole result set stays in one match-quality band (ranked by recency alone). On
/// macOS the stem is NFD-normalized to match the arena's NFD filenames — the same
/// normalization the engine's matcher applies to the pattern.
pub(crate) fn stem_for(query: &SearchQuery) -> String {
    match (&query.pattern_type, &query.name_pattern) {
        (PatternType::Glob, Some(p)) if !p.is_empty() && !p.contains('*') && !p.contains('?') => {
            #[cfg(target_os = "macos")]
            {
                use unicode_normalization::UnicodeNormalization;
                p.nfd().collect::<String>()
            }
            #[cfg(not(target_os = "macos"))]
            {
                p.clone()
            }
        }
        _ => String::new(),
    }
}

/// Classify how well `name` matches the user's plain-text query stem.
///
/// Only a wildcard-free, plain substring query (the auto-wrapped `*stem*` case)
/// carries a meaningful name-quality gradient, so that's the only case that
/// produces `Exact`/`Prefix`; every wildcard glob and every regex returns `Other`
/// for all results, leaving the whole result set in one band (pure recency order,
/// unchanged from before importance ranking existed).
///
/// `stem` and `name` are compared after the same normalization the engine's
/// matcher uses (case-folding on macOS via `case_insensitive`), passed in as
/// `case_insensitive` so this stays pure and platform-agnostic.
pub(crate) fn classify_match(name: &str, stem: &str, case_insensitive: bool) -> MatchQuality {
    // No stem (wildcard glob, regex, or empty pattern): no gradient, one band.
    if stem.is_empty() {
        return MatchQuality::Other;
    }
    let (name_cmp, stem_cmp) = if case_insensitive {
        (name.to_lowercase(), stem.to_lowercase())
    } else {
        (name.to_string(), stem.to_string())
    };
    if name_cmp == stem_cmp {
        MatchQuality::Exact
    } else if name_cmp.starts_with(&stem_cmp) {
        MatchQuality::Prefix
    } else {
        MatchQuality::Other
    }
}

/// The importance weight for a result's PARENT folder, as data the ranker blends.
///
/// A file takes its parent folder's weight; a folder takes its own. Absent
/// importance data the map is empty and every lookup returns `0.0` — neutral,
/// never a penalty (the degradation contract). Built per-volume in
/// `commands/search.rs` from [`ImportanceIndex`](crate::importance::ImportanceIndex);
/// the engine only ever sees this read-only view, so it stays pure.
#[derive(Debug, Default)]
pub(crate) struct ImportanceWeights {
    /// Folder absolute path → importance scalar (`0.0..=1.0`). Keyed by the SAME
    /// absolute-path shape the search index reconstructs (`/Users/…`, no `~`), so a
    /// lookup with a reconstructed parent path hits the right row.
    map: std::collections::HashMap<String, f64>,
}

impl ImportanceWeights {
    /// An empty weight map: every lookup is `0.0`. The neutral state for an
    /// unscored volume, a missing `importance.db`, or a disabled feature.
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    /// Build from a path→weight map (the per-volume snapshot loaded from the read
    /// API).
    pub(crate) fn from_map(map: std::collections::HashMap<String, f64>) -> Self {
        Self { map }
    }

    /// The weight for a folder path, or `0.0` when unscored/absent. `0.0` is
    /// neutral in the blend (multiplier `1.0`), never a penalty.
    pub(crate) fn weight_for(&self, folder_path: &str) -> f64 {
        self.map.get(folder_path).copied().unwrap_or(0.0)
    }

    /// Whether any weights are present. When empty, the engine can skip the whole
    /// per-result parent-path reconstruction the blend would need (a fast path that
    /// also guarantees byte-for-byte-today behavior).
    pub(crate) fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// The recency-and-importance sort key for one matched entry, WITHIN its band.
///
/// `recency` is the entry's `modified_at` (0 when unknown, as today). The returned
/// key is `recency * (1 + IMPORTANCE_BLEND_COEFF * weight)`: a multiplicative nudge
/// that is exactly `recency` when `weight == 0.0`. Kept as a small pure helper so
/// the "weight 0 ⇒ unchanged" property is unit-testable in isolation.
pub(crate) fn boosted_recency_key(recency: u64, weight: f64) -> f64 {
    recency as f64 * (1.0 + IMPORTANCE_BLEND_COEFF * weight)
}

/// Rank matched entry indices: sort by match-quality band (best first), then by
/// importance-boosted recency within each band (highest first).
///
/// `matching` is the set of entry indices the scan produced; `stem` is the
/// wildcard-free query stem (empty for wildcard/regex/empty patterns, which then
/// all land in one `Other` band). `weights` supplies each entry's parent-folder
/// importance. The sort is deterministic (a stable final tiebreak on entry id) so
/// equal keys don't reorder run to run.
///
/// Pure: no I/O. The engine calls this after its parallel filter, before
/// truncating to the limit.
pub(crate) fn rank(
    index: &SearchIndex,
    matching: &mut [usize],
    stem: &str,
    case_insensitive: bool,
    weights: &ImportanceWeights,
) {
    // Decorate-sort-undecorate: compute each entry's sort key EXACTLY ONCE, then
    // sort the decorated pairs. A naive `sort_by` recomputes the key for both
    // operands on every comparison — O(n log n) key computations, and each key with
    // weights does an O(depth) parent-path reconstruction. A result set can be tens
    // of thousands of candidates, so computing keys once (O(n)) instead is the
    // difference that keeps ranking off the hot path.
    //
    // The empty-map fast path skips the per-entry path reconstruction entirely,
    // preserving today's pure-recency order (the degradation contract).
    let no_weights = weights.is_empty();
    let mut decorated: Vec<(MatchQuality, f64, i64, usize)> = matching
        .iter()
        .map(|&idx| {
            let entry = &index.entries[idx];
            let band = classify_match(index.name(entry), stem, case_insensitive);
            let recency = entry.modified_at.unwrap_or(0);
            let key = if no_weights {
                recency as f64
            } else {
                // A file takes its parent folder's weight; a folder takes its own.
                let folder_id = if entry.is_directory { entry.id } else { entry.parent_id };
                let folder_path = super::engine::reconstruct_path_from_index(index, folder_id);
                boosted_recency_key(recency, weights.weight_for(&folder_path))
            };
            (band, key, entry.id, idx)
        })
        .collect();

    // Band descending, then boosted recency descending, then id ascending (a stable,
    // deterministic final tiebreak so equal keys don't reorder run to run).
    decorated.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.total_cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    for (slot, dec) in matching.iter_mut().zip(decorated.iter()) {
        *slot = dec.3;
    }
}

#[cfg(test)]
mod tests;
