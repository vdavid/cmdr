//! Partial rollback: a run takes only the rows its selection takes, every guard
//! still holds for those, a row it leaves out is never touched, and the op stays
//! `partially_rolled_back` until "Roll back the rest" takes what's left.
//!
//! The selection is THE resolver's (`cmdr-operation-log`'s `tree/selection.rs`, with
//! its own cells and brute force); these pin the engine around it, end to end
//! through the gate and the real executor.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::test_support::*;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};
use cmdr_operation_log::query::RollbackSelection;
use cmdr_operation_log::query::SelectionToggle::{Folder, Item};
use cmdr_operation_log::rollback::{
    ActRefusal, InverseAct, RollbackProgress, RollbackRefusal, RollbackRunner, SelectionRefusal,
    execute_rollback_with_selection, reconcile_rolling_back_on_open, rollback_operation_with_selection,
};
use cmdr_operation_log::types::{EntryType, Initiator, ItemOutcome, OpKind, RollbackState, RowRole, SkipReason};
use cmdr_operation_log::writer::JournalItem;

/// A folder copy onto `dst`: `/tree/a.txt` (seq 0), `/tree/b.txt` (1),
/// `/tree/sub/c.txt` (2), and the two folders it made, `/tree/sub` (3) and `/tree`
/// (4). Originals in place, so every copy may go.
async fn copied_tree() -> (Rig, Arc<InMemoryVolume>) {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    mkdir(&dst, "/tree").await;
    mkdir(&dst, "/tree/sub").await;
    put(&dst, "/tree/a.txt", b"a").await;
    put(&dst, "/tree/b.txt", b"bb").await;
    put(&dst, "/tree/sub/c.txt", b"ccc").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/tree/a.txt", "dst", "/tree/a.txt", 1),
            file_unit(1, "src", "/tree/b.txt", "dst", "/tree/b.txt", 2),
            file_unit(2, "src", "/tree/sub/c.txt", "dst", "/tree/sub/c.txt", 3),
            dir_unit(3, "dst", "/tree/sub"),
            dir_unit(4, "dst", "/tree"),
        ],
    );
    rig.put_originals("op").await;
    (rig, dst)
}

/// A trash of `/home/A` and `/home/B` on one volume, each landed in `/.Trash` with a
/// file inside.
async fn trashed_two_folders() -> (Rig, Arc<InMemoryVolume>) {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    mkdir(&v, "/home").await;
    mkdir(&v, "/.Trash").await;
    for name in ["A", "B"] {
        mkdir(&v, &format!("/.Trash/{name}")).await;
        put(&v, &format!("/.Trash/{name}/f.txt"), b"f").await;
    }
    rig.register("v", Arc::clone(&v));
    rig.seed(
        "op",
        OpKind::Trash,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![
            folder_unit(0, "v", "/home/A", "/.Trash/A"),
            folder_unit(1, "v", "/home/B", "/.Trash/B"),
        ],
    );
    (rig, v)
}

fn outcome_of(rig: &Rig, seq: i64) -> ItemOutcome {
    rig.read_items("op")
        .into_iter()
        .find(|row| row.seq == seq)
        .map(|row| row.outcome)
        .expect("the row is journaled")
}

