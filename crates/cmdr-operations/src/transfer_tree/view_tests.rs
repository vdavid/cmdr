//! Flattening the tree into a window of rows: effective expansion, compact
//! folders, windowing, and the Follow window.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use cmdr_fs::volume::ScanRecorder;

use super::super::model::{NodeKind, TreeModel, TreeRowState};
use super::super::test_support::{english, node_at};
use super::super::{TransferTree, TreeRecorder};
use super::{TransferTreeViewRequest, TreeRowKind, TreeWindow, flatten};
use crate::types::{ProgressAxis, WriteOperationError};

/// `folders` top-level folders, each holding `files` files, plus `loose` top-level files.
fn grid(folders: usize, files: usize, loose: usize) -> TreeModel {
    let recorder = TreeRecorder::new(&sources(folders, loose), english());
    for folder in 0..folders {
        let dir = PathBuf::from(format!("/src/folder_{folder}"));
        recorder.dir(&dir);
        for file in 0..files {
            recorder.file(&dir.join(format!("file_{file:05}")), 100);
        }
    }
    for file in 0..loose {
        recorder.file(&PathBuf::from(format!("/src/loose_{file:05}")), 10);
    }
    recorder.finish().expect("the tree builds")
}

fn sources(folders: usize, loose: usize) -> Vec<PathBuf> {
    (0..folders)
        .map(|folder| PathBuf::from(format!("/src/folder_{folder}")))
        .chain((0..loose).map(|file| PathBuf::from(format!("/src/loose_{file:05}"))))
        .collect()
}

fn request(window: TreeWindow, limit: u32) -> TransferTreeViewRequest {
    TransferTreeViewRequest {
        expanded: Vec::new(),
        collapsed: Vec::new(),
        window,
        limit,
    }
}

fn at(offset: u32, limit: u32) -> TransferTreeViewRequest {
    request(TreeWindow::At { offset }, limit)
}

/// The window as `depth`-indented labels.
fn labels(model: &TreeModel, request: &TransferTreeViewRequest) -> Vec<String> {
    flatten(model, request)
        .rows
        .iter()
        .map(|row| format!("{}{}", "  ".repeat(row.depth as usize), row.label))
        .collect()
}

fn live(bytes: u64) -> Arc<AtomicU64> {
    Arc::new(AtomicU64::new(bytes))
}

#[test]
fn a_flat_list_windows_anywhere_with_its_set_position() {
    let model = grid(0, 0, 5_000);

    let window = flatten(&model, &at(4_000, 3));

    assert_eq!(window.total_rows, 5_000);
    assert_eq!(window.offset, 4_000);
    let rows: Vec<(&str, u32, u32, u16)> = window
        .rows
        .iter()
        .map(|row| (row.label.as_str(), row.pos_in_set, row.set_size, row.depth))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("loose_04000", 4_001, 5_000, 0),
            ("loose_04001", 4_002, 5_000, 0),
            ("loose_04002", 4_003, 5_000, 0),
        ]
    );
}

#[test]
fn folders_stay_shut_until_the_work_is_inside_and_follow_centers_the_active_file() {
    let mut model = grid(5, 1_000, 0);
    assert_eq!(
        flatten(&model, &at(0, 10)).total_rows,
        5,
        "nothing started: five shut folders"
    );

    let active = node_at(&model, &["folder_2", "file_00500"]);
    model.begin(active, live(0));
    let window = flatten(&model, &request(TreeWindow::Follow { lead: 2 }, 5));

    assert_eq!(window.total_rows, 1_005);
    assert_eq!(
        window.focus_row,
        Some(503),
        "folder_2 sits at 2, its 501st file 501 rows below"
    );
    assert_eq!(window.offset, 501);
    assert_eq!(
        labels(&model, &request(TreeWindow::Follow { lead: 2 }, 5)),
        vec![
            "  file_00498",
            "  file_00499",
            "  file_00500",
            "  file_00501",
            "  file_00502"
        ]
    );
    assert_eq!(window.rows[2].state, TreeRowState::Active);
}

#[test]
fn between_two_files_the_last_one_started_keeps_its_folder_open() {
    let mut model = grid(2, 3, 0);
    let first = node_at(&model, &["folder_0", "file_00002"]);
    model.begin(first, live(0));
    model.complete(first);

    assert_eq!(
        flatten(&model, &at(0, 10)).total_rows,
        5,
        "folder_0 stays open in the gap"
    );
}

#[test]
fn a_folder_shuts_once_the_work_has_left_it() {
    let mut model = grid(2, 3, 0);
    let first = node_at(&model, &["folder_0", "file_00002"]);
    model.begin(first, live(0));
    model.complete(first);
    let next = node_at(&model, &["folder_1", "file_00000"]);
    model.begin(next, live(0));

    assert_eq!(
        labels(&model, &at(0, 10)),
        vec!["folder_0", "folder_1", "  file_00000", "  file_00001", "  file_00002"]
    );
}

