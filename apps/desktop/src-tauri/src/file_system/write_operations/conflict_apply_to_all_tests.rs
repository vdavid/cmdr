//! Part of `conflict.rs`, split out as a `#[path]` child so the module itself
//! stays readable. `super::` here is `conflict`, exactly as when these lived
//! inline.
//!
//! Pure-state tests for the per-shape `ApplyToAll` latch model.
//!
//! Rules:
//!   1. A choice lands in the bucket for the shape it was answered on, and
//!      nowhere else.
//!   2. Special case: if the FIRST clash of the operation is a cross-type one,
//!      a "* all" choice spreads to the same-kind bucket too. It never spreads
//!      to the OTHER cross-type bucket: that is a different destructive act.
//!   3. Carry-over: Skip/Rename in the same-kind bucket apply to subsequent
//!      cross-type clashes too (these are universally safe). Overwrite
//!      variants never carry over from same-kind to a cross-type shape.
//!   4. A value read out of its OWN cross-type bucket is flagged as answered
//!      for that shape, which is what lets `resolution_for_clash` honour it
//!      where it refuses a blanket policy. A same-kind carry never is.
use super::*;

fn fresh() -> ApplyToAll {
    ApplyToAll::default()
}

/// The latched resolution alone, for the cells that don't care about consent.
fn effective(state: &ApplyToAll, kind: ClashKind) -> Option<ConflictResolution> {
    apply_to_all_effective(state, kind).map(|l| l.resolution)
}

/// Whether the value the latches hand back at `kind` was answered on a prompt
/// for that shape.
fn answered_for_shape(state: &ApplyToAll, kind: ClashKind) -> bool {
    apply_to_all_effective(state, kind).is_some_and(|l| l.answered_for_this_shape)
}

#[test]
fn default_state_is_empty() {
    let state = fresh();
    assert!(effective(&state, ClashKind::SameKind).is_none());
    assert!(effective(&state, ClashKind::FileOverFolder).is_none());
    assert!(effective(&state, ClashKind::FolderOverFile).is_none());
}

#[test]
fn same_kind_overwrite_all_stays_in_its_own_bucket() {
    let mut state = fresh();
    apply_to_all_record(&mut state, ClashKind::SameKind, ConflictResolution::Overwrite, true);
    assert_eq!(
        effective(&state, ClashKind::SameKind),
        Some(ConflictResolution::Overwrite)
    );
    // Neither cross-type shape inherits it — the user has to be re-prompted.
    assert_eq!(effective(&state, ClashKind::FileOverFolder), None);
    assert_eq!(effective(&state, ClashKind::FolderOverFile), None);
}

#[test]
fn same_kind_skip_all_carries_over_to_both_cross_type_shapes() {
    let mut state = fresh();
    apply_to_all_record(&mut state, ClashKind::SameKind, ConflictResolution::Skip, true);
    assert_eq!(effective(&state, ClashKind::SameKind), Some(ConflictResolution::Skip));
    // Safe action: skip the cross-type ones too without re-prompting.
    assert_eq!(
        effective(&state, ClashKind::FileOverFolder),
        Some(ConflictResolution::Skip)
    );
    assert_eq!(
        effective(&state, ClashKind::FolderOverFile),
        Some(ConflictResolution::Skip)
    );
}

#[test]
fn same_kind_rename_all_carries_over_to_both_cross_type_shapes() {
    let mut state = fresh();
    apply_to_all_record(&mut state, ClashKind::SameKind, ConflictResolution::Rename, true);
    assert_eq!(
        effective(&state, ClashKind::FileOverFolder),
        Some(ConflictResolution::Rename)
    );
    assert_eq!(
        effective(&state, ClashKind::FolderOverFile),
        Some(ConflictResolution::Rename)
    );
}

#[test]
fn same_kind_conditional_variants_do_not_carry_over() {
    // OverwriteSmaller / OverwriteOlder are destructive — same rule as
    // Overwrite. They never reach a cross-type clash without its own prompt.
    for variant in [ConflictResolution::OverwriteSmaller, ConflictResolution::OverwriteOlder] {
        let mut state = fresh();
        apply_to_all_record(&mut state, ClashKind::SameKind, variant, true);
        assert_eq!(effective(&state, ClashKind::FileOverFolder), None);
        assert_eq!(effective(&state, ClashKind::FolderOverFile), None);
    }
}

