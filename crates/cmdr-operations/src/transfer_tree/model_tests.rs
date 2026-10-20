//! The arena's mutations: what each file boundary does to the rows above it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use cmdr_fs::volume::{CopyScanResult, ScanRecorder};

use super::super::TreeRecorder;
use super::super::test_support::{english, node_at, outline};
use super::{LeafState, NodeId, NodeKind, TreeModel, TreeRowState};
use crate::types::WriteOperationError;

/// `top/{a.bin 10 B, b.bin 20 B, sub/c.bin 30 B}`.
fn small() -> TreeModel {
    let recorder = TreeRecorder::new(&[PathBuf::from("/src/top")], english());
    recorder.dir(Path::new("/src/top"));
    recorder.file(Path::new("/src/top/a.bin"), 10);
    recorder.file(Path::new("/src/top/b.bin"), 20);
    recorder.dir(Path::new("/src/top/sub"));
    recorder.file(Path::new("/src/top/sub/c.bin"), 30);
    recorder.finish().expect("the tree builds")
}

fn live() -> Arc<AtomicU64> {
    Arc::new(AtomicU64::new(0))
}

fn done_of(model: &TreeModel, labels: &[&str]) -> (u32, u64, u32, u32) {
    let agg = model.folder(node_at(model, labels)).expect("a folder");
    (agg.files_done, agg.bytes_done, agg.skipped, agg.failed)
}

#[test]
fn a_landed_file_reaches_every_ancestor_once() {
    let mut model = small();
    let deep = node_at(&model, &["top", "sub", "c.bin"]);

    model.begin(deep, live());
    model.complete(deep);
    model.complete(deep);

    assert_eq!(done_of(&model, &["top", "sub"]), (1, 30, 0, 0));
    assert_eq!(done_of(&model, &["top"]), (1, 30, 0, 0));
    assert_eq!(model.leaf_state(deep), LeafState::Done);
    assert!(model.active().is_empty());
}

#[test]
fn a_skip_counts_as_done_and_as_skipped() {
    let mut model = small();
    for labels in [["top", "a.bin"], ["top", "b.bin"]] {
        model.skip(node_at(&model, &labels));
    }
    model.skip(node_at(&model, &["top", "sub", "c.bin"]));

    assert_eq!(done_of(&model, &["top"]), (3, 60, 3, 0));
    assert_eq!(
        model.folder_state(node_at(&model, &["top"]), false),
        TreeRowState::Skipped
    );
}

#[test]
fn one_landed_file_among_skips_makes_the_folder_done() {
    let mut model = small();
    model.complete(node_at(&model, &["top", "a.bin"]));
    model.skip(node_at(&model, &["top", "b.bin"]));
    model.skip(node_at(&model, &["top", "sub", "c.bin"]));

    assert_eq!(model.folder_state(node_at(&model, &["top"]), false), TreeRowState::Done);
}

#[test]
fn a_failure_marks_every_ancestor_failed_and_keeps_its_reason() {
    let mut model = small();
    let deep = node_at(&model, &["top", "sub", "c.bin"]);
    model.fail(
        deep,
        WriteOperationError::IoError {
            path: "/src/top/sub/c.bin".to_string(),
            message: "gone".into(),
        },
    );

    assert_eq!(done_of(&model, &["top"]), (0, 0, 0, 1));
    assert_eq!(
        model.folder_state(node_at(&model, &["top"]), false),
        TreeRowState::Failed
    );
    assert!(matches!(model.failure(deep), Some(WriteOperationError::IoError { .. })));
}

#[test]
fn a_withdrawn_file_goes_back_to_waiting_without_counting() {
    let mut model = small();
    let file = node_at(&model, &["top", "a.bin"]);
    model.begin(file, live());
    assert_eq!(
        model.folder_state(node_at(&model, &["top"]), true),
        TreeRowState::Active
    );

    model.withdraw(file);

    assert_eq!(model.leaf_state(file), LeafState::Waiting);
    assert_eq!(done_of(&model, &["top"]), (0, 0, 0, 0));
    assert_eq!(
        model.folder_state(node_at(&model, &["top"]), false),
        TreeRowState::Waiting
    );
    assert_eq!(model.focus(), vec![file], "the last file started stays the focus");
}

