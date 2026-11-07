//! The selection: nearest wins, every normalization rule, the cap, base flips,
//! tri-state, the engine's per-row decision, and "N of M" against a brute force that
//! shares no code with the resolver.

use std::collections::HashMap;

use super::read::OpReader;
use super::selection::{MAX_TOGGLES, RollbackSelection, Selection, SelectionBase, SelectionToggle};
use super::summary::{FACT_COLUMNS, map_facts, read_op_tree_context, read_tree_dirs};
use super::test_support::{Journal, created_dir, item, journal};
use super::view::TreeCheckbox;
use super::view_test_support::{dir, finish, key, render, request, tree};
use crate::store::reconstruct_dir_path;
use crate::types::{EntryType, ItemOutcome, OpKind, RollbackState, RowRole, SearchCoverage};
use crate::writer::JournalItem;

use SelectionBase::{Everything, Nothing};

fn folder(dir_id: i64, on: bool) -> SelectionToggle {
    SelectionToggle::Folder { dir_id, on }
}

fn one(seq: i64, on: bool) -> SelectionToggle {
    SelectionToggle::Item { seq, on }
}

fn file(seq: i64, dir: &'static str, name: &'static str) -> JournalItem {
    let src: &'static str = Box::leak(format!("/src{}", dir.trim_start_matches("/dst")).into_boxed_str());
    item(
        seq,
        EntryType::File,
        RowRole::RollbackUnit,
        ("v", src, name),
        Some(("v", dir, name)),
        Some(10 + seq),
    )
}

/// A copy into `/dst/proj`: `a.txt` (0), `sub/b.txt` (1), `sub/c.txt` (2), and the
/// created folders `sub` (3) and `proj` (4).
fn copy() -> Journal {
    let j = journal();
    j.open("op", OpKind::Copy);
    j.record(
        "op",
        vec![
            file(0, "/dst/proj", "a.txt"),
            file(1, "/dst/proj/sub", "b.txt"),
            file(2, "/dst/proj/sub", "c.txt"),
            created_dir(3, ("v", "/dst/proj", "sub")),
            created_dir(4, ("v", "/dst", "proj")),
        ],
    );
    finish(&j, "op", RollbackState::Rollbackable, SearchCoverage::Full, None);
    j
}

/// The dir ids of `proj` and `sub`.
fn dirs(j: &Journal) -> (i64, i64) {
    let proj = key(&tree(j, "op", &request()).rows, "proj");
    let open = tree(j, "op", &request().open(&[proj]));
    (dir(&open.rows, "proj"), dir(&open.rows, "sub"))
}

fn resolve(j: &Journal, base: SelectionBase, toggles: &[SelectionToggle]) -> RollbackSelection {
    let conn = j.read();
    let context = read_op_tree_context(&conn, "op").expect("read").expect("op");
    let mut reader = OpReader::new(&conn, "op", context);
    let selection = RollbackSelection {
        base,
        toggles: toggles.to_vec(),
    };
    Selection::resolve(&mut reader, &selection)
        .expect("resolve")
        .normalized()
        .clone()
}

fn n_of_m(j: &Journal, base: SelectionBase, toggles: &[SelectionToggle]) -> (u64, u64) {
    let view = tree(j, "op", &request().select(base, toggles));
    (view.tally.selected, view.tally.selectable)
}

#[test]
fn the_nearest_toggle_wins() {
    let j = copy();
    let (proj, sub) = dirs(&j);
    assert_eq!(n_of_m(&j, Everything, &[]), (3, 3));
    assert_eq!(n_of_m(&j, Everything, &[folder(sub, false)]), (1, 3));
    assert_eq!(n_of_m(&j, Everything, &[folder(sub, false), one(1, true)]), (2, 3));
    assert_eq!(n_of_m(&j, Nothing, &[]), (0, 3));
    assert_eq!(n_of_m(&j, Nothing, &[folder(sub, true)]), (2, 3));
    assert_eq!(
        n_of_m(&j, Nothing, &[folder(proj, true), folder(sub, false), one(2, true)]),
        (2, 3)
    );
}