#[test]
fn a_cross_type_first_overwrite_all_spreads_to_same_kind() {
    // If a cross-type clash is the first one, a "* all" choice applies to
    // subsequent same-kind clashes as well: the person had seen nothing else,
    // so they were answering for the operation.
    for kind in [ClashKind::FileOverFolder, ClashKind::FolderOverFile] {
        let mut state = fresh();
        apply_to_all_record(&mut state, kind, ConflictResolution::Overwrite, true);
        assert_eq!(effective(&state, kind), Some(ConflictResolution::Overwrite));
        assert_eq!(
            effective(&state, ClashKind::SameKind),
            Some(ConflictResolution::Overwrite)
        );
    }
}

/// The spread stops at same-kind. Consenting to throw a FOLDER away says
/// nothing about throwing a FILE away, so the other cross-type shape must still
/// ask — and must not inherit an Overwrite through the same-kind bucket either.
#[test]
fn a_cross_type_overwrite_all_never_spreads_to_the_other_cross_type_shape() {
    let mut state = fresh();
    apply_to_all_record(
        &mut state,
        ClashKind::FileOverFolder,
        ConflictResolution::Overwrite,
        true,
    );
    assert_eq!(effective(&state, ClashKind::FolderOverFile), None);

    let mut state = fresh();
    apply_to_all_record(
        &mut state,
        ClashKind::FolderOverFile,
        ConflictResolution::Overwrite,
        true,
    );
    assert_eq!(effective(&state, ClashKind::FileOverFolder), None);
}

#[test]
fn a_later_cross_type_overwrite_all_does_not_spread() {
    // User picks Overwrite all on a same-kind clash; later a file→folder clash
    // comes up and they pick Skip all in it — that Skip all applies to
    // file→folder only.
    let mut state = fresh();
    apply_to_all_record(&mut state, ClashKind::SameKind, ConflictResolution::Overwrite, true);
    apply_to_all_record(&mut state, ClashKind::FileOverFolder, ConflictResolution::Skip, true);

    assert_eq!(
        effective(&state, ClashKind::SameKind),
        Some(ConflictResolution::Overwrite)
    );
    assert_eq!(
        effective(&state, ClashKind::FileOverFolder),
        Some(ConflictResolution::Skip)
    );
}

#[test]
fn single_choice_does_not_set_apply_to_all_but_still_seeds_first_clash_flag() {
    // A non-"apply to all" choice doesn't latch, but it DOES mean the next
    // cross-type clash isn't "the first" any more, so its "* all" choice
    // shouldn't spread to same-kind.
    let mut state = fresh();
    apply_to_all_record(
        &mut state,
        ClashKind::SameKind,
        ConflictResolution::Overwrite,
        /* apply_to_all */ false,
    );

    // Nothing latched yet.
    assert_eq!(effective(&state, ClashKind::SameKind), None);
    assert_eq!(effective(&state, ClashKind::FileOverFolder), None);

    // Now a file→folder clash; user picks Overwrite all. Because a same-kind
    // clash already happened, this is NOT the first clash any more → no spread.
    apply_to_all_record(
        &mut state,
        ClashKind::FileOverFolder,
        ConflictResolution::Overwrite,
        true,
    );
    assert_eq!(
        effective(&state, ClashKind::FileOverFolder),
        Some(ConflictResolution::Overwrite)
    );
    assert_eq!(effective(&state, ClashKind::SameKind), None);
}

#[test]
fn a_cross_type_latch_wins_over_a_same_kind_carry_over() {
    // If both buckets have a value, the directly-set cross-type one wins
    // (don't fall back to the same-kind Skip/Rename carry-over).
    let mut state = fresh();
    apply_to_all_record(&mut state, ClashKind::SameKind, ConflictResolution::Skip, true);
    apply_to_all_record(
        &mut state,
        ClashKind::FileOverFolder,
        ConflictResolution::Overwrite,
        true,
    );
    assert_eq!(
        effective(&state, ClashKind::FileOverFolder),
        Some(ConflictResolution::Overwrite)
    );
}

// ============================================================================
// Which values carry the person's consent
// ============================================================================

/// A value out of a cross-type bucket was answered on a prompt for that shape;
/// a same-kind carry reaching across types was not. This flag is the whole
/// difference between honouring an "Overwrite folders with files" and refusing
/// a policy nobody was asked about, so pin it directly.
#[test]
fn only_a_cross_type_bucket_reports_an_answer_for_that_shape() {
    let mut state = fresh();
    apply_to_all_record(
        &mut state,
        ClashKind::FileOverFolder,
        ConflictResolution::Overwrite,
        true,
    );
    assert!(answered_for_shape(&state, ClashKind::FileOverFolder));

    // The same value read at SameKind arrived there by the first-clash spread,
    // where it is an ordinary blanket carry.
    assert!(!answered_for_shape(&state, ClashKind::SameKind));

    // And a Skip carried over from same-kind is not an answer about types.
    let mut state = fresh();
    apply_to_all_record(&mut state, ClashKind::SameKind, ConflictResolution::Skip, true);
    assert!(!answered_for_shape(&state, ClashKind::FileOverFolder));
}

