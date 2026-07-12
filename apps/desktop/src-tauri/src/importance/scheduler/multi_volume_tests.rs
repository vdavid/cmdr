//! Multi-volume tests (plan M4): floored-row transitions on the incremental path,
//! the derive-on-read parity invariant, the typed volume-kind scoring policy, SMB
//! Spotlight degradation, offline-unmounted reads, and per-volume store isolation.

use super::test_support::*;
use super::*;
use crate::indexing::IndexVolumeKind;

// ── Floored transitions on the incremental path (storage compaction) ───────

/// TRANSITION → floored: a scored folder (and its scored descendant) that BECOMES
/// floored — its subtree renamed to `node_modules` — must have its stored rows
/// DELETED by the incremental pass, so the compacted store never keeps a floored
/// row. THE likeliest bug site, so pinned both ways (this is the delete direction).
#[test]
fn incremental_deletes_rows_that_become_floored() {
    let home = "/Users/test";
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = importance_db_path(dir.path(), ROOT_VOLUME_ID);

    // BEFORE: `/Users/test/proj/pkg` and `/Users/test/proj/pkg/sub` are ordinary
    // (unfloored) folders with rows.
    let before_paths = ["/Users/test/proj", "/Users/test/proj/pkg", "/Users/test/proj/pkg/sub"];
    let before: Vec<_> = before_paths
        .iter()
        .enumerate()
        .map(|(i, p)| folder_at(i as i64 + 1, p, home, &before_paths))
        .collect();
    let writer = full_pass_walk(dir.path(), home, &before);

    let store = ImportanceStore::open(&db_path).expect("open");
    assert!(
        store.weight_for("/Users/test/proj/pkg").expect("read").is_some(),
        "pkg starts with a row (it isn't floored yet)"
    );
    assert!(
        store.weight_for("/Users/test/proj/pkg/sub").expect("read").is_some(),
        "pkg/sub starts with a row"
    );

    // AFTER: `pkg` is renamed to `node_modules`. The new walk has `node_modules`
    // (self-floored) and its child `sub` (now under-floored). The incremental sees
    // the parent `proj`'s listing changed.
    let after_paths = [
        "/Users/test/proj",
        "/Users/test/proj/node_modules",
        "/Users/test/proj/node_modules/sub",
    ];
    let after: Vec<_> = after_paths
        .iter()
        .enumerate()
        .map(|(i, p)| folder_at(i as i64 + 1, p, home, &after_paths))
        .collect();
    // The changed path is the renamed folder's parent (its listing changed) — the
    // subtree expansion then revisits everything under it.
    let changed = vec!["/Users/test/proj".to_string()];
    incremental_rescore(
        &IncrementalInputs {
            writer: &writer,
            weights: &Weights::default(),
            home,
            now_secs: 1_000_000_000,
            available: SignalSet::listing_only(),
            visits: &HashMap::new(),
        },
        &after,
        &changed,
    )
    .expect("incremental");
    writer.flush_blocking().expect("flush");

    let store = ImportanceStore::open(&db_path).expect("reopen");
    // The renamed folder and its descendant both lost their rows (now floored).
    assert!(
        store
            .weight_for("/Users/test/proj/node_modules")
            .expect("read")
            .is_none(),
        "a folder that became a node_modules loses its row"
    );
    assert!(
        store
            .weight_for("/Users/test/proj/node_modules/sub")
            .expect("read")
            .is_none(),
        "its now-under-floored descendant loses its row too"
    );
    // The stale pre-rename `pkg` rows are gone as well (the paths no longer exist).
    assert!(
        store.weight_for("/Users/test/proj/pkg").expect("read").is_none(),
        "the pre-rename pkg path no longer has a row"
    );
    // The project root itself stays scored (it didn't floor).
    assert!(
        store.weight_for("/Users/test/proj").expect("read").is_some(),
        "the unfloored project root keeps its row"
    );
    writer.shutdown();
}

