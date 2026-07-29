use super::*;

// ── The cache build / refill / patch (Fix: keep the cache warm, not cold) ──

fn img(path: &str) -> ImageEntry {
    ImageEntry {
        path: path.to_string(),
        mtime: Some(1),
        size: Some(2),
        kind: crate::media_index::predicate::MediaKind::Image,
    }
}

fn touched(dirs: &[&str]) -> HashSet<String> {
    dirs.iter().map(|d| d.to_string()).collect()
}

#[test]
fn build_counts_aggregates_per_folder_and_total() {
    let counts = build_counts(&[img("/p/a.jpg"), img("/p/b.jpg"), img("/q/c.jpg")]);
    assert_eq!(counts.total, 3);
    assert_eq!(counts.per_folder.get("/p").copied(), Some(2));
    assert_eq!(counts.per_folder.get("/q").copied(), Some(1));
}

#[test]
fn patch_updates_only_the_touched_dir_and_moves_total() {
    // /a re-walked to 1 image (was 3); /b is untouched. total moves by the /a delta only.
    let existing = FolderImageCounts {
        per_folder: [("/a".to_string(), 3u64), ("/b".to_string(), 5)].into_iter().collect(),
        total: 8,
    };
    let patched = patch_counts(&existing, &touched(&["/a"]), &[img("/a/x.jpg")]);
    assert_eq!(patched.per_folder.get("/a").copied(), Some(1), "/a re-counted");
    assert_eq!(patched.per_folder.get("/b").copied(), Some(5), "/b untouched");
    assert_eq!(patched.total, 6, "total moved by the /a delta (3 → 1)");
}

#[test]
fn patch_drops_a_dir_that_fell_to_zero() {
    // Every qualifying image left /a (the tick walked it and found none) ⇒ /a leaves
    // `per_folder` (which only holds folders with ≥ 1), and total drops by its old count.
    let existing = FolderImageCounts {
        per_folder: [("/a".to_string(), 3u64), ("/b".to_string(), 5)].into_iter().collect(),
        total: 8,
    };
    let patched = patch_counts(&existing, &touched(&["/a"]), &[]);
    assert!(!patched.per_folder.contains_key("/a"), "/a dropped at zero");
    assert_eq!(patched.per_folder.get("/b").copied(), Some(5));
    assert_eq!(patched.total, 5);
}

#[test]
fn patch_adds_a_newly_qualifying_dir() {
    // A touched dir absent from the cache (a folder's first qualifying image) is added.
    let existing = FolderImageCounts {
        per_folder: [("/b".to_string(), 5u64)].into_iter().collect(),
        total: 5,
    };
    let patched = patch_counts(&existing, &touched(&["/a"]), &[img("/a/x.jpg"), img("/a/y.jpg")]);
    assert_eq!(patched.per_folder.get("/a").copied(), Some(2), "/a added");
    assert_eq!(patched.total, 7);
}

#[test]
fn replace_then_patch_round_trips_through_the_global_cache() {
    // A unique volume id keeps this isolated from the process-global cache other tests use.
    let vid = "coverage-test-replace-patch";
    replace_from_entries(vid, &[img("/a/x.jpg"), img("/a/y.jpg"), img("/b/z.jpg")]);
    let after_replace = COUNTS.lock_ignore_poison().get(vid).cloned().expect("cached");
    assert_eq!(after_replace.total, 3);
    assert_eq!(after_replace.per_folder.get("/a").copied(), Some(2));

    // A live tick re-walks /a and finds one image now: the cache patches /a in place.
    patch_touched_dirs(vid, &touched(&["/a"]), &[img("/a/x.jpg")]);
    let after_patch = COUNTS.lock_ignore_poison().get(vid).cloned().expect("cached");
    assert_eq!(after_patch.per_folder.get("/a").copied(), Some(1), "/a patched");
    assert_eq!(after_patch.per_folder.get("/b").copied(), Some(1), "/b untouched");
    assert_eq!(after_patch.total, 2);
    invalidate(vid);
}

#[test]
fn patch_is_a_noop_without_a_cached_volume() {
    // No cached counts yet ⇒ the patch does nothing (the next preview builds them fresh),
    // never inserting a partial (touched-dirs-only) entry that would undercount the volume.
    let vid = "coverage-test-patch-noop";
    invalidate(vid);
    patch_touched_dirs(vid, &touched(&["/a"]), &[img("/a/x.jpg")]);
    assert!(
        !COUNTS.lock_ignore_poison().contains_key(vid),
        "a patch with nothing cached inserts nothing"
    );
}

