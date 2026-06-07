//! Tests for the `cmdr://state` `transfers:` section builder.

use crate::file_system::write_operations::{OperationStatus, WriteOperationPhase, WriteOperationType};
use crate::mcp::resources::transfers::build_transfers_yaml;

fn op(bytes_done: u64, bytes_total: u64, files_done: usize, files_total: usize) -> OperationStatus {
    OperationStatus {
        operation_id: "op-1".to_string(),
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
fn test_empty_transfers() {
    assert_eq!(build_transfers_yaml(&[], 20_000), "transfers: []\n");
}

#[test]
fn test_transfer_progress_speed_and_eta() {
    // 4 s elapsed, 200 MB of 1 GB done → 50 MB/s, ETA (824 MB at 50 MB/s) = 16 s
    let mb = 1_024 * 1_024;
    let yaml = build_transfers_yaml(&[op(200 * mb, 1_024 * mb, 3, 10)], 14_000);
    assert!(yaml.contains("- id: op-1"), "yaml: {yaml}");
    assert!(yaml.contains("type: copy"), "yaml: {yaml}");
    assert!(yaml.contains("phase: copying"), "yaml: {yaml}");
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
fn test_transfer_scanning_phase_has_no_bogus_numbers() {
    // Totals unknown during scanning: no percent, no ETA, no "0 B / 0 B".
    let mut scanning = op(0, 0, 0, 0);
    scanning.phase = WriteOperationPhase::Scanning;
    scanning.current_file = None;
    let yaml = build_transfers_yaml(&[scanning], 10_500);
    assert!(yaml.contains("phase: scanning"), "yaml: {yaml}");
    assert!(yaml.contains("progress: scanning"), "yaml: {yaml}");
    assert!(!yaml.contains("etaSeconds"), "yaml: {yaml}");
    assert!(!yaml.contains("speed"), "yaml: {yaml}");
    assert!(!yaml.contains("currentFile"), "yaml: {yaml}");
}

#[test]
fn test_transfer_partial_counters() {
    // Bytes known but file total still unknown (cross-volume scan mid-flight).
    let yaml = build_transfers_yaml(&[op(5 * 1_024 * 1_024, 0, 2, 0)], 12_000);
    assert!(yaml.contains("progress: 5 MB so far, 2 files so far"), "yaml: {yaml}");
}
