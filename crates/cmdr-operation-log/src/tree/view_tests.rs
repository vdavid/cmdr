//! The tree's flatten on tiny fixtures written through the real writer: the top,
//! compaction, windows, folder units and their contents, read-only ops, forests,
//! inverses, the cursor, and the summary states.

use super::selection::{SelectionBase, SelectionToggle};
use super::status::UnbuiltReason;
use super::test_support::{Journal, created_dir, item, journal};
use super::view::{TreeRowKey, TreeSummaryView, UnexpandableReason};
use super::view_test_support::{dir, finish, key, render, request, tree};
use crate::types::{EntryType, ItemOutcome, OpKind, RollbackState, RowRole, SearchCoverage, SearchCoverageReason};
use crate::writer::JournalItem;

fn file(
    seq: i64,
    src: (&'static str, &'static str, &'static str),
    dest: (&'static str, &'static str, &'static str),
) -> JournalItem {
    item(seq, EntryType::File, RowRole::RollbackUnit, src, Some(dest), Some(10))
}

/// A copy of `/src/proj` (`a.txt`, `sub/b.txt`, `sub/c.txt`) into `/dst`.
fn folder_copy() -> Journal {
    let j = journal();
    j.open("op", OpKind::Copy);
    j.record(
        "op",
        vec![
            file(0, ("v", "/src/proj", "a.txt"), ("v", "/dst/proj", "a.txt")),
            file(1, ("v", "/src/proj/sub", "b.txt"), ("v", "/dst/proj/sub", "b.txt")),
            file(2, ("v", "/src/proj/sub", "c.txt"), ("v", "/dst/proj/sub", "c.txt")),
            created_dir(3, ("v", "/dst/proj", "sub")),
            created_dir(4, ("v", "/dst", "proj")),
        ],
    );
    finish(&j, "op", RollbackState::Rollbackable, SearchCoverage::Full, None);
    j
}

/// A trash of `/home/pics` (holding `x.jpg` and `sub/y.jpg`) and `/home/docs`.
fn two_folder_trash() -> Journal {
    let j = journal();
    j.open("op", OpKind::Trash);
    let unit = |seq, name: &'static str| {
        item(
            seq,
            EntryType::Dir,
            RowRole::RollbackUnit,
            ("v", "/home", name),
            Some(("v", "/.Trash", name)),
            Some(96),
        )
    };
    let leaf = |seq, entry, dir: &'static str, trash: &'static str, name: &'static str| {
        item(
            seq,
            entry,
            RowRole::SearchOnly,
            ("v", dir, name),
            Some(("v", trash, name)),
            Some(5),
        )
    };
    j.record(
        "op",
        vec![
            unit(0, "pics"),
            leaf(1, EntryType::File, "/home/pics", "/.Trash/pics", "x.jpg"),
            leaf(2, EntryType::Dir, "/home/pics", "/.Trash/pics", "sub"),
            leaf(3, EntryType::File, "/home/pics/sub", "/.Trash/pics/sub", "y.jpg"),
            unit(4, "docs"),
        ],
    );
    finish(&j, "op", RollbackState::Rollbackable, SearchCoverage::Full, None);
    j
}

#[test]
fn a_folder_copy_opens_on_the_folder_it_copied() {
    let j = folder_copy();
    let shut = tree(&j, "op", &request());
    assert_eq!(shut.summary, TreeSummaryView::Built);
    assert_eq!(shut.root_path.as_deref(), Some("/dst"));
    assert_eq!(shut.other_root.as_deref(), Some("/src"));
    assert_eq!(render(&shut.rows), ["proj [+] (x) Housekeeping"]);
    assert_eq!(shut.total_rows, 1);
    assert_eq!(
        (shut.tally.selected, shut.tally.selectable, shut.tally.files),
        (3, 3, 3)
    );

    let proj = key(&shut.rows, "proj");
    let one = tree(&j, "op", &request().open(&[proj]));
    let sub = key(&one.rows, "sub");
    let open = tree(&j, "op", &request().open(&[proj, sub]));
    assert_eq!(
        render(&open.rows),
        [
            "proj [-] (x) Housekeeping",
            "  sub [-] (x) Housekeeping",
            "    b.txt (x) Selectable",
            "    c.txt (x) Selectable",
            "  a.txt (x) Selectable",
        ]
    );
    assert_eq!(open.total_rows, 5);
    let a = &open.rows[4];
    assert_eq!((a.pos_in_set, a.set_size, a.level, a.index), (2, 2, 2, 4));
    assert_eq!(a.full_path, "/dst/proj/a.txt");
    assert_eq!(a.other_end, None, "the source sits where the other root says");
}

