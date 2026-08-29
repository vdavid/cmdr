//! Rollback engine tests: the hard data-safety invariants, TDD'd.
//!
//! The pure decision helpers (`verify_snapshot`, `is_self_collision`,
//! `inverse_action`, …) are unit-tested in isolation. The per-kind reversal is
//! tested end-to-end against `InMemoryVolume`s: seed the journal with the rows an
//! operation would have recorded (with realistic size/mtime snapshots — the
//! capture layer's correctness is the capture layer's concern; here the engine is under test) and
//! the post-op filesystem state, run `execute_rollback`, and assert the invariant
//! **apply-then-rollback == original state**, plus the specific data-loss traps
//! D7 surfaced.
//!
//! The rig and its seeding helpers live in `test_support`, shared with `undo_tests`.

use std::path::Path;
use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::file_system::write_operations::rollback::Reversal;
use crate::operation_log::store::{open_read_connection, read_operation_items};
use crate::operation_log::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, OpKind, RollbackState, SearchCoverage,
};
use crate::operation_log::writer::{FinalizeOperation, OpenOperation};

// ── Pure decision helpers ────────────────────────────────────────────────────

#[test]
fn verify_snapshot_match_drift_unverifiable() {
    let live = entry("f", None, Some(10), Some(MT));
    // Both fields recorded and equal ⇒ Match.
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64), &live),
        SnapshotVerdict::Match
    );
    // A recorded field differs ⇒ Drift.
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64 + 1), &live),
        SnapshotVerdict::Drift
    );
    assert_eq!(
        verify_snapshot(Some(11), Some(MT as i64), &live),
        SnapshotVerdict::Drift
    );
    // Only size recorded (volume transfers carry no mtime), and it matches ⇒ Match.
    assert_eq!(verify_snapshot(Some(10), None, &live), SnapshotVerdict::Match);
    // A recorded field whose live counterpart is absent ⇒ Unverifiable (fail safe).
    let no_mtime = entry("f", None, Some(10), None);
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64), &no_mtime),
        SnapshotVerdict::Unverifiable
    );
    // Nothing recorded ⇒ nothing to prove identity on ⇒ Unverifiable.
    assert_eq!(verify_snapshot(None, None, &live), SnapshotVerdict::Unverifiable);
}

#[test]
fn self_collision_by_inode_and_by_case_fold() {
    // Real inodes (LocalPosix): same inode ⇒ self, whatever the names.
    let a = entry("dog.JPG", Some(7), Some(1), Some(MT));
    let b = entry("dog.jpg", Some(7), Some(1), Some(MT));
    assert!(is_self_collision(
        true,
        Path::new("/d/dog.JPG"),
        Path::new("/d/dog.jpg"),
        &a,
        &b
    ));
    // Different inodes ⇒ a real collision even with the same name.
    let c = entry("dog.jpg", Some(9), Some(1), Some(MT));
    assert!(!is_self_collision(
        true,
        Path::new("/d/dog.jpg"),
        Path::new("/d/dog.jpg"),
        &a,
        &c
    ));
    // No inodes (MTP/SMB), same volume, case-only difference ⇒ self by path fold.
    let x = entry("dog.JPG", None, Some(1), Some(MT));
    let y = entry("dog.jpg", None, Some(1), Some(MT));
    assert!(is_self_collision(
        true,
        Path::new("/d/dog.JPG"),
        Path::new("/d/dog.jpg"),
        &x,
        &y
    ));
    // No inodes, DIFFERENT volume, same relative path ⇒ NOT self (the occupant is a
    // genuinely different file — a move-back must never overwrite it).
    assert!(!is_self_collision(
        false,
        Path::new("/a.txt"),
        Path::new("/a.txt"),
        &x,
        &y
    ));
    // No inodes, same volume, genuinely different sibling ⇒ not self.
    assert!(!is_self_collision(
        true,
        Path::new("/d/cat.jpg"),
        Path::new("/d/dog.jpg"),
        &x,
        &y
    ));
}

