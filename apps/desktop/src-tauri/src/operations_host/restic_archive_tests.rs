//! An archive inside a restic snapshot, end to end (#631): a real repo on a real
//! drive whose snapshot holds a real zip, reached through the app's route, the
//! listing seam, and the write engine.
//!
//! The zip FILE is the backup's (it copies out as bytes), its insides are an
//! archive read through the view (they list and copy out), and nothing writes
//! into it or moves out of it: a snapshot can't be rewritten.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use crate::file_system::restic::test_repo::{BackedUpDrive, SourceFile, backed_up_drive};
use crate::file_system::volume::manager::get_volume_manager;
use crate::test_support::isolate_secrets;
use cmdr_archive::test_fixtures::{build_zip, deflated, dir, stored};
use cmdr_fs::volume::{RoutedKind, Volume, VolumeError};
use cmdr_listing::host::ListingVolumes;
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::types::Initiator;
use cmdr_operations::testing::{CollectorEventSink, copy_volumes_with_progress, new_operation_state};
use cmdr_operations::{MutationError, ReadOnlySide, VolumeCopyConfig, WriteOperationConfig, WriteOperationError};

const README: &[u8] = b"hello from the backup\n";
const NOTES: &[u8] = b"notes inside a zip inside a snapshot\n";

/// The zip the backed-up folder holds: a file at its root and one in a folder.
static ZIP: LazyLock<Vec<u8>> = LazyLock::new(|| {
    build_zip(&[
        stored("readme.txt", README.to_vec()),
        dir("docs/"),
        deflated("docs/notes.txt", NOTES.to_vec()),
    ])
});

/// A drive whose repo's `latest` snapshot holds `bundle.zip`, unlocked.
async fn drive_with_a_zip(tag: &str) -> (BackedUpDrive, PathBuf) {
    let zip = SourceFile {
        path: "bundle.zip",
        bytes: ZIP.as_slice(),
        mode: 0o644,
        modified: 1_600_000_000,
    };
    let drive = backed_up_drive(tag, &[zip], &[]).await;
    drive.unlock().await;
    let zip = drive.backed_up.join("bundle.zip");
    (drive, zip)
}

/// A fresh local folder registered as a drive, for a copy to land on.
fn dest_drive(tag: &str) -> (tempfile::TempDir, PathBuf, String, Arc<dyn Volume>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = std::fs::canonicalize(dir.path()).expect("canonical tempdir");
    let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "Dest",
        &root,
        crate::volume_host::host(),
    ));
    let id = cmdr_fs::testing::unique_test_id(tag);
    get_volume_manager().register(&id, Arc::clone(&volume));
    (dir, root, id, volume)
}

/// Copies `sources` off `drive` onto `dest_root` the way a pane's F5 does, after
/// checking which route the engine resolved the source to.
async fn copy_out(
    drive: &BackedUpDrive,
    sources: Vec<PathBuf>,
    dest: Arc<dyn Volume>,
    dest_root: &Path,
    route: RoutedKind,
) {
    let engine = super::test_engine();
    let (source, resolved) = engine
        .resolve_source_volume(&drive.volume_id, sources.first())
        .await
        .expect("the source volume");
    assert_eq!(resolved, Some(route));
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "restic-archive-copy-out",
        &Arc::new(new_operation_state(&engine, Duration::from_millis(50))),
        source,
        &sources,
        dest,
        dest_root,
        &config,
    )
    .await;
    assert!(result.is_ok(), "the copy out succeeds: {result:?}");
}

fn refused_as<T>(result: &Result<T, WriteOperationError>, side: ReadOnlySide) -> bool {
    matches!(result, Err(WriteOperationError::ReadOnlyDevice { side: s, .. }) if *s == side)
}

fn read_only<T>(result: &Result<T, MutationError>) -> bool {
    matches!(
        result,
        Err(MutationError::Volume {
            error: VolumeError::ReadOnly(_)
        })
    )
}

/// A pane entering the zip lists the archive; the view would have no folder there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_enters_the_zip_and_lists_its_insides() {
    let _secrets = isolate_secrets();
    let (drive, zip) = drive_with_a_zip("restic-archive-list").await;

    let seam = crate::listing_host::volumes::AppVolumes;
    let entered = seam.resolve(&drive.volume_id, &zip).await;
    assert_eq!(entered.routed, Some(RoutedKind::ArchiveInSnapshot));
    let archive = entered.volume.expect("the archive");
    let mut names: Vec<String> = archive
        .list_directory(&zip, None)
        .await
        .expect("the archive's root lists")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    assert_eq!(names, ["docs", "readme.txt"]);

    let inside = seam.resolve(&drive.volume_id, &zip.join("docs")).await;
    let docs = inside
        .volume
        .expect("the archive")
        .list_directory(&zip.join("docs"), None)
        .await
        .expect("a folder inside lists");
    assert_eq!(docs.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["notes.txt"]);
}

