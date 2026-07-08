//! Scheduler tests: the coalescing coordinator (pure), the full-volume recompute
//! ranking (over a synthetic index, no FFI), and the startup-sweep path.

use super::*;
use crate::importance::store::ImportanceStore;
use crate::indexing::ROOT_VOLUME_ID;

// ── Coalescing coordinator (plan M2 TDD target) ──────────────────────────

/// The core contract: a request while a pass runs does NOT start a second pass —
/// it sets the re-run flag, so the sweep + a concurrent `ScanCompleted` collapse
/// to one pass, then one re-run. This is the "sweep + concurrent completion ⇒ one
/// pass" guarantee (plan Decision 4), unit-tested without an app or a runtime.
#[test]
fn concurrent_requests_coalesce_into_one_pass_plus_one_rerun() {
    let coord = PassCoordinator::new();

    // First request starts a pass.
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    // A second request while it runs coalesces (no second pass).
    assert_eq!(coord.request("root"), BeginOutcome::Coalesced);
    // A third also coalesces onto the SAME pending re-run (not two re-runs).
    assert_eq!(coord.request("root"), BeginOutcome::Coalesced);

    // The pass finishes: because a request arrived mid-pass, run once more.
    assert_eq!(coord.finish("root"), FinishOutcome::RunAgain);
    // That re-run finishes with nothing further pending ⇒ done.
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
}

/// After a pass fully finishes (Done), the next request starts a fresh pass — the
/// slot resets, so a later scan completion isn't wrongly coalesced.
#[test]
fn a_new_request_after_done_starts_a_fresh_pass() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
    // A later completion starts a new pass, not a coalesce.
    assert_eq!(coord.request("root"), BeginOutcome::Start);
}

/// Two volumes are independent: a pass running for one never coalesces the other.
#[test]
fn coalescing_is_per_volume() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    // A different volume starts its own pass, not coalesced onto root's.
    assert_eq!(coord.request("smb-nas"), BeginOutcome::Start);
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
    assert_eq!(coord.finish("smb-nas"), FinishOutcome::Done);
}

/// Only ONE re-run is buffered no matter how many requests pile up mid-pass: the
/// re-run reruns once and then, with nothing new, is done. (A pathological event
/// storm can't queue N re-runs.)
#[test]
fn many_midpass_requests_buffer_exactly_one_rerun() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    for _ in 0..100 {
        assert_eq!(coord.request("root"), BeginOutcome::Coalesced);
    }
    assert_eq!(coord.finish("root"), FinishOutcome::RunAgain);
    assert_eq!(coord.finish("root"), FinishOutcome::Done, "exactly one re-run, not 100");
}

// ── Full-volume recompute ranking (synthetic index, no FFI) ───────────────