#[test]
fn a_window_starting_inside_an_open_folder_carries_its_ancestors() {
    let j = folder_copy();
    let proj = key(&tree(&j, "op", &request()).rows, "proj");
    let sub = key(&tree(&j, "op", &request().open(&[proj])).rows, "sub");
    let window = tree(&j, "op", &request().open(&[proj, sub]).window(3, 2));
    assert_eq!(
        render(&window.rows),
        ["    c.txt (x) Selectable", "  a.txt (x) Selectable"]
    );
    assert_eq!(window.offset, 3);
    assert_eq!(
        window
            .ancestors
            .iter()
            .map(|row| (row.index, row.label.as_str()))
            .collect::<Vec<_>>(),
        [(0, "proj"), (1, "sub")]
    );
    assert_eq!(window.rows[0].index, 3);
}

#[test]
fn a_shut_folder_above_hides_an_open_one_below() {
    let j = folder_copy();
    let proj = key(&tree(&j, "op", &request()).rows, "proj");
    let sub = key(&tree(&j, "op", &request().open(&[proj])).rows, "sub");
    let only_sub = tree(&j, "op", &request().open(&[sub]));
    assert_eq!(only_sub.total_rows, 1, "sub stays open by hand but out of sight");
}

#[test]
fn a_folder_of_files_windows_from_the_middle_in_name_order() {
    let j = journal();
    j.open("op", OpKind::Rename);
    let names: Vec<String> = (0..50).map(|i| format!("f{i:02}.txt")).collect();
    // Recorded out of name order, so only the index can put them in order.
    let rows = names
        .iter()
        .enumerate()
        .rev()
        .map(|(i, name)| {
            let name: &'static str = Box::leak(name.clone().into_boxed_str());
            let renamed: &'static str = Box::leak(format!("r{name}").into_boxed_str());
            item(
                i as i64,
                EntryType::File,
                RowRole::RollbackUnit,
                ("v", "/w", name),
                Some(("v", "/w", renamed)),
                Some(1),
            )
        })
        .collect();
    j.record("op", rows);
    finish(&j, "op", RollbackState::Rollbackable, SearchCoverage::Full, None);

    for (offset, limit) in [(0, 5), (20, 5), (30, 5), (45, 10), (24, 3)] {
        let view = tree(&j, "op", &request().window(offset, limit));
        let labels: Vec<&str> = view.rows.iter().map(|row| row.label.as_str()).collect();
        let end = (offset + limit).min(50) as usize;
        assert_eq!(labels, names[offset as usize..end], "window at {offset}");
        assert_eq!(view.rows[0].pos_in_set, offset + 1);
    }
    let first = &tree(&j, "op", &request()).rows[0];
    assert_eq!(
        first.other_end.as_deref(),
        Some("/w/rf00.txt"),
        "a rename shows where it went"
    );
}

#[test]
fn a_folder_with_no_files_and_one_child_shares_its_row() {
    let j = journal();
    j.open("op", OpKind::Rename);
    j.record(
        "op",
        vec![
            item(
                0,
                EntryType::File,
                RowRole::RollbackUnit,
                ("v", "/a/b/c", "f"),
                Some(("v", "/a/b/c", "g")),
                None,
            ),
            item(
                1,
                EntryType::File,
                RowRole::RollbackUnit,
                ("v", "/a", "x"),
                Some(("v", "/a", "y")),
                None,
            ),
        ],
    );
    finish(&j, "op", RollbackState::Rollbackable, SearchCoverage::Full, None);
    let view = tree(&j, "op", &request());
    assert_eq!(view.root_path.as_deref(), Some("/a"));
    let bc = key(&view.rows, "b/c");
    let open = tree(&j, "op", &request().open(&[bc]));
    assert_eq!(
        render(&open.rows),
        [
            "b/c [-] (x)",
            "  f (x) Selectable <- /a/b/c/g",
            "x (x) Selectable <- /a/y"
        ]
    );
    assert_eq!(open.rows[1].full_path, "/a/b/c/f");
}

#[test]
fn a_trash_of_one_folder_is_that_folder_as_the_one_top_row() {
    let j = journal();
    j.open("op", OpKind::Trash);
    j.record(
        "op",
        vec![item(
            0,
            EntryType::Dir,
            RowRole::RollbackUnit,
            ("v", "/home", "A"),
            Some(("v", "/.Trash", "A")),
            Some(96),
        )],
    );
    finish(
        &j,
        "op",
        RollbackState::Rollbackable,
        SearchCoverage::TopLevelOnly,
        Some(SearchCoverageReason::Capped),
    );
    let view = tree(&j, "op", &request());
    assert_eq!(render(&view.rows), ["A (x) Selectable !Capped"]);
    assert_eq!(view.root_path.as_deref(), Some("/home"));
    assert_eq!(view.rows[0].unexpandable, Some(UnexpandableReason::Capped));
}