// ============================================================================
// What `resolution_for_clash` does with each of those
// ============================================================================

/// The rule in one table. `dest` is only ever logged.
fn resolved(state: &ApplyToAll, kind: ClashKind, configured: ConflictResolution) -> ConflictResolution {
    resolution_for_clash(apply_to_all_effective(state, kind), configured, kind, &"dest")
}

#[test]
fn a_configured_overwrite_is_refused_across_types_and_kept_within_one() {
    let state = fresh();
    assert_eq!(
        resolved(&state, ClashKind::SameKind, ConflictResolution::Overwrite),
        ConflictResolution::Overwrite
    );
    for kind in [ClashKind::FileOverFolder, ClashKind::FolderOverFile] {
        assert_eq!(
            resolved(&state, kind, ConflictResolution::Overwrite),
            ConflictResolution::Skip,
            "a policy picked in the transfer dialog was never shown a {kind:?} clash"
        );
    }
}

/// The regression this whole model exists for: the carry from an answered
/// cross-type prompt replaces, so "Overwrite folders with files" means all of
/// them and not just the first.
#[test]
fn an_answered_cross_type_overwrite_all_carries_and_replaces() {
    for kind in [ClashKind::FileOverFolder, ClashKind::FolderOverFile] {
        let mut state = fresh();
        apply_to_all_record(&mut state, kind, ConflictResolution::Overwrite, true);
        assert_eq!(
            resolved(&state, kind, ConflictResolution::Stop),
            ConflictResolution::Overwrite,
            "the {kind:?} carry was answered on a prompt naming both kinds"
        );
    }
}

/// The conditional variants are refused across types whoever asked for them.
/// The dialog offers them on a cross-type prompt, but their labels name no
/// type and the comparison behind them is against a directory's own inode size,
/// which every ordinary file beats.
#[test]
fn an_answered_conditional_variant_is_still_refused_across_types() {
    for variant in [ConflictResolution::OverwriteSmaller, ConflictResolution::OverwriteOlder] {
        for kind in [ClashKind::FileOverFolder, ClashKind::FolderOverFile] {
            let mut state = fresh();
            apply_to_all_record(&mut state, kind, variant, true);
            assert_eq!(
                resolved(&state, kind, ConflictResolution::Stop),
                ConflictResolution::Skip,
                "{variant:?} cannot ask its own question at a {kind:?} clash"
            );
            assert_eq!(
                answered_resolution_for_clash(variant, kind, &"dest"),
                ConflictResolution::Skip,
                "{variant:?} answered on the prompt itself is refused the same way"
            );
        }
    }
}

/// Skip and Rename need no consent: neither destroys anything.
#[test]
fn the_safe_resolutions_pass_through_every_shape() {
    let state = fresh();
    for kind in [
        ClashKind::SameKind,
        ClashKind::FileOverFolder,
        ClashKind::FolderOverFile,
    ] {
        for safe in [ConflictResolution::Skip, ConflictResolution::Rename] {
            assert_eq!(resolved(&state, kind, safe), safe);
        }
    }
}

// ============================================================================
// Classifying a clash
// ============================================================================

#[test]
fn a_destination_that_would_not_stat_is_never_a_cross_type_clash() {
    // An unanswerable type is not grounds for a destructive act, so it reads as
    // same-kind and the write below refuses the occupied name on its own.
    for incoming in [IncomingItem::Leaf, IncomingItem::Directory] {
        assert_eq!(ClashKind::of(incoming, None), ClashKind::SameKind);
    }
}

#[test]
fn a_clash_is_classified_from_both_sides() {
    assert_eq!(ClashKind::of(IncomingItem::Leaf, Some(true)), ClashKind::FileOverFolder);
    assert_eq!(
        ClashKind::of(IncomingItem::Directory, Some(false)),
        ClashKind::FolderOverFile
    );
    assert_eq!(ClashKind::of(IncomingItem::Leaf, Some(false)), ClashKind::SameKind);
    assert_eq!(ClashKind::of(IncomingItem::Directory, Some(true)), ClashKind::SameKind);
}
