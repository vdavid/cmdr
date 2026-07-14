//! Incremental recompute tests (plan M3 TDD target): the touched-set computation
//! (ancestors, capped, unioned) and the integration that rescopes only the changed
//! subtree while leaving untouched folders' as-of generation intact.

use super::test_support::*;
use super::*;

// ── Incremental recompute (plan M3 TDD target) ────────────────────────────

/// The bare root `/` (the universal ancestor carried by every live dir-changed
/// batch) and empty strings are dropped, so a normal change never escalates to a
/// whole-volume rewrite and a root-only batch is a no-op. Real paths pass through
/// unchanged, in order.
#[test]
fn sanitize_incremental_batch_drops_root_and_empties() {
    assert_eq!(
        sanitize_incremental_batch(&["/".to_string(), "/a/b".to_string()]),
        vec!["/a/b".to_string()],
        "the bare root is dropped, the real path kept"
    );
    assert!(
        sanitize_incremental_batch(&["/".to_string(), String::new()]).is_empty(),
        "a batch of only root + empties has nothing real to rescore"
    );
    assert_eq!(
        sanitize_incremental_batch(&["/a".to_string(), "/b/c".to_string()]),
        vec!["/a".to_string(), "/b/c".to_string()],
        "real paths pass through unchanged and in order"
    );
}

/// The touched set is the changed folders PLUS their ancestor chains (so a marker
/// or size change raises parents), and the ancestor walk is CAPPED so a deep
/// change can't rescope half the volume (plan Decision 5 ancestor-fan-out cap).
#[test]
fn touched_set_includes_ancestors_and_is_capped() {
    // A single deep change pulls in its ancestors up to the cap, but no further:
    // 60 levels deep with a 32-level cap means the near ancestors are touched and
    // the far (near-root) ones are NOT rescoped (that's the fan-out bound).
    let components: Vec<String> = (0..60).map(|i| format!("d{i}")).collect();
    let deep = format!("/{}", components.join("/"));
    let touched = touched_folder_set(std::slice::from_ref(&deep));

    assert!(touched.contains(&deep), "the changed folder itself is touched");
    // The immediate parent (one level up) is always touched.
    let parent = format!("/{}", components[..components.len() - 1].join("/"));
    assert!(touched.contains(&parent), "the immediate parent is touched");
    // A far, near-root ancestor is BEYOND the cap and must NOT be touched.
    assert!(
        !touched.contains("/d0/d1"),
        "a near-root ancestor beyond the cap is not rescoped (fan-out bound)"
    );
    // The changed folder + at most ANCESTOR_WALK_CAP ancestors.
    assert!(
        touched.len() <= ANCESTOR_WALK_CAP + 1,
        "the ancestor walk is capped ({} > {})",
        touched.len(),
        ANCESTOR_WALK_CAP + 1
    );
    // The bare root `/` is never added as a folder.
    assert!(!touched.contains("/"), "the root sentinel isn't a scored folder");
}

/// A two-changed-path set unions both chains without duplication.
#[test]
fn touched_set_unions_multiple_changed_paths() {
    let touched = touched_folder_set(&["/a/b/c".to_string(), "/a/x".to_string()]);
    for p in ["/a/b/c", "/a/b", "/a", "/a/x"] {
        assert!(touched.contains(p), "{p} should be touched");
    }
}

/// THE incremental integration target: an incremental rescore rewrites ONLY the
/// changed subtree + ancestors and leaves every untouched folder's as-of
/// generation intact (and does not advance the store generation). Built over a
/// synthetic index + a directly-built writer (no registry, no FFI).
#[test]
fn incremental_rescore_rescopes_and_preserves_untouched_generation() {
    use crate::importance::fixtures::SyntheticHome;

    let now = 1_000_000_000;
    let home = SyntheticHome::canonical(now);
    let dir = tempfile::tempdir().expect("temp dir");
    let index_path = dir.path().join("index-root.db");
    build_index_from_home(&index_path, &home);
    let pool = crate::indexing::ReadPool::new(index_path).expect("read pool");
    let folders = pool
        .with_conn(|conn| walk_index_folders(conn, &home.home))
        .expect("pool")
        .expect("walk");

    let db_path = importance_db_path(dir.path(), ROOT_VOLUME_ID);
    let writer = ImportanceWriter::spawn(&db_path).expect("writer");
    let weights = Weights::default();

    // Full pass 1: score everything at generation 1.
    let outcome = recompute_folders(
        &RecomputeInputs {
            writer: &writer,
            weights: &weights,
            home: &home.home,
            now_secs: now,
            available: SignalSet::listing_only(),
            visits: &HashMap::new(),
            last_used: &HashMap::new(),
        },
        &folders,
    )
    .expect("full pass");
    writer.flush_blocking().expect("flush");
    assert_eq!(outcome.generation, 1, "first full pass is generation 1");

    // Incremental rescore of only the Downloads subtree.
    let changed = vec![format!("{}/Downloads", home.home)];
    let count = incremental_rescore(
        &IncrementalInputs {
            writer: &writer,
            weights: &weights,
            home: &home.home,
            now_secs: now,
            available: SignalSet::listing_only(),
            visits: &HashMap::new(),
        },
        &folders,
        &changed,
    )
    .expect("incremental");
    writer.flush_blocking().expect("flush");

    let store = ImportanceStore::open(&db_path).expect("open");
    // The store generation did NOT advance (incremental leaves it).
    assert_eq!(
        store.recompute_generation().expect("gen"),
        1,
        "an incremental rescore does not advance the generation"
    );
    // Only the touched subtree was rescored (Downloads + its ancestor chain),
    // which is far fewer than the whole tree.
    assert!(count >= 1, "at least Downloads was rescored");
    assert!(
        count < folders.len(),
        "incremental rescored a subset ({count}), not all {} folders",
        folders.len()
    );

    // Downloads' row still stamped at gen 1 (incremental keeps the current gen).
    let downloads = store
        .weight_for(&format!("{}/Downloads", home.home))
        .expect("read")
        .expect("scored");
    assert_eq!(
        downloads.as_of_generation, 1,
        "touched rows carry the current generation"
    );

    // An UNTOUCHED, unfloored folder (Documents/invoices, not under Downloads)
    // keeps its gen-1 as-of marker — the incremental pass didn't rewrite it, and the
    // generation didn't move, so it isn't spuriously stale. (The fixture's `logs`
    // folder is denylisted, so it has no row to check — floored folders are omitted.)
    let untouched = store
        .weight_for(&format!("{}/Documents/invoices", home.home))
        .expect("read")
        .expect("scored");
    assert_eq!(
        untouched.as_of_generation, 1,
        "an untouched folder keeps its as-of marker"
    );
    writer.shutdown();
}
