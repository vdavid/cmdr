//! Pure ranking tests: the dominance property (match quality beats importance),
//! within-band importance ordering, and the degradation contract (zero/absent
//! weights reproduce today's pure-recency order).

use std::collections::HashMap;

use super::*;
use crate::search::index::{SearchEntry, SearchIndex};

// ── Index builder ─────────────────────────────────────────────────────────

/// Push a name into the arena and return `(offset, len)`.
fn arena_push(names: &mut String, name: &str) -> (u32, u16) {
    let offset = names.len() as u32;
    let len = name.len() as u16;
    names.push_str(name);
    (offset, len)
}

/// A file entry under a given parent, with a name and mtime. Directories/parents
/// are added separately so paths reconstruct.
struct Spec {
    id: i64,
    parent_id: i64,
    name: &'static str,
    is_directory: bool,
    modified_at: Option<u64>,
}

/// Build an index from specs. Every parent referenced must appear as a spec (or be
/// `ROOT_ID`-adjacent); the root sentinel (id 1, name "") is added automatically.
fn build_index(specs: &[Spec]) -> SearchIndex {
    let mut names = String::new();
    let root = arena_push(&mut names, "");
    let mut entries = vec![SearchEntry {
        id: 1,
        parent_id: 0,
        name_offset: root.0,
        name_len: root.1,
        is_directory: true,
        size: None,
        modified_at: None,
    }];
    for s in specs {
        let (off, len) = arena_push(&mut names, s.name);
        entries.push(SearchEntry {
            id: s.id,
            parent_id: s.parent_id,
            name_offset: off,
            name_len: len,
            is_directory: s.is_directory,
            size: None,
            modified_at: s.modified_at,
        });
    }
    let mut id_to_index = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        id_to_index.insert(e.id, i);
    }
    SearchIndex {
        names,
        entries,
        id_to_index,
        generation: 1,
    }
}

/// The ordered result names after ranking the given matching entry ids.
fn ranked_names(index: &SearchIndex, matching_ids: &[i64], stem: &str, weights: &ImportanceWeights) -> Vec<String> {
    let mut matching: Vec<usize> = matching_ids.iter().map(|id| index.id_to_index[id]).collect();
    rank(index, &mut matching, stem, false, weights);
    matching
        .iter()
        .map(|&i| index.name(&index.entries[i]).to_string())
        .collect()
}

// ── classify_match ─────────────────────────────────────────────────────────

#[test]
fn classify_exact_prefix_other() {
    assert_eq!(classify_match("report", "report", false), MatchQuality::Exact);
    assert_eq!(classify_match("report.pdf", "report", false), MatchQuality::Prefix);
    assert_eq!(classify_match("Q1-report.pdf", "report", false), MatchQuality::Other);
}

#[test]
fn classify_empty_stem_is_other() {
    // A wildcard glob or regex has no stem: everything lands in one band so recency
    // alone orders (today's behavior for those patterns).
    assert_eq!(classify_match("anything.pdf", "", false), MatchQuality::Other);
}

#[test]
fn classify_case_insensitive_exact() {
    assert_eq!(classify_match("REPORT", "report", true), MatchQuality::Exact);
    assert_eq!(classify_match("REPORT", "report", false), MatchQuality::Other);
}

// ── boosted_recency_key ─────────────────────────────────────────────────────

#[test]
fn zero_weight_leaves_recency_unchanged() {
    // The degradation contract in miniature: weight 0 ⇒ key == recency exactly.
    assert_eq!(boosted_recency_key(1000, 0.0), 1000.0);
}

#[test]
fn positive_weight_raises_the_key() {
    assert!(boosted_recency_key(1000, 1.0) > 1000.0);
    assert_eq!(boosted_recency_key(1000, 1.0), 1000.0 * (1.0 + IMPORTANCE_BLEND_COEFF));
}

// ── The dominance property (THE load-bearing test) ──────────────────────────