#[test]
fn tri_state_follows_nested_on_off_on() {
    let j = copy();
    let (proj, sub) = dirs(&j);
    let pk = key(&tree(&j, "op", &request()).rows, "proj");
    let sk = key(&tree(&j, "op", &request().open(&[pk])).rows, "sub");
    let view = tree(
        &j,
        "op",
        &request()
            .open(&[pk, sk])
            .select(Everything, &[folder(proj, false), folder(sub, true), one(2, false)]),
    );
    assert_eq!(
        render(&view.rows),
        [
            "proj [-] (~) Housekeeping",
            "  sub [-] (~) Housekeeping",
            "    b.txt (x) Selectable",
            "    c.txt ( ) Selectable",
            "  a.txt ( ) Selectable",
        ]
    );
    let all_off = tree(&j, "op", &request().open(&[pk]).select(Nothing, &[]));
    assert_eq!(all_off.rows[0].checkbox, TreeCheckbox::Off);
}

#[test]
fn a_later_folder_toggle_covers_earlier_ones_inside_it() {
    let j = copy();
    let (proj, sub) = dirs(&j);
    assert_eq!(
        resolve(
            &j,
            Everything,
            &[one(1, false), folder(sub, false), folder(proj, false)]
        )
        .toggles,
        [folder(proj, false)]
    );
    assert_eq!(
        resolve(&j, Everything, &[folder(proj, false), folder(sub, true)]).toggles,
        [folder(proj, false), folder(sub, true)],
        "an earlier ancestor stays under a later descendant"
    );
    assert_eq!(resolve(&j, Everything, &[one(1, false), one(1, true)]).toggles, []);
    assert_eq!(
        resolve(&j, Everything, &[one(1, true), one(1, false)]).toggles,
        [one(1, false)]
    );
}

#[test]
fn no_op_toggles_go() {
    let j = copy();
    let (proj, sub) = dirs(&j);
    assert_eq!(resolve(&j, Everything, &[folder(sub, true), one(0, true)]).toggles, []);
    assert_eq!(resolve(&j, Nothing, &[folder(proj, false)]).toggles, []);
    assert_eq!(
        resolve(&j, Everything, &[folder(proj, false), folder(sub, false)]).toggles,
        [folder(proj, false)],
        "a toggle equal to its ancestor's"
    );
}

#[test]
fn toggles_naming_nothing_selectable_of_this_op_go() {
    let j = copy();
    let (_, sub) = dirs(&j);
    // Another op's folder: `dirs` is interned for every op.
    j.open("other", OpKind::Copy);
    j.record("other", vec![file(0, "/elsewhere", "z")]);
    let conn = j.read();
    let foreign: i64 = conn
        .query_row("SELECT dir_id FROM dirs WHERE name = 'elsewhere'", [], |row| row.get(0))
        .expect("foreign dir");
    drop(conn);
    j.outcomes("op", &[(0, ItemOutcome::RolledBack, None)]);
    let normalized = resolve(
        &j,
        Everything,
        &[
            folder(foreign, false),
            one(99, false),
            one(0, false), // settled since
            one(3, false), // housekeeping
            one(4, false),
            folder(sub, false),
        ],
    );
    assert_eq!(normalized.toggles, [folder(sub, false)]);
}

#[test]
fn a_folder_unit_is_decided_by_its_own_folder() {
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
    let top = tree(&j, "op", &request());
    let pics = dir(&top.rows, "pics");
    let open = tree(&j, "op", &request().open(&[key(&top.rows, "pics")]));
    let inner = dir(&open.rows, "sub");

    assert_eq!(
        n_of_m(&j, Everything, &[folder(pics, false)]),
        (1, 2),
        "exactly its unit"
    );
    assert_eq!(
        resolve(&j, Everything, &[one(0, false)]).toggles,
        [folder(pics, false)],
        "an item toggle on a folder unit becomes its folder's"
    );
    assert_eq!(
        resolve(&j, Everything, &[folder(inner, false), one(1, false), one(2, false)]).toggles,
        [],
        "contents have no checkbox"
    );
}

