//! Tests for the `cmdr://state` `operations:` section builder (the two-source
//! join of registry membership/status and the live progress cache).

use crate::file_system::write_operations::{
    ConflictId, LifecycleStatus, OperationSnapshot, OperationStatus, WriteConflictEvent, WriteOperationPhase,
    WriteOperationType,
};
use crate::mcp::resources::operations::{OperationRow, build_operations_yaml};

fn clash(conflict_id: u64, destination_size: Option<u64>) -> WriteConflictEvent {
    WriteConflictEvent {
        operation_id: "op-1".to_string(),
        conflict_id: ConflictId(conflict_id),
        source_path: "/src/photos/dsc-1.raw".to_string(),
        destination_path: "/dst/photos/dsc-1.raw".to_string(),
        source_size: Some(2_048),
        destination_size,
        source_modified: Some(1_700_000_000),
        destination_modified: Some(1_700_000_500),
        destination_is_newer: true,
        size_difference: None,
        source_is_directory: false,
        destination_is_directory: false,
    }
}

fn snapshot(id: &str, status: LifecycleStatus) -> OperationSnapshot {
    OperationSnapshot {
        operation_id: id.to_string(),
        operation_type: WriteOperationType::Copy,
        status,
        source: Some("/src/photos".to_string()),
        destination: Some("/dst/photos".to_string()),
        supports_rollback: true,
        error: None,
    }
}

fn progress(id: &str, bytes_done: u64, bytes_total: u64, files_done: usize, files_total: usize) -> OperationStatus {
    OperationStatus {
        operation_id: id.to_string(),
        operation_type: WriteOperationType::Copy,
        phase: WriteOperationPhase::Copying,
        is_running: true,
        current_file: Some("photo.jpg".to_string()),
        files_done,
        files_total,
        bytes_done,
        bytes_total,
        started_at: 10_000,
    }
}

#[test]
fn empty_operations_render_as_empty_list() {
    assert_eq!(build_operations_yaml(&[], 20_000), "operations: []\n");
}

#[test]
fn running_op_shows_status_progress_speed_and_eta() {
    // 4 s elapsed, 200 MB of 1 GB done → 50 MB/s, ETA (824 MB at 50 MB/s) = 16 s
    let mb = 1_024 * 1_024;
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: Some(progress("op-1", 200 * mb, 1_024 * mb, 3, 10)),
        pending_conflict: None,
    }];
    let yaml = build_operations_yaml(&rows, 14_000);
    assert!(yaml.contains("- operationId: op-1"), "yaml: {yaml}");
    assert!(yaml.contains("type: copy"), "yaml: {yaml}");
    assert!(yaml.contains("status: running"), "yaml: {yaml}");
    assert!(
        yaml.contains("progress: 200 MB / 1 GB (19%), 3/10 files"),
        "yaml: {yaml}"
    );
    assert!(yaml.contains("currentFile: \"photo.jpg\""), "yaml: {yaml}");
    assert!(yaml.contains("speed: 50 MB/s"), "yaml: {yaml}");
    assert!(yaml.contains("etaSeconds: 16"), "yaml: {yaml}");
    assert!(yaml.contains("elapsedSeconds: 4"), "yaml: {yaml}");
}

