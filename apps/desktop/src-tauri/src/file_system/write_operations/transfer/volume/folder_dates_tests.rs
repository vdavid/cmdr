//! A copied folder keeps its source folder's date, through both drivers.
//!
//! `InMemoryVolume` moves a folder's date when an entry lands in it, as every
//! real store does, so a folder dated BEFORE its contents land reads "now" here
//! and fails these cells: the stamp has to come after the last child (post-order).
//! The contract: `DETAILS.md` § "Copies keep the source's date".

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::copy_volumes_with_progress;
use super::faulty_volume::{FaultyOp, FaultyVolume};
use crate::file_system::volume::{InMemoryVolume, Volume, VolumeError};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::WriteOperationState;
use crate::file_system::write_operations::types::VolumeCopyConfig;

/// The copied folder's date: 2021-01-29 08:30:15 UTC.
const FOLDER_DATE: u64 = 1_611_909_015;
/// The folder inside it, dated apart so a stamp on the wrong level can't pass.
const INNER_FOLDER_DATE: u64 = 1_577_836_800;

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

/// `/<name>` holding a file and `inner/` (with a file of its own), both folders
/// dated after their contents landed, the way a real old folder lists.
async fn seed_dated_folder(source: &InMemoryVolume, name: &str) -> PathBuf {
    let root = PathBuf::from(format!("/{name}"));
    let inner = root.join("inner");
    source.create_directory(&root).await.unwrap();
    source.create_directory(&inner).await.unwrap();
    source.create_file(&root.join("photo.bin"), b"photo").await.unwrap();
    source.create_file(&inner.join("deep.bin"), b"deep").await.unwrap();
    source.set_modified_at(&inner, Some(INNER_FOLDER_DATE));
    source.set_modified_at(&root, Some(FOLDER_DATE));
    root
}

fn volumes() -> (Arc<InMemoryVolume>, Arc<InMemoryVolume>) {
    (
        Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000)),
        Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000)),
    )
}

async fn listed_date(volume: &InMemoryVolume, path: &str) -> Option<u64> {
    volume
        .get_metadata(Path::new(path))
        .await
        .unwrap_or_else(|e| panic!("{path} must exist, got {e:?}"))
        .modified_at
}

async fn copy(
    op: &str,
    source: &Arc<InMemoryVolume>,
    paths: &[PathBuf],
    dest: Arc<dyn Volume>,
) -> Result<(), crate::file_system::write_operations::types::WriteOperationError> {
    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op,
        &make_state(),
        Arc::clone(source) as Arc<dyn Volume>,
        paths,
        dest,
        Path::new("/inbox"),
        &VolumeCopyConfig::default(),
    )
    .await
    .map(|_| ())
    .map_err(|failure| failure.error)
}

/// One folder source takes the serial driver.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copied_folder_keeps_its_date_and_so_does_the_folder_inside_it() {
    let (source, dest) = volumes();
    dest.create_directory(Path::new("/inbox")).await.unwrap();
    let album = seed_dated_folder(&source, "album").await;

    copy(
        "op-folder-dates-serial",
        &source,
        &[album],
        Arc::clone(&dest) as Arc<dyn Volume>,
    )
    .await
    .expect("the copy succeeds");

    assert_eq!(listed_date(&dest, "/inbox/album").await, Some(FOLDER_DATE));
    assert_eq!(listed_date(&dest, "/inbox/album/inner").await, Some(INNER_FOLDER_DATE));
}

/// Three sources and a remote peer take the concurrent driver.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn copied_folders_keep_their_dates_through_the_concurrent_driver() {
    let (source, dest) = volumes();
    dest.create_directory(Path::new("/inbox")).await.unwrap();
    let mut sources = Vec::new();
    for name in ["one", "two", "three"] {
        sources.push(seed_dated_folder(&source, name).await);
    }

    copy(
        "op-folder-dates-concurrent",
        &source,
        &sources,
        Arc::clone(&dest) as Arc<dyn Volume>,
    )
    .await
    .expect("the copy succeeds");

    for name in ["one", "two", "three"] {
        assert_eq!(listed_date(&dest, &format!("/inbox/{name}")).await, Some(FOLDER_DATE));
        assert_eq!(
            listed_date(&dest, &format!("/inbox/{name}/inner")).await,
            Some(INNER_FOLDER_DATE)
        );
    }
}

/// A merge into a folder that was already there leaves that folder's date to
/// the store: it's the user's folder, and the copy only added to it. A folder
/// the merge created inside it is the copy's own and keeps its source date.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_merge_leaves_the_existing_folders_date_alone() {
    let (source, dest) = volumes();
    dest.create_directory(Path::new("/inbox")).await.unwrap();
    dest.create_directory(Path::new("/inbox/album")).await.unwrap();
    let album = seed_dated_folder(&source, "album").await;

    copy(
        "op-folder-dates-merge",
        &source,
        &[album],
        Arc::clone(&dest) as Arc<dyn Volume>,
    )
    .await
    .expect("the merge succeeds");

    assert_ne!(listed_date(&dest, "/inbox/album").await, Some(FOLDER_DATE));
    assert_eq!(listed_date(&dest, "/inbox/album/inner").await, Some(INNER_FOLDER_DATE));
}

/// A copy that fails partway dates no folder: `album` holds only part of its
/// contents and must not claim the date of the whole. `inner` (listed first, so
/// it filled completely before `photo.bin` failed) stays undated too, since
/// the stamp runs only once the whole subtree landed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_that_fails_partway_dates_no_folder() {
    let (source, dest_inner) = volumes();
    dest_inner.create_directory(Path::new("/inbox")).await.unwrap();
    let album = seed_dated_folder(&source, "album").await;
    let dest = FaultyVolume::wrapping(Arc::clone(&dest_inner))
        .failing_call(
            FaultyOp::WriteFromStream,
            2,
            VolumeError::IoError {
                message: "the disk gave up".to_string(),
                raw_os_error: None,
            },
        )
        .arc();

    let result = copy(
        "op-folder-dates-failed",
        &source,
        &[album],
        Arc::clone(&dest) as Arc<dyn Volume>,
    )
    .await;

    assert!(
        dest.fault_fired(FaultyOp::WriteFromStream),
        "the injected write failure never fired, so this cell proves nothing"
    );
    assert!(result.is_err(), "the second file's write failing fails the copy");
    for folder in ["/inbox/album", "/inbox/album/inner"] {
        if dest_inner.exists(Path::new(folder)).await {
            let listed = listed_date(&dest_inner, folder).await;
            assert!(
                listed != Some(FOLDER_DATE) && listed != Some(INNER_FOLDER_DATE),
                "{folder}: only part of its source landed, so it must not carry a source date; it lists {listed:?}"
            );
        }
    }
}