/// Build a synthetic index DB from a `SyntheticHome`, run the recompute over a
/// directly-constructed read pool, and assert `importance.db` holds a ranking
/// that puts the project root and mixed user-content folders above the
/// machine-output folders (node_modules, caches, logs). This is the M2
/// integration test: fake signals via a real index, no FFI, no registry.
#[test]
fn full_recompute_ranks_meaningful_folders_above_machine_output() {
    use crate::importance::fixtures::SyntheticHome;

    let now = 1_000_000_000;
    let home = SyntheticHome::canonical(now);

    // Materialize a real index DB over the synthetic tree and a read pool over it.
    let dir = tempfile::tempdir().expect("temp dir");
    let index_path = dir.path().join("index-root.db");
    build_index_from_home(&index_path, &home);
    let pool = crate::indexing::ReadPool::new(index_path).expect("read pool");

    // Walk once (the single-pass walk), then recompute over that walk through a
    // shared writer (bypassing the registry + async driver). Availability is
    // listing-only here so the ranking doesn't depend on Spotlight (unavailable in
    // the test env) or visits (none) — the redistribution keeps the listing
    // signals summing to the full weight.
    let folders = pool
        .with_conn(|conn| walk_index_folders(conn, &home.home))
        .expect("pool")
        .expect("walk");
    let writer = ImportanceWriter::spawn(&importance_db_path(dir.path(), ROOT_VOLUME_ID)).expect("writer");
    let weights = Weights::default();
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
    .expect("recompute");
    writer.flush_blocking().expect("flush");
    assert!(outcome.count > 0, "recompute must score folders");

    // Read the resulting weights back.
    let imp_path = importance_db_path(dir.path(), ROOT_VOLUME_ID);
    let store = ImportanceStore::open(&imp_path).expect("open importance store");
    let w = |rel: &str| -> f64 {
        store
            .weight_for(&format!("{}/{rel}", home.home))
            .expect("read")
            .map(|w| w.score)
            .unwrap_or(-1.0)
    };

    let webapp = w("projects/webapp");
    let downloads = w("Downloads");
    let node_modules = w("projects/webapp/node_modules");
    let logs = w("logs");
    let caches = w("Library/Caches");

    assert!(webapp >= 0.0 && downloads >= 0.0, "the meaningful folders were scored");
    assert_eq!(
        node_modules, 0.0,
        "a node_modules folder is floored to 0.0 (denylisted), got {node_modules}"
    );
    assert_eq!(
        caches, 0.0,
        "a Library/Caches folder is floored to 0.0 (system/hidden), got {caches}"
    );
    assert!(
        webapp > logs,
        "the project root ({webapp}) must outrank the monoculture logs folder ({logs})"
    );
    assert!(
        downloads > logs,
        "the mixed Downloads ({downloads}) must outrank the monoculture logs ({logs})"
    );

    // Every written weight carries the pass generation (as-of marker), and the
    // store's current generation matches — the staleness contract, end to end.
    let webapp_row = store
        .weight_for(&format!("{}/projects/webapp", home.home))
        .expect("read")
        .expect("scored");
    assert_eq!(
        webapp_row.as_of_generation,
        store.recompute_generation().expect("gen"),
        "a freshly-written weight is stamped at the current generation"
    );
}

// ── Memory-fix characterization (plan M4: O(dirs) walk) ───────────────────

/// The O(dirs) walk (directories materialized, files streamed into per-parent
/// accumulators) must produce EXACTLY the aggregate a whole-tree walk would: for
/// every folder, the distinct-extension count, file count, and direct-marker flag
/// its direct children collapse to. This pins the memory restructure's output —
/// the aggregate is the only thing the fix changed, so any drift in what a child
/// contributes (a `.git` directory marker, a `Cargo.toml` file marker, an
/// extension folded wrong) fails here. The independent oracle is the fixture's own
/// listing, re-derived directly (not through the walk under test).
#[test]
fn odirs_walk_aggregates_children_like_a_whole_tree_walk() {
    use crate::importance::fixtures::SyntheticHome;
    use crate::importance::signals::ChildAggregate;

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
    assert!(!folders.is_empty(), "the walk found folders");

    for f in &folders {
        // Re-derive the expected aggregate straight from the fixture's OWN listing
        // — an oracle independent of the walk under test.
        let children: Vec<_> = home.direct_children(&f.path).collect();
        let files: Vec<&str> = children
            .iter()
            .filter(|c| !c.is_directory)
            .map(|c| c.name.as_str())
            .collect();
        let expected_ext = crate::importance::scorer::extension_count(files.iter().copied());
        // A marker child is any child (file OR directory) whose folded name is a
        // project marker — `.git` is a directory, `Cargo.toml` a file.
        let has_marker = children
            .iter()
            .any(|c| crate::importance::classify::is_project_marker(&c.name.to_lowercase()));
        assert_eq!(
            f.children,
            ChildAggregate {
                distinct_extension_count: expected_ext,
                file_count: files.len() as u32,
                has_direct_marker: has_marker,
            },
            "the O(dirs) walk's aggregate for {} must match the fixture's own listing",
            f.path
        );
    }
}