#[test]
fn concurrent_cold_builds_run_the_walk_once() {
    // The cold build is an O(entries) index walk costing gigabytes of transient heap on
    // a multi-million-entry volume. Several callers can go cold at once (the volume-state
    // poll, the slider preview, the reclaim preview all land within milliseconds of a
    // launch), so N concurrent callers must NOT each run their own walk — they queue on
    // the volume's build and the losers find the cache warm.
    let vid = "coverage-test-concurrent-build";
    invalidate(vid);
    let builds = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let threads = 8;
    let barrier = Arc::new(std::sync::Barrier::new(threads));

    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let builds = Arc::clone(&builds);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                // Every thread arrives before any of them looks at the cache, so they all
                // genuinely race the cold path.
                barrier.wait();
                let counts = get_or_build_with(vid, || {
                    builds.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    // Stand in for the walk's duration, so a racing caller would have
                    // time to start a second one.
                    // allowed-test-sleep: the fake walk latency IS the subject — without it the racers could serialize by luck and pass an un-deduplicated build
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    Some(FolderImageCounts {
                        per_folder: [("/photos".to_string(), 3u64)].into_iter().collect(),
                        total: 3,
                    })
                });
                counts.expect("built").total
            })
        })
        .collect();
    for handle in handles {
        assert_eq!(handle.join().expect("thread"), 3, "every caller gets the counts");
    }

    assert_eq!(
        builds.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the concurrent callers share ONE walk"
    );
    invalidate(vid);
}

#[test]
fn covered_counts_folders_and_images_above_threshold() {
    let counts = FolderImageCounts {
        per_folder: [
            ("/high".to_string(), 100u64),
            ("/mid".to_string(), 40),
            ("/low".to_string(), 5),
        ]
        .into_iter()
        .collect(),
        total: 145,
    };
    let scores: HashMap<String, f64> = [
        ("/high".to_string(), 0.9),
        ("/mid".to_string(), 0.5),
        ("/low".to_string(), 0.1),
    ]
    .into_iter()
    .collect();

    // Threshold 0.5: /high and /mid qualify ⇒ 2 folders, 140 images.
    assert_eq!(covered_for_volume(&counts, &scores, 0.5), (2, 140));
    // Threshold 0.95: nothing qualifies.
    assert_eq!(covered_for_volume(&counts, &scores, 0.95), (0, 0));
    // Threshold 0.0: all three ⇒ 3 folders, 145 images.
    assert_eq!(covered_for_volume(&counts, &scores, 0.0), (3, 145));
}

#[test]
fn covered_ignores_a_scored_folder_with_no_qualifying_images() {
    // A folder importance scored but holding no images contributes a folder, zero
    // images (honest: the count never over-promises images).
    let counts = FolderImageCounts {
        per_folder: [("/photos".to_string(), 10u64)].into_iter().collect(),
        total: 10,
    };
    let scores: HashMap<String, f64> = [("/photos".to_string(), 0.8), ("/empty".to_string(), 0.8)]
        .into_iter()
        .collect();
    assert_eq!(covered_for_volume(&counts, &scores, 0.5), (2, 10));
}

// ── The reclaim partition: stored rows inside vs outside coverage ──────

fn scores(entries: &[(&str, f64)]) -> HashMap<String, f64> {
    entries.iter().map(|(p, s)| (p.to_string(), *s)).collect()
}

#[test]
fn partition_splits_stored_rows_at_the_threshold_boundary() {
    // A folder scoring exactly AT the threshold survives; one below is doomed; one
    // with no score row at all is doomed (floored / scored away → score 0.0).
    let stored = vec![
        "/at/a.jpg".to_string(),
        "/below/b.jpg".to_string(),
        "/floored/c.jpg".to_string(),
    ];
    let folder_scores = scores(&[("/at", 0.4), ("/below", 0.2)]);
    let no = |_: &str| false;
    let part = partition_stored(&stored, &folder_scores, 0.4, IndexScope::ByImportance, &no, &no);
    assert_eq!(part.surviving, 1, "the at-threshold folder survives");
    assert_eq!(
        part.doomed,
        vec!["/below/b.jpg".to_string(), "/floored/c.jpg".to_string()],
        "the below-threshold and the no-score-row folders are doomed"
    );
    // The partition invariant: every stored row lands in exactly one bucket.
    assert_eq!(part.surviving as usize + part.doomed.len(), stored.len());
}