/// An EXACT filename match in a boring (unimportant) folder MUST outrank a weaker
/// (substring) match in a very important folder — no matter how large the
/// importance weight is. This is the "exact in a boring folder beats fuzzy in
/// Documents" property, and it must hold BY CONSTRUCTION (bands compared before
/// importance), so a deliberately-wrong blend that folds importance into the band
/// comparison fails this.
#[test]
fn exact_match_beats_fuzzy_match_regardless_of_importance() {
    // /boring/report        (id 20) — exact match "report", boring parent (weight 0)
    // /Documents/report.pdf (id 21) — substring match, MAX-importance parent
    let index = build_index(&[
        Spec {
            id: 10,
            parent_id: 1,
            name: "boring",
            is_directory: true,
            modified_at: Some(1),
        },
        Spec {
            id: 11,
            parent_id: 1,
            name: "Documents",
            is_directory: true,
            modified_at: Some(1),
        },
        Spec {
            id: 20,
            parent_id: 10,
            name: "report",
            is_directory: false,
            modified_at: Some(100),
        },
        Spec {
            id: 21,
            parent_id: 11,
            name: "report.pdf",
            is_directory: false,
            modified_at: Some(9_999_999),
        },
    ]);
    // Documents is maximally important; boring is unscored. Even the freshest,
    // most-important weaker match must lose to the exact match.
    let mut map = HashMap::new();
    map.insert("/Documents".to_string(), 1.0);
    let weights = ImportanceWeights::from_map(map);

    let names = ranked_names(&index, &[20, 21], "report", &weights);
    assert_eq!(
        names,
        vec!["report", "report.pdf"],
        "the exact match ranks first despite the other's max importance and newer mtime"
    );
}

/// Even a prefix match beats a mid-string substring match regardless of importance.
#[test]
fn prefix_match_beats_substring_match_regardless_of_importance() {
    let index = build_index(&[
        Spec {
            id: 10,
            parent_id: 1,
            name: "boring",
            is_directory: true,
            modified_at: Some(1),
        },
        Spec {
            id: 11,
            parent_id: 1,
            name: "Documents",
            is_directory: true,
            modified_at: Some(1),
        },
        // "report.pdf" is a PREFIX match for "report"; boring parent.
        Spec {
            id: 20,
            parent_id: 10,
            name: "report.pdf",
            is_directory: false,
            modified_at: Some(100),
        },
        // "Q1-report.pdf" is a mid-string SUBSTRING match; max-important parent, newer.
        Spec {
            id: 21,
            parent_id: 11,
            name: "Q1-report.pdf",
            is_directory: false,
            modified_at: Some(9_999_999),
        },
    ]);
    let mut map = HashMap::new();
    map.insert("/Documents".to_string(), 1.0);
    let weights = ImportanceWeights::from_map(map);

    let names = ranked_names(&index, &[20, 21], "report", &weights);
    assert_eq!(names, vec!["report.pdf", "Q1-report.pdf"]);
}

// ── Importance as a within-band tiebreak/boost ──────────────────────────────

/// Among matches of the SAME quality band, the file in the more important folder
/// ranks higher, even when it's slightly older. This is the secondary boost.
#[test]
fn importance_breaks_ties_within_a_band() {
    // Both are substring matches ("report" mid-string), so same band. The important
    // one is slightly OLDER but must still win on the importance boost.
    let index = build_index(&[
        Spec {
            id: 10,
            parent_id: 1,
            name: "boring",
            is_directory: true,
            modified_at: Some(1),
        },
        Spec {
            id: 11,
            parent_id: 1,
            name: "important",
            is_directory: true,
            modified_at: Some(1),
        },
        Spec {
            id: 20,
            parent_id: 10,
            name: "a-report.pdf",
            is_directory: false,
            modified_at: Some(1000),
        },
        Spec {
            id: 21,
            parent_id: 11,
            name: "b-report.pdf",
            is_directory: false,
            modified_at: Some(900),
        },
    ]);
    let mut map = HashMap::new();
    map.insert("/important".to_string(), 1.0);
    let weights = ImportanceWeights::from_map(map);

    let names = ranked_names(&index, &[20, 21], "report", &weights);
    assert_eq!(
        names,
        vec!["b-report.pdf", "a-report.pdf"],
        "the important folder's file wins the tie despite being older (boost 900*1.5=1350 > 1000)"
    );
}

