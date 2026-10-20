//! A copied folder keeps its source folder's dates, through both drivers.
//!
//! `InMemoryVolume` moves a folder's date when an entry lands in it, as every
//! real store does, so a folder dated BEFORE its contents land reads "now" here
//! and fails these cells: the stamp has to come after the last child (post-order).
//! The contract: `DETAILS.md` § "Copies keep the source's metadata".

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::copy_volumes_with_progress;
use super::faulty_volume::{FaultyOp, FaultyVolume};
use crate::event_sinks::CollectorEventSink;
use crate::outage_slot::OutagePolicy;
use crate::state::WriteOperationState;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::OutageVolume;
use crate::types::VolumeCopyConfig;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};

/// The copied folder's date: 2021-01-29 08:30:15 UTC.
const FOLDER_DATE: u64 = 1_611_909_015;
/// The folder inside it, dated apart so a stamp on the wrong level can't pass.
const INNER_FOLDER_DATE: u64 = 1_577_836_800;

fn make_state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::from_millis(50)))
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
    engine: &TestOperations,
    op: &str,
    source: &Arc<InMemoryVolume>,
    paths: &[PathBuf],
    dest: Arc<dyn Volume>,
) -> Result<(), crate::types::WriteOperationError> {
    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op,
        &make_state(engine),
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
    let engine = TestOperations::new();
    let (source, dest) = volumes();
    dest.create_directory(Path::new("/inbox")).await.unwrap();
    let album = seed_dated_folder(&source, "album").await;

    copy(
        &engine,
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
    let engine = TestOperations::new();
    let (source, dest) = volumes();
    dest.create_directory(Path::new("/inbox")).await.unwrap();
    let mut sources = Vec::new();
    for name in ["one", "two", "three"] {
        sources.push(seed_dated_folder(&source, name).await);
    }

    copy(
        &engine,
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
    let engine = TestOperations::new();
    let (source, dest) = volumes();
    dest.create_directory(Path::new("/inbox")).await.unwrap();
    dest.create_directory(Path::new("/inbox/album")).await.unwrap();
    let album = seed_dated_folder(&source, "album").await;

    copy(
        &engine,
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
    let engine = TestOperations::new();
    let (source, dest_inner) = volumes();
    dest_inner.create_directory(Path::new("/inbox")).await.unwrap();
    let album = seed_dated_folder(&source, "album").await;
    let dest = FaultyVolume::wrapping(Arc::clone(&dest_inner))
        .failing_call(
            FaultyOp::WriteFromStream,
            2,
            VolumeError::IoError {
                message: "the disk gave up".into(),
                raw_os_error: None,
            },
        )
        .arc();

    let result = copy(
        &engine,
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

/// The creation dates ride the same pass: every folder the copy created is born
/// when its source folder was, through both drivers, and a top-level folder's
/// comes from the scan's stat like its modification date.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn copied_folders_keep_their_creation_dates_through_both_drivers() {
    let engine = TestOperations::new();
    const BORN: u64 = 1_425_168_000; // 2015-03-01
    const INNER_BORN: u64 = 1_451_606_400; // 2016-01-01
    let born = |secs: u64| cmdr_fs::metadata::CarriedMetadata {
        created: Some(std::time::UNIX_EPOCH + Duration::from_secs(secs)),
        ..Default::default()
    };
    for (op, names) in [
        ("op-folder-birth-serial", vec!["solo"]),
        ("op-folder-birth-concurrent", vec!["one", "two", "three"]),
    ] {
        let (source, dest) = volumes();
        dest.create_directory(Path::new("/inbox")).await.unwrap();
        let mut sources = Vec::new();
        for name in &names {
            let root = seed_dated_folder(&source, name).await;
            source.set_carried_metadata(&root.join("inner"), born(INNER_BORN));
            source.set_carried_metadata(&root, born(BORN));
            sources.push(root);
        }

        copy(&engine, op, &source, &sources, Arc::clone(&dest) as Arc<dyn Volume>)
            .await
            .expect("the copy succeeds");

        for name in &names {
            let created = |path: String| {
                let dest = Arc::clone(&dest);
                async move { dest.get_metadata(Path::new(&path)).await.unwrap().created_at }
            };
            assert_eq!(
                created(format!("/inbox/{name}")).await,
                Some(BORN),
                "{op}: /inbox/{name}"
            );
            assert_eq!(
                created(format!("/inbox/{name}/inner")).await,
                Some(INNER_BORN),
                "{op}: /inbox/{name}/inner"
            );
        }
    }
}

/// The outage wait's cadence at test speed.
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

/// A copy onto a share whose session the cell can take away, with the outage
/// wait tuned to test speed. Answers the share and the copy's result.
async fn copy_onto_a_share(
    engine: &TestOperations,
    op: &str,
    arm: impl FnOnce(&OutageVolume),
) -> (
    Arc<InMemoryVolume>,
    Arc<OutageVolume>,
    Result<(), crate::types::WriteOperationError>,
) {
    let (source, dest_inner) = volumes();
    dest_inner.create_directory(Path::new("/inbox")).await.unwrap();
    let album = seed_dated_folder(&source, "album").await;
    let share = OutageVolume::wrap(Arc::clone(&dest_inner), op);
    arm(&share);
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_outage_policy(OutagePolicy {
                budget: Duration::from_secs(30),
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    );
    let registered = engine.register_state(op, Arc::clone(&state));
    let waiting = Arc::clone(&state);
    let coming_back = Arc::clone(&share);
    tokio::spawn(async move {
        wait_until_async(Duration::from_secs(10), "the copy to wait for the share", || {
            waiting.outage.waiting_for().is_some()
        })
        .await;
        coming_back.become_reachable();
    });
    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        registered.id().as_str(),
        &state,
        Arc::clone(&source) as Arc<dyn Volume>,
        &[album],
        Arc::clone(&share) as Arc<dyn Volume>,
        Path::new("/inbox"),
        &VolumeCopyConfig::default(),
    )
    .await
    .map(|_| ())
    .map_err(|failure| failure.error);
    (dest_inner, share, result)
}

/// The share drops as the inner folder's dates go on. A folder's stamp has no
/// error to classify (a backend reports the drop as a refused class), so the
/// share's own state says it went away: the copy waits for it and stamps again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_share_that_drops_as_a_folder_is_dated_is_waited_for_and_dated_again() {
    let engine = TestOperations::new();
    let (dest, share, result) = copy_onto_a_share(&engine, "op-folder-dates-outage", |share| {
        share.drop_at_apply_metadata_of(Path::new("/inbox/album/inner"));
    })
    .await;

    result.expect("a share that comes back within the budget must not fail the copy");
    assert!(share.reconnect_attempts() >= 1, "the share really dropped at the stamp");
    assert_eq!(listed_date(&dest, "/inbox/album/inner").await, Some(INNER_FOLDER_DATE));
    assert_eq!(listed_date(&dest, "/inbox/album").await, Some(FOLDER_DATE));
}

/// A live share that refuses a folder's dates is a refusal: the folder keeps
/// the store's date, nothing waits, and the copy stands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_share_that_refuses_a_folders_dates_is_never_waited_for() {
    let engine = TestOperations::new();
    let (dest, share, result) = copy_onto_a_share(&engine, "op-folder-dates-refused", |share| {
        share.refuse_metadata_of(Path::new("/inbox/album/inner"));
    })
    .await;

    result.expect("a refused date is a metadata loss, never a failed copy");
    assert_eq!(
        share.reconnect_attempts(),
        0,
        "a live share's refusal is never waited for"
    );
    assert_ne!(listed_date(&dest, "/inbox/album/inner").await, Some(INNER_FOLDER_DATE));
    assert_eq!(listed_date(&dest, "/inbox/album").await, Some(FOLDER_DATE));
}
