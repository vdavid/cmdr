//! What a driver's reports do to the tree, and the guarantee that none of them
//! can reach back into the driver: no tree, no row, or a tree bug all leave the
//! transfer exactly as it was.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use cmdr_fs::volume::ScanRecorder;

use super::model::{LeafState, NodeId, TreeRowState};
use super::test_support::{english, node_at};
use super::view::{TransferTreeViewRequest, TreeWindow};
use super::{TransferTree, TransferTreeSlot, TreeLanding, TreeLeaf, TreeRecorder, TreeReporter};
use crate::types::{ProgressAxis, WriteOperationError};

/// `/src/top/{a.bin 10 B, sub/c.bin 30 B}`, landing in `/dest`, installed in a slot.
fn installed() -> TransferTreeSlot {
    installed_landing(TreeLanding::under(Path::new("/dest"), None))
}

fn installed_landing(landing: TreeLanding) -> TransferTreeSlot {
    let recorder = TreeRecorder::new(&[PathBuf::from("/src/top")], english());
    recorder.dir(Path::new("/src/top"));
    recorder.file(Path::new("/src/top/a.bin"), 10);
    recorder.dir(Path::new("/src/top/sub"));
    recorder.file(Path::new("/src/top/sub/c.bin"), 30);
    let slot = TransferTreeSlot::default();
    slot.install(recorder.finish(), landing);
    slot
}

fn full_disk_at(path: &str) -> WriteOperationError {
    WriteOperationError::DestinationFull { path: path.to_string() }
}

fn tree(slot: &TransferTreeSlot) -> &Arc<TransferTree> {
    slot.get().expect("installed")
}

fn node(slot: &TransferTreeSlot, labels: &[&str]) -> NodeId {
    tree(slot).read(|model| node_at(model, labels)).expect("the tree is on")
}

fn state_of(slot: &TransferTreeSlot, node: NodeId) -> LeafState {
    tree(slot).read(|model| model.leaf_state(node)).expect("the tree is on")
}

fn request() -> TransferTreeViewRequest {
    TransferTreeViewRequest {
        expanded: Vec::new(),
        collapsed: Vec::new(),
        window: TreeWindow::At { offset: 0 },
        limit: 10,
    }
}

#[test]
fn a_leaf_is_active_while_it_lives_and_back_to_waiting_when_dropped_unsettled() {
    let slot = installed();
    let reporter = slot.reporter();
    let file = node(&slot, &["top", "a.bin"]);

    {
        let row = reporter.leaf(Some(file));
        row.bytes().store(4, Ordering::Relaxed);
        assert_eq!(state_of(&slot, file), LeafState::Active);
        assert_eq!(
            tree(&slot).read(|model| model.in_flight_bytes(file)).flatten(),
            Some(4),
            "the view reads the bytes the driver stored, without a report"
        );
    }

    assert_eq!(state_of(&slot, file), LeafState::Waiting);
    let landed = reporter.leaf(Some(file));
    landed.complete();
    drop(landed);
    assert_eq!(
        state_of(&slot, file),
        LeafState::Done,
        "a settled leaf's drop changes nothing"
    );
}

#[test]
fn a_walker_finds_each_child_by_name_and_a_newcomer_joins_the_tree() {
    let slot = installed();
    let reporter = slot.reporter();
    let top = reporter.source(Path::new("/src/top"));
    assert_eq!(top, Some(node(&slot, &["top"])));

    let mut level = reporter.level(top, []);

    assert_eq!(level.child("a.bin", false, 10), Some(node(&slot, &["top", "a.bin"])));
    assert_eq!(level.child("sub", true, 0), Some(node(&slot, &["top", "sub"])));
    assert_eq!(
        level.child("a.bin", true, 0),
        None,
        "a name that changed kind since the scan has no row"
    );
    let late = level.child("late.bin", false, 5).expect("a newcomer joins the tree");
    assert_eq!(level.child("late.bin", false, 5), Some(late), "and is found again");
    let top = node(&slot, &["top"]);
    assert_eq!(
        tree(&slot).read(|model| model.folder(top).map(|agg| agg.files_total)),
        Some(Some(3))
    );
}

