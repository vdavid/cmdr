//! A cross-volume copy or move that leaves Mac-only files out (#314), over
//! `InMemoryVolume`, the double every backend's copy walk is held to: the merge
//! walk never lands a `.DS_Store` or `._*`, and a move's sweep takes them with
//! the folders that moved.

use super::test_support::make_volumes;
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::mac_only_files::MacOnlyFiles;
use std::time::Duration;

async fn seed(source: &Arc<dyn Volume>) {
    source.create_directory(Path::new("/work")).await.unwrap();
    source.create_directory(Path::new("/work/notes")).await.unwrap();
    source.create_directory(Path::new("/work/only-metadata")).await.unwrap();
    source
        .create_file(Path::new("/work/.DS_Store"), &[0u8; 64])
        .await
        .unwrap();
    source
        .create_file(Path::new("/work/._report.txt"), &[0u8; 32])
        .await
        .unwrap();
    source.create_file(Path::new("/work/report.txt"), b"one").await.unwrap();
    source
        .create_file(Path::new("/work/notes/.DS_Store"), &[0u8; 64])
        .await
        .unwrap();
    source
        .create_file(Path::new("/work/notes/todo.md"), b"two")
        .await
        .unwrap();
    source
        .create_file(Path::new("/work/only-metadata/.DS_Store"), &[0u8; 64])
        .await
        .unwrap();
}

fn skipping_state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(Duration::from_millis(0))
            .leaving_out(MacOnlyFiles::Skip),
    )
}

fn skipping_config() -> VolumeCopyConfig {
    VolumeCopyConfig {
        mac_only_files: MacOnlyFiles::Skip,
        ..VolumeCopyConfig::default()
    }
}

async fn assert_lands_without_mac_only_files(dest: &Arc<dyn Volume>) {
    assert!(dest.exists(Path::new("/work/report.txt")).await);
    assert!(dest.exists(Path::new("/work/notes/todo.md")).await);
    assert!(
        dest.exists(Path::new("/work/only-metadata")).await,
        "the folder is the person's"
    );
    for gone in [
        "/work/.DS_Store",
        "/work/._report.txt",
        "/work/notes/.DS_Store",
        "/work/only-metadata/.DS_Store",
    ] {
        assert!(!dest.exists(Path::new(gone)).await, "{gone} reached the destination");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_copy_that_skips_mac_only_files_writes_and_counts_none_of_them() {
    let engine = TestOperations::new();
    let (source, dest) = make_volumes();
    seed(&source).await;

    let events = Arc::new(CollectorEventSink::new());
    super::super::copy_volumes_with_progress(
        events.clone(),
        "op-volume-copy-skip-mac",
        &skipping_state(&engine),
        Arc::clone(&source),
        &[PathBuf::from("/work")],
        Arc::clone(&dest),
        Path::new("/"),
        &skipping_config(),
    )
    .await
    .expect("the copy");

    assert_lands_without_mac_only_files(&dest).await;
    let bytes_processed = events.complete.lock_ignore_poison()[0].bytes_processed;
    assert_eq!(bytes_processed, 6, "the totals leave them out too");
    assert!(
        source.exists(Path::new("/work/.DS_Store")).await,
        "a copy never touches the source"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_move_that_skips_mac_only_files_leaves_no_husk_in_the_source() {
    let engine = TestOperations::new();
    let (source, dest) = make_volumes();
    seed(&source).await;

    let events = Arc::new(CollectorEventSink::new());
    move_volumes_with_progress(
        events.clone(),
        "op-volume-move-skip-mac",
        &skipping_state(&engine),
        Arc::clone(&source),
        &[PathBuf::from("/work")],
        Arc::clone(&dest),
        Path::new("/"),
        &skipping_config(),
    )
    .await
    .expect("the move");

    assert_lands_without_mac_only_files(&dest).await;
    assert!(
        !source.exists(Path::new("/work")).await,
        "the folders moved, so their metadata went with them"
    );
}

/// A folder that stays for a real reason keeps its metadata.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_move_keeps_the_metadata_of_a_folder_that_stays() {
    let engine = TestOperations::new();
    let (source, dest) = make_volumes();
    seed(&source).await;
    // Already at the destination, and Skip keeps the source's copy, so its
    // folder stays.
    dest.create_directory(Path::new("/work")).await.unwrap();
    dest.create_directory(Path::new("/work/notes")).await.unwrap();
    dest.create_file(Path::new("/work/notes/todo.md"), b"old")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: crate::types::ConflictResolution::Skip,
        ..skipping_config()
    };
    move_volumes_with_progress(
        events.clone(),
        "op-volume-move-kept",
        &skipping_state(&engine),
        Arc::clone(&source),
        &[PathBuf::from("/work")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await
    .expect("the move");

    assert!(
        source.exists(Path::new("/work/notes/todo.md")).await,
        "the Skip kept it"
    );
    assert!(
        source.exists(Path::new("/work/notes/.DS_Store")).await,
        "so its folder keeps its metadata"
    );
    assert!(
        !source.exists(Path::new("/work/only-metadata")).await,
        "a folder that went took its metadata along"
    );
}