#[test]
fn a_hand_collapse_beats_the_work_and_a_hand_expand_adds_to_it() {
    let mut model = grid(3, 2, 0);
    let active = node_at(&model, &["folder_1", "file_00000"]);
    model.begin(active, live(0));
    let mut req = at(0, 10);
    req.collapsed = vec![node_at(&model, &["folder_1"]).get()];
    req.expanded = vec![node_at(&model, &["folder_2"]).get()];

    let window = flatten(&model, &req);

    assert_eq!(
        labels(&model, &req),
        vec!["folder_0", "folder_1", "folder_2", "  file_00000", "  file_00001"]
    );
    assert_eq!(
        window.focus_row,
        Some(1),
        "the work hides under folder_1, so its row is the focus"
    );
}

#[test]
fn a_single_child_folder_chain_is_one_row_keyed_by_its_deepest_folder() {
    let recorder = TreeRecorder::new(&[PathBuf::from("/src/a"), PathBuf::from("/src/z.txt")], english());
    for dir in ["/src/a", "/src/a/b", "/src/a/b/c"] {
        recorder.dir(Path::new(dir));
    }
    recorder.file(Path::new("/src/a/b/c/1"), 1);
    recorder.file(Path::new("/src/a/b/c/2"), 1);
    recorder.file(Path::new("/src/z.txt"), 1);
    let mut model = recorder.finish().expect("the tree builds");
    let c = node_at(&model, &["a", "b", "c"]);

    let window = flatten(&model, &at(0, 10));
    assert_eq!(window.rows[0].label, "a/b/c");
    assert_eq!(window.rows[0].id, c.get());
    assert!(window.rows[0].has_children);
    assert!(!window.rows[0].expanded);

    let mut open = at(0, 10);
    open.expanded = vec![c.get()];
    assert_eq!(labels(&model, &open), vec!["a/b/c", "  1", "  2", "z.txt"]);

    // A file turning up beside `c` breaks the chain at `b`.
    let b = node_at(&model, &["a", "b"]);
    model.discover(b, "x.txt", NodeKind::File, 1);
    open.expanded = vec![b.get(), c.get()];
    assert_eq!(
        labels(&model, &open),
        vec!["a/b", "  c", "    1", "    2", "  x.txt", "z.txt"]
    );
}

#[test]
fn a_window_names_the_open_rows_above_its_first_row() {
    let recorder = TreeRecorder::new(&[PathBuf::from("/src/a"), PathBuf::from("/src/z.txt")], english());
    for dir in ["/src/a", "/src/a/b", "/src/a/b/c", "/src/a/b/c/d"] {
        recorder.dir(Path::new(dir));
    }
    recorder.file(Path::new("/src/a/b/c/1"), 1);
    for file in 0..20 {
        recorder.file(&PathBuf::from(format!("/src/a/b/c/d/f_{file:02}")), 1);
    }
    recorder.file(Path::new("/src/z.txt"), 1);
    let model = recorder.finish().expect("the tree builds");
    let c = node_at(&model, &["a", "b", "c"]);
    let d = node_at(&model, &["a", "b", "c", "d"]);
    let mut open = at(0, 3);
    open.expanded = vec![c.get(), d.get()];
    assert_eq!(labels(&model, &open), vec!["a/b/c", "  d", "    f_00"]);

    let ancestors = |offset: u32| -> Vec<(u32, String, u32)> {
        let mut request = at(offset, 3);
        request.expanded = open.expanded.clone();
        flatten(&model, &request)
            .ancestors
            .into_iter()
            .map(|ancestor| (ancestor.row.id, ancestor.row.label, ancestor.index))
            .collect()
    };

    assert_eq!(ancestors(0), vec![], "the first row is a top-level row");
    assert_eq!(ancestors(1), vec![(c.get(), "a/b/c".to_string(), 0)]);
    assert_eq!(
        ancestors(12),
        vec![(c.get(), "a/b/c".to_string(), 0), (d.get(), "d".to_string(), 1)],
        "deep in `d`, both open rows above are named with their flat index"
    );
    let deep = {
        let mut request = at(12, 3);
        request.expanded = open.expanded.clone();
        flatten(&model, &request).ancestors
    };
    assert_eq!(
        deep.iter()
            .map(|ancestor| (ancestor.row.depth, ancestor.row.files_total, ancestor.row.expanded))
            .collect::<Vec<_>>(),
        vec![(0, 21, true), (1, 20, true)],
        "each ancestor is a whole row, counts included, so a view can pin it"
    );
    assert_eq!(ancestors(23), vec![], "`z.txt` is top-level again");
}

#[test]
fn a_file_never_compacts_into_its_folder() {
    let recorder = TreeRecorder::new(&[PathBuf::from("/src/a")], english());
    recorder.dir(Path::new("/src/a"));
    recorder.file(Path::new("/src/a/only.txt"), 1);
    let model = recorder.finish().expect("the tree builds");
    let mut open = at(0, 10);
    open.expanded = vec![node_at(&model, &["a"]).get()];

    assert_eq!(labels(&model, &open), vec!["a", "  only.txt"]);
}