#[test]
fn pure_mapping_helpers() {
    assert_eq!(inverse_kind(OpKind::Copy), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::CreateFolder), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::ArchiveEdit), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::Move), OpKind::Move);
    assert_eq!(inverse_kind(OpKind::Trash), OpKind::Move);
    assert_eq!(inverse_kind(OpKind::Rename), OpKind::Rename);

    assert_eq!(
        inverse_action(OpKind::Copy, EntryType::File),
        Some(InverseAction::RemoveFileIfUnchanged)
    );
    assert_eq!(
        inverse_action(OpKind::Copy, EntryType::Dir),
        Some(InverseAction::RemoveDirIfEmpty)
    );
    assert_eq!(
        inverse_action(OpKind::CreateFolder, EntryType::Dir),
        Some(InverseAction::RemoveDirIfEmpty)
    );
    assert_eq!(
        inverse_action(OpKind::Move, EntryType::File),
        Some(InverseAction::RestoreMove)
    );
    assert_eq!(inverse_action(OpKind::Delete, EntryType::File), None);

    assert_eq!(resolve_final_state(3, 0, false), RollbackState::RolledBack);
    assert_eq!(resolve_final_state(2, 1, false), RollbackState::PartiallyRolledBack);
    // Non-canceled, all skipped ⇒ partial (won't clear on retry).
    assert_eq!(resolve_final_state(0, 3, false), RollbackState::PartiallyRolledBack);
    // Canceled with nothing reversed ⇒ back to rollbackable (a clean retry).
    assert_eq!(resolve_final_state(0, 0, true), RollbackState::Rollbackable);
    // Canceled after reversing some ⇒ partial.
    assert_eq!(resolve_final_state(3, 0, true), RollbackState::PartiallyRolledBack);
}

// ── Per-kind reversal: apply-then-rollback == original ───────────────────────

#[tokio::test]
async fn copy_rollback_removes_copies_and_created_dirs() {
    let rig = Rig::new();
    let src = Arc::new(InMemoryVolume::new("Src"));
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    // Post-copy DST state: a whole /tree copied in (files + the dirs the copy made).
    mkdir(&dst, "/tree").await;
    mkdir(&dst, "/tree/sub").await;
    put(&dst, "/tree/a.txt", b"aaa").await;
    put(&dst, "/tree/sub/b.txt", b"bbbb").await;
    rig.register("src", src);
    rig.register("dst", dst.clone());

    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/tree/a.txt", "dst", "/tree/a.txt", 3),
            file_unit(1, "src", "/tree/sub/b.txt", "dst", "/tree/sub/b.txt", 4),
            dir_unit(2, "dst", "/tree"),
            dir_unit(3, "dst", "/tree/sub"),
        ],
    );

    let report = rig.rollback("op").await;
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert_eq!(report.reversed, 4);
    assert_eq!(report.skipped, 0);
    // DST is back to its original (empty) state — no copied files, NO empty dirs left.
    assert!(!exists(&dst, "/tree/a.txt").await);
    assert!(!exists(&dst, "/tree/sub/b.txt").await);
    assert!(!exists(&dst, "/tree/sub").await, "the deeper created dir is removed");
    assert!(
        !exists(&dst, "/tree").await,
        "the created dir is removed after its contents"
    );

    // The original op's items are all marked rolled_back, and the inverse op is
    // journaled linking back to it.
    let conn = open_read_connection(rig.writer.db_path()).expect("conn");
    let items = read_operation_items(&conn, "op", 100).expect("items");
    assert!(items.iter().all(|i| i.outcome == ItemOutcome::RolledBack));
    let inverse = read_inverse_op(&conn, "op").expect("inv").expect("present");
    assert_eq!(inverse.op_id, "inv-1");
    assert_eq!(inverse.kind, OpKind::Delete, "undoing a copy is a delete");
    assert_eq!(inverse.rolls_back_op_id.as_deref(), Some("op"));
}