#[test]
fn an_unexpandable_folder_says_why_from_the_op_coverage() {
    let j = journal();
    j.open("op", OpKind::Trash);
    j.record(
        "op",
        vec![item(
            0,
            EntryType::Dir,
            RowRole::RollbackUnit,
            ("v", "/home", "A"),
            Some(("v", "/.Trash", "A")),
            Some(96),
        )],
    );
    finish(
        &j,
        "op",
        RollbackState::Rollbackable,
        SearchCoverage::TopLevelOnly,
        Some(SearchCoverageReason::IndexAbsent),
    );
    assert_eq!(
        tree(&j, "op", &request()).rows[0].unexpandable,
        Some(UnexpandableReason::NotRecorded)
    );

    let k = journal();
    k.open("op", OpKind::Trash);
    k.record(
        "op",
        vec![item(
            0,
            EntryType::Dir,
            RowRole::RollbackUnit,
            ("v", "/home", "A"),
            Some(("v", "/.Trash", "A")),
            Some(96),
        )],
    );
    finish(&k, "op", RollbackState::Rollbackable, SearchCoverage::Full, None);
    assert_eq!(
        tree(&k, "op", &request()).rows[0].unexpandable,
        Some(UnexpandableReason::Empty),
        "a fully recorded folder with nothing inside was empty"
    );
}

#[test]
fn a_folder_units_contents_mirror_it() {
    let j = two_folder_trash();
    let top = tree(&j, "op", &request());
    assert_eq!(
        render(&top.rows),
        ["docs (x) Selectable !Empty", "pics [+] (x) Selectable"]
    );
    assert_eq!(top.rows[1].other_end, None, "never for a trash");
    let pics = key(&top.rows, "pics");
    let sub = key(&tree(&j, "op", &request().open(&[pics])).rows, "sub");
    let pics_dir = dir(&top.rows, "pics");
    let off = request().open(&[pics, sub]).select(
        SelectionBase::Everything,
        &[SelectionToggle::Folder {
            dir_id: pics_dir,
            on: false,
        }],
    );
    let view = tree(&j, "op", &off);
    assert_eq!(
        render(&view.rows),
        [
            "docs (x) Selectable !Empty",
            "pics [-] ( ) Selectable",
            "  sub [-] ( ) Contents",
            "    y.jpg ( ) Contents",
            "  x.jpg ( ) Contents",
        ]
    );
    assert_eq!(
        (view.tally.selected, view.tally.selectable),
        (1, 2),
        "unchecking the folder takes exactly its unit"
    );
}

#[test]
fn contents_show_their_units_outcome_never_their_own() {
    let j = two_folder_trash();
    j.outcomes(
        "op",
        &[(0, ItemOutcome::Skipped, Some(crate::types::SkipReason::Drift))],
    );
    let pics = key(&tree(&j, "op", &request()).rows, "pics");
    let view = tree(&j, "op", &request().open(&[pics]));
    let x = view.rows.iter().find(|row| row.label == "x.jpg").expect("x.jpg");
    assert_eq!(x.outcome, Some(ItemOutcome::Skipped));
    assert_eq!(x.skip_reason, Some(crate::types::SkipReason::Drift));
}

#[test]
fn a_read_only_op_has_no_checkboxes() {
    for state in [
        RollbackState::NotRollbackable,
        RollbackState::RollingBack,
        RollbackState::RolledBack,
    ] {
        let j = journal();
        j.open("op", OpKind::Copy);
        j.record("op", vec![file(0, ("v", "/s", "a"), ("v", "/d", "a"))]);
        finish(&j, "op", state, SearchCoverage::Full, None);
        let view = tree(&j, "op", &request());
        assert!(view.read_only, "{state:?}");
        assert_eq!(render(&view.rows), ["a Selectable"], "{state:?}");
    }
}