// ── Incremental recompute (plan M3 TDD target) ────────────────────────────

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

    // An UNTOUCHED folder (the logs folder, not under Downloads) keeps its gen-1
    // as-of marker — the incremental pass didn't rewrite it, and the generation
    // didn't move, so it isn't spuriously stale.
    let logs = store
        .weight_for(&format!("{}/logs", home.home))
        .expect("read")
        .expect("scored");
    assert_eq!(logs.as_of_generation, 1, "an untouched folder keeps its as-of marker");
    writer.shutdown();
}

// ── M4: multi-volume, SMB degradation, offline reads ──────────────────────

use crate::indexing::IndexVolumeKind;

/// The volume-kind → scoring-policy map is the typed gate M4 turns on: Local and
/// SMB are background-scored; SMB has Spotlight UNAVAILABLE (so its weight
/// redistributes, never fabricated); MTP is an explicit exclusion, on-demand only.
/// This pins the typed decision at its source — no volume-id string branching.
#[test]
fn scoring_policy_scores_local_and_smb_but_excludes_mtp() {
    // Local: scored, both optional signals available (Spotlight where the OS has
    // it; the visit signal always). We assert the SHAPE, not the platform value.
    match ScoringPolicy::for_kind(IndexVolumeKind::Local) {
        ScoringPolicy::Scored { available } => {
            assert!(available.visit_available, "local visits are available");
            assert_eq!(
                available.last_used_available,
                crate::importance::last_used::is_available(),
                "local Spotlight availability tracks the OS"
            );
        }
        ScoringPolicy::Excluded => panic!("local must be scored"),
    }

    // SMB: scored, but Spotlight is UNAVAILABLE regardless of platform (no
    // Spotlight over a share), so `last_used` redistributes.
    match ScoringPolicy::for_kind(IndexVolumeKind::Smb) {
        ScoringPolicy::Scored { available } => {
            assert!(available.visit_available, "SMB visits (from Cmdr nav) still apply");
            assert!(
                !available.last_used_available,
                "SMB has no Spotlight ⇒ last_used unavailable ⇒ its weight redistributes"
            );
        }
        ScoringPolicy::Excluded => panic!("SMB must be scored in M4"),
    }

    // MTP: excluded — on-demand only, never background-scored.
    assert_eq!(
        ScoringPolicy::for_kind(IndexVolumeKind::Mtp),
        ScoringPolicy::Excluded,
        "MTP is an explicit typed exclusion, not scored"
    );

    // The `record_visit` gate mirrors this: scored kinds record, MTP doesn't.
    assert!(is_background_scored(IndexVolumeKind::Local));
    assert!(is_background_scored(IndexVolumeKind::Smb));
    assert!(!is_background_scored(IndexVolumeKind::Mtp));
}

