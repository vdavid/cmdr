//! Part of `conflict.rs`, split out as a `#[path]` child so the module itself
//! stays readable. `super::` here is `conflict`, exactly as when these lived
//! inline.
//!
//! Pure-state tests for the two-bucket `ApplyToAll` latch model.
//!
//! Rules (per UX spec):
//!   1. Normal clash → choice lands in the `normal` bucket only.
//!   2. File-to-folder clash → choice lands in the `file_to_folder` bucket
//!      only.
//!   3. Special case: if the FIRST clash of the operation is a
//!      file-to-folder one, a "* all" choice spreads to both buckets.
//!   4. Carry-over: Skip/Rename in the `normal` bucket apply to subsequent
//!      file-to-folder clashes too (these are universally safe). Overwrite
//!      variants never carry over from normal → file-to-folder.
use super::*;

fn fresh() -> ApplyToAll {
    ApplyToAll::default()
}

#[test]
fn default_state_is_empty() {
    let state = fresh();
    assert!(apply_to_all_effective(&state, false).is_none());
    assert!(apply_to_all_effective(&state, true).is_none());
}

#[test]
fn normal_overwrite_all_stays_in_normal_bucket() {
    let mut state = fresh();
    apply_to_all_record(&mut state, false, ConflictResolution::Overwrite, true);
    assert_eq!(
        apply_to_all_effective(&state, false),
        Some(ConflictResolution::Overwrite)
    );
    // Does NOT spread to file-to-folder — user has to be re-prompted.
    assert_eq!(apply_to_all_effective(&state, true), None);
}

#[test]
fn normal_skip_all_carries_over_to_file_to_folder() {
    let mut state = fresh();
    apply_to_all_record(&mut state, false, ConflictResolution::Skip, true);
    assert_eq!(apply_to_all_effective(&state, false), Some(ConflictResolution::Skip));
    // Safe action: skip the file-to-folder one too without re-prompting.
    assert_eq!(apply_to_all_effective(&state, true), Some(ConflictResolution::Skip));
}

#[test]
fn normal_rename_all_carries_over_to_file_to_folder() {
    let mut state = fresh();
    apply_to_all_record(&mut state, false, ConflictResolution::Rename, true);
    assert_eq!(apply_to_all_effective(&state, true), Some(ConflictResolution::Rename));
}

#[test]
fn normal_conditional_variants_do_not_carry_over() {
    // OverwriteSmaller / OverwriteOlder are destructive — same rule as
    // Overwrite. They never reach file-to-folder without an explicit prompt.
    let mut state = fresh();
    apply_to_all_record(&mut state, false, ConflictResolution::OverwriteSmaller, true);
    assert_eq!(apply_to_all_effective(&state, true), None);

    let mut state = fresh();
    apply_to_all_record(&mut state, false, ConflictResolution::OverwriteOlder, true);
    assert_eq!(apply_to_all_effective(&state, true), None);
}

#[test]
fn file_to_folder_first_overwrite_all_spreads_to_normal() {
    // Spec: "if a file-to-folder clash is the first one, then any '* all'
    // choices should apply to ALL types of clashes."
    let mut state = fresh();
    apply_to_all_record(&mut state, true, ConflictResolution::Overwrite, true);
    assert_eq!(
        apply_to_all_effective(&state, true),
        Some(ConflictResolution::Overwrite)
    );
    assert_eq!(
        apply_to_all_effective(&state, false),
        Some(ConflictResolution::Overwrite)
    );
}

#[test]
fn file_to_folder_later_overwrite_all_does_not_spread() {
    // Spec example: user picks Overwrite all on a normal clash; later a
    // file-to-folder clash comes up and the user picks Skip all in it —
    // that Skip all applies to file-to-folder only.
    let mut state = fresh();
    apply_to_all_record(&mut state, false, ConflictResolution::Overwrite, true);
    // Now a file-to-folder clash arrives. Even though a normal "Overwrite all"
    // is set, file-to-folder is destructive enough to re-prompt → user picks
    // Skip all in the file-to-folder dialog.
    apply_to_all_record(&mut state, true, ConflictResolution::Skip, true);

    // Normal bucket keeps the original Overwrite — the new Skip is
    // file-to-folder-only.
    assert_eq!(
        apply_to_all_effective(&state, false),
        Some(ConflictResolution::Overwrite)
    );
    assert_eq!(apply_to_all_effective(&state, true), Some(ConflictResolution::Skip));
}

#[test]
fn single_choice_does_not_set_apply_to_all_but_still_seeds_first_clash_flag() {
    // A non-"apply to all" choice doesn't latch, but it DOES mean the next
    // file-to-folder clash isn't "the first" any more, so its "* all"
    // choice shouldn't spread to normal.
    let mut state = fresh();
    apply_to_all_record(
        &mut state,
        false,
        ConflictResolution::Overwrite,
        /* apply_to_all */ false,
    );

    // Nothing latched yet.
    assert_eq!(apply_to_all_effective(&state, false), None);
    assert_eq!(apply_to_all_effective(&state, true), None);

    // Now a file-to-folder clash; user picks Overwrite all. Because a
    // normal clash already happened, this is NOT the first clash any more
    // → don't spread.
    apply_to_all_record(&mut state, true, ConflictResolution::Overwrite, true);
    assert_eq!(
        apply_to_all_effective(&state, true),
        Some(ConflictResolution::Overwrite)
    );
    assert_eq!(apply_to_all_effective(&state, false), None);
}

#[test]
fn file_to_folder_latch_wins_over_normal_carry_over() {
    // If both buckets have a value, the directly-set file-to-folder one
    // wins (don't fall back to the normal-bucket Skip/Rename carry-over).
    let mut state = fresh();
    apply_to_all_record(&mut state, false, ConflictResolution::Skip, true);
    apply_to_all_record(&mut state, true, ConflictResolution::Overwrite, true);
    assert_eq!(
        apply_to_all_effective(&state, true),
        Some(ConflictResolution::Overwrite)
    );
}