#[test]
fn past_the_cap_the_newest_toggles_are_refused() {
    let j = journal();
    j.open("op", OpKind::Rename);
    let names: Vec<&'static str> = (0..MAX_TOGGLES + 5)
        .map(|i| &*Box::leak(format!("f{i:05}").into_boxed_str()))
        .collect();
    let rows = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            item(
                i as i64,
                EntryType::File,
                RowRole::RollbackUnit,
                ("v", "/w", name),
                Some(("v", "/w", "r")),
                Some(1),
            )
        })
        .collect();
    j.record("op", rows);
    finish(&j, "op", RollbackState::Rollbackable, SearchCoverage::Full, None);
    let toggles: Vec<SelectionToggle> = (0..MAX_TOGGLES as i64 + 3).map(|seq| one(seq, false)).collect();
    let view = tree(&j, "op", &request().select(Everything, &toggles).window(0, 1));
    assert!(view.toggles_capped);
    assert_eq!(view.selection.toggles.len(), MAX_TOGGLES);
    assert_eq!(view.selection.toggles.last(), Some(&one(MAX_TOGGLES as i64 - 1, false)));
    assert_eq!(view.tally.selected, 5);
}

#[test]
fn a_created_folder_goes_along_only_when_everything_under_it_does() {
    let j = copy();
    let (proj, sub) = dirs(&j);
    let decide = |base, toggles: &[SelectionToggle]| -> HashMap<i64, bool> {
        let conn = j.read();
        let context = read_op_tree_context(&conn, "op").expect("read").expect("op");
        let mut reader = OpReader::new(&conn, "op", context);
        let selection = RollbackSelection {
            base,
            toggles: toggles.to_vec(),
        };
        let mut resolved = Selection::resolve(&mut reader, &selection).expect("resolve");
        rows(&conn)
            .iter()
            .map(|facts| (facts.seq, resolved.includes(&mut reader, facts).expect("decide")))
            .collect()
    };
    let all = decide(Everything, &[]);
    assert!(all.values().all(|&on| on), "{all:?}");
    let one_off = decide(Everything, &[one(1, false)]);
    assert_eq!(
        (one_off[&1], one_off[&3], one_off[&4], one_off[&2]),
        (false, false, false, true)
    );
    let sub_only = decide(Nothing, &[folder(sub, true)]);
    assert_eq!((sub_only[&3], sub_only[&4], sub_only[&0]), (true, false, false));
    let _ = proj;
}

fn rows(conn: &rusqlite::Connection) -> Vec<super::summary::RowFacts> {
    let sql = format!("SELECT {FACT_COLUMNS} FROM operation_items WHERE op_id = 'op' ORDER BY seq");
    let mut stmt = conn.prepare(&sql).expect("prepare");
    let mut out = Vec::new();
    let mut found = stmt.query([]).expect("query");
    while let Some(row) = found.next().expect("row") {
        out.push(map_facts(row).expect("facts"));
    }
    out
}