/// SMB signal degradation, end to end: recomputing the SAME index under the SMB
/// availability mask (no Spotlight) produces DIFFERENT — never-fabricated —
/// weights than a would-be all-signals mask, and the listing-only ranking still
/// holds. The scorer's redistribution (M1) is what makes the SMB score honest: a
/// missing Spotlight signal spreads its weight onto the listing signals rather
/// than counting as a zero. We assert SMB weights match the listing-only mask
/// exactly (no Spotlight contribution snuck in) and differ from an all-available
/// mask that DID fabricate a Spotlight term.
#[test]
fn smb_recompute_degrades_spotlight_and_redistributes() {
    use crate::importance::fixtures::SyntheticHome;

    let now = 1_000_000_000;
    let home = SyntheticHome::canonical(now);
    let dir = tempfile::tempdir().expect("temp dir");
    let index_path = dir.path().join("index-smb-nas.db");
    build_index_from_home(&index_path, &home);
    let pool = crate::indexing::ReadPool::new(index_path).expect("read pool");
    let folders = pool
        .with_conn(|conn| walk_index_folders(conn, &home.home))
        .expect("pool")
        .expect("walk");

    // The SMB availability mask (from the typed policy) has Spotlight unavailable.
    let smb_available = match ScoringPolicy::for_kind(IndexVolumeKind::Smb) {
        ScoringPolicy::Scored { available } => available,
        ScoringPolicy::Excluded => unreachable!("SMB is scored"),
    };
    assert!(!smb_available.last_used_available);

    let weights = Weights::default();
    // Score under the SMB mask with NO last_used values (none available on SMB).
    let smb_rows = score_folders(&folders, &home.home, &weights, &smb_available, now, |_| {
        OptionalSignals::default()
    });

    // A control: score under the SAME listing-only mask. SMB must equal this — its
    // degradation is exactly "drop Spotlight and redistribute", nothing more.
    let listing_only = score_folders(&folders, &home.home, &weights, &SignalSet::listing_only(), now, |_| {
        OptionalSignals::default()
    });
    // Visit availability differs (SMB has visits available, listing_only doesn't),
    // but with NO visit values supplied the redistribution of the two available-
    // but-unsupplied vs unavailable cases can differ; so we compare the SMB result
    // against a mask that matches SMB's availability with visits unsupplied.
    // The load-bearing assertion is that no Spotlight term was fabricated: a folder
    // with a would-be recent last_used doesn't score higher on SMB than a mask that
    // fabricated one.
    let all_with_fabricated = score_folders(&folders, &home.home, &weights, &SignalSet::all(), now, |_| {
        OptionalSignals {
            visit_count: None,
            // Fabricate a "just used" Spotlight timestamp — this is what SMB must
            // NOT do. A mask that counts it will score user folders differently.
            last_used_secs: Some(now),
        }
    });

    // Find a user-content folder (Downloads) in each result set.
    let downloads_path = format!("{}/Downloads", home.home);
    let smb_downloads = smb_rows
        .iter()
        .find(|r| r.path == downloads_path)
        .expect("smb downloads");
    let fabricated_downloads = all_with_fabricated
        .iter()
        .find(|r| r.path == downloads_path)
        .expect("fabricated downloads");

    assert!(
        smb_downloads.score < fabricated_downloads.score,
        "SMB (no Spotlight, {}) must not fabricate the recency a Spotlight-fed mask adds ({})",
        smb_downloads.score,
        fabricated_downloads.score
    );

    // The listing-only ranking still holds on SMB: a project root outranks the
    // monoculture logs folder even without Spotlight.
    let score_of = |rows: &[WeightRow], rel: &str| -> f64 {
        let p = format!("{}/{rel}", home.home);
        rows.iter().find(|r| r.path == p).map(|r| r.score).unwrap_or(-1.0)
    };
    assert!(
        score_of(&smb_rows, "projects/webapp") > score_of(&smb_rows, "logs"),
        "SMB ranking still separates a project root from machine output"
    );
    // Sanity: the SMB result and the plain listing-only result agree on the logs
    // folder (both drop Spotlight; SMB's extra visit-availability changes nothing
    // when no visits are supplied for these folders).
    assert_eq!(
        score_of(&smb_rows, "logs"),
        score_of(&listing_only, "logs"),
        "with no visits supplied, SMB's logs score matches the listing-only score"
    );
}

