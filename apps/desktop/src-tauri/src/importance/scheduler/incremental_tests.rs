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
    let home = "/Users/test";
    assert_eq!(
        sanitize_incremental_batch(&["/".to_string(), "/a/b".to_string()], home),
        vec!["/a/b".to_string()],
        "the bare root is dropped, the real path kept"
    );
    assert!(
        sanitize_incremental_batch(&["/".to_string(), String::new()], home).is_empty(),
        "a batch of only root + empties has nothing real to rescore"
    );
    assert_eq!(
        sanitize_incremental_batch(&["/a".to_string(), "/b/c".to_string()], home),
        vec!["/a".to_string(), "/b/c".to_string()],
        "real paths pass through unchanged and in order"
    );
}

/// THE idle gate: a batch of nothing but machine churn is dropped whole, so the
/// pass returns before it opens the read pool or walks the index.
///
/// A boot volume is never silent — builds, caches, and agents write constantly —
/// and every one of those paths FLOORS, so a rescore of them would write zero rows
/// after paying a full O(dirs) walk. Without this, background churn alone drove a
/// walk plus a 161,094-weight reload every 60 s for a whole session.
#[test]
fn a_batch_of_only_floored_churn_is_dropped_whole() {
    let home = "/Users/test";
    let churn = [
        // A cargo build under a worktree: denylisted `target`, AND under a dot-dir.
        "/Users/test/proj/.claude/worktrees/wt/target/debug/deps".to_string(),
        // Build output straight under a project.
        "/Users/test/proj/target/debug/build".to_string(),
        // A dependency tree.
        "/Users/test/proj/node_modules/lodash".to_string(),
        // Browser and toolchain caches.
        "/Users/test/Library/Caches/com.example.app".to_string(),
        // Repository internals.
        "/Users/test/proj/.git/objects/pack".to_string(),
        // And the universal ancestor that rides along.
        "/".to_string(),
    ];
    assert!(
        sanitize_incremental_batch(&churn, home).is_empty(),
        "a batch that can produce no weight row must not cost a walk"
    );

    // A real edit in the same batch still gets through — with the churn stripped, so
    // the surviving scope is exactly the folders that can score.
    let mixed = [
        "/Users/test/proj/target/debug".to_string(),
        "/Users/test/proj/src".to_string(),
        "/Users/test/Library/Caches/x".to_string(),
    ];
    assert_eq!(
        sanitize_incremental_batch(&mixed, home),
        vec!["/Users/test/proj/src".to_string()],
        "the one path that can score survives, the churn around it doesn't"
    );

    // The filter is the SAME floor predicate the writer applies, so it never drops a
    // path that would have earned a row.
    for scoring in ["/Users/test/proj/src", "/Users/test/Documents/invoices", "/Users/test"] {
        assert_eq!(
            sanitize_incremental_batch(&[scoring.to_string()], home),
            vec![scoring.to_string()],
            // allowed-pluralize-noun: "scores" is a verb here (the path scores), not a plural noun after a count
            "{scoring} scores, so it must drive a rescore"
        );
    }
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

/// The downward subtree expansion matches on SEPARATOR BOUNDARIES, not on raw
/// string prefixes: `/a/bc` is a sibling of `/a/b`, not a descendant, and a careless
/// `starts_with(changed)` silently drags it (and its whole subtree) into every
/// rescore. Also pins the exact-match case and the empty batch.
#[test]
fn changed_subtree_matches_on_separator_boundaries() {
    let changed = vec!["/a/b".to_string()];

    assert!(is_in_changed_subtree("/a/b", &changed), "the changed folder itself");
    assert!(is_in_changed_subtree("/a/b/c", &changed), "a direct child");
    assert!(is_in_changed_subtree("/a/b/c/d/e", &changed), "a deep descendant");

    // THE trap: a prefix without a separator boundary is a sibling, not a child.
    assert!(
        !is_in_changed_subtree("/a/bc", &changed),
        "/a/bc is a sibling of /a/b, not a descendant"
    );
    assert!(
        !is_in_changed_subtree("/a/bc/d", &changed),
        "nor is anything under that sibling"
    );
    assert!(
        !is_in_changed_subtree("/a", &changed),
        "an ancestor is not in the subtree"
    );
    assert!(!is_in_changed_subtree("/x/y", &changed), "an unrelated path");
    assert!(!is_in_changed_subtree("/a/b", &[]), "an empty batch matches nothing");
}

/// Any one of several changed paths is enough, and the odd inputs a live batch
/// could carry behave as before: a trailing-slash entry only matches itself (it's
/// not a folder path the walk produces), and the bare root `/` claims no subtree
/// (`sanitize_incremental_batch` drops it upstream anyway, so this pins that the
/// predicate isn't a second line of defense that changed shape).
#[test]
fn changed_subtree_handles_multiple_and_odd_paths() {
    let changed = vec!["/a".to_string(), "/x/y".to_string()];
    assert!(is_in_changed_subtree("/x/y/z", &changed), "matches the second entry");
    assert!(is_in_changed_subtree("/a/q", &changed), "matches the first entry");
    assert!(!is_in_changed_subtree("/ab", &changed), "/ab is not under /a");

    let trailing = vec!["/a/".to_string()];
    assert!(is_in_changed_subtree("/a/", &trailing), "exact match still holds");
    assert!(
        !is_in_changed_subtree("/a/b", &trailing),
        "a trailing-slash entry needs `//` to match a child, so it claims no subtree"
    );

    let root = vec!["/".to_string()];
    assert!(is_in_changed_subtree("/", &root), "the root matches itself");
    assert!(
        !is_in_changed_subtree("/a", &root),
        "the bare root claims no subtree here; sanitize_incremental_batch drops it upstream"
    );
}

/// A two-changed-path set unions both chains without duplication.
#[test]
fn touched_set_unions_multiple_changed_paths() {
    let touched = touched_folder_set(&["/a/b/c".to_string(), "/a/x".to_string()]);
    for p in ["/a/b/c", "/a/b", "/a", "/a/x"] {
        assert!(touched.contains(p), "{p} should be touched");
    }
}

/// Build a wide, deep synthetic volume: `projects` × `subs` folders under
/// `/Users/test/projects`, plus the chain above it. Returns every folder path.
fn wide_tree(projects: usize, subs: usize) -> Vec<String> {
    let mut paths = vec![
        "/Users".to_string(),
        "/Users/test".to_string(),
        "/Users/test/projects".to_string(),
    ];
    for p in 0..projects {
        paths.push(format!("/Users/test/projects/p{p}"));
        for s in 0..subs {
            paths.push(format!("/Users/test/projects/p{p}/s{s}"));
        }
    }
    paths
}

/// Run one incremental rescore over `folders` for `changed`, returning how many
/// rows it wrote.
fn rescore(writer: &ImportanceWriter, home: &str, folders: &mut WalkedFolders, changed: &[String]) -> usize {
    incremental_rescore(
        &IncrementalInputs {
            writer,
            weights: &Weights::default(),
            home,
            now_secs: 1_000_000_000,
            available: SignalSet::listing_only(),
            visits: &HashMap::new(),
        },
        folders,
        changed,
    )
    .expect("incremental")
}

/// THE scope contract, measured: the rescore's cost tracks the batch, and the
/// batch is now the dirs whose OWN listings changed.
///
/// Fed the single ORIGIN dir the live pipeline publishes, a one-folder change
/// rewrites 5 rows out of 423. Fed the ancestor closure the bus used to carry
/// (`/Users` rides in every batch as the universal ancestor), the very same code
/// rewrites all 423 — because `is_in_changed_subtree` expands each entry DOWNWARD.
/// That is the 90,308-row-per-minute treadmill, reproduced in miniature; the fix
/// is upstream, in what the bus carries (`indexing/lifecycle/lifecycle_bus.rs`).
#[test]
fn incremental_scope_follows_the_changed_dir_not_its_ancestors() {
    let home = "/Users/test";
    let owned = wide_tree(20, 20);
    let paths: Vec<&str> = owned.iter().map(String::as_str).collect();
    let mut folders = WalkedFolders::synthetic(&paths, home);
    assert_eq!(
        folders.len(),
        423,
        "20 projects × 20 subfolders, the project dirs, and the chain above"
    );

    let dir = tempfile::tempdir().expect("temp dir");
    let writer = full_pass_walk(dir.path(), home, &mut folders);

    // What the pipeline publishes today: the one directory whose listing changed.
    let origins = vec!["/Users/test/projects/p7/s3".to_string()];
    let narrow = rescore(&writer, home, &mut folders, &origins);
    assert_eq!(
        narrow, 5,
        "the changed folder plus its four ancestors — nothing else is affected"
    );

    // What it used to publish: that dir AND its whole ancestor chain. `/Users`
    // alone drags in every folder on the volume.
    let with_ancestors = vec![
        "/Users/test/projects/p7/s3".to_string(),
        "/Users/test/projects/p7".to_string(),
        "/Users/test/projects".to_string(),
        "/Users/test".to_string(),
        "/Users".to_string(),
    ];
    let wide = rescore(&writer, home, &mut folders, &with_ancestors);
    assert_eq!(wide, folders.len(), "an ancestor in the batch rescores the whole tree");
    assert!(
        wide > narrow * 50,
        "the ancestor closure costs {} against the origin's {}",
        crate::pluralize::pluralize(wide as u64, "row"),
        crate::pluralize::pluralize(narrow as u64, "row")
    );
    writer.shutdown();
}

/// A floor transition still propagates through the narrowed scope, and stays
/// contained.
///
/// The live pipeline reports the RENAMED directory's PARENT as the origin (its
/// listing is what changed), and the downward subtree expansion from that parent is
/// what re-floors the renamed subtree. This pins BOTH halves in one deep tree: the
/// renamed `node_modules` and its child lose their rows, AND a sibling project's
/// subtree is not dragged in. Narrowing the scope any further — to the origin dir
/// alone, without its subtree — would silently leave a scored row under a fresh
/// `node_modules`.
#[test]
fn a_floor_transition_propagates_from_the_parent_origin_without_widening() {
    let home = "/Users/test";
    let before: Vec<String> = ["/Users/test/proj/a/pkg", "/Users/test/proj/a/pkg/deep"]
        .iter()
        .map(|p| p.to_string())
        .chain(wide_tree(5, 5))
        .collect();
    let before_refs: Vec<&str> = before.iter().map(String::as_str).collect();
    let mut walk = WalkedFolders::synthetic(&before_refs, home);

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = importance_db_path(dir.path(), ROOT_VOLUME_ID);
    let writer = full_pass_walk(dir.path(), home, &mut walk);
    let store = ImportanceStore::open(&db_path).expect("open");
    assert!(
        store.weight_for("/Users/test/proj/a/pkg/deep").expect("read").is_some(),
        "the pre-rename subtree is scored"
    );
    let sibling_before = store
        .weight_for("/Users/test/projects/p1/s1")
        .expect("read")
        .expect("scored")
        .score;

    // `pkg` is renamed to `node_modules`: the new walk has the floored name.
    let after: Vec<String> = [
        "/Users/test/proj/a/node_modules",
        "/Users/test/proj/a/node_modules/deep",
    ]
    .iter()
    .map(|p| p.to_string())
    .chain(wide_tree(5, 5))
    .collect();
    let after_refs: Vec<&str> = after.iter().map(String::as_str).collect();
    let mut walk = WalkedFolders::synthetic(&after_refs, home);

    // The batch the pipeline publishes: the renamed dir's parent, and only that.
    let count = rescore(&writer, home, &mut walk, &["/Users/test/proj/a".to_string()]);
    writer.flush_blocking().expect("flush");

    let store = ImportanceStore::open(&db_path).expect("reopen");
    assert!(
        store
            .weight_for("/Users/test/proj/a/node_modules")
            .expect("read")
            .is_none(),
        "the renamed folder floors and loses its row"
    );
    assert!(
        store
            .weight_for("/Users/test/proj/a/node_modules/deep")
            .expect("read")
            .is_none(),
        "so does its now-under-floored descendant, reached by the downward expansion"
    );
    assert!(
        store.weight_for("/Users/test/proj/a/pkg").expect("read").is_none(),
        "and the stale pre-rename path is cleared"
    );
    // Containment: the sibling half of the volume was never rewritten.
    assert!(
        count <= 4,
        "only the origin's chain and its (now floored) subtree were touched, not {}",
        crate::pluralize::pluralize(count as u64, "folder")
    );
    assert_eq!(
        store
            .weight_for("/Users/test/projects/p1/s1")
            .expect("read")
            .expect("still scored")
            .score,
        sibling_before,
        "an unrelated subtree keeps its full-pass row untouched"
    );
    writer.shutdown();
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
    let mut folders = pool
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
        &mut folders,
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
        &mut folders,
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