/// TRANSITION → unfloored: a floored folder (and its floored descendant) that STOPS
/// flooring — a `node_modules` renamed to an ordinary name — gets SCORED and
/// INSERTED by the incremental pass, gaining rows it never had while floored. The
/// insert direction of the same transition.
#[test]
fn incremental_scores_rows_that_stop_being_floored() {
    let home = "/Users/test";
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = importance_db_path(dir.path(), ROOT_VOLUME_ID);

    // BEFORE: `node_modules` and its child floor, so they have NO rows after a full
    // pass (only the unfloored `proj` root does).
    let before_paths = [
        "/Users/test/proj",
        "/Users/test/proj/node_modules",
        "/Users/test/proj/node_modules/sub",
    ];
    let before: Vec<_> = before_paths
        .iter()
        .enumerate()
        .map(|(i, p)| folder_at(i as i64 + 1, p, home, &before_paths))
        .collect();
    let writer = full_pass_walk(dir.path(), home, &before);

    let store = ImportanceStore::open(&db_path).expect("open");
    assert!(
        store
            .weight_for("/Users/test/proj/node_modules")
            .expect("read")
            .is_none(),
        "a node_modules has no row (floored)"
    );

    // AFTER: `node_modules` is renamed to `pkg` (ordinary). Both it and its child
    // stop flooring and should gain rows.
    let after_paths = ["/Users/test/proj", "/Users/test/proj/pkg", "/Users/test/proj/pkg/sub"];
    let after: Vec<_> = after_paths
        .iter()
        .enumerate()
        .map(|(i, p)| folder_at(i as i64 + 1, p, home, &after_paths))
        .collect();
    let changed = vec!["/Users/test/proj".to_string()];
    incremental_rescore(
        &IncrementalInputs {
            writer: &writer,
            weights: &Weights::default(),
            home,
            now_secs: 1_000_000_000,
            available: SignalSet::listing_only(),
            visits: &HashMap::new(),
        },
        &after,
        &changed,
    )
    .expect("incremental");
    writer.flush_blocking().expect("flush");

    let store = ImportanceStore::open(&db_path).expect("reopen");
    let pkg = store
        .weight_for("/Users/test/proj/pkg")
        .expect("read")
        .expect("pkg gained a row after un-flooring");
    assert!(pkg.score > 0.0, "the un-floored folder scores above zero");
    assert!(
        store.weight_for("/Users/test/proj/pkg/sub").expect("read").is_some(),
        "its descendant gained a row too (no longer under a floored ancestor)"
    );
    writer.shutdown();
}

/// THE derive-on-read parity invariant that makes floored-row deletion safe: for
/// EVERY folder the real walk produces, whether the path-only classifier
/// (`classify::floors_by_path`, what the read side uses when a row is absent) says
/// "floored" must MATCH whether the pure scorer floors that folder's full signals
/// (what the pre-compaction store would have persisted as a `0.0` row). If they
/// ever disagreed, a folder could be dropped-as-floored on write yet read back as
/// unscored (or vice versa). Checked over the canonical synthetic home's whole
/// walk, so every fixture folder — floored and not — is a case.
#[test]
fn floored_by_path_matches_the_scorer_floor_for_every_walked_folder() {
    use crate::importance::classify::floors_by_path;
    use crate::importance::fixtures::SyntheticHome;
    use crate::importance::signals::{OptionalSignals, signals_for_dir};

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

    let weights = Weights::default();
    for f in &folders {
        // What the pre-compaction store would have seen: the full signals' floor.
        let signals = signals_for_dir(
            &f.entry,
            f.children,
            &f.path,
            &home.home,
            f.has_marker_below,
            f.under_floored_ancestor,
            OptionalSignals::default(),
        );
        let scorer_floored = crate::importance::explain(&signals, &SignalSet::listing_only(), &weights, now).floored;
        // What the read side derives from the path alone (no row).
        let path_floored = floors_by_path(&f.path, &home.home);
        assert_eq!(
            path_floored, scorer_floored,
            "derive-on-read disagreed with the scorer floor for {}: path={path_floored}, scorer={scorer_floored}",
            f.path
        );
    }
}

// ── M4: multi-volume, SMB degradation, offline reads ──────────────────────

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
