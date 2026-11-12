//! The axis and role decisions, and golden summaries: tiny ops through the real
//! writer, their expected `operation_tree_dirs` rows written out by hand. Literal on
//! purpose: a wrong axis or role would make any code-vs-code comparison agree with
//! itself. The maintenance cells (outcome changes, prune, failures, reconcile) are in
//! `maintenance_tests.rs`.

use super::test_support::{created_dir, item, journal};
use super::{TreeAxis, UnitRole, unit_role};
use crate::types::{EntryType, ItemOutcome, OpKind, RowRole, SkipReason};

#[test]
fn the_axis_is_where_the_undo_acts() {
    for kind in [
        OpKind::Copy,
        OpKind::Move,
        OpKind::CreateFile,
        OpKind::CreateFolder,
        OpKind::ArchiveEdit,
    ] {
        assert_eq!(TreeAxis::of(kind, false), TreeAxis::Dest, "{kind:?}");
        assert_eq!(
            TreeAxis::of(kind, true),
            TreeAxis::Source,
            "an inverse {kind:?} has no dests"
        );
    }
    for kind in [OpKind::Trash, OpKind::Delete, OpKind::Rename, OpKind::Tag] {
        assert_eq!(TreeAxis::of(kind, false), TreeAxis::Source, "{kind:?}");
    }
    assert_eq!(TreeAxis::Dest.side((1, "a"), Some((2, "b"))), (2, "b"));
    assert_eq!(
        TreeAxis::Dest.side((1, "a"), None),
        (1, "a"),
        "a row missing its axis side falls back to its other one"
    );
    assert_eq!(TreeAxis::Source.side((1, "a"), Some((2, "b"))), (1, "a"));
}

#[test]
fn a_rows_role_follows_its_own_fields() {
    use ItemOutcome::{Done, Failed, RolledBack, Skipped};
    let unit = RowRole::RollbackUnit;
    let role =
        |kind, row_role, entry, in_place, outcome, reason| unit_role(kind, row_role, entry, in_place, outcome, reason);

    assert_eq!(
        role(OpKind::Copy, unit, EntryType::File, false, Done, None),
        UnitRole::Selectable
    );
    assert_eq!(
        role(
            OpKind::Copy,
            unit,
            EntryType::File,
            false,
            Skipped,
            Some(SkipReason::Drift)
        ),
        UnitRole::Selectable,
        "held back by an earlier pass: still waiting"
    );
    assert_eq!(
        role(OpKind::Copy, unit, EntryType::Dir, true, Done, None),
        UnitRole::Housekeeping
    );
    assert_eq!(
        role(OpKind::Move, unit, EntryType::Dir, true, Done, None),
        UnitRole::Housekeeping
    );
    assert_eq!(
        role(OpKind::Move, unit, EntryType::Dir, false, Done, None),
        UnitRole::Selectable,
        "a moved folder is the unit itself"
    );
    assert_eq!(
        role(OpKind::CreateFolder, unit, EntryType::Dir, true, Done, None),
        UnitRole::Selectable,
        "the folder a create made is what its undo takes away"
    );
    assert_eq!(
        role(OpKind::Trash, RowRole::SearchOnly, EntryType::File, false, Done, None),
        UnitRole::Contents
    );
    assert_eq!(
        role(OpKind::Copy, unit, EntryType::File, false, RolledBack, None),
        UnitRole::Settled
    );
    assert_eq!(
        role(OpKind::Copy, unit, EntryType::File, false, Failed, None),
        UnitRole::Settled
    );
    assert_eq!(
        role(OpKind::Rename, unit, EntryType::File, false, Skipped, None),
        UnitRole::Settled,
        "the operation's own skip"
    );
    assert_eq!(
        role(OpKind::Delete, unit, EntryType::File, false, Done, None),
        UnitRole::NotReversible
    );
}

#[test]
fn golden_a_two_file_folder_copy() {
    let j = journal();
    j.open("op", OpKind::Copy);
    j.record(
        "op",
        vec![
            item(
                0,
                EntryType::File,
                RowRole::RollbackUnit,
                ("v", "/src/A", "x.txt"),
                Some(("v", "/dst/A", "x.txt")),
                Some(10),
            ),
            item(
                1,
                EntryType::File,
                RowRole::RollbackUnit,
                ("v", "/src/A", "y.txt"),
                Some(("v", "/dst/A", "y.txt")),
                Some(20),
            ),
            created_dir(2, ("v", "/dst", "A")),
        ],
    );
    assert_eq!(
        j.golden("op"),
        [
            "v:/ kids=1 direct=0 files=2 bytes=30 sel=2/2/30 hk=1",
            "v:/dst kids=1 direct=0 files=2 bytes=30 sel=2/2/30 hk=1",
            "v:/dst/A self=2 kids=0 direct=2 files=2 bytes=30 sel=2/2/30 hk=1",
        ]
    );
    j.assert_exact("op");
}

#[test]
fn golden_a_trash_of_one_folder_with_its_leaves() {
    let j = journal();
    j.open("op", OpKind::Trash);
    let leaf = |seq, entry, dir: &'static str, trash_dir: &'static str, name: &'static str, size| {
        item(
            seq,
            entry,
            RowRole::SearchOnly,
            ("v", dir, name),
            Some(("v", trash_dir, name)),
            size,
        )
    };
    j.record(
        "op",
        vec![
            // A folder unit's `size` is its inode's length, never bytes.
            item(
                0,
                EntryType::Dir,
                RowRole::RollbackUnit,
                ("v", "/home", "A"),
                Some(("v", "/.Trash", "A")),
                Some(96),
            ),
            leaf(1, EntryType::File, "/home/A", "/.Trash/A", "p.txt", Some(5)),
            leaf(2, EntryType::Dir, "/home/A", "/.Trash/A", "sub", Some(64)),
            leaf(3, EntryType::File, "/home/A/sub", "/.Trash/A/sub", "q.txt", Some(7)),
        ],
    );
    assert_eq!(
        j.golden("op"),
        [
            "v:/ kids=1 direct=0 files=2 bytes=12 sel=1/0/0",
            "v:/home kids=1 direct=0 files=2 bytes=12 sel=1/0/0",
            "v:/home/A self=0 kids=1 direct=1 files=2 bytes=12 sel=1/0/0",
            "v:/home/A/sub self=2 kids=0 direct=1 files=1 bytes=7 sel=0/0/0",
        ]
    );
    j.assert_exact("op");
}

