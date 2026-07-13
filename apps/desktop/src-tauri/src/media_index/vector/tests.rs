//! Vector-store tests (M2 TDD targets): cosine correctness, top-k ranking + source
//! exclusion, and near-duplicate dedup grouping by cosine threshold. Pure, no FFI, no
//! DB — the brute-force store is exercised directly over synthetic vectors.

use super::{BruteForceVectorStore, VectorStore, cosine};

fn store(entries: &[(&str, &[f32])]) -> BruteForceVectorStore {
    BruteForceVectorStore::new(entries.iter().map(|(p, v)| (p.to_string(), v.to_vec())).collect())
}

#[test]
fn cosine_is_one_for_identical_direction_and_zero_for_orthogonal() {
    assert!(
        (cosine(&[1.0, 0.0], &[3.0, 0.0]) - 1.0).abs() < 1e-6,
        "same direction ⇒ 1"
    );
    assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6, "orthogonal ⇒ 0");
    assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6, "opposite ⇒ -1");
}

#[test]
fn cosine_guards_degenerate_inputs() {
    assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0, "zero magnitude ⇒ 0, not NaN");
    assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0, "length mismatch ⇒ 0");
    assert_eq!(cosine(&[], &[]), 0.0, "empty ⇒ 0");
}

#[test]
fn top_k_ranks_by_similarity_and_excludes_the_source() {
    let store = store(&[
        ("/a.jpg", &[1.0, 0.0, 0.0]),
        ("/near.jpg", &[0.9, 0.1, 0.0]),
        ("/mid.jpg", &[0.5, 0.5, 0.0]),
        ("/far.jpg", &[0.0, 0.0, 1.0]),
    ]);
    // Query with /a.jpg's vector, excluding itself: /near ranks above /mid above /far.
    let hits = store.top_k(&[1.0, 0.0, 0.0], 10, Some("/a.jpg"));
    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(paths, vec!["/near.jpg", "/mid.jpg", "/far.jpg"]);
    assert!(!paths.contains(&"/a.jpg"), "the source is excluded");
    // Scores descend.
    assert!(hits[0].score > hits[1].score && hits[1].score > hits[2].score);
}

#[test]
fn top_k_caps_at_k_and_zero_k_is_empty() {
    let store = store(&[
        ("/a.jpg", &[1.0, 0.0]),
        ("/b.jpg", &[0.9, 0.1]),
        ("/c.jpg", &[0.8, 0.2]),
    ]);
    assert_eq!(store.top_k(&[1.0, 0.0], 2, None).len(), 2);
    assert!(store.top_k(&[1.0, 0.0], 0, None).is_empty());
}

#[test]
fn dedup_groups_near_duplicates_above_the_threshold() {
    let store = store(&[
        ("/dup1.jpg", &[1.0, 0.0, 0.0]),
        ("/dup2.jpg", &[0.99, 0.01, 0.0]), // near-identical to dup1
        ("/other.jpg", &[0.0, 1.0, 0.0]),  // unrelated
        ("/lone.jpg", &[0.0, 0.0, 1.0]),   // unrelated
    ]);
    let clusters = store.dedup_clusters(0.95);
    assert_eq!(clusters.len(), 1, "only the two near-dupes cluster");
    assert_eq!(clusters[0].paths, vec!["/dup1.jpg", "/dup2.jpg"]);
}

#[test]
fn dedup_single_linkage_chains_a_transitive_group() {
    // a~b and b~c (each pair above threshold) but a and c are further apart: single-
    // linkage still puts all three in one cluster.
    let store = store(&[
        ("/a.jpg", &[1.0, 0.0]),
        ("/b.jpg", &[0.98, 0.2]),
        ("/c.jpg", &[0.9, 0.44]),
    ]);
    let clusters = store.dedup_clusters(0.95);
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].paths, vec!["/a.jpg", "/b.jpg", "/c.jpg"]);
}

#[test]
fn dedup_returns_nothing_when_all_distinct() {
    let store = store(&[
        ("/a.jpg", &[1.0, 0.0, 0.0]),
        ("/b.jpg", &[0.0, 1.0, 0.0]),
        ("/c.jpg", &[0.0, 0.0, 1.0]),
    ]);
    assert!(
        store.dedup_clusters(0.9).is_empty(),
        "no pair within threshold ⇒ no clusters"
    );
}