/// A copy of nested folders into `/dst`, some files settled, for the brute force.
fn nested_copy() -> (Journal, Vec<(i64, String, bool)>) {
    let j = journal();
    j.open("op", OpKind::Copy);
    let mut items = Vec::new();
    // (seq, the file's dest folder, selectable)
    let mut expected = Vec::new();
    let mut seq = 0;
    let mut created = Vec::new();
    for a in ["a", "b"] {
        for b in ["x", "y", "z"] {
            let folder: &'static str = Box::leak(format!("/dst/{a}/{b}").into_boxed_str());
            for f in 0..3 {
                let name: &'static str = Box::leak(format!("f{f}").into_boxed_str());
                items.push(file(seq, folder, name));
                expected.push((seq, folder.to_string(), true));
                seq += 1;
            }
            created.push((format!("/dst/{a}"), b));
        }
        created.push(("/dst".to_string(), a));
        let top_file: &'static str = Box::leak(format!("/dst/{a}").into_boxed_str());
        items.push(file(seq, top_file, "top"));
        expected.push((seq, top_file.to_string(), true));
        seq += 1;
    }
    for (parent, name) in created {
        let parent: &'static str = Box::leak(parent.into_boxed_str());
        items.push(created_dir(seq, ("v", parent, name)));
        seq += 1;
    }
    j.record("op", items);
    // Settle a few: they leave "M".
    j.outcomes(
        "op",
        &[(0, ItemOutcome::RolledBack, None), (7, ItemOutcome::RolledBack, None)],
    );
    for entry in &mut expected {
        if entry.0 == 0 || entry.0 == 7 {
            entry.2 = false;
        }
    }
    finish(&j, "op", RollbackState::PartiallyRolledBack, SearchCoverage::Full, None);
    (j, expected)
}

/// A tiny deterministic generator, so a failure replays.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, below: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % below
    }
}

#[test]
fn n_of_m_matches_a_brute_force_over_paths() {
    let (j, files) = nested_copy();
    let conn = j.read();
    let folder_paths: HashMap<i64, String> = read_tree_dirs(&conn, "op")
        .expect("summary")
        .iter()
        .map(|row| (row.dir_id, reconstruct_dir_path(&conn, row.dir_id).expect("path")))
        .collect();
    let dir_ids: Vec<i64> = folder_paths.keys().copied().collect();
    let seqs = files.len() as u64 + 10;
    let mut rng = Lcg(7);
    for round in 0..200 {
        let base = if rng.next(2) == 0 { Everything } else { Nothing };
        let toggles: Vec<SelectionToggle> = (0..rng.next(8))
            .map(|_| {
                let on = rng.next(2) == 0;
                if rng.next(2) == 0 {
                    folder(dir_ids[rng.next(dir_ids.len() as u64) as usize], on)
                } else {
                    one(rng.next(seqs) as i64, on)
                }
            })
            .collect();
        // The engine's per-row decision, through the same resolver.
        let context = read_op_tree_context(&conn, "op").expect("read").expect("op");
        let mut reader = OpReader::new(&conn, "op", context);
        let wire = RollbackSelection {
            base,
            toggles: toggles.clone(),
        };
        let mut resolved = Selection::resolve(&mut reader, &wire).expect("resolve");
        let facts: HashMap<i64, super::summary::RowFacts> = rows(&conn).into_iter().map(|f| (f.seq, f)).collect();
        // The brute force: the LATEST toggle that applies to a row wins (its own item
        // toggle, or a folder toggle on its folder or any folder above), else the base.
        let mut expected = 0;
        for (seq, folder_path, selectable) in &files {
            if !selectable {
                continue;
            }
            let mut on = base == Everything;
            for toggle in &toggles {
                match *toggle {
                    SelectionToggle::Item { seq: s, on: o } if s == *seq => on = o,
                    SelectionToggle::Folder { dir_id, on: o } => {
                        let path = &folder_paths[&dir_id];
                        if folder_path == path || folder_path.starts_with(&format!("{path}/")) || path == "/" {
                            on = o;
                        }
                    }
                    SelectionToggle::Item { .. } => {}
                }
            }
            let decided = resolved.includes(&mut reader, &facts[seq]).expect("decide");
            assert_eq!(decided, on, "round {round}: seq {seq} under {base:?} {toggles:?}");
            if on {
                expected += 1;
            }
        }
        let (selected, selectable) = n_of_m(&j, base, &toggles);
        assert_eq!(selectable, files.iter().filter(|f| f.2).count() as u64);
        assert_eq!(selected, expected, "round {round}: {base:?} {toggles:?}");
    }
}