#[tokio::test]
async fn move_rollback_restores_files_cross_volume() {
    let rig = Rig::new();
    let src = Arc::new(InMemoryVolume::new("Src"));
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    // Post-move: files live on DST, SRC is empty.
    put(&dst, "/one.txt", b"1").await;
    put(&dst, "/two.txt", b"22").await;
    rig.register("src", src.clone());
    rig.register("dst", dst.clone());

    rig.seed(
        "op",
        OpKind::Move,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/one.txt", "dst", "/one.txt", 1),
            file_unit(1, "src", "/two.txt", "dst", "/two.txt", 2),
        ],
    );

    let report = rig.rollback("op").await;
    assert_eq!(report.final_state, RollbackState::RolledBack);
    // Files are back on SRC with their bytes, and gone from DST.
    assert_eq!(read(&src, "/one.txt").await, b"1");
    assert_eq!(read(&src, "/two.txt").await, b"22");
    assert!(!exists(&dst, "/one.txt").await);
    assert!(!exists(&dst, "/two.txt").await);
}

#[tokio::test]
async fn same_volume_move_and_rename_restore_by_rename() {
    // A same-volume move (top-level) and a rename both reverse via a same-volume
    // rename back.
    for (kind, src, dst) in [
        (OpKind::Move, "/src/a.txt", "/dst/a.txt"),
        (OpKind::Rename, "/photo.jpg", "/image.jpg"),
    ] {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, dst, b"data").await; // the item now sits at its post-op location
        rig.register("v", v.clone());
        rig.seed(
            "op",
            kind,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", src, "v", dst, 4)],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::RolledBack, "{kind:?}");
        assert!(exists(&v, src).await, "{kind:?}: restored to its original name");
        assert!(!exists(&v, dst).await, "{kind:?}: gone from its post-op name");
    }
}

#[tokio::test]
async fn trash_rollback_restores_from_recorded_in_trash_location() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    // The trashed item sits at its recorded in-trash location.
    put(&v, "/.Trash/doc.txt", b"hello").await;
    rig.register("v", v.clone());
    // Trash rows carry the original path as source, the in-trash location as dest.
    rig.seed(
        "op",
        OpKind::Trash,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "v", "/doc.txt", "v", "/.Trash/doc.txt", 5)],
    );
    let report = rig.rollback("op").await;
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert_eq!(read(&v, "/doc.txt").await, b"hello", "restored to the original path");
    assert!(!exists(&v, "/.Trash/doc.txt").await, "removed from the trash");
}

#[tokio::test]
async fn trash_rollback_skips_when_trash_was_emptied() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    // The trash was emptied since: the in-trash location is gone.
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Trash,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "v", "/doc.txt", "v", "/.Trash/doc.txt", 5)],
    );
    let report = rig.rollback("op").await;
    // Gone ⇒ the desired end state (absent from trash) already holds, an idempotent
    // no-op, so nothing to restore and the item isn't a blocking skip.
    assert!(!exists(&v, "/doc.txt").await);
    assert_eq!(report.reversed, 1, "an already-gone item is an idempotent no-op");
}

#[tokio::test]
async fn create_folder_rollback_removes_if_empty_but_skips_if_a_file_was_added() {
    // Empty ⇒ removed.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        mkdir(&v, "/newdir").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::CreateFolder,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![dir_unit(0, "v", "/newdir")],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::RolledBack);
        assert!(!exists(&v, "/newdir").await);
    }
    // A file added since ⇒ the folder is NOT swept away; partial.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        mkdir(&v, "/newdir").await;
        put(&v, "/newdir/added.txt", b"mine").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::CreateFolder,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![dir_unit(0, "v", "/newdir")],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
        assert!(exists(&v, "/newdir").await, "the non-empty dir is kept");
        assert_eq!(read(&v, "/newdir/added.txt").await, b"mine", "the added file survives");
    }
}