#[test]
fn a_file_found_after_the_scan_joins_its_folders_totals() {
    let mut model = small();
    let sub = node_at(&model, &["top", "sub"]);

    model.discover(sub, "late.bin", NodeKind::File, 5);

    assert_eq!(
        outline(&model),
        vec![
            "top/ 4 files 65 B",
            "  sub/ 2 files 35 B",
            "    c.bin 30 B",
            "    late.bin 5 B",
            "  a.bin 10 B",
            "  b.bin 20 B",
        ]
    );
}

#[test]
fn an_opaque_folders_contents_add_up_below_it_and_never_above() {
    let recorder = TreeRecorder::new(&[PathBuf::from("/src/opaque")], english());
    recorder.subtree(
        Path::new("/src/opaque"),
        &CopyScanResult {
            file_count: 2,
            dir_count: 1,
            total_bytes: 50,
            dedup_bytes: 50,
            top_level_is_directory: true,
            top_level_modified_at: None,
            top_level_created_at: None,
        },
    );
    let mut model = recorder.finish().expect("the tree builds");
    let opaque = node_at(&model, &["opaque"]);

    let inner = model.discover(opaque, "inner", NodeKind::Folder, 0);
    let file = model.discover(inner, "x.bin", NodeKind::File, 50);

    assert_eq!(
        outline(&model),
        vec!["opaque/? 2 files 50 B", "  inner/ 1 files 50 B", "    x.bin 50 B"]
    );
    model.complete(file);
    assert_eq!(
        done_of(&model, &["opaque"]),
        (1, 50, 0, 0),
        "completion still counts all the way up"
    );
    assert_eq!(model.position_in_parent(inner), 0);
}

#[test]
fn an_empty_folder_waits_until_the_driver_creates_it() {
    let recorder = TreeRecorder::new(&[PathBuf::from("/src/empty")], english());
    recorder.dir(Path::new("/src/empty"));
    let mut model = recorder.finish().expect("the tree builds");
    let empty = node_at(&model, &["empty"]);
    assert_eq!(model.folder_state(empty, false), TreeRowState::Waiting);

    model.folder_created(empty);

    assert_eq!(model.folder_state(empty, false), TreeRowState::Done);
}

#[test]
fn a_path_finds_its_row_through_its_source_and_a_stranger_finds_none() {
    let model = small();

    assert_eq!(
        model.source_node(Path::new("/src/top")),
        Some(node_at(&model, &["top"]))
    );
    assert_eq!(
        model.node_at_path(Path::new("/src/top/sub/c.bin")),
        Some(node_at(&model, &["top", "sub", "c.bin"]))
    );
    assert_eq!(
        model.node_at_path(Path::new("/src/top")),
        Some(node_at(&model, &["top"]))
    );
    assert_eq!(model.node_at_path(Path::new("/src/top/sub/missing.bin")), None);
    assert_eq!(
        model.node_at_path(Path::new("/dest/top/a.bin")),
        None,
        "a destination path is no row"
    );
    assert_eq!(model.source_node(Path::new("/src/elsewhere")), None);
}

#[test]
fn skipping_a_folder_settles_every_file_below_it_once() {
    let mut model = small();
    let landed = node_at(&model, &["top", "a.bin"]);
    model.complete(landed);
    let top = node_at(&model, &["top"]);

    model.skip_subtree(top);
    model.skip_subtree(top);

    assert_eq!(
        done_of(&model, &["top"]),
        (3, 60, 2, 0),
        "a.bin stays landed; b.bin and c.bin skip once"
    );
    assert_eq!(model.leaf_state(landed), LeafState::Done);
    assert_eq!(model.folder_state(top, false), TreeRowState::Done);
    assert_eq!(
        model.folder_state(node_at(&model, &["top", "sub"]), false),
        TreeRowState::Skipped
    );
}