/// A file takes its PARENT folder's weight; a folder takes its OWN weight.
#[test]
fn folder_uses_own_weight_file_uses_parent_weight() {
    let index = build_index(&[
        Spec {
            id: 10,
            parent_id: 1,
            name: "proj",
            is_directory: true,
            modified_at: Some(500),
        },
        Spec {
            id: 20,
            parent_id: 10,
            name: "sub-report",
            is_directory: false,
            modified_at: Some(500),
        },
    ]);
    // The folder /proj itself is important; the file's parent is also /proj.
    let mut map = HashMap::new();
    map.insert("/proj".to_string(), 1.0);
    let weights = ImportanceWeights::from_map(map);

    // Both are substring matches; both should get the same boost (folder via own
    // path, file via parent path), so they order by their equal recency then id.
    let mut matching: Vec<usize> = [10i64, 20].iter().map(|id| index.id_to_index[id]).collect();
    rank(&index, &mut matching, "report", false, &weights);
    // /proj is a substring match for "report"? No — classify "proj" vs "report" is
    // Other; "sub-report" is Other too. Same band, same recency, boosted equally.
    // Tie broken by id ascending: 10 before 20.
    let names: Vec<String> = matching
        .iter()
        .map(|&i| index.name(&index.entries[i]).to_string())
        .collect();
    assert_eq!(names, vec!["proj", "sub-report"]);
    // Sanity: the folder's own path resolves to a weight.
    assert_eq!(weights.weight_for("/proj"), 1.0);
}

// ── The degradation contract: absent weights == today's recency order ───────

/// With an EMPTY weight map and a wildcard/empty stem (no quality gradient), the
/// order is pure recency descending — byte-for-byte what the engine produced
/// before importance ranking existed.
#[test]
fn empty_weights_and_no_stem_is_pure_recency() {
    let index = build_index(&[
        Spec {
            id: 10,
            parent_id: 1,
            name: "old.pdf",
            is_directory: false,
            modified_at: Some(100),
        },
        Spec {
            id: 11,
            parent_id: 1,
            name: "new.pdf",
            is_directory: false,
            modified_at: Some(300),
        },
        Spec {
            id: 12,
            parent_id: 1,
            name: "mid.pdf",
            is_directory: false,
            modified_at: Some(200),
        },
    ]);
    let weights = ImportanceWeights::empty();
    // Empty stem models a wildcard glob (`*.pdf`) where every result is one band.
    let names = ranked_names(&index, &[10, 11, 12], "", &weights);
    assert_eq!(names, vec!["new.pdf", "mid.pdf", "old.pdf"], "pure recency DESC");
}

/// With an empty weight map but a real stem, banding still applies (that's the new
/// baseline ranking), but WITHIN a band the order is pure recency — importance
/// contributes nothing.
#[test]
fn empty_weights_within_band_is_pure_recency() {
    let index = build_index(&[
        // All three are exact matches for "report" — same band.
        Spec {
            id: 10,
            parent_id: 1,
            name: "report",
            is_directory: false,
            modified_at: Some(100),
        },
        Spec {
            id: 11,
            parent_id: 1,
            name: "report",
            is_directory: false,
            modified_at: Some(300),
        },
        Spec {
            id: 12,
            parent_id: 1,
            name: "report",
            is_directory: false,
            modified_at: Some(200),
        },
    ]);
    let weights = ImportanceWeights::empty();
    // Ids 10/11/12 map to recency 100/300/200 ⇒ 11, 12, 10.
    let mut matching: Vec<usize> = [10i64, 11, 12].iter().map(|id| index.id_to_index[id]).collect();
    rank(&index, &mut matching, "report", false, &weights);
    let recencies: Vec<u64> = matching
        .iter()
        .map(|&i| index.entries[i].modified_at.unwrap())
        .collect();
    assert_eq!(recencies, vec![300, 200, 100], "within-band order is pure recency DESC");
}

/// Determinism: equal band + equal boosted key ⇒ stable id-ascending tiebreak.
#[test]
fn equal_keys_break_by_id_deterministically() {
    let index = build_index(&[
        Spec {
            id: 30,
            parent_id: 1,
            name: "report",
            is_directory: false,
            modified_at: Some(100),
        },
        Spec {
            id: 20,
            parent_id: 1,
            name: "report",
            is_directory: false,
            modified_at: Some(100),
        },
        Spec {
            id: 40,
            parent_id: 1,
            name: "report",
            is_directory: false,
            modified_at: Some(100),
        },
    ]);
    let weights = ImportanceWeights::empty();
    let mut matching: Vec<usize> = [30i64, 20, 40].iter().map(|id| index.id_to_index[id]).collect();
    rank(&index, &mut matching, "report", false, &weights);
    let ids: Vec<i64> = matching.iter().map(|&i| index.entries[i].id).collect();
    assert_eq!(ids, vec![20, 30, 40], "equal keys sort by id ascending");
}