#[tokio::test]
async fn a_subset_of_a_folder_copy_leaves_the_rest_and_the_folders_holding_it() {
    let (rig, dst) = copied_tree().await;

    let report = rig
        .rollback_selected("op", &everything_but(vec![Item { seq: 2, on: false }]))
        .await
        .expect("the gate takes it");

    assert_eq!(report.reversed, 2, "{report:?}");
    // The folders holding the unchecked file are left out with it, never attempted,
    // so no misleading "folder not empty" skip lands on them.
    assert_eq!(report.skipped, 0, "{report:?}");
    assert_eq!(report.left_out, 3, "the file and both folders above it: {report:?}");
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert!(!exists(&dst, "/tree/a.txt").await);
    assert!(!exists(&dst, "/tree/b.txt").await);
    assert!(exists(&dst, "/tree/sub/c.txt").await, "the unchecked copy stays");
    assert!(exists(&dst, "/tree/sub").await);
    assert!(exists(&dst, "/tree").await);
    for seq in [2, 3, 4] {
        assert_eq!(outcome_of(&rig, seq), ItemOutcome::Done, "seq {seq} was never touched");
    }
    // The inverse's header counts what this run did, never the whole op.
    let inverse = rig.read_op("op").inverse_op_id.expect("the inverse is linked");
    assert_eq!(rig.read_op(&inverse).item_count, 2);

    // "Roll back the rest" is the plain re-entry, and it finishes the job.
    let rest = rig.rollback_as("op", "inv-rest").await;
    assert_eq!(rest.final_state, RollbackState::RolledBack, "{rest:?}");
    assert_eq!(rest.left_out, 0);
    assert!(!exists(&dst, "/tree").await, "the whole copy is gone now");
}

#[tokio::test]
async fn a_created_folder_goes_when_each_file_in_it_was_checked_one_by_one() {
    // Nothing checked as a folder: every file picked on its own. The folders a copy
    // made around exactly those files go with them, an empty one it made among them
    // too, and the op reads fully rolled back.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    mkdir(&dst, "/tree").await;
    mkdir(&dst, "/tree/sub").await;
    mkdir(&dst, "/tree/empty").await;
    put(&dst, "/tree/a.txt", b"a").await;
    put(&dst, "/tree/sub/c.txt", b"ccc").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/tree/a.txt", "dst", "/tree/a.txt", 1),
            file_unit(1, "src", "/tree/sub/c.txt", "dst", "/tree/sub/c.txt", 3),
            dir_unit(2, "dst", "/tree/empty"),
            dir_unit(3, "dst", "/tree/sub"),
            dir_unit(4, "dst", "/tree"),
        ],
    );
    rig.put_originals("op").await;

    let both_files = only(vec![Item { seq: 0, on: true }, Item { seq: 1, on: true }]);
    let report = rig
        .rollback_selected("op", &both_files)
        .await
        .expect("the gate takes it");

    assert_eq!(
        (report.reversed, report.skipped, report.left_out),
        (5, 0, 0),
        "two files and the three folders: {report:?}"
    );
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert!(!exists(&dst, "/tree").await, "every created folder went with the files");
}

#[tokio::test]
async fn a_created_folder_goes_when_its_own_files_are_checked_and_its_parent_stays() {
    let (rig, dst) = copied_tree().await;

    let report = rig
        .rollback_selected("op", &only(vec![Item { seq: 2, on: true }]))
        .await
        .expect("the gate takes it");

    assert_eq!(
        (report.reversed, report.skipped),
        (2, 0),
        "the file and `/tree/sub`: {report:?}"
    );
    assert_eq!(report.left_out, 3, "the two other files and `/tree`: {report:?}");
    assert!(
        !exists(&dst, "/tree/sub").await,
        "everything in it was checked, so it goes"
    );
    assert!(exists(&dst, "/tree/a.txt").await);
    assert_eq!(
        outcome_of(&rig, 4),
        ItemOutcome::Done,
        "`/tree` still holds unchecked files"
    );
}