#[test]
fn partition_keeps_an_override_covered_row_below_threshold() {
    // An "always index" override survives even when its folder scores below the
    // threshold (or isn't scored at all) — same precedence as enrichment.
    let stored = vec!["/archive/a.jpg".to_string()];
    let folder_scores = scores(&[]);
    let is_override = |p: &str| p.starts_with("/archive/");
    let no = |_: &str| false;
    let part = partition_stored(
        &stored,
        &folder_scores,
        0.8,
        IndexScope::ByImportance,
        &is_override,
        &no,
    );
    assert_eq!(part.surviving, 1, "an override-covered row survives");
    assert!(part.doomed.is_empty());
}

#[test]
fn narrowing_the_scope_dooms_the_importance_covered_rows_but_keeps_the_chosen_ones() {
    // Switching to "only folders I choose" doesn't delete anything by itself: the
    // rows an above-threshold folder earned simply fall OUTSIDE coverage, becoming
    // the same kept/doomed set the reclaim line already offers to free. The chosen
    // folder's rows survive, whatever importance thinks of it.
    let stored = vec!["/important/a.jpg".to_string(), "/chosen/b.jpg".to_string()];
    let folder_scores = scores(&[("/important", 0.9)]);
    let is_override = |p: &str| p.starts_with("/chosen/");
    let no = |_: &str| false;

    let automatic = partition_stored(
        &stored,
        &folder_scores,
        0.0,
        IndexScope::ByImportance,
        &is_override,
        &no,
    );
    assert_eq!(automatic.surviving, 2, "both are covered automatically");

    let chosen = partition_stored(
        &stored,
        &folder_scores,
        0.0,
        IndexScope::ChosenFolders,
        &is_override,
        &no,
    );
    assert_eq!(chosen.surviving, 1, "only the chosen folder stays covered");
    assert_eq!(
        chosen.doomed,
        vec!["/important/a.jpg".to_string()],
        "the importance-covered row is reclaimable, not deleted here"
    );
    // The partition invariant still holds, so the reclaim arithmetic adds up.
    assert_eq!(chosen.surviving as usize + chosen.doomed.len(), stored.len());
}

#[test]
fn the_narrow_scope_ignores_the_threshold_entirely() {
    // No sentinel threshold: at the broadest slider position (0.0) a scored folder
    // still isn't covered in the narrow scope. Only the chosen folders are.
    let stored = vec!["/scored/a.jpg".to_string()];
    let folder_scores = scores(&[("/scored", 1.0)]);
    let no = |_: &str| false;
    for threshold in [0.0, 0.5, 1.0] {
        let part = partition_stored(&stored, &folder_scores, threshold, IndexScope::ChosenFolders, &no, &no);
        assert_eq!(part.surviving, 0, "threshold {threshold} must not matter");
    }
}

#[test]
fn an_exclusion_still_beats_a_chosen_folder() {
    // The privacy veto is a hard veto in BOTH scopes; naming a folder can't unblock it.
    let stored = vec!["/chosen/secret.jpg".to_string()];
    let always = |_: &str| true;
    let excluded = |_: &str| true;
    for scope in [IndexScope::ChosenFolders, IndexScope::ByImportance] {
        let part = partition_stored(&stored, &scores(&[]), 0.0, scope, &always, &excluded);
        assert_eq!(part.surviving, 0);
        assert_eq!(part.doomed, vec!["/chosen/secret.jpg".to_string()]);
    }
}

#[test]
fn counts_follow_the_scope_the_same_way_the_gate_does() {
    let counts = FolderImageCounts {
        per_folder: [("/important".to_string(), 100u64), ("/chosen".to_string(), 7)]
            .into_iter()
            .collect(),
        total: 107,
    };
    let folder_scores = scores(&[("/important", 0.9)]);
    let is_override = |p: &str| p == "/chosen";

    // Narrow: only the chosen folder counts, however broad the slider.
    assert_eq!(
        covered_in_scope(&counts, &folder_scores, 0.0, IndexScope::ChosenFolders, &is_override),
        (1, 7)
    );
    // Automatic: the above-threshold folders PLUS the chosen one (which importance
    // doesn't score at all, so a plain threshold count would miss it).
    assert_eq!(
        covered_in_scope(&counts, &folder_scores, 0.5, IndexScope::ByImportance, &is_override),
        (2, 107)
    );
}