#[tokio::test]
async fn create_file_rollback_removes_unchanged_but_skips_modified() {
    // Unchanged ⇒ removed.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, "/new.txt", b"orig").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::CreateFile,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", "/new.txt", "v", "/new.txt", 4)],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::RolledBack);
        assert!(!exists(&v, "/new.txt").await);
    }
    // Modified since (size changed) ⇒ drift ⇒ skipped, the file survives.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, "/new.txt", b"orig-and-then-edited").await; // size != recorded 4
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::CreateFile,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", "/new.txt", "v", "/new.txt", 4)],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
        assert!(exists(&v, "/new.txt").await, "a modified created file is never deleted");
    }
}

#[tokio::test]
async fn compress_rollback_deletes_net_new_archive_but_never_a_modified_one() {
    // Unchanged net-new archive ⇒ deleted.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, "/out.zip", b"ZIPBYTES").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::ArchiveEdit,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", "/out.zip", "v", "/out.zip", 8)],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::RolledBack);
        assert!(!exists(&v, "/out.zip").await);
    }
    // The user zip-edited the archive afterward (size changed) ⇒ the recheck sees
    // drift ⇒ the archive is untouched (deleting it would destroy their additions).
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, "/out.zip", b"ZIPBYTES-plus-a-new-entry").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            OpKind::ArchiveEdit,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", "/out.zip", "v", "/out.zip", 8)],
        );
        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
        assert_eq!(
            read(&v, "/out.zip").await,
            b"ZIPBYTES-plus-a-new-entry",
            "a modified archive is untouched"
        );
    }
}

// ── The data-loss traps ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_new_file_at_the_restore_path_is_never_overwritten() {
    // move / trash / rename: occupy the restore target with a NEW file and assert
    // the undo skips that item and leaves the new file byte-for-byte intact.
    for (kind, orig, landed) in [
        (OpKind::Move, "/a.txt", "/moved/a.txt"),
        (OpKind::Trash, "/a.txt", "/.Trash/a.txt"),
        (OpKind::Rename, "/a.txt", "/b.txt"),
    ] {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, landed, b"original").await;
        // The user has since created a NEW file at the original path.
        put(&v, orig, b"the-users-new-file").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            kind,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", orig, "v", landed, 8)],
        );

        let report = rig.rollback("op").await;
        assert_eq!(report.final_state, RollbackState::PartiallyRolledBack, "{kind:?}");
        assert_eq!(report.skipped, 1, "{kind:?}: the occupied-target item is skipped");
        assert_eq!(
            read(&v, orig).await,
            b"the-users-new-file",
            "{kind:?}: the new file is intact"
        );
        assert!(
            exists(&v, landed).await,
            "{kind:?}: the moved item stays put (not lost)"
        );
    }
}

#[tokio::test]
async fn drift_on_one_item_skips_only_that_item() {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/keep.txt", b"XXXX").await; // will be modified below (drift)
    put(&dst, "/gone.txt", b"YY").await; // unchanged ⇒ reversible
    // Modify keep.txt so it no longer matches its recorded size snapshot.
    dst.set_reported_size(Path::new("/keep.txt"), 9999);
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/keep.txt", "dst", "/keep.txt", 4),
            file_unit(1, "src", "/gone.txt", "dst", "/gone.txt", 2),
        ],
    );
    let report = rig.rollback("op").await;
    assert_eq!(report.reversed, 1);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert!(exists(&dst, "/keep.txt").await, "the drifted copy is NOT deleted");
    assert!(!exists(&dst, "/gone.txt").await, "the unchanged copy IS deleted");
}