#[test]
fn a_whole_subtree_landing_at_once_counts_an_opaque_folders_unwalked_files() {
    let recorder = TreeRecorder::new(&[PathBuf::from("/src/opaque"), PathBuf::from("/src/empty")], english());
    recorder.subtree(
        Path::new("/src/opaque"),
        &CopyScanResult {
            file_count: 3,
            dir_count: 1,
            total_bytes: 90,
            dedup_bytes: 90,
            top_level_is_directory: true,
            top_level_modified_at: None,
            top_level_created_at: None,
        },
    );
    recorder.dir(Path::new("/src/empty"));
    let mut model = recorder.finish().expect("the tree builds");
    let opaque = node_at(&model, &["opaque"]);
    let found = model.discover(opaque, "found.bin", NodeKind::File, 30);
    model.complete(found);

    model.complete_subtree(opaque);
    model.complete_subtree(node_at(&model, &["empty"]));

    assert_eq!(
        done_of(&model, &["opaque"]),
        (3, 90, 0, 0),
        "the two files the walk never named count too"
    );
    assert_eq!(model.folder_state(opaque, false), TreeRowState::Done);
    assert_eq!(model.folder(NodeId::ROOT).map(|agg| agg.files_done), Some(3));
    assert_eq!(
        model.folder_state(node_at(&model, &["empty"]), false),
        TreeRowState::Done
    );
}

#[test]
fn a_folder_that_failed_is_failed_and_so_is_everything_above_it() {
    let mut model = small();
    let sub = node_at(&model, &["top", "sub"]);
    let error = || WriteOperationError::IoError {
        path: "/src/top/sub".to_string(),
        message: "gone".into(),
    };

    model.fail(sub, error());
    model.fail(sub, error());

    assert_eq!(done_of(&model, &["top"]), (0, 0, 0, 1));
    assert_eq!(model.folder_state(sub, false), TreeRowState::Failed);
    assert!(model.failure(sub).is_some());
}

#[test]
fn empty_folders_finish_together_and_folders_with_files_keep_their_counts() {
    let recorder = TreeRecorder::new(&[PathBuf::from("/src/top")], english());
    recorder.dir(Path::new("/src/top"));
    recorder.dir(Path::new("/src/top/empty"));
    recorder.file(Path::new("/src/top/a.bin"), 10);
    let mut model = recorder.finish().expect("the tree builds");

    model.empty_folders_created();

    assert_eq!(
        model.folder_state(node_at(&model, &["top", "empty"]), false),
        TreeRowState::Done
    );
    assert_eq!(
        model.folder_state(node_at(&model, &["top"]), false),
        TreeRowState::Waiting
    );
}

#[test]
fn a_level_names_its_children_and_a_newcomer_joins_it() {
    let mut model = small();
    let top = node_at(&model, &["top"]);

    let names = model.children_by_name(top);

    assert_eq!(names.get("a.bin"), Some(&node_at(&model, &["top", "a.bin"])));
    assert_eq!(names.get("sub"), Some(&node_at(&model, &["top", "sub"])));
    assert_eq!(names.len(), 3);
    let late = model.discover(top, "late.bin", NodeKind::File, 1);
    assert_eq!(model.children_by_name(top).get("late.bin"), Some(&late));
}

/// An opaque folder's scan counted fewer files than its walk finds (one
/// arrived since): its counts read complete while the extra one is still in
/// flight, and the folder must not read done before that lands.
#[test]
fn a_folder_with_work_in_flight_below_is_never_done() {
    let recorder = TreeRecorder::new(&[PathBuf::from("/src/opaque")], english());
    recorder.subtree(
        Path::new("/src/opaque"),
        &CopyScanResult {
            file_count: 1,
            dir_count: 0,
            total_bytes: 10,
            dedup_bytes: 10,
            top_level_is_directory: true,
            top_level_modified_at: None,
            top_level_created_at: None,
        },
    );
    let mut model = recorder.finish().expect("the tree builds");
    let opaque = node_at(&model, &["opaque"]);
    let counted = model.discover(opaque, "a.bin", NodeKind::File, 10);
    let arrived = model.discover(opaque, "b.bin", NodeKind::File, 10);
    model.complete(counted);
    model.begin(arrived, Arc::new(AtomicU64::new(0)));

    assert_eq!(model.folder_state(opaque, true), TreeRowState::Active);
}