/// Copying the zip ITSELF out restores the file the backup holds, byte for byte:
/// a copy acts on the file, it doesn't enter it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_zip_itself_copies_out_as_the_backed_up_file() {
    let _secrets = isolate_secrets();
    let (drive, zip) = drive_with_a_zip("restic-archive-file").await;
    let (_dest_dir, dest_root, _dest_id, dest) = dest_drive("restic-archive-file-dest");

    copy_out(&drive, vec![zip], dest, &dest_root, RoutedKind::Restic).await;

    let landed = dest_root.join("bundle.zip");
    assert!(landed.is_file(), "a file landed, not a folder of its insides");
    assert_eq!(std::fs::read(&landed).expect("landed"), *ZIP);
}

/// Files inside the zip copy out: decompressed through the archive, its bytes
/// read through the snapshot's view.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn files_inside_the_zip_copy_out() {
    let _secrets = isolate_secrets();
    let (drive, zip) = drive_with_a_zip("restic-archive-inside").await;
    let (_dest_dir, dest_root, _dest_id, dest) = dest_drive("restic-archive-inside-dest");

    copy_out(
        &drive,
        vec![zip.join("readme.txt"), zip.join("docs")],
        dest,
        &dest_root,
        RoutedKind::ArchiveInSnapshot,
    )
    .await;

    assert_eq!(std::fs::read(dest_root.join("readme.txt")).expect("landed"), README);
    assert_eq!(std::fs::read(dest_root.join("docs/notes.txt")).expect("landed"), NOTES);
}

/// Nothing writes into a zip in a snapshot, and nothing moves out of one: every
/// seam refuses typed, before a byte moves, where a zip on a drive would start
/// an archive edit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn writes_into_the_zip_and_a_move_out_refuse_before_anything_moves() {
    let _secrets = isolate_secrets();
    let engine = super::test_engine();
    let (drive, zip) = drive_with_a_zip("restic-archive-writes").await;
    let (_dest_dir, dest_root, dest_id, _dest) = dest_drive("restic-archive-writes-dest");
    std::fs::write(dest_root.join("new.txt"), b"new").expect("a file to copy in");

    let copy_in = engine
        .start_volume_copy(
            Arc::new(CollectorEventSink::new()),
            dest_id.clone(),
            vec![dest_root.join("new.txt")],
            drive.volume_id.clone(),
            zip.join("docs").to_string_lossy().into_owned(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    assert!(refused_as(&copy_in, ReadOnlySide::Destination), "{copy_in:?}");

    let move_out = engine
        .start_volume_move(
            Arc::new(CollectorEventSink::new()),
            drive.volume_id.clone(),
            vec![zip.join("readme.txt")],
            dest_id,
            dest_root.to_string_lossy().into_owned(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    assert!(refused_as(&move_out, ReadOnlySide::Source), "{move_out:?}");
    assert!(!dest_root.join("readme.txt").exists(), "the move copied nothing out");

    let delete = engine
        .delete_files_start(
            Arc::new(CollectorEventSink::new()),
            vec![zip.join("readme.txt")],
            WriteOperationConfig::default(),
            Some(drive.volume_id.clone()),
            Initiator::User,
            None,
        )
        .await;
    assert!(refused_as(&delete, ReadOnlySide::Source), "{delete:?}");

    let renamed = engine
        .rename_managed(
            zip.join("readme.txt"),
            zip.join("renamed.txt"),
            false,
            drive.volume_id.clone(),
            Initiator::User,
        )
        .await;
    assert!(read_only(&renamed), "{renamed:?}");

    let folder = engine
        .create_directory_managed(
            Some(drive.volume_id.clone()),
            zip.join("docs").to_string_lossy().into_owned(),
            "new".to_string(),
            Initiator::User,
        )
        .await;
    assert!(read_only(&folder), "{folder:?}");

    // The snapshot still holds the zip it held.
    let archive = get_volume_manager()
        .resolve_to_list(&drive.volume_id, &zip)
        .await
        .volume
        .expect("the archive");
    let names: Vec<String> = archive
        .list_directory(&zip, None)
        .await
        .expect("lists")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert!(names.contains(&"readme.txt".to_string()), "{names:?}");
}