/// A skipped item persists the REASON it was skipped, per item — so a partial undo can
/// say which file was left alone and why, instead of naming a reason class for the whole
/// batch. The reversed item alongside it records no reason (there's nothing to explain).
#[tokio::test]
async fn a_skipped_item_persists_the_reason_it_was_skipped() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    put(&v, "/invoice-2026.pdf", b"data").await;
    put(&v, "/receipt-2026.pdf", b"data").await;
    // The first file was edited after the rename (size no longer matches the snapshot);
    // the second file's old name has since been taken by something else.
    v.set_reported_size(Path::new("/invoice-2026.pdf"), 9999);
    put(&v, "/scan002.pdf", b"someone-elses-file").await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Rename,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "v", "/scan001.pdf", "v", "/invoice-2026.pdf", 4),
            file_unit(1, "v", "/scan002.pdf", "v", "/receipt-2026.pdf", 4),
        ],
    );

    let report = rig.rollback("op").await;

    assert_eq!(report.skipped, 2);
    let items = rig.read_items("op");
    assert_eq!(items[0].outcome, ItemOutcome::Skipped);
    assert_eq!(
        items[0].rollback_skip_reason,
        Some(SkipReason::Drift),
        "the edited file's row names drift, not a reason class"
    );
    assert_eq!(items[1].outcome, ItemOutcome::Skipped);
    assert_eq!(
        items[1].rollback_skip_reason,
        Some(SkipReason::RestoreTargetOccupied),
        "the taken-name file's row names the occupied target"
    );
}

/// The report a rollback hands back carries the per-reason breakdown, so the caller can
/// name a specific file rather than a reason class. Counts are complete (never a sample),
/// so they always add up to `skipped` — the honesty the whole surface exists for.
#[tokio::test]
async fn the_report_breaks_skips_down_by_reason_with_an_example_file() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    for name in ["/invoice-a.pdf", "/invoice-b.pdf", "/invoice-c.pdf"] {
        put(&v, name, b"data").await;
    }
    // a and c were edited after the rename; b's old name has since been taken.
    v.set_reported_size(Path::new("/invoice-a.pdf"), 9999);
    v.set_reported_size(Path::new("/invoice-c.pdf"), 9999);
    put(&v, "/scan002.pdf", b"someone-elses-file").await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Rename,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "v", "/scan001.pdf", "v", "/invoice-a.pdf", 4),
            file_unit(1, "v", "/scan002.pdf", "v", "/invoice-b.pdf", 4),
            file_unit(2, "v", "/scan003.pdf", "v", "/invoice-c.pdf", 4),
        ],
    );

    let report = rig.rollback("op").await;

    assert_eq!(report.skipped, 3);
    // Reversal runs seq DESC, so `invoice-c.pdf` is the first drift seen.
    assert_eq!(
        report.skips,
        vec![
            SkipBreakdown {
                reason: SkipReason::Drift,
                count: 2,
                example_name: "invoice-c.pdf".to_string(),
            },
            SkipBreakdown {
                reason: SkipReason::RestoreTargetOccupied,
                count: 1,
                example_name: "invoice-b.pdf".to_string(),
            },
        ]
    );
    assert_eq!(
        report.skips.iter().map(|g| g.count).sum::<u64>(),
        report.skipped,
        "the breakdown accounts for every skipped item"
    );
}

/// An item the rollback actually reversed records no skip reason: the column explains a
/// skip, so a reversed item must read as "nothing to report", never as a leftover reason.
#[tokio::test]
async fn a_reversed_item_records_no_skip_reason() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    put(&v, "/invoice-2026.pdf", b"data").await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Rename,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "v", "/scan001.pdf", "v", "/invoice-2026.pdf", 4)],
    );

    let report = rig.rollback("op").await;

    assert_eq!(report.final_state, RollbackState::RolledBack);
    let items = rig.read_items("op");
    assert_eq!(items[0].outcome, ItemOutcome::RolledBack);
    assert_eq!(items[0].rollback_skip_reason, None);
}

/// An item that was ALREADY back where it belongs counts as reversed (an idempotent
/// re-issue), so it records no reason either — `AlreadyGone` never reaches the column.
#[tokio::test]
async fn an_already_restored_item_counts_as_reversed_and_records_no_reason() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    // Nothing at the renamed-to path: a previous undo already put the name back.
    put(&v, "/scan001.pdf", b"data").await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Rename,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "v", "/scan001.pdf", "v", "/invoice-2026.pdf", 4)],
    );

    let report = rig.rollback("op").await;

    assert_eq!(report.reversed, 1);
    assert_eq!(report.skipped, 0);
    let items = rig.read_items("op");
    assert_eq!(items[0].outcome, ItemOutcome::RolledBack);
    assert_eq!(
        items[0].rollback_skip_reason, None,
        "an idempotent no-op is not a skip, so it explains nothing"
    );
}