#[tokio::test]
async fn a_selected_item_still_goes_through_every_guard() {
    // Drift: the selected copy changed since, so it stays; the unselected one is
    // left alone either way.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/a.txt", b"a").await;
    put(&dst, "/b.txt", b"bb").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/a.txt", "dst", "/a.txt", 1),
            file_unit(1, "src", "/b.txt", "dst", "/b.txt", 2),
        ],
    );
    rig.put_originals("op").await;
    dst.delete(std::path::Path::new("/b.txt")).await.expect("edited since");
    put(&dst, "/b.txt", b"changed").await;

    let report = rig
        .rollback_selected("op", &only(vec![Item { seq: 1, on: true }]))
        .await
        .expect("the gate takes it");
    assert_eq!(
        (report.reversed, report.skipped, report.left_out),
        (0, 1, 1),
        "{report:?}"
    );
    assert_eq!(report.skips[0].reason, SkipReason::Drift);
    assert_eq!(
        read(&dst, "/b.txt").await,
        b"changed",
        "a changed copy is never removed"
    );
    assert!(exists(&dst, "/a.txt").await, "the unselected copy is untouched");
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);

    // Occupied target: the selected move's original spot holds a stranger now.
    let rig = Rig::new();
    let src = Arc::new(InMemoryVolume::new("Src"));
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/x.txt", b"x").await;
    put(&dst, "/y.txt", b"y").await;
    put(&src, "/x.txt", b"stranger").await;
    rig.register("src", Arc::clone(&src));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Move,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/x.txt", "dst", "/x.txt", 1),
            file_unit(1, "src", "/y.txt", "dst", "/y.txt", 1),
        ],
    );
    let report = rig
        .rollback_selected("op", &only(vec![Item { seq: 0, on: true }]))
        .await
        .expect("the gate takes it");
    assert_eq!(report.skips[0].reason, SkipReason::RestoreTargetOccupied, "{report:?}");
    assert_eq!(read(&src, "/x.txt").await, b"stranger", "never overwritten");
    assert!(
        exists(&dst, "/y.txt").await,
        "the unselected move stays where it landed"
    );
}

#[tokio::test]
async fn unchecking_a_trashed_folders_own_row_keeps_it_in_the_trash() {
    // Both encodings the dialog can send for that row: the folder key, and the
    // folder unit's own item seq (which the resolver turns into the folder's).
    for encoding in ["folder", "item"] {
        let (rig, v) = trashed_two_folders().await;
        let toggle = match encoding {
            "folder" => Folder {
                dir_id: rig.dir_id("v", "/home/A"),
                on: false,
            },
            _ => Item { seq: 0, on: false },
        };

        let report = rig
            .rollback_selected("op", &everything_but(vec![toggle]))
            .await
            .expect("the gate takes it");

        assert_eq!((report.reversed, report.left_out), (1, 1), "{encoding}: {report:?}");
        assert!(exists(&v, "/.Trash/A/f.txt").await, "{encoding}: A stays in the Trash");
        assert!(!exists(&v, "/home/A").await, "{encoding}");
        assert!(exists(&v, "/home/B/f.txt").await, "{encoding}: B came back");
        assert_eq!(report.final_state, RollbackState::PartiallyRolledBack, "{encoding}");
    }
}

#[tokio::test]
async fn unchecking_a_moved_folder_by_its_landed_row_keeps_it_where_it_landed() {
    // A move reads on the DEST axis, so the dialog hands out the landed folder's key.
    // A folder unit decided by its SOURCE-side folder would never match it, and the
    // unchecked folder would move back.
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    mkdir(&v, "/src").await;
    mkdir(&v, "/dst").await;
    for name in ["A", "B"] {
        mkdir(&v, &format!("/dst/{name}")).await;
        put(&v, &format!("/dst/{name}/f.txt"), b"f").await;
    }
    rig.register("v", Arc::clone(&v));
    rig.seed(
        "op",
        OpKind::Move,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![
            folder_unit(0, "v", "/src/A", "/dst/A"),
            folder_unit(1, "v", "/src/B", "/dst/B"),
        ],
    );

    let report = rig
        .rollback_selected(
            "op",
            &everything_but(vec![Folder {
                dir_id: rig.dir_id("v", "/dst/A"),
                on: false,
            }]),
        )
        .await
        .expect("the gate takes it");

    assert_eq!((report.reversed, report.left_out), (1, 1), "{report:?}");
    assert!(exists(&v, "/dst/A/f.txt").await, "A stays where the move put it");
    assert!(!exists(&v, "/src/A").await);
    assert!(exists(&v, "/src/B/f.txt").await, "B went back");
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
}

