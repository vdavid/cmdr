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

use cmdr_fs::path_hash::{PrehashedState, hash_path};
use rayon::prelude::*;

use super::arena::SearchIndex;
use super::context::NameRules;
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

/// The fewest matches one parallel ranking task takes, so the per-task folder memo
/// stays warm (`rank_indices`). ~230 tasks for 1.9 M matches, against 16 workers.
const RANK_MIN_SPLIT: usize = 8_192;

/// The wildcard-free query stem used for match-quality classification, or an empty
/// string when the pattern carries a wildcard or is regex.
///
/// Only a plain glob substring (`report`, auto-wrapped to `*report*` for matching)
/// has a meaningful exact-vs-prefix-vs-substring gradient. A wildcard glob
/// (`report*`, `*.pdf`) or a regex has no such gradient, so it returns `""` and the
/// whole result set stays in one match-quality band (ranked by recency alone). Under
/// `NameRules::nfd_patterns` (macOS) the stem is NFD-normalized to match the arena's NFD filenames — the same
/// normalization the engine's matcher applies to the pattern.
pub(crate) fn stem_for(query: &SearchQuery, rules: &NameRules) -> String {
    match (&query.pattern_type, &query.name_pattern) {
        (PatternType::Glob, Some(p)) if !p.is_empty() && !p.contains('*') && !p.contains('?') => {
            rules.normalize_pattern(p).into_owned()
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
    // Allocation-free for the two cases that cover essentially every real filename;
    // this runs once per MATCHED entry, and a broad query matches millions, so the
    // two `to_lowercase()` Strings the general path needs are the difference between
    // a ranked result set and a stall. The general path stays exactly as it was, so
    // Unicode case folding (final-sigma and friends) is unchanged where it matters.
    if !case_insensitive {
        return band_of(name == stem, name.starts_with(stem));
    }
    if name.is_ascii() && stem.is_ascii() {
        return band_of(
            name.eq_ignore_ascii_case(stem),
            name.len() >= stem.len() && name[..stem.len()].eq_ignore_ascii_case(stem),
        );
    }
    let (name_cmp, stem_cmp) = (name.to_lowercase(), stem.to_lowercase());
    band_of(name_cmp == stem_cmp, name_cmp.starts_with(&stem_cmp))
}

/// The band for an already-computed (equal, starts-with) pair.
fn band_of(exact: bool, prefix: bool) -> MatchQuality {
    if exact {
        MatchQuality::Exact
    } else if prefix {
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
/// the host (the desktop's `search/volumes.rs`, from `cmdr-index`'s `ImportanceIndex`);
/// the engine only ever sees this read-only view, so it stays pure.
///
/// ## Why the paths aren't stored
///
/// Root's map is permanently resident and real volumes are big (a measured 158,457
/// scored folders on a home, 368,043 on a NAS, absolute paths averaging 113 bytes), so
/// every byte per folder is steady-state cost. Nothing ever ENUMERATES this map —
/// `weight_for` does exact lookups and nothing else — so the paths
/// themselves are dead weight once they've been hashed. Storing
/// [`hash_path`] in their place leaves a 17-byte table slot (a
/// `(u64, f64)` entry plus its control byte) as the entire per-folder cost:
/// 58 MB → 8.9 MB on that NAS, 27 MB → 4.5 MB on that home. Guarded by
/// `memory_tests.rs`.
///
/// **Don't narrow the weight to an `f32` for size.** `(u64, f32)` and `(u64, f64)` are
/// both 16 bytes: the key's 8-byte alignment pads the `f32` straight back, so the
/// narrower value buys zero bytes and only costs precision. The `u64` key is what makes
/// the entry small, and it sets the floor.
///
/// ## Collisions
///
/// Two different folders whose paths hash to the same `u64` share one entry, so one of
/// them reads the other's weight. At 368,043 keys against 64 bits that's a ~3.7e-9
/// chance of ANY collision on a volume that size. The failure mode is soft by
/// construction: a weight is never a penalty and importance only ever reorders WITHIN a
/// match-quality band, so a collision at worst gives one unrelated folder's files a
/// spurious nudge among equally-good matches. It cannot corrupt a result, drop one, or
/// lift a weaker match above a stronger one.
///
/// [`remove`](Self::remove) widens that failure mode slightly, and deliberately: a
/// removal keyed on a colliding hash DROPS the other folder's legitimate weight
/// instead of nudging it. The result is still bounded (that folder ranks by pure
/// recency within its band, which is the neutral no-importance behavior) and still
/// self-healing (the next FULL pass rebuilds the map from scratch). At the collision
/// probability above it's an event that essentially never happens; the point of naming
/// it is that the delta path made "wrong weight" into "no weight", not that the risk
/// changed size.
///
/// ## Patching
///
/// The map is rebuilt wholesale from a full pass and PATCHED from an incremental one
/// (`cmdr_index::importance::read::WeightDelta`, applied in
/// the host's `search/volumes.rs`). The invariant a patch must preserve: the map holds
/// exactly the folders scoring ABOVE zero, matching what `for_each_nonzero_weight`
/// streams. That's why the delta reports a rescored-to-zero folder as a removal
/// rather than an [`insert`](Self::insert) of `0.0` — an absent key and a `0.0` entry
/// rank identically, so keeping only one of the two shapes is what lets a patched map
/// and a rebuilt one be compared for equality.
#[derive(Debug, Default, Clone)]
// A patched map and a rebuilt one must be the same map; comparing them directly is
// how that's pinned (`a_patched_map_matches_one_rebuilt_from_the_store`).
#[cfg_attr(any(test, feature = "testing"), derive(PartialEq))]
pub struct ImportanceWeights {
    /// `hash_path(folder absolute path)` → importance scalar (`0.0..=1.0`). Keyed off
    /// the SAME absolute-path shape the search index reconstructs (`/Users/…`, no `~`),
    /// so a lookup with a reconstructed parent path hits the right row.
    map: std::collections::HashMap<u64, f64, PrehashedState>,
}

impl ImportanceWeights {
    /// An empty weight map: every lookup is `0.0`. The neutral state for an
    /// unscored volume, a missing `importance.db`, or a disabled feature.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Record one folder's weight. The path is hashed and dropped; only the hash and
    /// the weight are kept. The per-volume load streams rows through here so the full
    /// `path → weight` map never exists in memory at all.
    pub fn insert(&mut self, folder_path: &str, weight: f64) {
        self.map.insert(hash_path(folder_path), weight);
    }

    /// Drop one folder's weight, so it ranks as unscored (`0.0`, neutral) again. The
    /// incremental-delta path calls this for a folder whose row left the store — it
    /// was renamed away, deleted, became floored, or rescored to zero.
    ///
    /// Keyed by the path hash like everything else here, so on a hash collision this
    /// drops the OTHER folder's weight; see the type's "Collisions" note for why
    /// that's bounded and self-healing.
    pub fn remove(&mut self, folder_path: &str) {
        self.map.remove(&hash_path(folder_path));
    }

    /// Build from a path→weight map. A test convenience; production streams rows
    /// through [`insert`](Self::insert) instead of materializing this map.
    #[cfg(any(test, feature = "testing"))]
    pub fn from_map(map: std::collections::HashMap<String, f64>) -> Self {
        let mut weights = Self::empty();
        for (path, weight) in map {
            weights.insert(&path, weight);
        }
        weights
    }

    /// The weight for a folder path, or `0.0` when unscored/absent. `0.0` is
    /// neutral in the blend (multiplier `1.0`), never a penalty. Production ranks via
    /// [`weight_for_hash`](Self::weight_for_hash) (it never builds the path); this
    /// path-taking form is what the tests assert against, hence `#[cfg(test)]`.
    #[cfg(any(test, feature = "testing"))]
    pub fn weight_for(&self, folder_path: &str) -> f64 {
        self.weight_for_hash(hash_path(folder_path))
    }

    /// The weight for a folder whose path the caller has already hashed with
    /// [`hash_path`]. The ranking hot path hashes a folder's path straight off the
    /// index's parent chain ([`PathHasher`](cmdr_fs::path_hash::PathHasher)) instead of materializing a `String` only
    /// to hash it and drop it.
    pub(crate) fn weight_for_hash(&self, path_hash: u64) -> f64 {
        self.map.get(&path_hash).copied().unwrap_or(0.0)
    }

    /// How many scored folders the map holds. For the load-time log line, which is how
    /// a volume's real scored-folder count gets measured.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether any weights are present. When empty, the engine can skip the whole
    /// per-result parent-path reconstruction the blend would need (a fast path that
    /// also guarantees byte-for-byte-today behavior).
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// A matched entry's full sort key: its match-quality band, its importance-boosted
/// recency, and its entry id (the final deterministic tiebreak).
///
/// Internal to ranking: it decorates each match once so the sort compares scalars
/// instead of recomputing bands, and is dropped as soon as the order is fixed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RankKey {
    band: MatchQuality,
    boosted_recency: f64,
    id: i64,
}

impl RankKey {
    /// Order two keys best-first: higher band, then higher boosted recency, then
    /// lower id (stable, deterministic). The exact ordering the single-volume sort
    /// applies, reused for the cross-volume merge.
    pub(crate) fn cmp_best_first(&self, other: &Self) -> std::cmp::Ordering {
        other
            .band
            .cmp(&self.band)
            .then_with(|| other.boosted_recency.total_cmp(&self.boosted_recency))
            .then_with(|| self.id.cmp(&other.id))
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
/// Pure: no I/O. Production calls [`rank_indices`]; this in-place form is what the
/// ranking tests assert against, hence `#[cfg(test)]`.
#[cfg(test)]
pub(crate) fn rank(
    index: &SearchIndex,
    matching: &mut [usize],
    stem: &str,
    case_insensitive: bool,
    weights: &ImportanceWeights,
) {
    let ranked = rank_indices(index, matching, stem, case_insensitive, weights, usize::MAX);
    matching.copy_from_slice(&ranked);
}

/// Rank matched entry indices and return the best `keep` of them, best-first. THE
/// production entry point: the returned order is the result order, so nothing
/// downstream sorts again.
///
/// `keep` is how many results the caller can actually use (`usize::MAX` for "all of
/// them", which the count-only directory pass needs). Everything below it is
/// partitioned away instead of sorted, because a search matching millions of entries
/// still shows 30: a full `sort` of every match is work thrown away by the truncate
/// that follows. The comparator is a TOTAL order (the final tiebreak is the unique
/// entry id), so the top-`keep` set and its order are unique — an unstable partition
/// gives exactly what the full sort did.
///
/// Pure: no I/O. Decorate-sort-undecorate computes each entry's key EXACTLY ONCE (a
/// naive `sort_by` recomputes it per comparison). The decorate pass is the expensive
/// half when importance weights are present, so it runs in parallel with a per-thread
/// folder→weight memo: matches cluster heavily by folder, and hashing a folder's path
/// means walking its parent chain. The empty-map fast path skips that entirely,
/// preserving today's pure-recency order (the degradation contract).
pub(crate) fn rank_indices(
    index: &SearchIndex,
    matching: &[usize],
    stem: &str,
    case_insensitive: bool,
    weights: &ImportanceWeights,
    keep: usize,
) -> Vec<usize> {
    let no_weights = weights.is_empty();
    let key_for = |memo: &mut std::collections::HashMap<i64, f64>, idx: usize| {
        let band = classify_match(index.name(idx), stem, case_insensitive);
        let recency = index.modified_at(idx).unwrap_or(0);
        let boosted_recency = if no_weights {
            recency as f64
        } else {
            // A file takes its parent folder's weight; a folder takes its own.
            let folder_id = if index.is_directory(idx) {
                index.id(idx)
            } else {
                index.parent_id(idx)
            };
            let weight = *memo
                .entry(folder_id)
                .or_insert_with(|| weights.weight_for_hash(super::engine::hash_path_from_index(index, folder_id)));
            boosted_recency_key(recency, weight)
        };
        (
            RankKey {
                band,
                boosted_recency,
                id: index.id(idx),
            },
            idx,
        )
    };

    let mut decorated: Vec<(RankKey, usize)> = if no_weights {
        // No weight lookups ⇒ no memo to keep warm and a cheap body; the sequential
        // pass avoids rayon's split overhead.
        let mut memo = std::collections::HashMap::new();
        matching.iter().map(|&idx| key_for(&mut memo, idx)).collect()
    } else {
        // `map_init` builds a fresh memo per rayon split, and each one grows from empty,
        // so a floor on the split length keeps the memos few and warm: unbounded, a
        // broad query split thousands of ways and re-hashed the same folders in each.
        matching
            .par_iter()
            .with_min_len(RANK_MIN_SPLIT)
            .map_init(std::collections::HashMap::new, |memo, &idx| key_for(memo, idx))
            .collect()
    };

    if keep < decorated.len() {
        decorated.select_nth_unstable_by(keep, |a, b| a.0.cmp_best_first(&b.0));
        decorated.truncate(keep);
    }
    decorated.sort_unstable_by(|a, b| a.0.cmp_best_first(&b.0));
    decorated.into_iter().map(|(_, idx)| idx).collect()
}

#[cfg(test)]
mod memory_tests;
#[cfg(test)]
mod tests;