#[test]
fn golden_a_capped_trash_is_one_folder_row() {
    let j = journal();
    j.open("op", OpKind::Trash);
    j.record(
        "op",
        vec![item(
            0,
            EntryType::Dir,
            RowRole::RollbackUnit,
            ("v", "/home", "Big"),
            Some(("v", "/.Trash", "Big")),
            Some(96),
        )],
    );
    assert_eq!(
        j.golden("op"),
        [
            "v:/ kids=1 direct=0 files=0 bytes=0 sel=1/0/0",
            "v:/home kids=1 direct=0 files=0 bytes=0 sel=1/0/0",
            "v:/home/Big self=0 kids=0 direct=0 files=0 bytes=0 sel=1/0/0",
        ]
    );
}

#[test]
fn golden_a_cross_volume_move_with_a_held_back_folder() {
    let j = journal();
    j.open("op", OpKind::Move);
    j.record(
        "op",
        vec![
            item(
                0,
                EntryType::File,
                RowRole::RollbackUnit,
                ("a", "/s/F", "x"),
                Some(("b", "/d/F", "x")),
                Some(3),
            ),
            created_dir(1, ("b", "/d", "F")),
        ],
    );
    j.outcomes(
        "op",
        &[
            (0, ItemOutcome::RolledBack, None),
            (1, ItemOutcome::Skipped, Some(SkipReason::DirNotEmpty)),
        ],
    );
    // Only the dest volume is on the tree; the folder still waits, held back.
    assert_eq!(
        j.golden("op"),
        [
            "b:/ kids=1 direct=0 files=1 bytes=3 sel=0/0/0 rb=1 held=1 hk=1",
            "b:/d kids=1 direct=0 files=1 bytes=3 sel=0/0/0 rb=1 held=1 hk=1",
            "b:/d/F self=1 kids=0 direct=1 files=1 bytes=3 sel=0/0/0 rb=1 held=1 hk=1",
        ]
    );
    j.assert_exact("op");
}

/// The wrong side reverses an unchecked folder: the selection hands out the folder's
/// DEST-side dir, so a same-FS move's unit must own its landed folder.
#[test]
fn golden_a_same_fs_move_of_a_folder_owns_its_landed_folder() {
    let j = journal();
    j.open("op", OpKind::Move);
    j.record(
        "op",
        vec![
            item(
                0,
                EntryType::Dir,
                RowRole::RollbackUnit,
                ("v", "/s", "A"),
                Some(("v", "/d", "A")),
                Some(96),
            ),
            item(
                1,
                EntryType::File,
                RowRole::SearchOnly,
                ("v", "/s/A", "x"),
                Some(("v", "/d/A", "x")),
                Some(4),
            ),
        ],
    );
    assert_eq!(j.self_dir_path("op", 0), "v:/d/A");
    assert_eq!(
        j.golden("op"),
        [
            "v:/ kids=1 direct=0 files=1 bytes=4 sel=1/0/0",
            "v:/d kids=1 direct=0 files=1 bytes=4 sel=1/0/0",
            "v:/d/A self=0 kids=0 direct=1 files=1 bytes=4 sel=1/0/0",
        ]
    );
}

#[test]
fn a_dir_rows_own_folder_resolves_to_its_own_path_local_and_scheme_rooted() {
    let j = journal();
    j.open("op", OpKind::Trash);
    j.record(
        "op",
        vec![
            item(
                0,
                EntryType::Dir,
                RowRole::RollbackUnit,
                ("root", "/Users/test_user_1", "Photos"),
                None,
                None,
            ),
            item(
                1,
                EntryType::Dir,
                RowRole::RollbackUnit,
                ("smb-1", "smb://nas/share/x", "y"),
                None,
                None,
            ),
        ],
    );
    assert_eq!(j.self_dir_path("op", 0), "root:/Users/test_user_1/Photos");
    assert_eq!(j.self_dir_path("op", 1), "smb-1:smb://nas/share/x/y");
    assert_eq!(
        j.golden("op"),
        [
            "root:/ kids=1 direct=0 files=0 bytes=0 sel=1/0/0",
            "root:/Users kids=1 direct=0 files=0 bytes=0 sel=1/0/0",
            "root:/Users/test_user_1 kids=1 direct=0 files=0 bytes=0 sel=1/0/0",
            "root:/Users/test_user_1/Photos self=0 kids=0 direct=0 files=0 bytes=0 sel=1/0/0",
            "smb-1:/ kids=1 direct=0 files=0 bytes=0 sel=1/0/0",
            "smb-1:smb://nas kids=1 direct=0 files=0 bytes=0 sel=1/0/0",
            "smb-1:smb://nas/share kids=1 direct=0 files=0 bytes=0 sel=1/0/0",
            "smb-1:smb://nas/share/x kids=1 direct=0 files=0 bytes=0 sel=1/0/0",
            "smb-1:smb://nas/share/x/y self=1 kids=0 direct=0 files=0 bytes=0 sel=1/0/0",
        ],
        "two volumes make a forest: one root each"
    );
}