#[test]
fn rows_under_several_volume_roots_make_a_forest() {
    let j = journal();
    j.open("op", OpKind::Copy);
    j.record(
        "op",
        vec![
            file(0, ("v", "/src", "a.txt"), ("w", "/dst", "a.txt")),
            // The copy's own failure, recorded with no dest: it falls back to its source.
            JournalItem {
                outcome: ItemOutcome::Failed,
                ..item(
                    1,
                    EntryType::File,
                    RowRole::RollbackUnit,
                    ("v", "/src", "b.txt"),
                    None,
                    Some(3),
                )
            },
        ],
    );
    finish(&j, "op", RollbackState::Rollbackable, SearchCoverage::Full, None);
    let view = tree(&j, "op", &request());
    assert_eq!(view.root_path, None);
    let labels: Vec<&str> = view.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels.len(), 2, "{:?}", render(&view.rows));
    assert!(labels.contains(&"/src") && labels.contains(&"/dst"), "{labels:?}");
    let src = key(&view.rows, "/src");
    let open = tree(&j, "op", &request().open(&[src]));
    let b = open
        .rows
        .iter()
        .find(|row| row.label == "b.txt")
        .expect("the row missing its dest still shows");
    assert_eq!(b.full_path, "/src/b.txt");
    assert_eq!(b.level, 2);
}

#[test]
fn an_inverse_op_reads_on_its_source_side() {
    let j = journal();
    j.open("orig", OpKind::Move);
    j.record("orig", vec![file(0, ("v", "/a", "f"), ("v", "/b", "f"))]);
    finish(&j, "orig", RollbackState::RolledBack, SearchCoverage::Full, None);
    j.open_with("inv", OpKind::Move, Some("orig"));
    j.record(
        "inv",
        vec![item(
            0,
            EntryType::File,
            RowRole::RollbackUnit,
            ("v", "/b", "f"),
            None,
            Some(10),
        )],
    );
    finish(&j, "inv", RollbackState::Rollbackable, SearchCoverage::Full, None);
    let view = tree(&j, "inv", &request());
    assert_eq!(view.root_path.as_deref(), Some("/b"));
    assert_eq!(view.rows.len(), 1);
}

#[test]
fn the_cursor_keeps_its_row_and_falls_back_to_its_folder() {
    let j = folder_copy();
    let proj = key(&tree(&j, "op", &request()).rows, "proj");
    let sub = key(&tree(&j, "op", &request().open(&[proj])).rows, "sub");
    let c = TreeRowKey::Item { seq: 2 };
    assert_eq!(
        tree(&j, "op", &request().open(&[proj, sub]).cursor(c)).cursor_index,
        Some(3)
    );
    assert_eq!(
        tree(&j, "op", &request().open(&[proj]).cursor(c)).cursor_index,
        Some(1),
        "sub collapsed"
    );
    assert_eq!(
        tree(&j, "op", &request().cursor(c)).cursor_index,
        Some(0),
        "proj collapsed"
    );
    let a = TreeRowKey::Item { seq: 0 };
    assert_eq!(
        tree(&j, "op", &request().open(&[proj, sub]).cursor(a)).cursor_index,
        Some(4)
    );
    assert_eq!(tree(&j, "op", &request().open(&[proj]).cursor(a)).cursor_index, Some(2));
    assert_eq!(
        tree(&j, "op", &request().open(&[proj]).cursor(sub)).cursor_index,
        Some(1)
    );
    assert_eq!(
        tree(&j, "op", &request().cursor(TreeRowKey::Item { seq: 99 })).cursor_index,
        None
    );
}

#[test]
fn an_unbuilt_summary_answers_why_and_no_rows() {
    let j = folder_copy();
    raw(&j)
        .execute("UPDATE operations SET tree_summary = NULL WHERE op_id = 'op'", [])
        .expect("unbuild");
    let view = tree(&j, "op", &request());
    assert_eq!(
        view.summary,
        TreeSummaryView::Unbuilt {
            reason: UnbuiltReason::NotBuilt
        }
    );
    assert!(view.rows.is_empty());

    let running = journal();
    running.open("op", OpKind::Copy);
    raw(&running)
        .execute("UPDATE operations SET tree_summary = NULL WHERE op_id = 'op'", [])
        .expect("unbuild");
    assert_eq!(
        tree(&running, "op", &request()).summary,
        TreeSummaryView::Unbuilt {
            reason: UnbuiltReason::OperationRunning
        }
    );

    let no_indexes = folder_copy();
    raw(&no_indexes)
        .execute_batch("DROP INDEX operation_items_source_dir;")
        .expect("drop index");
    assert_eq!(
        tree(&no_indexes, "op", &request()).summary,
        TreeSummaryView::IndexesBuilding
    );
}

/// A writable connection beside the writer, for a cell that breaks something on
/// purpose.
fn raw(j: &Journal) -> rusqlite::Connection {
    j.writer.flush_blocking().expect("flush");
    cmdr_fs::sqlite_util::open(&j.path).expect("open")
}