#[tokio::test]
async fn toggles_naming_nothing_selectable_count_for_nothing() {
    let (rig, dst) = copied_tree().await;
    rig.rollback_selected("op", &only(vec![Item { seq: 0, on: true }]))
        .await
        .expect("the gate takes it");
    assert_eq!(outcome_of(&rig, 0), ItemOutcome::RolledBack);

    // Settled since, a foreign seq, a created folder's seq (housekeeping, no
    // checkbox), a foreign folder: nothing selectable, so nothing selected.
    let stale = only(vec![
        Item { seq: 0, on: true },
        Item { seq: 999, on: true },
        Item { seq: 4, on: true },
        Folder {
            dir_id: 999_999,
            on: true,
        },
    ]);
    assert_eq!(
        rig.rollback_selected("op", &stale).await.expect_err("nothing selected"),
        RollbackRefusal::Selection(SelectionRefusal::NothingSelected)
    );
    assert_eq!(
        rig.read_op("op").rollback_state,
        RollbackState::PartiallyRolledBack,
        "a refused selection never sets the op rolling back"
    );

    // The same keys switched off take nothing away: that's everything, and it ends
    // the op.
    let stale_off = everything_but(vec![
        Item { seq: 0, on: false },
        Item { seq: 999, on: false },
        Item { seq: 4, on: false },
        Folder {
            dir_id: 999_999,
            on: false,
        },
    ]);
    let report = rig
        .rollback_selected("op", &stale_off)
        .await
        .expect("the gate takes it");
    assert_eq!(report.left_out, 0, "{report:?}");
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert!(!exists(&dst, "/tree").await);
}

#[tokio::test]
async fn contents_rows_and_foreign_folders_are_normalized_away() {
    let (rig, v) = trashed_two_folders().await;
    // A `search_only` leaf under A (its contents, no checkbox of its own).
    let (dir, name) = split("/home/A/f.txt");
    let (dest_dir, dest_name) = split("/.Trash/A/f.txt");
    rig.writer
        .record_items(
            "op",
            vec![JournalItem {
                seq: 2,
                entry_type: EntryType::File,
                row_role: RowRole::SearchOnly,
                source_volume_id: "v".into(),
                source_dir: dir,
                source_name: name,
                dest_volume_id: Some("v".into()),
                dest_dir: Some(dest_dir),
                dest_name: Some(dest_name),
                size: Some(1),
                mtime: Some(MT as i64),
                outcome: ItemOutcome::Done,
                overwrote: false,
                dest_identity: None,
                tags: None,
            }],
        )
        .expect("record");
    rig.writer.flush_blocking().expect("flush");
    // A folder only some other operation has.
    let elsewhere = Arc::new(InMemoryVolume::new("Elsewhere"));
    rig.register("w", Arc::clone(&elsewhere));
    rig.seed(
        "other",
        OpKind::Copy,
        "w",
        Some("w"),
        RollbackState::Rollbackable,
        vec![dir_unit(0, "w", "/foreign")],
    );

    let report = rig
        .rollback_selected(
            "op",
            &everything_but(vec![
                Item { seq: 2, on: false },
                Folder {
                    dir_id: rig.dir_id("w", "/foreign"),
                    on: false,
                },
            ]),
        )
        .await
        .expect("the gate takes it");

    assert_eq!((report.reversed, report.left_out), (2, 0), "{report:?}");
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert!(exists(&v, "/home/A/f.txt").await);
}