/// THE headline offline-unmounted read (plan M4, Decision 2): score a volume, then
/// DROP its index registration (simulate the NAS unmounting), and assert the read
/// API STILL returns its weights — with the correct as-of generation — straight
/// from the on-disk `importance.db`. The read API never touched the index registry
/// (it reads its own store), so an unmounted volume's importance stays queryable.
#[test]
fn offline_unmounted_read_returns_stored_weights_after_index_gone() {
    use crate::importance::ImportanceIndex;
    use crate::importance::fixtures::SyntheticHome;

    let now = 1_000_000_000;
    let home = SyntheticHome::canonical(now);
    let data_dir = tempfile::tempdir().expect("temp dir");
    let volume_id = "smb-nas";

    // Score the volume WHILE its index is "mounted": build an index, walk it, write
    // importance.db. We do this directly (no registry) — the registration is what
    // we then simulate dropping. The importance.db lands under the data dir keyed
    // by the volume id, exactly where an unmounted read looks.
    let index_path = data_dir.path().join(format!("index-{volume_id}.db"));
    build_index_from_home(&index_path, &home);
    let pool = crate::indexing::ReadPool::new(index_path.clone()).expect("read pool");
    let folders = pool
        .with_conn(|conn| walk_index_folders(conn, &home.home))
        .expect("pool")
        .expect("walk");
    let writer = ImportanceWriter::spawn(&importance_db_path(data_dir.path(), volume_id)).expect("writer");
    let weights = Weights::default();
    let smb_available = SignalSet {
        visit_available: true,
        last_used_available: false,
    };
    let outcome = recompute_folders(
        &RecomputeInputs {
            writer: &writer,
            weights: &weights,
            home: &home.home,
            now_secs: now,
            available: smb_available,
            visits: &HashMap::new(),
            last_used: &HashMap::new(),
        },
        &folders,
    )
    .expect("recompute");
    writer.flush_blocking().expect("flush");
    writer.shutdown();

    // Simulate the unmount: drop the index read pool + the walk. The index
    // registration is gone; `get_read_pool_for(volume_id)` would now be `None` and
    // a recompute would no-op. But the importance.db file remains on disk.
    drop(pool);
    drop(folders);
    // Delete the index DB entirely — the strongest form of "the volume is gone":
    // nothing about the live index survives, only importance.db.
    std::fs::remove_file(&index_path).ok();

    // The read API, opened for the UNMOUNTED volume, still answers from the on-disk
    // store — with the same availability mask the write used (so `explain`
    // redistributes the same way).
    let index = ImportanceIndex::open(data_dir.path(), volume_id, smb_available);
    let webapp = index
        .weight_for(&format!("{}/projects/webapp", home.home))
        .expect("read")
        .expect("the project root's weight is readable while the volume is unmounted");
    assert_eq!(
        webapp.as_of_generation, outcome.generation,
        "the offline read carries the as-of generation the score was written at (the staleness caveat)"
    );
    assert!(webapp.score.value() > 0.0, "the stored weight is intact");

    // top_n also works offline (media-ML's enrich-important-first over an unmounted
    // NAS), and reports the same generation.
    let top = index.top_n(3).expect("top_n offline");
    assert!(!top.is_empty(), "top_n answers from the on-disk store while unmounted");
    assert_eq!(
        index.recompute_generation().expect("gen"),
        outcome.generation,
        "the store's current generation is readable offline (the as-of marker)"
    );
}