#[tokio::test]
async fn unverifiable_precondition_skips_never_proceeds() {
    // A copy leaf whose mtime was recorded but whose live entry can't report it
    // (an InMemoryVolume standing in for MTP/SMB with modified: None): the recheck
    // is Unverifiable, so the item is skipped rather than deleted.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/x.bin", b"1234").await;
    dst.set_modified_at(Path::new("/x.bin"), None); // backend can't prove the mtime
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "src", "/x.bin", "dst", "/x.bin", 4)],
    );
    let report = rig.rollback("op").await;
    assert_eq!(report.reversed, 0);
    assert_eq!(report.skipped, 1);
    assert_eq!(
        report.final_state,
        RollbackState::PartiallyRolledBack,
        "an unverifiable skip lands partial (won't clear on retry)"
    );
    assert!(exists(&dst, "/x.bin").await, "an unverifiable item is never deleted");
}

/// A batch rename on a remote volume journals the mtime its fingerprint held. If
/// the backend can no longer report one (MTP, some SMB servers), the recheck must
/// stay Unverifiable and skip — never fall back to size and read as a match,
/// which is what let a same-size replacement be renamed back.
#[tokio::test]
async fn rename_undo_stays_unverifiable_when_the_backend_reports_no_mtime() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("Remote"));
    put(&v, "/invoice-2026.pdf", b"data").await;
    v.set_modified_at(Path::new("/invoice-2026.pdf"), None);
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Rename,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "v", "/scan001.pdf", "v", "/invoice-2026.pdf", 4)],
    );

    let report = rig.rollback("op").await;

    assert_eq!(report.reversed, 0);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert!(exists(&v, "/invoice-2026.pdf").await, "the unprovable row is untouched");
    assert!(!exists(&v, "/scan001.pdf").await, "nothing is restored on a guess");
}

#[tokio::test]
async fn cancel_stops_and_keeps_what_was_reversed() {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    for i in 0..4 {
        put(&dst, &format!("/f{i}.txt"), b"x").await;
    }
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        (0..4)
            .map(|i| file_unit(i, "src", &format!("/f{i}.txt"), "dst", &format!("/f{i}.txt"), 1))
            .collect(),
    );
    // Stop it before it starts, the way the queue window would.
    let reversal = Reversal::new("cancel-before-any-item");
    crate::file_system::write_operations::cancel_write_operation(reversal.op_id(), false);
    let report = rig.rollback_driven_by("op", "inv-1", &reversal).await;
    assert!(report.canceled);
    assert_eq!(report.reversed, 0, "canceled before any item ran");
    assert_eq!(
        report.final_state,
        RollbackState::Rollbackable,
        "nothing reversed ⇒ retryable"
    );
    // The copies are untouched — a canceled rollback keeps the pre-rollback state.
    assert!(exists(&dst, "/f0.txt").await);
}

/// The E2E throttle hook is actually wired into the item loop.
///
/// Pinned here rather than only in `test_mode`, because the helper existing proves
/// nothing: the value of the hook is that a Playwright spec gets a real window in
/// which to press Cancel on a running reversal, and that window exists only if the
/// loop consults it. Deleting the three lines at the call site would leave every
/// `test_mode` assertion green and every rollback E2E racing the engine.
///
/// A lower bound on elapsed time, never an upper one: a sleep can overshoot under
/// load but never returns early, so this can't flake. The throttle is process-wide,
/// so the window it's held open for is deliberately short (three items at 20 ms).
#[tokio::test]
async fn the_e2e_throttle_hook_paces_the_item_loop() {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    for i in 0..3 {
        put(&dst, &format!("/f{i}.txt"), b"x").await;
    }
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        (0..3)
            .map(|i| file_unit(i, "src", &format!("/f{i}.txt"), "dst", &format!("/f{i}.txt"), 1))
            .collect(),
    );

    crate::test_mode::set_rollback_throttle_override(Some(20));
    let started = std::time::Instant::now();
    let report = rig.rollback("op").await;
    let elapsed = started.elapsed();
    crate::test_mode::set_rollback_throttle_override(None);

    assert!(
        elapsed >= std::time::Duration::from_millis(40),
        "three items at a 20 ms throttle should take at least two throttles, took {elapsed:?}"
    );
    // Strictly additive: pacing the loop changes nothing about what it does.
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert_eq!(report.reversed, 3);
    assert!(!exists(&dst, "/f0.txt").await);
}