// ── The hashed weight map ─────────────────────────────────────────────────

/// The map keys on a hash instead of the path, so the lookup contract is what has to
/// hold: a stored folder reads its own weight back, and every other path reads the
/// neutral `0.0`. Paths that differ only in a segment, in case, or by a trailing
/// component are the ones a weak hash would confuse.
#[test]
fn a_stored_folder_reads_back_its_own_weight_and_nothing_else_does() {
    let mut weights = ImportanceWeights::empty();
    weights.insert("/Users/me/projects/cmdr", 0.75);
    weights.insert("/Users/me/projects/cmdr/docs", 0.25);

    assert_eq!(weights.weight_for("/Users/me/projects/cmdr"), 0.75);
    assert_eq!(weights.weight_for("/Users/me/projects/cmdr/docs"), 0.25);
    for other in [
        "/Users/me/projects/cmdrx",
        "/Users/me/projects/Cmdr",
        "/Users/me/projects",
        "/Users/me/projects/cmdr/",
        "/Users/me/projects/cmdr/src",
        "",
    ] {
        assert_eq!(weights.weight_for(other), 0.0, "'{other}' is not a stored folder");
    }
}

/// `is_empty` gates the engine's fast path (skip parent-path reconstruction entirely),
/// so it has to stay honest about whether any weight was recorded.
#[test]
fn is_empty_tracks_whether_any_weight_was_recorded() {
    let mut weights = ImportanceWeights::empty();
    assert!(weights.is_empty(), "a fresh map is empty");
    assert_eq!(weights.len(), 0);
    weights.insert("/Users/me", 0.1);
    assert!(!weights.is_empty(), "one weight makes it non-empty");
    assert_eq!(weights.len(), 1);
}

/// The hash is fixed, not `RandomState`: the same path hashes to the same value every
/// time, in this run and in any other. Distinct paths (including near-identical ones)
/// hash apart. A pinned value documents that the function is a stated constant rather
/// than an implementation detail that can drift under the map.
#[test]
fn hash_path_is_deterministic_and_separates_similar_paths() {
    assert_eq!(
        hash_path("/Users/me/projects/cmdr"),
        hash_path("/Users/me/projects/cmdr")
    );
    assert_eq!(
        hash_path("/Users/me/projects/cmdr"),
        0xfc85_ae89_3f25_ed17,
        "the hash is a fixed function; update this only when changing it deliberately"
    );

    let similar = [
        "/Users/me/projects/cmdr",
        "/Users/me/projects/cmds",
        "/Users/me/projects/cmdr/",
        "/Users/me/projects/Cmdr",
        "/users/me/projects/cmdr",
        "",
        "/",
    ];
    let distinct: std::collections::HashSet<u64> = similar.iter().copied().map(hash_path).collect();
    assert_eq!(distinct.len(), similar.len(), "similar paths must not share a hash");
}

/// The finalizer earns its place: paths sharing a long prefix AND differing only late
/// must land in different low bits, because that's where hashbrown takes its bucket
/// index. Raw FNV-1a barely moves its low bits, so a bucket-index collision rate here
/// is what catches dropping the mix.
#[test]
fn hash_path_spreads_sibling_paths_across_buckets() {
    const SIBLINGS: usize = 4096;
    let low_bits: std::collections::HashSet<u64> = (0..SIBLINGS)
        .map(|i| hash_path(&format!("/Users/me/Library/Application Support/vendor/cache/entry-{i}")) & 0xfff)
        .collect();
    // 4,096 hashes into 4,096 buckets fills ~63 % of them when the bits are random;
    // a hash that barely mixes its low bits collapses far below this.
    assert!(
        low_bits.len() > SIBLINGS / 2,
        "sibling paths filled only {} of {SIBLINGS} low-bit buckets",
        low_bits.len()
    );
}