#[test]
fn a_folder_row_counts_the_bytes_in_flight_below_it() {
    let mut model = grid(1, 3, 0);
    let first = node_at(&model, &["folder_0", "file_00000"]);
    let second = node_at(&model, &["folder_0", "file_00001"]);
    model.begin(first, live(0));
    model.complete(first);
    model.begin(second, live(40));

    let window = flatten(&model, &at(0, 10));

    let folder = &window.rows[0];
    assert_eq!(folder.kind, TreeRowKind::Folder);
    assert_eq!(folder.state, TreeRowState::Active);
    assert_eq!((folder.files_done, folder.files_total), (1, 3));
    assert_eq!((folder.bytes_done, folder.bytes_total), (140, 300));
    let file = &window.rows[2];
    assert_eq!(
        (file.state, file.bytes_done, file.bytes_total),
        (TreeRowState::Active, 40, 100)
    );
    assert_eq!((file.files_done, file.files_total), (0, 1));
}

#[test]
fn settled_rows_say_how_they_ended() {
    let mut model = grid(2, 2, 0);
    let landed = node_at(&model, &["folder_0", "file_00000"]);
    let skipped = node_at(&model, &["folder_0", "file_00001"]);
    let failed = node_at(&model, &["folder_1", "file_00000"]);
    model.complete(landed);
    model.skip(skipped);
    model.fail(
        failed,
        WriteOperationError::IoError {
            path: "/src/folder_1/file_00000".to_string(),
            message: "gone".into(),
        },
    );
    let mut open = at(0, 10);
    open.expanded = vec![
        node_at(&model, &["folder_0"]).get(),
        node_at(&model, &["folder_1"]).get(),
    ];

    let rows = flatten(&model, &open).rows;

    let states: Vec<(&str, TreeRowState)> = rows.iter().map(|row| (row.label.as_str(), row.state)).collect();
    assert_eq!(
        states,
        vec![
            ("folder_0", TreeRowState::Done),
            ("file_00000", TreeRowState::Done),
            ("file_00001", TreeRowState::Skipped),
            ("folder_1", TreeRowState::Failed),
            ("file_00000", TreeRowState::Failed),
            ("file_00001", TreeRowState::Waiting),
        ]
    );
    assert_eq!(rows[0].skipped, 1);
    assert_eq!(rows[3].failed, 1);
    assert!(matches!(rows[4].failure, Some(WriteOperationError::IoError { .. })));
}

#[test]
fn an_empty_folder_is_done_once_created() {
    let recorder = TreeRecorder::new(&[PathBuf::from("/src/empty")], english());
    recorder.dir(Path::new("/src/empty"));
    let mut model = recorder.finish().expect("the tree builds");
    let empty = node_at(&model, &["empty"]);
    assert_eq!(flatten(&model, &at(0, 1)).rows[0].state, TreeRowState::Waiting);

    model.folder_created(empty);

    let row = &flatten(&model, &at(0, 1)).rows[0];
    assert_eq!(row.state, TreeRowState::Done);
    assert!(!row.has_children);
}

#[test]
fn follow_near_the_end_fills_the_window_and_an_offset_past_the_end_returns_nothing() {
    let mut model = grid(0, 0, 20);
    let last = node_at(&model, &["loose_00019"]);
    model.begin(last, live(0));

    let follow = flatten(&model, &request(TreeWindow::Follow { lead: 2 }, 8));
    assert_eq!(follow.offset, 12, "eight rows end exactly at the last one");
    assert_eq!(follow.rows.len(), 8);

    let past = flatten(&model, &at(50, 8));
    assert_eq!(past.offset, 20);
    assert!(past.rows.is_empty());
}

#[test]
fn the_limit_and_the_toggle_lists_are_capped() {
    let model = grid(0, 0, 500);
    let mut req = at(0, 10_000);
    req.expanded = (0..10_000).collect();

    assert_eq!(flatten(&model, &req).rows.len(), 200);
}

#[test]
fn every_mutation_bumps_the_revision_a_view_reports() {
    let tree = TransferTree::new(
        grid(1, 2, 0),
        Arc::new(crate::host::testing::RecordingEngineEvents::default()),
    );
    let view = |tree: &TransferTree| tree.view(&at(0, 5), ProgressAxis::Files).expect("the tree is on");
    let before = view(&tree).revision;
    let file = tree
        .read(|model| node_at(model, &["folder_0", "file_00000"]))
        .expect("the tree is on");

    tree.mutate(|model| model.complete(file));

    let after = view(&tree);
    assert!(after.revision > before);
    assert!(after.ready);
    assert_eq!(
        after.axis,
        ProgressAxis::Files,
        "the view carries the axis it was handed"
    );
    assert_eq!(
        tree.read(|model| model.leaf_state(file)),
        Some(super::super::model::LeafState::Done)
    );
}
