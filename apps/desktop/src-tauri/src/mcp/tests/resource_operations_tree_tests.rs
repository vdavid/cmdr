//! Tests for the `activeFolders:` block of a `cmdr://state` operation row: which
//! folder and file a running transfer is in, read off the transfer tree's
//! follow-the-work window.

use crate::mcp::resources::operations::{OperationRow, build_operations_yaml};
use cmdr_operations::testing::{ProgressAxis, TransferTreeAncestor};
use cmdr_operations::{
    LifecycleStatus, OperationSnapshot, TransferTreeRow, TransferTreeView, TreeRowKind, TreeRowState,
    WriteOperationType,
};

const MB: u64 = 1_024 * 1_024;

fn folder(label: &str, depth: u16, files_done: u64, files_total: u64) -> TransferTreeRow {
    TransferTreeRow {
        id: u32::from(depth) * 1_000 + files_total as u32,
        depth,
        label: label.to_string(),
        kind: TreeRowKind::Folder,
        state: TreeRowState::Active,
        expanded: true,
        has_children: true,
        pos_in_set: 1,
        set_size: 1,
        files_done,
        files_total,
        bytes_done: files_done * 3 * MB,
        bytes_total: files_total * 3 * MB,
        skipped: 0,
        failed: 0,
        failure: None,
    }
}

fn file(label: &str, depth: u16, state: TreeRowState, bytes_done: u64) -> TransferTreeRow {
    TransferTreeRow {
        id: 0,
        depth,
        label: label.to_string(),
        kind: TreeRowKind::File,
        state,
        expanded: false,
        has_children: false,
        pos_in_set: 1,
        set_size: 1,
        files_done: u64::from(state == TreeRowState::Done),
        files_total: 1,
        bytes_done,
        bytes_total: 3 * MB,
        skipped: 0,
        failed: 0,
        failure: None,
    }
}

fn view(ancestors: Vec<TransferTreeRow>, rows: Vec<TransferTreeRow>, total_rows: u32) -> TransferTreeView {
    TransferTreeView {
        ready: true,
        revision: 7,
        total_rows,
        offset: ancestors.len() as u32,
        ancestors: ancestors
            .into_iter()
            .enumerate()
            .map(|(index, row)| TransferTreeAncestor {
                index: index as u32,
                row,
            })
            .collect(),
        rows,
        focus_row: Some(0),
        axis: ProgressAxis::Bytes,
    }
}

/// A running copy whose transfer tree answered `tree`. No progress-cache half:
/// the block renders off the tree alone.
fn yaml_for(tree: TransferTreeView) -> String {
    let rows = vec![OperationRow {
        snapshot: OperationSnapshot {
            operation_id: "op-1".to_string(),
            operation_type: WriteOperationType::Copy,
            status: LifecycleStatus::Running,
            source: Some("/src/Photos 2024".to_string()),
            destination: Some("/Volumes/naspi/photos".to_string()),
            rollback_support: cmdr_operations::testing::RollbackSupport::Supported,
            reverses: None,
            error: None,
            can_retry: false,
            items: None,
            conflict_policy: None,
            started_at: None,
        },
        progress: None,
        pending_conflict: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
        tree: Some(tree),
    }];
    build_operations_yaml(&rows, 12_000)
}