#[tokio::test]
async fn a_selection_already_put_back_by_hand_counts_as_reversed() {
    // Everything selected was already gone, and nothing else awaits: done.
    let (rig, dst) = copied_tree().await;
    dst.delete(std::path::Path::new("/tree/a.txt")).await.expect("by hand");
    let all_files = only(vec![Folder {
        dir_id: rig.dir_id("dst", "/tree"),
        on: true,
    }]);
    let report = rig
        .rollback_selected("op", &all_files)
        .await
        .expect("the gate takes it");
    assert_eq!(
        (report.reversed, report.skipped, report.left_out),
        (5, 0, 0),
        "{report:?}"
    );
    assert_eq!(report.final_state, RollbackState::RolledBack);

    // Only the gone one selected, the others left out: partial.
    let (rig, dst) = copied_tree().await;
    dst.delete(std::path::Path::new("/tree/a.txt")).await.expect("by hand");
    let report = rig
        .rollback_selected("op", &only(vec![Item { seq: 0, on: true }]))
        .await
        .expect("the gate takes it");
    assert_eq!((report.reversed, report.skipped), (1, 0), "{report:?}");
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
}

#[tokio::test]
async fn a_crash_after_a_selective_pass_never_reads_as_untouched() {
    let (rig, _dst) = copied_tree().await;
    rig.rollback_selected("op", &only(vec![Item { seq: 1, on: true }]))
        .await
        .expect("the gate takes it");
    // A second pass set the op rolling back, then the app died before it wrote anything.
    rig.writer
        .set_rollback_state("op", RollbackState::RollingBack, None)
        .expect("set");
    rig.writer.flush_blocking().expect("flush");

    reconcile_rolling_back_on_open(&rig.writer);

    assert_eq!(rig.read_op("op").rollback_state, RollbackState::PartiallyRolledBack);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selective_bar_counts_exactly_what_the_run_credits() {
    let (rig, _dst) = copied_tree().await;
    let reversal = rig.reversal("rollback-selected-progress");

    // Off: b (2 bytes). Taken: a (1) and c (3), and `/tree/sub`, which only c was in;
    // `/tree` still holds b, so it's left out with it.
    let report = rig
        .rollback_selected_driven_by("op", &everything_but(vec![Item { seq: 1, on: false }]), &reversal)
        .await
        .expect("the gate takes it");
    assert_eq!((report.reversed, report.left_out), (3, 2), "{report:?}");

    let frames = reversal.frames();
    let first = frames.first().expect("a reversal reports progress");
    // The created folders are leftovers the sweep removes, not items on the bar.
    assert_eq!((first.files_total, first.bytes_total), (2, 4));
    let last = frames.last().expect("frames");
    assert_eq!((last.files_done, last.bytes_done), (2, 4), "the bar ends on its total");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trash_of_folders_moves_the_bar_once_per_folder() {
    // A trashed folder is one unit, put back in the deferred sweep: the bar counts it
    // and the sweep credits it. Its recorded size is an inode length, never bytes.
    let (rig, _v) = trashed_two_folders().await;
    let reversal = rig.reversal("rollback-folder-progress");

    let report = rig.rollback_driven_by("op", "inv-1", &reversal).await;
    assert_eq!(report.reversed, 2, "{report:?}");

    let frames = reversal.frames();
    let first = frames.first().expect("a reversal reports progress");
    assert_eq!((first.files_total, first.bytes_total), (2, 0));
    let last = frames.last().expect("frames");
    assert_eq!((last.files_done, last.bytes_done), (2, 0));
}

#[tokio::test]
async fn a_folder_held_back_with_nothing_selectable_left_still_rolls_back() {
    // "0 of 0" with a folder waiting: the plain Roll back sends no selection, and the
    // sweep removes the folder once the stray file is gone.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    mkdir(&dst, "/tree").await;
    put(&dst, "/tree/a.txt", b"a").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/tree/a.txt", "dst", "/tree/a.txt", 1),
            dir_unit(1, "dst", "/tree"),
        ],
    );
    rig.put_originals("op").await;
    put(&dst, "/tree/new.txt", b"mine").await;
    let held = rig.rollback("op").await;
    assert_eq!(held.final_state, RollbackState::PartiallyRolledBack, "{held:?}");
    dst.delete(std::path::Path::new("/tree/new.txt"))
        .await
        .expect("by hand");

    // A deselected-everything selection has nothing to take.
    assert_eq!(
        rig.rollback_selected("op", &only(Vec::new()))
            .await
            .expect_err("0 of 0"),
        RollbackRefusal::Selection(SelectionRefusal::NothingSelected)
    );
    let rest = rig.rollback_as("op", "inv-rest").await;
    assert_eq!(rest.final_state, RollbackState::RolledBack, "{rest:?}");
    assert!(!exists(&dst, "/tree").await);
}