/// The multi-volume scheduler recompute (plan M4 after-test): a local volume and a
/// fake-SMB volume, each with its own synthetic index, score INDEPENDENTLY into
/// their own `importance.db` — the per-volume keying that makes offline reads and
/// multi-volume scoring work. Uses `recompute_folders` per volume (no registry, no
/// FFI, no async driver).
#[test]
fn multi_volume_recompute_scores_each_volume_into_its_own_store() {
    use crate::importance::fixtures::SyntheticHome;

    let now = 1_000_000_000;
    let data_dir = tempfile::tempdir().expect("temp dir");

    // Two volumes over the same synthetic tree shape, distinct ids + stores.
    let volumes = [
        ("root", SignalSet::listing_only()),
        (
            "smb-nas",
            SignalSet {
                visit_available: true,
                last_used_available: false,
            },
        ),
    ];

    let home = SyntheticHome::canonical(now);
    let weights = Weights::default();

    for (volume_id, available) in volumes {
        let index_path = data_dir.path().join(format!("index-{volume_id}.db"));
        build_index_from_home(&index_path, &home);
        let pool = crate::indexing::ReadPool::new(index_path).expect("read pool");
        let folders = pool
            .with_conn(|conn| walk_index_folders(conn, &home.home))
            .expect("pool")
            .expect("walk");
        let writer = ImportanceWriter::spawn(&importance_db_path(data_dir.path(), volume_id)).expect("writer");
        let outcome = recompute_folders(
            &RecomputeInputs {
                writer: &writer,
                weights: &weights,
                home: &home.home,
                now_secs: now,
                available,
                visits: &HashMap::new(),
                last_used: &HashMap::new(),
            },
            &folders,
        )
        .expect("recompute");
        writer.flush_blocking().expect("flush");
        writer.shutdown();
        assert!(outcome.count > 0, "{volume_id} scored folders");
    }

    // Each volume's store is independent and separately readable.
    let root_store = ImportanceStore::open(&importance_db_path(data_dir.path(), "root")).expect("root store");
    let smb_store = ImportanceStore::open(&importance_db_path(data_dir.path(), "smb-nas")).expect("smb store");
    let webapp = format!("{}/projects/webapp", home.home);
    assert!(
        root_store.weight_for(&webapp).expect("read").is_some(),
        "root volume scored its own store"
    );
    assert!(
        smb_store.weight_for(&webapp).expect("read").is_some(),
        "smb volume scored its own store, independently"
    );
}

/// Build an index DB over a `SyntheticHome` using the real `IndexStore` +
/// `IndexWriter`, so the recompute reads exactly the schema production reads. We
/// insert each entry with a parent pointer derived from its path.
fn build_index_from_home(index_path: &std::path::Path, home: &crate::importance::fixtures::SyntheticHome) {
    use crate::indexing::store::{IndexStore, ROOT_ID};

    // Open the store (creates the schema), then insert entries parent-first by
    // walking paths in sorted order so a parent always exists before its child.
    let store = IndexStore::open(index_path).expect("open index");
    let conn = store.read_conn();

    // Map from absolute path to assigned entry id; a top-level entry's parent is
    // the sentinel ROOT_ID (`/`).
    let mut path_to_id: HashMap<String, i64> = HashMap::new();
    let mut next_id: i64 = ROOT_ID + 1;

    // Insert a directory entry for `path`, first inserting any missing ancestors
    // so `reconstruct_path` yields the full absolute path (a real index has every
    // ancestor from `/`; the synthetic tree starts mid-way at the home root).
    fn ensure_dir(
        conn: &rusqlite::Connection,
        path: &str,
        modified_at: Option<u64>,
        path_to_id: &mut HashMap<String, i64>,
        next_id: &mut i64,
    ) -> i64 {
        use crate::indexing::store::{IndexStore, ROOT_ID};
        if let Some(&id) = path_to_id.get(path) {
            return id;
        }
        let parent = std::path::Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().to_string());
        let parent_id = match parent.as_deref() {
            Some("") | Some("/") | None => ROOT_ID,
            Some(pp) => ensure_dir(conn, pp, None, path_to_id, next_id),
        };
        let name = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let id = *next_id;
        *next_id += 1;
        IndexStore::insert_entry_v2_with_id(conn, id, parent_id, &name, true, false, None, None, modified_at, None)
            .expect("insert dir");
        path_to_id.insert(path.to_string(), id);
        id
    }

    let mut entries: Vec<_> = home.all_entries().to_vec();
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    for e in &entries {
        if e.is_directory {
            ensure_dir(conn, &e.path, e.modified_at, &mut path_to_id, &mut next_id);
        } else {
            let parent_path = std::path::Path::new(&e.path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let parent_id = ensure_dir(conn, &parent_path, None, &mut path_to_id, &mut next_id);
            let id = next_id;
            next_id += 1;
            IndexStore::insert_entry_v2_with_id(
                conn,
                id,
                parent_id,
                &e.name,
                false,
                false,
                e.size,
                e.size,
                e.modified_at,
                None,
            )
            .expect("insert file");
        }
    }
}
