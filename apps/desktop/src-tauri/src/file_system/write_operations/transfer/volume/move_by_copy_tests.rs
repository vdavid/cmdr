//! A same-volume move on a volume whose renames copy (an object store's
//! folders and big files, `Volume::rename_work`).
//!
//! The move must never call `rename` for such an entry: it runs through the
//! copy-then-delete engine on the one volume (server-side copy where the
//! backend has one), and deletes the sources in batches only after
//! everything copied. `InMemoryVolume::with_renames_by_copy` refuses `rename`
//! outright, so a move that forgot to route fails loudly here.
//!
//! The rename-as-a-move cells give a source a NAME at the destination
//! (`target_names.rs`): F2 on an S3 folder is a move into its own parent under
//! the new name.

use super::super::move_same::move_within_same_volume_with_progress;
use super::test_support::make_state_with_interval_ms;
use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::{OperationIntent, WriteOperationState};
use crate::file_system::write_operations::target_names::TargetNames;
use crate::file_system::write_operations::types::WriteOperationError;

fn store() -> Arc<InMemoryVolume> {
    Arc::new(
        InMemoryVolume::new("Bucket")
            .with_whole_publish()
            .with_renames_by_copy()
            .with_space_info(10_000_000, 10_000_000),
    )
}

async fn read(volume: &Arc<InMemoryVolume>, path: &str) -> Vec<u8> {
    let mut stream = volume.open_read_stream(Path::new(path)).await.unwrap();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend(chunk.unwrap());
    }
    bytes
}

async fn seed_tree(volume: &Arc<InMemoryVolume>, root: &str) {
    volume.create_directory(Path::new(root)).await.unwrap();
    volume.create_directory(&Path::new(root).join("sub")).await.unwrap();
    volume
        .create_file(&Path::new(root).join("a.txt"), b"alpha")
        .await
        .unwrap();
    volume
        .create_file(&Path::new(root).join("b.txt"), b"bravo")
        .await
        .unwrap();
    volume
        .create_file(&Path::new(root).join("sub/c.txt"), b"charlie")
        .await
        .unwrap();
}

fn state_with(target_names: TargetNames) -> Arc<WriteOperationState> {
    let state = make_state_with_interval_ms(0);
    Arc::new(
        Arc::try_unwrap(state)
            .unwrap_or_else(|_| panic!("a fresh state has one owner"))
            .with_target_names(target_names),
    )
}

async fn run_move(
    volume: &Arc<InMemoryVolume>,
    state: &Arc<WriteOperationState>,
    sources: &[&str],
    dest: &str,
) -> (Result<(), WriteOperationError>, Arc<CollectorEventSink>) {
    let events = Arc::new(CollectorEventSink::new());
    let as_volume: Arc<dyn Volume> = volume.clone();
    let sources: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-move-by-copy",
        state,
        as_volume,
        &sources,
        Path::new(dest),
        &VolumeCopyConfig {
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        },
    )
    .await;
    (result, events)
}

/// ❗ A folder that can't rename in one call moves by copy, then delete: every
/// file arrives, the source goes, and `rename` is never asked.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_that_renames_by_copy_moves_by_copy_then_delete() {
    let volume = store();
    seed_tree(&volume, "/src/tree").await;
    volume.create_directory(Path::new("/dst")).await.unwrap();
    let state = state_with(TargetNames::default());

    let (result, events) = run_move(&volume, &state, &["/src/tree"], "/dst").await;

    assert!(result.is_ok(), "got {result:?}");
    assert_eq!(read(&volume, "/dst/tree/a.txt").await, b"alpha");
    assert_eq!(read(&volume, "/dst/tree/b.txt").await, b"bravo");
    assert_eq!(read(&volume, "/dst/tree/sub/c.txt").await, b"charlie");
    assert!(!volume.exists(Path::new("/src/tree")).await, "the source is gone");
    assert!(!events.complete.lock().unwrap().is_empty(), "the move completed");
}

/// The source sweep deletes in batches (S3's `DeleteObjects`), one per folder
/// level it clears, never a request per file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_source_sweep_deletes_files_in_batches() {
    let volume = store();
    volume.create_directory(Path::new("/src/many")).await.unwrap();
    for n in 0..30 {
        volume
            .create_file(&PathBuf::from(format!("/src/many/f{n:02}.txt")), b"x")
            .await
            .unwrap();
    }
    let state = state_with(TargetNames::default());

    let (result, _) = run_move(&volume, &state, &["/src/many"], "/dst").await;

    assert!(result.is_ok(), "got {result:?}");
    assert_eq!(volume.delete_batches(), vec![30], "one batch for the level's 30 files");
    assert!(!volume.exists(Path::new("/src/many")).await);
}

/// F2 on such a folder: a move into its own parent under the new name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_that_copies_moves_the_folder_to_its_new_name() {
    let volume = store();
    seed_tree(&volume, "/a/foo").await;
    let state = state_with(TargetNames::new([(PathBuf::from("/a/foo"), "bar".to_string())]).unwrap());

    let (result, _) = run_move(&volume, &state, &["/a/foo"], "/a").await;

    assert!(result.is_ok(), "got {result:?}");
    assert_eq!(read(&volume, "/a/bar/a.txt").await, b"alpha");
    assert_eq!(read(&volume, "/a/bar/sub/c.txt").await, b"charlie");
    assert!(!volume.exists(Path::new("/a/foo")).await, "the old name is gone");
}

/// The same target names work on a volume whose renames are one call: the
/// rename-merge path renames straight to the new name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_target_name_renames_in_one_call_where_the_volume_can() {
    let volume = Arc::new(InMemoryVolume::new("Disk").with_space_info(10_000_000, 10_000_000));
    seed_tree(&volume, "/a/foo").await;
    let state = state_with(TargetNames::new([(PathBuf::from("/a/foo"), "bar".to_string())]).unwrap());

    let (result, _) = run_move(&volume, &state, &["/a/foo"], "/a").await;

    assert!(result.is_ok(), "got {result:?}");
    assert_eq!(read(&volume, "/a/bar/b.txt").await, b"bravo");
    assert!(!volume.exists(Path::new("/a/foo")).await);
}

/// ❗ A stopped move leaves every source where it was: nothing is deleted
/// before everything copied.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopped_move_by_copy_keeps_every_source() {
    let volume = store();
    seed_tree(&volume, "/src/tree").await;
    let state = state_with(TargetNames::default());
    state.intent.store(OperationIntent::Stopped as u8, Ordering::SeqCst);

    let (result, _) = run_move(&volume, &state, &["/src/tree"], "/dst").await;

    assert!(
        matches!(result, Err(WriteOperationError::Cancelled { .. })),
        "got {result:?}"
    );
    assert_eq!(read(&volume, "/src/tree/a.txt").await, b"alpha");
    assert_eq!(read(&volume, "/src/tree/sub/c.txt").await, b"charlie");
    assert!(volume.delete_batches().is_empty(), "nothing was deleted");
}