#[test]
fn paused_op_keeps_its_progress_but_reports_paused() {
    let mb = 1_024 * 1_024;
    let rows = vec![OperationRow {
        snapshot: snapshot("op-2", LifecycleStatus::Paused),
        progress: Some(progress("op-2", 100 * mb, 1_024 * mb, 1, 10)),
        pending_conflict: None,
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("- operationId: op-2"), "yaml: {yaml}");
    assert!(yaml.contains("status: paused"), "yaml: {yaml}");
    assert!(yaml.contains("progress: 100 MB / 1 GB"), "yaml: {yaml}");
}

#[test]
fn queued_op_has_status_but_no_progress_fields() {
    // A queued op has no status-cache entry, so progress is None: status only.
    let rows = vec![OperationRow {
        snapshot: snapshot("op-3", LifecycleStatus::Queued),
        progress: None,
        pending_conflict: None,
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("- operationId: op-3"), "yaml: {yaml}");
    assert!(yaml.contains("status: queued"), "yaml: {yaml}");
    assert!(
        !yaml.contains("progress:"),
        "queued op must have no progress line: {yaml}"
    );
    assert!(!yaml.contains("etaSeconds"), "yaml: {yaml}");
}

#[test]
fn a_running_paused_queued_mix_renders_every_row() {
    let mb = 1_024 * 1_024;
    let rows = vec![
        OperationRow {
            snapshot: snapshot("op-run", LifecycleStatus::Running),
            progress: Some(progress("op-run", 50 * mb, 100 * mb, 2, 4)),
            pending_conflict: None,
        },
        OperationRow {
            snapshot: snapshot("op-pause", LifecycleStatus::Paused),
            progress: Some(progress("op-pause", 10 * mb, 100 * mb, 1, 4)),
            pending_conflict: None,
        },
        OperationRow {
            snapshot: snapshot("op-queue", LifecycleStatus::Queued),
            progress: None,
            pending_conflict: None,
        },
    ];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("operationId: op-run"), "yaml: {yaml}");
    assert!(yaml.contains("operationId: op-pause"), "yaml: {yaml}");
    assert!(yaml.contains("operationId: op-queue"), "yaml: {yaml}");
    assert!(yaml.contains("status: running"), "yaml: {yaml}");
    assert!(yaml.contains("status: paused"), "yaml: {yaml}");
    assert!(yaml.contains("status: queued"), "yaml: {yaml}");
}

#[test]
fn an_operation_parked_on_a_clash_says_which_clash_and_how_to_answer_it() {
    // The reason this block exists: the row still says `running` and its
    // counters have stopped moving. Without the clash on the row an agent can
    // see a stuck operation and has no way to learn what it's waiting for, or
    // which id an answer has to name.
    let mb = 1_024 * 1_024;
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: Some(progress("op-1", 50 * mb, 100 * mb, 2, 4)),
        pending_conflict: Some(clash(3, Some(4_096))),
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("pendingConflict:"), "yaml: {yaml}");
    assert!(yaml.contains("conflictId: 3"), "yaml: {yaml}");
    // Verbatim: an agent choosing between skip and overwrite has to know which
    // file it is deciding about, and `redact_line` would leave it `<file>.raw`.
    assert!(yaml.contains("source: \"/src/photos/dsc-1.raw\""), "yaml: {yaml}");
    assert!(yaml.contains("destination: \"/dst/photos/dsc-1.raw\""), "yaml: {yaml}");
    assert!(yaml.contains("destinationIsNewer: true"), "yaml: {yaml}");
    assert!(yaml.contains("answerWith: resolve_conflict"), "yaml: {yaml}");
}

#[test]
fn a_size_the_backend_could_not_read_says_unknown_rather_than_zero() {
    // A folder outside the index has no destination size. Rendering `0 B` would
    // read as "the destination is empty, overwriting costs nothing".
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: None,
        pending_conflict: Some(clash(1, None)),
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("destinationSize: unknown"), "yaml: {yaml}");
    assert!(!yaml.contains("destinationSize: 0 B"), "yaml: {yaml}");
}

#[test]
fn an_operation_that_is_not_asking_anything_carries_no_conflict_block() {
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: None,
        pending_conflict: None,
    }];
    assert!(!build_operations_yaml(&rows, 12_000).contains("pendingConflict"));
}

#[test]
fn scanning_op_has_no_bogus_numbers() {
    // Totals unknown during scanning: no percent, no ETA, no "0 B / 0 B".
    let mut scanning = progress("op-scan", 0, 0, 0, 0);
    scanning.phase = WriteOperationPhase::Scanning;
    scanning.current_file = None;
    let rows = vec![OperationRow {
        snapshot: snapshot("op-scan", LifecycleStatus::Running),
        progress: Some(scanning),
        pending_conflict: None,
    }];
    let yaml = build_operations_yaml(&rows, 10_500);
    assert!(yaml.contains("progress: scanning"), "yaml: {yaml}");
    assert!(!yaml.contains("etaSeconds"), "yaml: {yaml}");
    assert!(!yaml.contains("speed"), "yaml: {yaml}");
    assert!(!yaml.contains("currentFile"), "yaml: {yaml}");
}
