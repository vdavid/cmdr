//! A volume delete reports each top-level source as it finishes (`write-source-item-done`), the
//! way the local walker does.
//!
//! Regression anchor: the volume walker emitted nothing per source, so a caller tracking
//! outcomes per source (an approved AI suggestion's ops, `proposal_ops.status`) saw every file
//! deleted and every op still `pending`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::super::event_sinks::CollectorEventSink;
use super::super::testing::TestOperations;
use super::super::types::{SourceItemOutcome, WriteOperationConfig};
use super::volume_walker::delete_volume_files_with_progress_inner;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{InMemoryVolume, Volume};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_delete_reports_each_top_level_source_once_it_is_gone() {
    let volume = Arc::new(InMemoryVolume::new("naspi"));
    volume.create_directory(Path::new("/exports")).await.expect("dir");
    volume
        .create_file(Path::new("/exports/a.key"), b"aa")
        .await
        .expect("file in folder");
    volume
        .create_file(Path::new("/exports/b.key"), b"bb")
        .await
        .expect("file in folder");
    volume.create_file(Path::new("/old.iso"), b"iso").await.expect("file");
    let engine = TestOperations::new();
    let volume_id = format!("volume-source-done-{}", std::process::id());
    engine.volumes().register(&volume_id, volume.clone() as Arc<dyn Volume>);
    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));

    let result = delete_volume_files_with_progress_inner(
        volume.clone() as Arc<dyn Volume>,
        &volume_id,
        events.as_ref(),
        "op-volume-source-done",
        &state,
        &[PathBuf::from("/exports"), PathBuf::from("/old.iso")],
        &WriteOperationConfig::default(),
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    let mut done: Vec<(String, SourceItemOutcome, bool)> = events
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .map(|event| (event.source_path.clone(), event.outcome, event.source_removed))
        .collect();
    done.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        done,
        vec![
            ("/exports".to_string(), SourceItemOutcome::Done, true),
            ("/old.iso".to_string(), SourceItemOutcome::Done, true),
        ],
        "one event per top-level source, the folder only after its own entry is gone"
    );
}