#[test]
fn the_automatic_scope_never_double_counts_a_chosen_and_scored_folder() {
    // A folder that is BOTH above the threshold and explicitly chosen contributes once.
    let counts = FolderImageCounts {
        per_folder: [("/photos".to_string(), 12u64)].into_iter().collect(),
        total: 12,
    };
    let folder_scores = scores(&[("/photos", 0.9)]);
    let always = |_: &str| true;
    assert_eq!(
        covered_in_scope(&counts, &folder_scores, 0.5, IndexScope::ByImportance, &always),
        (1, 12)
    );
}

#[test]
fn partition_dooms_an_excluded_row_even_when_covered() {
    // The privacy exclusion is a HARD veto: an excluded row is doomed even if an
    // override would otherwise cover it (exclusion beats coverage everywhere).
    let stored = vec!["/archive/secret.jpg".to_string()];
    let folder_scores = scores(&[]);
    let always = |_: &str| true; // override covers everything
    let is_excluded = |p: &str| p.starts_with("/archive/");
    let part = partition_stored(
        &stored,
        &folder_scores,
        0.0,
        IndexScope::ByImportance,
        &always,
        &is_excluded,
    );
    assert_eq!(part.surviving, 0, "an excluded row never survives");
    assert_eq!(part.doomed, vec!["/archive/secret.jpg".to_string()]);
}

// ── The accounted numerator: seed, increment, decrement, subtree rollup ──────

#[test]
fn build_subtree_rollup_sums_over_a_dir_and_its_descendants() {
    // `/a` holds no direct images but two descendant dirs do, so its subtree total is
    // their sum; the root `/` totals everything; a leaf reports only its own count.
    let per_folder: HashMap<String, u64> = [
        ("/a/b".to_string(), 2u64),
        ("/a/c".to_string(), 3),
        ("/x".to_string(), 1),
    ]
    .into_iter()
    .collect();
    let rollup = build_subtree_rollup(&per_folder);
    assert_eq!(rollup.get("/a/b").copied(), Some(2), "leaf is its own count");
    assert_eq!(rollup.get("/a/c").copied(), Some(3));
    assert_eq!(rollup.get("/a").copied(), Some(5), "/a rolls up its two child dirs");
    assert_eq!(rollup.get("/x").copied(), Some(1));
    assert_eq!(rollup.get("/").copied(), Some(6), "the root totals the whole volume");
    assert_eq!(rollup.get("/missing"), None, "a dir with nothing under it is absent");
}

#[test]
fn accounted_seed_increment_decrement_and_subtree() {
    let vid = "coverage-test-accounted-seed";
    invalidate_accounted(vid);
    // Seed one dir with two enriched rows, then add a sibling dir via increment.
    seed_accounted_if_absent(vid, [("/a/b".to_string(), 2u64)].into_iter().collect());
    accounted_inc(vid, "/a/c");
    // The subtree of /a rolls up both dirs (2 + 1).
    assert_eq!(accounted_subtrees(vid, &["/a".to_string()]), vec![3]);
    assert_eq!(
        accounted_subtrees(vid, &["/a/b".to_string(), "/a/c".to_string()]),
        vec![2, 1]
    );

    // Decrement /a/b twice: it drains to zero and is dropped from the map.
    accounted_dec(vid, "/a/b");
    accounted_dec(vid, "/a/b");
    assert_eq!(accounted_subtrees(vid, &["/a/b".to_string()]), vec![0], "/a/b drained");
    assert_eq!(
        accounted_subtrees(vid, &["/a".to_string()]),
        vec![1],
        "only /a/c remains"
    );

    // A decrement past zero never goes negative (a stray delete of an untracked dir).
    accounted_dec(vid, "/a/b");
    accounted_dec(vid, "/a/c");
    accounted_dec(vid, "/a/c");
    assert_eq!(accounted_subtrees(vid, &["/a".to_string()]), vec![0], "never negative");
    invalidate_accounted(vid);
}

#[test]
fn accounted_ops_on_an_unseeded_volume_are_noops() {
    // A delta before seeding must NOT insert a partial (un-seeded) entry that a later
    // `ensure_accounted_seeded` would trust as a complete baseline.
    let vid = "coverage-test-accounted-unseeded";
    invalidate_accounted(vid);
    accounted_inc(vid, "/a");
    assert!(
        !ACCOUNTED.lock_ignore_poison().contains_key(vid),
        "an increment on an unseeded volume inserts nothing"
    );
    assert_eq!(accounted_subtrees(vid, &["/a".to_string()]), vec![0]);
}