#[tokio::test]
async fn a_selection_needs_the_tree_summary_and_a_bounded_toggle_list() {
    let (rig, _dst) = copied_tree().await;
    let conn = cmdr_fs::sqlite_util::open(rig.writer.db_path()).expect("write conn");
    conn.execute("UPDATE operations SET tree_summary = NULL WHERE op_id = 'op'", [])
        .expect("unbuild");
    assert_eq!(
        rig.rollback_selected("op", &everything_but(vec![Item { seq: 0, on: false }]))
            .await
            .expect_err("unbuilt"),
        RollbackRefusal::Selection(SelectionRefusal::SummaryNotBuilt)
    );
    assert_eq!(rig.read_op("op").rollback_state, RollbackState::Rollbackable);

    // Past the cap: normalizing would drop the newest toggles, and a dropped `off`
    // would reverse a row the person unchecked.
    let rig = Rig::new();
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::new(InMemoryVolume::new("Dst")));
    let files = 2_001;
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        (0..files)
            .map(|i| file_unit(i, "src", &format!("/f{i}"), "dst", &format!("/f{i}"), 1))
            .collect(),
    );
    let toggles = (0..files).map(|seq| Item { seq, on: false }).collect();
    assert_eq!(
        rig.rollback_selected("op", &everything_but(toggles))
            .await
            .expect_err("capped"),
        RollbackRefusal::Selection(SelectionRefusal::TooManyToggles)
    );
}

#[tokio::test]
async fn a_page_the_selection_keeps_nothing_of_still_moves_the_stream_on() {
    // 600 files: the first page (512 rows, newest first) holds none of the selection.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    let mut units = Vec::new();
    for i in 0..600 {
        let path = format!("/f{i}.txt");
        put(&dst, &path, b"x").await;
        units.push(file_unit(i, "src", &path, "dst", &path, 1));
    }
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        units,
    );
    rig.put_originals("op").await;

    let report = rig
        .rollback_selected("op", &only(vec![Item { seq: 0, on: true }]))
        .await
        .expect("the gate takes it");

    assert_eq!((report.reversed, report.left_out), (1, 599), "{report:?}");
    assert!(!exists(&dst, "/f0.txt").await);
    assert!(exists(&dst, "/f1.txt").await);
}

#[tokio::test]
async fn a_stop_lands_while_the_selection_skips_whole_pages() {
    // The one selected file streams first; every page after it keeps nothing, so no
    // item check ever runs again. The page check is what notices the stop.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    let mut units = Vec::new();
    for i in 0..600 {
        let path = format!("/f{i}.txt");
        put(&dst, &path, b"x").await;
        units.push(file_unit(i, "src", &path, "dst", &path, 1));
    }
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        units,
    );
    rig.put_originals("op").await;
    let runner = ParkCounting {
        stop_after_an_act: true,
        ..ParkCounting::default()
    };
    let original = rig.read_op("op");

    let report = execute_rollback_with_selection(
        &rig.vm,
        &rig.writer,
        &original,
        "inv-1",
        Initiator::User,
        Some(&only(vec![Item { seq: 599, on: true }])),
        &runner,
    )
    .await;

    assert!(report.canceled, "{report:?}");
    assert_eq!(report.reversed, 1);
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
}