#[test]
fn a_running_copy_names_the_folders_it_is_in_and_the_files_in_flight() {
    // Two files landing in Prague, one in Vienna: a parallel SMB copy's shape.
    // Prague's own row sits above the window, so its name comes from the
    // window's ancestors, and Vienna's from a row inside it.
    let yaml = yaml_for(view(
        vec![folder("Photos 2024", 0, 50, 100), folder("Prague", 1, 47, 60)],
        vec![
            file("IMG_0048.jpg", 2, TreeRowState::Active, MB),
            file("IMG_0049.jpg", 2, TreeRowState::Active, 0),
            file("IMG_0050.jpg", 2, TreeRowState::Waiting, 0),
            folder("Vienna", 1, 3, 40),
            file("IMG_0004.jpg", 2, TreeRowState::Active, 2 * MB),
            file("IMG_0005.jpg", 2, TreeRowState::Waiting, 0),
        ],
        120,
    ));

    let expected = "    activeFolders:
      - folder: \"Photos 2024/Prague\"
        progress: 141 MiB / 180 MiB (78%), 47/60 files
        files:
          - \"IMG_0048.jpg\" (1 MiB / 3 MiB)
          - \"IMG_0049.jpg\" (0 B / 3 MiB)
      - folder: \"Photos 2024/Vienna\"
        progress: 9 MiB / 120 MiB (7%), 3/40 files
        files:
          - \"IMG_0004.jpg\" (2 MiB / 3 MiB)
";
    assert!(yaml.contains(expected), "yaml: {yaml}");
    assert!(!yaml.contains("IMG_0050"), "a waiting file isn't in flight: {yaml}");
    assert!(!yaml.contains("moreActiveFiles"), "nothing was cut: {yaml}");
}

#[test]
fn a_compact_folder_chain_reads_as_its_full_path() {
    // `2024/Prague` is one row (a folder whose only child is a folder).
    let yaml = yaml_for(view(
        vec![folder("Photos/2024/Prague", 0, 1, 2)],
        vec![file("IMG_0001.jpg", 1, TreeRowState::Active, 0)],
        3,
    ));
    assert!(
        yaml.contains("      - folder: \"Photos/2024/Prague\"\n"),
        "yaml: {yaml}"
    );
}

#[test]
fn a_top_level_file_in_flight_has_no_folder_to_name() {
    let yaml = yaml_for(view(
        Vec::new(),
        vec![
            file("notes.txt", 0, TreeRowState::Done, 3 * MB),
            file("movie.mov", 0, TreeRowState::Active, MB),
        ],
        2,
    ));
    let expected = "    activeFolders:
      - folder: (top level)
        files:
          - \"movie.mov\" (1 MiB / 3 MiB)
";
    assert!(yaml.contains(expected), "yaml: {yaml}");
}

#[test]
fn a_wide_parallel_copy_lists_a_few_files_and_counts_the_rest() {
    // 20 files in flight at once must not flood a resource an agent reads
    // constantly; the cut is never silent.
    let rows: Vec<TransferTreeRow> = (0..20)
        .map(|i| file(&format!("part_{i:02}.bin"), 1, TreeRowState::Active, 0))
        .collect();
    let yaml = yaml_for(view(vec![folder("Backups", 0, 0, 20)], rows, 21));

    assert_eq!(yaml.matches("part_").count(), 8, "yaml: {yaml}");
    assert!(yaml.contains("    moreActiveFiles: 12\n"), "yaml: {yaml}");
}

#[test]
fn a_cut_window_says_its_count_of_the_rest_is_a_floor() {
    // The window stopped before the tree's end, so more files may be in flight
    // past it.
    let rows: Vec<TransferTreeRow> = (0..10)
        .map(|i| file(&format!("part_{i:02}.bin"), 1, TreeRowState::Active, 0))
        .collect();
    let yaml = yaml_for(view(vec![folder("Backups", 0, 0, 5_000)], rows, 5_001));

    assert!(yaml.contains("    moreActiveFiles: ≥2\n"), "yaml: {yaml}");
}

#[test]
fn a_tree_with_nothing_in_flight_or_not_built_yet_adds_nothing() {
    let between_files = yaml_for(view(
        vec![folder("Photos", 0, 1, 2)],
        vec![file("IMG_0001.jpg", 1, TreeRowState::Done, 3 * MB)],
        3,
    ));
    assert!(!between_files.contains("activeFolders"), "yaml: {between_files}");

    let scanning = yaml_for(TransferTreeView::not_ready(ProgressAxis::Bytes));
    assert!(!scanning.contains("activeFolders"), "yaml: {scanning}");
}