#[tokio::test]
async fn a_canceled_original_op_rolls_back_exactly_its_completed_items() {
    // A copy canceled mid-way journals only the files it actually completed (capture),
    // so rolling it back reverses exactly those — a canceled `execution_status`
    // never blocks rollback (D4). Here only one of the two intended files landed.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/done.txt", b"1").await; // the completed copy
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.writer
        .open_operation(OpenOperation {
            op_id: "op".into(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: Some("src".into()),
            dest_volume_id: Some("dst".into()),
            item_count: 2, // two were planned...
            started_at: 1,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");
    // ...but only one completed before the cancel, so only one row exists.
    rig.writer
        .record_items("op", vec![file_unit(0, "src", "/done.txt", "dst", "/done.txt", 1)])
        .expect("record");
    rig.writer
        .finalize_operation(FinalizeOperation {
            op_id: "op".into(),
            execution_status: ExecutionStatus::Canceled,
            rollback_state: RollbackState::Rollbackable,
            not_rollbackable_reason: None,
            archive_subkind: None,
            search_coverage: SearchCoverage::Full,
            search_coverage_reason: None,
            ended_at: 2,
            item_count: None,
            items_done: 1,
            bytes_total: 0,
            dev_summary: None,
        })
        .expect("finalize");
    rig.writer.flush_blocking().expect("flush");

    let report = rig.rollback("op").await;
    assert_eq!(report.reversed, 1, "reverses exactly the completed item");
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert!(!exists(&dst, "/done.txt").await, "the one completed copy is undone");
}

#[tokio::test]
async fn a_restore_recreates_the_folder_the_move_emptied() {
    // A move of a FOLDER empties the source tree and removes it, so putting one of
    // its files back means putting the folder back first. Without that the rename
    // fails with ENOENT, which the engine reads as the item being already gone — an
    // item counted as reversed while the file never moved. A reversal reporting
    // success having restored nothing is the one outcome the engine may never
    // produce.
    //
    // On a REAL filesystem deliberately: `InMemoryVolume::rename` creates the
    // target's missing parents, so an in-memory double can't tell this bug from a
    // fix.
    let rig = Rig::new();
    let work = tempfile::tempdir().expect("work");
    let landed = work.path().join("dst/album/song.txt");
    std::fs::create_dir_all(landed.parent().expect("a parent")).expect("mk dst");
    std::fs::write(&landed, b"SONG").expect("write");
    filetime::set_file_mtime(&landed, filetime::FileTime::from_unix_time(MT as i64, 0)).expect("pin mtime");
    let original = work.path().join("src/album/song.txt");
    rig.vm.register(
        "root",
        Arc::new(crate::file_system::volume::LocalPosixVolume::new("Test root", "/")) as Arc<dyn Volume>,
    );
    rig.seed(
        "op",
        OpKind::Move,
        "root",
        Some("root"),
        RollbackState::Rollbackable,
        vec![file_unit(
            0,
            "root",
            &original.to_string_lossy(),
            "root",
            &landed.to_string_lossy(),
            4,
        )],
    );

    let report = rig.rollback("op").await;

    assert_eq!(report.skipped, 0, "nothing to skip, got {report:?}");
    assert!(
        original.exists(),
        "the file is really back, not merely reported as reversed"
    );
    assert!(!landed.exists());
}

#[tokio::test]
async fn a_directory_a_move_created_is_removed_rather_than_restored_onto_itself() {
    // A cross-FS move creates destination directories exactly like a copy does, and
    // journals them as created-dir rows (source == dest == the created path).
    // Reading those as a move's usual restore renames the directory onto itself: a
    // no-op the engine счит counts as reversed, leaving the moved folder's empty
    // skeleton at the destination. A row the operation CREATED is removed.
    let rig = Rig::new();
    let vol = Arc::new(InMemoryVolume::new("Root"));
    mkdir(&vol, "/dst").await;
    mkdir(&vol, "/dst/album").await;
    rig.register("root", vol.clone());
    rig.seed(
        "op",
        OpKind::Move,
        "root",
        Some("root"),
        RollbackState::Rollbackable,
        vec![dir_unit(0, "root", "/dst/album")],
    );

    rig.rollback("op").await;

    assert!(
        !exists(&vol, "/dst/album").await,
        "the directory the move created is gone"
    );
    assert!(exists(&vol, "/dst").await, "and the folder it was created in stays");
}

#[tokio::test]
async fn the_inverse_operations_header_counts_what_the_reversal_walked() {
    // The inverse op's `item_count` was seeded from the ORIGINAL's `items_done`,
    // which counts files only, while its `items_done` counts every row the
    // reversal walked — directory rows included. A copy of two files into one
    // created folder therefore finished as "3 of 2 done", which the history dialog
    // renders verbatim.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    mkdir(&dst, "/album").await;
    put(&dst, "/album/one.txt", b"1").await;
    put(&dst, "/album/two.txt", b"22").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.writer
        .open_operation(OpenOperation {
            op_id: "op".into(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: Some("src".into()),
            dest_volume_id: Some("dst".into()),
            item_count: 1,
            started_at: 1,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");
    rig.writer
        .record_items(
            "op",
            vec![
                file_unit(0, "src", "/album/one.txt", "dst", "/album/one.txt", 1),
                file_unit(1, "src", "/album/two.txt", "dst", "/album/two.txt", 2),
                dir_unit(2, "dst", "/album"),
            ],
        )
        .expect("record");
    rig.writer
        .finalize_operation(FinalizeOperation {
            op_id: "op".into(),
            execution_status: ExecutionStatus::Done,
            rollback_state: RollbackState::Rollbackable,
            not_rollbackable_reason: None,
            archive_subkind: None,
            search_coverage: SearchCoverage::Full,
            search_coverage_reason: None,
            ended_at: 2,
            item_count: Some(2),
            // What the capture layer records: the status cache counts FILES, so a
            // copy that created a folder finishes with fewer `items_done` than it
            // has rows.
            items_done: 2,
            bytes_total: 0,
            dev_summary: None,
        })
        .expect("finalize");
    rig.writer.flush_blocking().expect("flush");

    let report = rig.rollback_as("op", "inv").await;
    assert_eq!(report.reversed, 3);

    let inverse = rig.read_op("inv");
    assert_eq!(
        inverse.item_count, 3,
        "the header counts the rows the reversal walked, dirs included"
    );
    assert!(
        inverse.items_done <= inverse.item_count,
        "an operation can never finish more items than it had: {} of {}",
        inverse.items_done,
        inverse.item_count
    );
}

#[tokio::test]
async fn streams_a_large_op_across_pages() {
    // More units than one page (ROLLBACK_PAGE = 512) proves the paged cursor
    // advances across pages without materializing the whole list.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    const N: i64 = 1_200;
    let mut units = Vec::new();
    for i in 0..N {
        let p = format!("/f{i}.bin");
        put(&dst, &p, b"x").await;
        units.push(file_unit(i, "src", &p, "dst", &p, 1));
    }
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        units,
    );
    let report = rig.rollback("op").await;
    assert_eq!(report.reversed, N as u64, "every page's items reversed");
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert!(!exists(&dst, "/f0.bin").await);
    assert!(!exists(&dst, &format!("/f{}.bin", N - 1)).await);
}