/// A destination-side failure (a full disk, a folder that won't take a write)
/// names where the file was landing: the landing folder plus the source's own
/// name plus the path below it.
#[test]
fn a_failure_on_the_destination_side_marks_the_row_it_was_writing() {
    let slot = installed();
    let deep = node(&slot, &["top", "sub", "c.bin"]);

    slot.note_failure(&full_disk_at("/dest/top/sub/c.bin"));

    assert_eq!(state_of(&slot, deep), LeafState::Failed);
    assert_eq!(state_of(&slot, node(&slot, &["top", "a.bin"])), LeafState::Waiting);
}

/// A single source the user named lands under that name, and a cross-FS move
/// stages under its own folder first; both find their rows.
#[test]
fn a_named_copy_and_a_staging_folder_find_their_rows_too() {
    let named = installed_landing(TreeLanding::under(Path::new("/dest"), Some("renamed")));
    named.note_failure(&full_disk_at("/dest/renamed/a.bin"));
    assert_eq!(state_of(&named, node(&named, &["top", "a.bin"])), LeafState::Failed);

    let staged = installed_landing(
        TreeLanding::under(Path::new("/dest"), None).and_first_under(PathBuf::from("/dest/.cmdr-staging-op")),
    );
    staged.note_failure(&full_disk_at("/dest/.cmdr-staging-op/top/sub/c.bin"));
    assert_eq!(
        state_of(&staged, node(&staged, &["top", "sub", "c.bin"])),
        LeafState::Failed
    );
}

/// A path the tree never held marks nothing: a landing renamed on a clash, the
/// destination folder itself, a path under neither side.
#[test]
fn a_destination_path_the_tree_never_held_marks_nothing() {
    let slot = installed();
    for path in [
        "/dest/top (1)/a.bin",
        "/dest",
        "/dest/top/nope.bin",
        "/elsewhere/top/a.bin",
    ] {
        slot.note_failure(&full_disk_at(path));
    }
    let failed = tree(&slot)
        .read(|model| model.folder(node_at(model, &["top"])).map(|agg| agg.failed))
        .flatten();
    assert_eq!(failed, Some(0));
}

#[test]
fn a_failure_marks_the_row_its_source_path_names() {
    let slot = installed();
    let deep = node(&slot, &["top", "sub", "c.bin"]);

    slot.note_failure(&WriteOperationError::ReadError {
        path: "/src/top/sub/c.bin".to_string(),
        message: "unreadable".into(),
    });

    assert_eq!(state_of(&slot, deep), LeafState::Failed);
    assert!(matches!(
        tree(&slot).read(|model| model.failure(deep).cloned()).flatten(),
        Some(WriteOperationError::ReadError { .. })
    ));
}

#[test]
fn with_no_tree_and_no_row_every_report_is_silent() {
    let nothing = TreeReporter::none();
    assert_eq!(nothing.source(Path::new("/src/top")), None);
    let mut level = nothing.level(None, []);
    assert_eq!(level.child("a.bin", false, 1), None);
    nothing.skip_whole(None);
    nothing.complete_whole(None);
    nothing.empty_folders_in_place();
    let row = nothing.leaf(None);
    row.bytes().store(9, Ordering::Relaxed);
    row.complete();
    drop(TreeLeaf::detached());

    let empty = TransferTreeSlot::default();
    empty.note_failure(&WriteOperationError::IoError {
        path: "/src/top/a.bin".to_string(),
        message: "gone".into(),
    });
    assert!(
        !empty.view(&request(), ProgressAxis::Bytes).ready,
        "no tree yet reads as not ready"
    );
}

#[test]
fn a_report_that_panics_turns_the_tree_off_and_the_driver_carries_on() {
    let slot = installed();
    let file = node(&slot, &["top", "a.bin"]);

    // A tree bug, inside a report.
    let answer = tree(&slot).mutate(|_| -> () { panic!("a tree bug") });

    assert!(answer.is_none(), "the panic stops at the tree");
    // Every later report is a no-op, and a view says there's no tree.
    let reporter = slot.reporter();
    let row = reporter.leaf(Some(file));
    row.complete();
    reporter.skip_whole(Some(file));
    assert!(tree(&slot).read(|model| model.leaf_state(file)).is_none());
    assert!(!slot.view(&request(), ProgressAxis::Bytes).ready);
}

