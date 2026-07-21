//! Command-layer tests. The seeded-`media.db` → `OcrHit`-with-snippet round-trip the
//! command delegates to is covered end-to-end in `read/tests.rs`
//! (`search_finds_the_image_by_ocr_text_and_survives_unmount`); here we cover the
//! command-specific limit resolution.

use super::policy::{folder_override_should_kick, scope_change_should_kick, threshold_change_should_kick};
use super::{DEFAULT_LIMIT, MAX_LIMIT, resolve_limit};
use crate::media_index::gate::IndexScope;

#[test]
fn a_missing_limit_takes_the_default() {
    assert_eq!(resolve_limit(None), DEFAULT_LIMIT as usize);
}

#[test]
fn a_supplied_limit_is_honored_below_the_ceiling() {
    assert_eq!(resolve_limit(Some(25)), 25);
}

#[test]
fn an_oversized_limit_is_clamped_to_the_ceiling() {
    assert_eq!(resolve_limit(Some(100_000)), MAX_LIMIT as usize);
}

// ── The threshold-change kick decision (item 2c) ─────────────────────────────
// `media_index_set_importance_threshold` needs an `AppHandle` to kick, so the
// decide-then-kick logic is extracted here. The pure direction check
// (`gate::threshold_decreased`) has its own test in `gate`; these pin the combined
// decision the command actually makes.

#[test]
fn a_threshold_decrease_while_enabled_kicks() {
    // Lowering the threshold broadens coverage, so newly-covered folders enrich now.
    assert!(threshold_change_should_kick(0.6, 0.3, true));
}

#[test]
fn a_threshold_raise_never_kicks() {
    // A raise only defers future work (forward-only): nothing to enrich now.
    assert!(!threshold_change_should_kick(0.3, 0.6, true));
}

#[test]
fn an_unchanged_threshold_never_kicks() {
    assert!(!threshold_change_should_kick(0.5, 0.5, true));
}

#[test]
fn a_decrease_while_disabled_never_kicks() {
    // With the feature off there is no pass to run.
    assert!(!threshold_change_should_kick(0.6, 0.3, false));
}

// ── The chosen-folder and scope kick decisions ───────────────────────────────
// Same shape: the commands need an `AppHandle` to kick, so the decision is extracted.

#[test]
fn choosing_a_folder_while_enabled_kicks_a_pass() {
    // Without this the folder sits unindexed until some volume happens to rescan, and
    // the whole "add the folder, watch it index" promise falls over.
    assert!(folder_override_should_kick(true, true));
}

#[test]
fn removing_a_chosen_folder_never_kicks() {
    // Coverage only narrows; the rows persist (forward-only) until a reclaim.
    assert!(!folder_override_should_kick(false, true));
}

#[test]
fn choosing_a_folder_while_disabled_never_kicks() {
    assert!(!folder_override_should_kick(true, false));
}

#[test]
fn broadening_the_scope_while_enabled_kicks_a_pass() {
    assert!(scope_change_should_kick(
        IndexScope::ChosenFolders,
        IndexScope::ByImportance,
        true
    ));
}

#[test]
fn narrowing_the_scope_never_kicks() {
    // Narrowing has nothing new to enrich; the now-uncovered rows stay searchable and
    // surface as the reclaim offer instead.
    assert!(!scope_change_should_kick(
        IndexScope::ByImportance,
        IndexScope::ChosenFolders,
        true
    ));
}

#[test]
fn broadening_the_scope_while_disabled_never_kicks() {
    assert!(!scope_change_should_kick(
        IndexScope::ChosenFolders,
        IndexScope::ByImportance,
        false
    ));
}
