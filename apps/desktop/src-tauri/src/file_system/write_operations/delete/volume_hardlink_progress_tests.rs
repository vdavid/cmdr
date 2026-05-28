//! Hardlink-aware progress accounting for the volume-aware delete walker.
//!
//! The local-FS walker already dedupes hardlinks by inode (see
//! `hardlink_progress_tests.rs`), but `target/` on an external SSD or a
//! Time Machine backup on an external HFS+ drive routes through
//! `delete_volume_files_with_progress_inner` (via `LocalPosixVolume` because
//! `volume_id != "root"`), where neither the scan nor the delete loop
//! consults inode info — both ends are inflated by hardlink count. Bar fills
//! 0→100% smoothly, but the absolute byte numbers are wrong.
//!
//! Pins the same invariant as the local-FS test: terminal `bytes_processed`
//! must equal the dedup'd source-side total.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::super::state::{WRITE_OPERATION_STATE, WriteOperationState};
use super::super::types::{CollectorEventSink, WriteOperationConfig};
use super::walker::delete_volume_files_with_progress_inner;
use crate::file_system::get_volume_manager;
use crate::file_system::volume::{LocalPosixVolume, Volume};

fn create_temp_root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cmdr_volume_hardlink_{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp root");
    dir
}

fn unique_op_id(name: &str) -> String {
    format!(
        "test-vol-hardlink-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

fn install_state(op_id: &str) -> Arc<WriteOperationState> {
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(10)));
    WRITE_OPERATION_STATE
        .write()
        .unwrap()
        .insert(op_id.to_string(), Arc::clone(&state));
    state
}

fn uninstall_state(op_id: &str) {
    WRITE_OPERATION_STATE.write().unwrap().remove(op_id);
}

/// Build a tree where one inode is shared by three hardlinks plus one
/// standalone file. Sizes chosen so the overshoot would be unambiguous:
/// un-dedup'd total = 3 * 1024 + 4096 = 7168; dedup'd = 1024 + 4096 = 5120.
fn build_hardlink_tree(root: &Path, subdir: &str) -> PathBuf {
    let dir = root.join(subdir);
    fs::create_dir_all(&dir).unwrap();
    let payload = vec![0u8; 1024];
    let original = dir.join("original");
    fs::write(&original, &payload).unwrap();
    fs::hard_link(&original, dir.join("hardlink_a")).unwrap();
    fs::hard_link(&original, dir.join("hardlink_b")).unwrap();
    fs::write(dir.join("standalone"), vec![0u8; 4096]).unwrap();
    dir
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_delete_hardlinked_files_reports_dedup_d_bytes() {
    let root = create_temp_root("delete_dedup");
    let payload_dir = build_hardlink_tree(&root, "to_delete");

    let volume_id = format!("vol-hardlink-delete-{}", unique_op_id("vid"));
    let volume = Arc::new(LocalPosixVolume::new(
        "HardlinkVol",
        root.to_str().expect("temp dir is utf8"),
    ));
    get_volume_manager().register(&volume_id, Arc::clone(&volume) as Arc<dyn Volume>);

    let op_id = unique_op_id("op");
    let state = install_state(&op_id);
    let sink = CollectorEventSink::new();
    // Volume paths are anchored at the volume root. Source = `/to_delete`.
    let sources = vec![PathBuf::from("/to_delete")];
    let config = WriteOperationConfig::default();

    let result = delete_volume_files_with_progress_inner(
        Arc::clone(&volume) as Arc<dyn Volume>,
        &volume_id,
        &sink,
        &op_id,
        &state,
        &sources,
        &config,
    )
    .await;

    get_volume_manager().unregister(&volume_id);
    uninstall_state(&op_id);

    assert!(result.is_ok(), "volume delete must succeed; got {result:?}");

    let bytes_processed = sink
        .complete
        .lock()
        .unwrap()
        .first()
        .map(|e| e.bytes_processed)
        .expect("write-complete must fire");

    let expected_dedup = 1024 + 4096;
    let msg = format!(
        "volume delete sums entry.size per hardlink — expected dedup'd {expected_dedup} \
         byte(s) processed, got {bytes_processed} byte(s) (un-dedup'd is 7168). \
         Cargo target/ on an external SSD would overstate the freed-bytes claim by ~3x."
    );
    assert_eq!(bytes_processed, expected_dedup, "{msg}");

    // Final progress event's denominator must also be dedup'd; mid-flight
    // events must never exceed their reported denominator.
    let progress = sink.progress.lock().unwrap();
    for event in progress.iter() {
        assert!(
            event.bytes_done <= event.bytes_total,
            "mid-flight overshoot: bytes_done={} > bytes_total={} (phase={:?})",
            event.bytes_done,
            event.bytes_total,
            event.phase
        );
    }
    let final_total = progress
        .iter()
        .last()
        .map(|e| e.bytes_total)
        .expect("at least one progress event");
    assert_eq!(
        final_total, expected_dedup,
        "scan-phase denominator inflated by hardlinks"
    );

    drop(payload_dir);
    let _ = fs::remove_dir_all(&root);
}
