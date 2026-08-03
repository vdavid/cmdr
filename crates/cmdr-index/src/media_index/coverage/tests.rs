//! The coverage rule (what counts as covered at a threshold, in each scope, for a
//! preview and for a reclaim) and the joined per-folder read over both caches.

use super::*;
use crate::media_index::scheduler::enrich::ImageEntry;

fn img(path: &str) -> ImageEntry {
    ImageEntry {
        path: path.to_string(),
        mtime: Some(1),
        size: Some(2),
        kind: crate::media_index::predicate::MediaKind::Image,
    }
}

fn scores(entries: &[(&str, f64)]) -> HashMap<String, f64> {
    entries.iter().map(|(p, s)| (p.to_string(), *s)).collect()
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

// ── The join: one read over both caches ──────────────────────────────────────

#[test]
fn folder_coverage_rolls_up_eligible_and_accounted_over_subtrees() {
    let vid = "coverage-test-folder-coverage";
    invalidate(vid);
    accounted::invalidate(vid);
    // Eligible: three qualifying images across two leaf dirs under /a.
    replace_from_entries(vid, &[img("/a/b/x.jpg"), img("/a/b/y.jpg"), img("/a/c/z.jpg")]);
    // Accounted: seed from a missing `media.db` (no enriched rows), then let the
    // writer's delta land — only one of the three has been enriched so far, in /a/b.
    accounted::ensure_seeded(vid, Path::new("/nonexistent"));
    accounted::inc(vid, "/a/b");

    let folders = vec!["/a".to_string(), "/a/b".to_string(), "/a/c".to_string()];
    let cov = folder_coverage(Path::new("/nonexistent"), vid, &folders);
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
    accounted::invalidate(vid);
}

#[test]
fn folder_coverage_is_zero_for_an_unseeded_unbuilt_volume() {
    // No eligible cache and no accounted seed ⇒ honest zeros, not a panic.
    let vid = "coverage-test-folder-coverage-empty";
    invalidate(vid);
    accounted::invalidate(vid);
    // The read seeds the accounted side itself; eligible has no index, so it stays zero.
    let cov = folder_coverage(Path::new("/nonexistent"), vid, &["/anything".to_string()]);
    assert_eq!(
        cov,
        vec![FolderCoverageCounts {
            eligible: 0,
            accounted: 0
        }]
    );
    accounted::invalidate(vid);
}
