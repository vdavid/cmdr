//! Hardlink-aware progress accounting for local-FS delete.
//!
//! Pins the invariant that `bytes_done` reported during the active phase
//! never exceeds `bytes_total` reported during the scan. Before this work,
//! scan dedup'd hardlinks by inode (`scan_result.total_bytes` counted each
//! inode once) while the delete walker summed `file_info.size` per entry, so
//! a hardlink-heavy tree (cargo `target/`, sccache caches, deduplicated
//! backups) overshot the progress bar — 81.6 GB delete numerator against a
//! 59.84 GB scan denominator on a real-world repro. See
//! `walker.rs::delete_files_with_progress_inner` for the active site.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::super::state::WriteOperationState;
use super::super::test_support::TestOperationGuard;
use super::super::types::{CollectorEventSink, WriteOperationConfig};
use super::walker::delete_files_with_progress_inner;

/// Builds a unique-per-test scratch directory under `$TMPDIR`.
fn create_temp_dir(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir().join(format!("cmdr_hardlink_progress_{name}"));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
    temp_dir
}

fn cleanup(path: &PathBuf) {
    let _ = fs::remove_dir_all(path);
}

fn unique_op_id(name: &str) -> String {
    format!(
        "test-hardlink-delete-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

/// Delete a tree where one file is hardlinked twice. Scan dedupes hardlinks
/// (counts the inode once), so the active-phase numerator must also dedupe
/// or the progress bar overshoots. The contract is: the terminal
/// `WriteCompleteEvent.bytes_processed` matches the scan-phase
/// `WriteProgressEvent.bytes_total` seen at the start of the active phase.
#[test]
fn delete_hardlinked_files_does_not_overshoot_progress() {
    let dir = create_temp_dir("delete_hardlinks");

    // 1 KiB of unique payload + a 4 KiB regular file. Sizes are chosen so the
    // overshoot is unambiguous: an un-dedup'd numerator would be 1024*3 + 4096
    // = 7168, while the dedup'd scan denominator is 1024 + 4096 = 5120.
    let payload = vec![0u8; 1024];
    let original = dir.join("original");
    fs::write(&original, &payload).unwrap();
    fs::hard_link(&original, dir.join("hardlink_a")).unwrap();
    fs::hard_link(&original, dir.join("hardlink_b")).unwrap();
    fs::write(dir.join("standalone"), vec![0u8; 4096]).unwrap();

    let op_id = unique_op_id("overshoot");
    let op = TestOperationGuard::register_as(
        op_id.clone(),
        Arc::new(WriteOperationState::new(Duration::from_millis(10))),
    );

    let sink = CollectorEventSink::new();
    let sources = vec![dir.clone()];
    let config = WriteOperationConfig::default();

    let result = delete_files_with_progress_inner(&sink, &op_id, op.state(), &sources, &config);

    assert!(result.is_ok(), "delete must succeed; got {result:?}");

    let progress = sink.progress.lock().unwrap();
    let complete = sink.complete.lock().unwrap();

    // The active-phase denominator the FE renders.
    let bytes_total = progress
        .iter()
        .find(|e| matches!(e.phase, super::super::types::WriteOperationPhase::Deleting))
        .map(|e| e.bytes_total)
        .expect("at least one Deleting-phase progress event");

    let bytes_processed = complete
        .first()
        .map(|e| e.bytes_processed)
        .expect("write-complete must fire");

    let msg = format!(
        "delete numerator overshoots scan denominator on hardlink-heavy trees: \
         scan reported {bytes_total} byte(s), delete reported {bytes_processed} byte(s). \
         Same set, same files; the asymmetry is the hardlink dedup."
    );
    assert_eq!(bytes_processed, bytes_total, "{msg}");

    // Every emitted progress event must respect the same invariant — not
    // just the terminal one. Catches "bar overshoots mid-delete then snaps
    // back to 100% at the end" regressions.
    for event in progress.iter() {
        assert!(
            event.bytes_done <= event.bytes_total,
            "mid-flight progress event overshoots: bytes_done={} > bytes_total={} \
             (phase={:?})",
            event.bytes_done,
            event.bytes_total,
            event.phase
        );
    }

    cleanup(&dir);
}