/// What the tree adds per file: one row opened (an allocation and a lock) and
/// settled (a lock and an O(depth) walk up), the two boundaries every engine
/// reports. Set against `local_copy_bench_many_small_files`'s per-file time in
/// `DETAILS.md` § "What a report costs".
///
/// `#[ignore]`d, a measurement. Run it with:
///
/// ```text
/// cd apps/desktop/src-tauri && cargo test --release --lib \
///   tree_boundary_cost_bench -- --ignored --nocapture --test-threads=1
/// ```
#[test]
#[ignore = "benchmark: run on demand with --ignored --nocapture"]
#[allow(
    clippy::print_stdout,
    reason = "Bench prints its timing report by design (run with --nocapture)."
)]
fn tree_boundary_cost_bench() {
    const FILES: usize = 2_000;
    const ROUNDS: usize = 5;
    let mut nanos_per_file = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let recorder = TreeRecorder::new(&[PathBuf::from("/src/top")], english());
        recorder.dir(Path::new("/src/top"));
        for file in 0..FILES {
            recorder.file(&PathBuf::from(format!("/src/top/sub/f_{file:05}.bin")), 4_096);
        }
        let slot = TransferTreeSlot::default();
        slot.install(recorder.finish(), TreeLanding::nowhere());
        let reporter = slot.reporter();
        let mut level = reporter.level(reporter.source(Path::new("/src/top")), []);
        let sub = level.child("sub", true, 0);
        let mut level = reporter.level(sub, []);
        let rows: Vec<Option<NodeId>> = (0..FILES)
            .map(|file| level.child(&format!("f_{file:05}.bin"), false, 4_096))
            .collect();

        let started = std::time::Instant::now();
        for row in rows {
            let leaf = reporter.leaf(row);
            leaf.bytes().store(4_096, Ordering::Relaxed);
            leaf.complete();
        }
        nanos_per_file.push(started.elapsed().as_nanos() / FILES as u128);
    }
    nanos_per_file.sort_unstable();
    println!(
        "transfer tree: one file's begin + complete, rounds(ns)={nanos_per_file:?}, median={} ns",
        nanos_per_file[ROUNDS / 2]
    );
}

#[test]
fn a_whole_source_skipped_by_its_path_settles_its_rows() {
    let slot = installed();
    let reporter = slot.reporter();

    reporter.skip_whole(reporter.source(Path::new("/src/top")));

    let view = slot.view(&request(), ProgressAxis::Bytes);
    assert!(view.ready);
    assert_eq!(view.rows[0].label, "top");
    assert_eq!(view.rows[0].state, TreeRowState::Skipped);
    assert_eq!((view.rows[0].files_done, view.rows[0].skipped), (2, 2));
}

/// A folder whose contents grew after the scan: the walker lists `late.bin`,
/// which the scan never saw. It joins the folder's counts as the level is
/// listed, before any child lands, so the folder can't read done while
/// `late.bin` is still to come (it's last in tree order).
#[test]
fn a_listed_newcomer_keeps_its_folder_open_until_it_lands() {
    let slot = installed();
    let reporter = slot.reporter();
    let top = reporter.source(Path::new("/src/top"));
    let mut level = reporter.level(top, [("sub", true, 0), ("a.bin", false, 10), ("late.bin", false, 5)]);
    let top = node(&slot, &["top"]);
    let folder_state = |slot: &TransferTreeSlot| {
        tree(slot)
            .read(|model| model.folder_state(top, !model.in_flight_by_folder().is_empty()))
            .expect("the tree is on")
    };

    for (name, is_folder) in [("sub", true), ("a.bin", false)] {
        let row = level.child(name, is_folder, 0);
        if is_folder {
            let mut sub_level = reporter.level(row, [("c.bin", false, 30)]);
            reporter.leaf(sub_level.child("c.bin", false, 30)).complete();
        } else {
            reporter.leaf(row).complete();
        }
    }
    assert_eq!(folder_state(&slot), TreeRowState::Active, "late.bin is still to come");

    reporter.leaf(level.child("late.bin", false, 5)).complete();
    assert_eq!(folder_state(&slot), TreeRowState::Done);
}
