//! Vector-store tests (TDD targets): cosine correctness, top-k ranking + source
//! exclusion, and near-duplicate dedup grouping by cosine threshold. Pure, no FFI, no
//! DB — the brute-force store is exercised directly over synthetic vectors.

use half::f16;

use super::{BruteForceVectorStore, VectorStore, cosine};

/// Build a store from `f32` fixtures, converting to the `f16` the store now holds (plan M3).
/// The fixtures stay well-separated so `f16` rounding never flips a ranking or a dedup edge.
fn store(entries: &[(&str, &[f32])]) -> BruteForceVectorStore {
    BruteForceVectorStore::new(
        entries
            .iter()
            .map(|(p, v)| (p.to_string(), v.iter().map(|x| f16::from_f32(*x)).collect()))
            .collect(),
    )
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
fn top_k_order_matches_the_f32_reference_over_100_vectors() {
    // Plan M3: storing embeddings as f16 must not reorder search results. Build 100
    // 512-d vectors whose cosine to the query strictly descends with clear gaps (0.008,
    // well above f16's ~1e-3 noise), rank them exactly in f32, then confirm the f16-backed
    // store returns the SAME path order.
    let dim = 512;
    let query: Vec<f32> = std::iter::once(1.0).chain(std::iter::repeat_n(0.0, dim - 1)).collect();
    // `owned[i]` = vector with cos-to-query ≈ 1 - i*0.008, in a deliberately scrambled path
    // order so a correct ranking can't come from insertion order.
    let owned: Vec<(String, Vec<f32>)> = (0..100)
        .map(|i| {
            let a = 1.0 - (i as f32) * 0.008;
            let b = (1.0 - a * a).max(0.0).sqrt();
            let mut v = vec![0.0f32; dim];
            v[0] = a;
            v[1] = b;
            // Scramble the path label so lexical order ≠ rank.
            (format!("/p{:03}.jpg", (i * 37) % 100), v)
        })
        .collect();

    // The exact f32 reference order (highest cosine first, ties by path — the store's rule).
    let mut reference: Vec<(String, f32)> = owned.iter().map(|(p, v)| (p.clone(), cosine(&query, v))).collect();
    reference.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    let reference_paths: Vec<&str> = reference.iter().map(|(p, _)| p.as_str()).collect();

    let entries: Vec<(&str, &[f32])> = owned.iter().map(|(p, v)| (p.as_str(), v.as_slice())).collect();
    let store = store(&entries);
    let hits = store.top_k(&query, 100, None);
    let hit_paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(hit_paths, reference_paths, "f16 store preserves the f32 top-k order");
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