#[tokio::test]
async fn the_dispatch_names_where_the_selection_will_act() {
    // The newest unit sits in `/other`; a selection elsewhere names its own folder.
    let rig = Rig::new();
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::new(InMemoryVolume::new("Dst")));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/tree/a.txt", "dst", "/tree/a.txt", 1),
            file_unit(1, "src", "/other/z.txt", "dst", "/other/z.txt", 1),
        ],
    );
    // The spawn refuses, so the gate puts the op back and each dispatch here is a
    // fresh one; only the plan it built matters, which `spawn` sees.
    let named = |selection: RollbackSelection| {
        let mut from = None;
        let refused = rollback_operation_with_selection(&rig.vm, &rig.writer, "op", Some(&selection), |plan| {
            from = plan.summary.from.clone();
            Err(RollbackRefusal::UnknownOperation)
        });
        assert!(refused.is_err(), "the spawn refused on purpose");
        from
    };
    assert_eq!(named(only(vec![Item { seq: 0, on: true }])).as_deref(), Some("/tree"));
    assert_eq!(
        named(only(vec![Folder {
            dir_id: rig.dir_id("dst", "/tree"),
            on: true,
        }]))
        .as_deref(),
        Some("/tree")
    );
    assert_eq!(
        named(everything_but(vec![Item { seq: 0, on: false }])).as_deref(),
        Some("/other"),
        "no toggle on: the newest unit, as an unfiltered dispatch names it"
    );
}

/// A runner that restores and removes as asked, counts each pause check, and
/// records how many it had seen when each act ran.
#[derive(Default)]
struct ParkCounting {
    parks: AtomicUsize,
    parks_at_act: Mutex<Vec<usize>>,
    frames: Mutex<Vec<(u64, u64)>>,
    /// Answer "stop" once anything was acted on.
    stop_after_an_act: bool,
}

impl RollbackRunner for ParkCounting {
    fn perform<'a>(
        &'a self,
        act: InverseAct<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), ActRefusal>> + Send + 'a>> {
        Box::pin(async move {
            self.parks_at_act
                .lock()
                .expect("lock")
                .push(self.parks.load(Ordering::SeqCst));
            match act {
                InverseAct::Restore {
                    from,
                    from_path,
                    to_path,
                    force,
                    ..
                } => {
                    if let Some(parent) = to_path.parent() {
                        from.create_directory_all(parent).await?;
                    }
                    Ok(from.rename(from_path, to_path, force).await?)
                }
                InverseAct::RemoveDir { volume, path } | InverseAct::RemoveFile { volume, path } => {
                    Ok(volume.delete(path).await?)
                }
                InverseAct::WriteXattr { .. } => Err(VolumeError::NotSupported.into()),
            }
        })
    }

    fn should_stop(&self) -> bool {
        self.stop_after_an_act && !self.parks_at_act.lock().expect("lock").is_empty()
    }

    fn wait_while_paused(&self) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.parks.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::ready(()))
    }

    fn report_progress(&self, progress: RollbackProgress<'_>) {
        self.frames
            .lock()
            .expect("lock")
            .push((progress.files_done, progress.files_total));
    }
}

#[tokio::test]
async fn the_folder_sweep_parks_before_each_folder_and_skips_the_left_out_one() {
    // A trashed folder is reversed in the deferred sweep, so the sweep owes it what
    // the file loop owes a file: a pause check BEFORE it's verified, and the filter.
    let (rig, v) = trashed_two_folders().await;
    let original = rig.read_op("op");
    let runner = ParkCounting::default();
    let selection = everything_but(vec![Folder {
        dir_id: rig.dir_id("v", "/home/A"),
        on: false,
    }]);

    let report = execute_rollback_with_selection(
        &rig.vm,
        &rig.writer,
        &original,
        "inv-1",
        Initiator::User,
        Some(&selection),
        &runner,
    )
    .await;

    assert_eq!((report.reversed, report.left_out), (1, 1), "{report:?}");
    assert!(exists(&v, "/.Trash/A").await, "the left-out folder is never attempted");
    assert_eq!(
        *runner.parks_at_act.lock().expect("lock"),
        [1],
        "the one folder restored was parked for first"
    );
    assert_eq!(
        runner.frames.lock().expect("lock").last().copied(),
        Some((1, 1)),
        "and credited on the bar"
    );
}