#[test]
fn seed_if_absent_never_clobbers_an_existing_entry() {
    // The insert-if-absent concurrency line: a second seed (e.g. a command scan that
    // lost the race to the writer) must not overwrite the live counts.
    let vid = "coverage-test-accounted-noclobber";
    invalidate_accounted(vid);
    seed_accounted_if_absent(vid, [("/a".to_string(), 5u64)].into_iter().collect());
    accounted_inc(vid, "/a");
    // A late, stale seed is discarded — the incremented count survives.
    seed_accounted_if_absent(vid, [("/a".to_string(), 5u64)].into_iter().collect());
    assert_eq!(accounted_subtrees(vid, &["/a".to_string()]), vec![6]);
    invalidate_accounted(vid);
}

#[test]
fn accounted_reset_empties_but_keeps_the_volume_seeded() {
    let vid = "coverage-test-accounted-reset";
    invalidate_accounted(vid);
    seed_accounted_if_absent(vid, [("/a".to_string(), 3u64)].into_iter().collect());
    accounted_reset(vid);
    assert_eq!(accounted_subtrees(vid, &["/a".to_string()]), vec![0], "emptied");
    assert!(
        ACCOUNTED.lock_ignore_poison().contains_key(vid),
        "still seeded, so a later insert bumps from zero rather than re-scanning"
    );
    accounted_inc(vid, "/a");
    assert_eq!(accounted_subtrees(vid, &["/a".to_string()]), vec![1]);
    invalidate_accounted(vid);
}

#[test]
fn folder_coverage_rolls_up_eligible_and_accounted_over_subtrees() {
    let vid = "coverage-test-folder-coverage";
    invalidate(vid);
    invalidate_accounted(vid);
    // Eligible: three qualifying images across two leaf dirs under /a.
    replace_from_entries(vid, &[img("/a/b/x.jpg"), img("/a/b/y.jpg"), img("/a/c/z.jpg")]);
    // Accounted: only one of them enriched so far (in /a/b).
    seed_accounted_if_absent(vid, [("/a/b".to_string(), 1u64)].into_iter().collect());

    let folders = vec!["/a".to_string(), "/a/b".to_string(), "/a/c".to_string()];
    let cov = folder_coverage(vid, &folders);
    assert_eq!(
        cov[0],
        FolderCoverageCounts {
            eligible: 3,
            accounted: 1
        },
        "/a subtree"
    );
    assert_eq!(
        cov[1],
        FolderCoverageCounts {
            eligible: 2,
            accounted: 1
        },
        "/a/b"
    );
    assert_eq!(
        cov[2],
        FolderCoverageCounts {
            eligible: 1,
            accounted: 0
        },
        "/a/c has an eligible image but nothing accounted yet"
    );
    invalidate(vid);
    invalidate_accounted(vid);
}

#[test]
fn folder_coverage_is_zero_for_an_unseeded_unbuilt_volume() {
    // No eligible cache and no accounted seed ⇒ honest zeros, not a panic.
    let vid = "coverage-test-folder-coverage-empty";
    invalidate(vid);
    invalidate_accounted(vid);
    // Accounted must be seeded (as the command does) before reading; eligible has no
    // index, so it stays zero.
    ensure_accounted_seeded(vid, Path::new("/nonexistent/media.db"));
    let cov = folder_coverage(vid, &["/anything".to_string()]);
    assert_eq!(
        cov,
        vec![FolderCoverageCounts {
            eligible: 0,
            accounted: 0
        }]
    );
    invalidate_accounted(vid);
}

#[test]
fn partition_at_threshold_zero_still_dooms_a_floored_folder() {
    // At threshold 0.0 a SCORED folder survives, but a floored folder (no score row)
    // is still doomed — it keys on map membership, never a `>= 0.0` on a default 0.0.
    let stored = vec!["/scored/a.jpg".to_string(), "/floored/b.jpg".to_string()];
    let folder_scores = scores(&[("/scored", 0.0)]);
    let no = |_: &str| false;
    let part = partition_stored(&stored, &folder_scores, 0.0, IndexScope::ByImportance, &no, &no);
    assert_eq!(part.surviving, 1, "the scored folder survives at threshold 0");
    assert_eq!(
        part.doomed,
        vec!["/floored/b.jpg".to_string()],
        "the floored folder (no score row) is doomed even at threshold 0"
    );
}
