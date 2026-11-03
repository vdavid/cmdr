//! Kani proofs of rollback's pure decisions: every input, no size bound beyond
//! the types themselves (every `u64` and `i64`, every variant). Run them with
//! `pnpm check kani`.
//!
//! **Assumption**: a snapshot field holds the live value it recorded cast to
//! `i64` (`as i64`, or `i64::try_from` that records nothing when it can't).
//! That's how every journal writer records size and mtime.

use super::*;
use crate::types::{EntryType, OpKind, RollbackState};

/// A recorded field, as the journal holds it: absent, or a live value cast.
fn any_recorded() -> (Option<u64>, Option<i64>) {
    let original: Option<u64> = kani::any();
    (original, original.map(|value| value as i64))
}

/// Whether a recorded field holds for this live value: nothing recorded holds
/// trivially, a recorded one needs a present, equal live value.
fn field_holds(recorded: Option<u64>, live: Option<u64>) -> bool {
    match recorded {
        None => true,
        Some(value) => live == Some(value),
    }
}

/// Rollback acts on an item only when its snapshot says Match, so Match must
/// mean nothing changed: at least one field was recorded, and every recorded
/// field is present live and equal to what was recorded. Every such item
/// matches, so an unchanged item is never left behind either.
#[kani::proof]
fn a_snapshot_matches_exactly_when_nothing_changed() {
    let (size, snap_size) = any_recorded();
    let (mtime, snap_mtime) = any_recorded();
    let live_size: Option<u64> = kani::any();
    let live_mtime: Option<u64> = kani::any();

    let verdict = verify_snapshot(snap_size, snap_mtime, live_size, live_mtime);
    let unchanged =
        (size.is_some() || mtime.is_some()) && field_holds(size, live_size) && field_holds(mtime, live_mtime);
    assert_eq!(
        verdict == SnapshotVerdict::Match,
        unchanged,
        "a Match that doesn't mean unchanged"
    );
    if verdict == SnapshotVerdict::Drift {
        let differs =
            |recorded: Option<u64>, live: Option<u64>| matches!((recorded, live), (Some(r), Some(l)) if r != l);
        assert!(
            differs(size, live_size) || differs(mtime, live_mtime),
            "a Drift with no recorded field that differs"
        );
    }
}

/// The state a rollback run leaves the op in. Only a run that tried every item
/// and skipped none (and, resuming a partial op, brought something back) may
/// call it rolled back; a run that changed nothing leaves the op as it found
/// it; and once anything came back, the op never reads as untouched again.
#[kani::proof]
fn a_run_never_claims_more_than_it_reversed() {
    let reversed: u64 = kani::any();
    let skipped: u64 = kani::any();
    let stopped: bool = kani::any();
    let before: RollbackState = kani::any();
    let resuming = before == RollbackState::PartiallyRolledBack;

    let after = resolve_final_state(reversed, skipped, stopped, before);
    assert!(
        matches!(
            after,
            RollbackState::Rollbackable | RollbackState::RolledBack | RollbackState::PartiallyRolledBack
        ),
        "a run settled in a state no run settles in"
    );
    if after == RollbackState::RolledBack {
        assert!(!stopped && skipped == 0, "rolled back with items left behind");
        assert!(!(resuming && reversed == 0), "an empty pass finished a partial op");
    }
    if stopped && reversed == 0 {
        assert_eq!(
            after,
            untouched_state(before),
            "a run that changed nothing changed the state"
        );
    }
    if reversed > 0 || resuming {
        assert_ne!(
            after,
            RollbackState::Rollbackable,
            "a partly reversed op reads as untouched"
        );
    }
}

/// The per-item inverse: every kind but Delete has one, a folder the operation
/// created is only ever removed (empty), a copied file is only removed while its
/// original stays, and a moved item's action agrees with the inverse op's kind.
#[kani::proof]
fn every_item_gets_the_inverse_its_kind_promises() {
    let kind: OpKind = kani::any();
    let entry_type: EntryType = kani::any();
    let created_in_place: bool = kani::any();

    let action = item_action(kind, entry_type, created_in_place);
    let created_dir = entry_type == EntryType::Dir && created_in_place;
    if created_dir {
        assert_eq!(
            action,
            Some(InverseAction::RemoveDirIfEmpty),
            "a created folder isn't removed"
        );
        return;
    }
    assert_eq!(action.is_none(), kind == OpKind::Delete, "a kind without its inverse");
    if kind == OpKind::Copy && entry_type == EntryType::File {
        assert_eq!(
            action,
            Some(InverseAction::RemoveCopyIfOriginalStays),
            "a copy removed without checking its original"
        );
    }
    if let Some(action) = action {
        assert_eq!(
            action == InverseAction::RestoreMove,
            inverse_kind(kind) != OpKind::Delete,
            "the action and the inverse op's kind disagree"
        );
    }
}
