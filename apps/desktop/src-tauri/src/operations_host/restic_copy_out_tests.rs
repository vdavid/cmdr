//! Copying OUT of a restic snapshot, end to end: a real repo on a real drive,
//! reached through the app's route, read through the view's streams by the
//! cross-volume engine, landed on a real disk.
//!
//! The git twin is `copy_snapshot_out_tests.rs`. The refusals that need no
//! repo (a delete, a rename, a copy in) are `restic_guard_tests.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use crate::file_system::restic::test_repo::{NoBytes, SourceFile, backed_up_drive};
use crate::file_system::volume::manager::get_volume_manager;
use crate::test_support::{isolate_secrets, wait_until_async};
use cmdr_fs::volume::{LockedContainer, RoutedKind, Volume};
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::types::Initiator;
use cmdr_operations::testing::{CollectorEventSink, copy_volumes_with_progress, new_operation_state};
use cmdr_operations::{ReadOnlySide, VolumeCopyConfig, WriteOperationError};

const NOTES: SourceFile = SourceFile {
    path: "notes.txt",
    bytes: b"hello restic\n",
    mode: 0o640,
    modified: 1_600_000_000,
};
const DOC: SourceFile = SourceFile {
    path: "docs/a.txt",
    bytes: b"aaa",
    mode: 0o600,
    modified: 1_600_000_100,
};
const SCRIPT: SourceFile = SourceFile {
    path: "docs/run.sh",
    bytes: b"#!/bin/sh\n",
    mode: 0o755,
    modified: 1_600_000_200,
};

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

fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()))
        .permissions()
        .mode()
        & 0o777
}

fn modified_of(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .expect("mtime")
        .duration_since(UNIX_EPOCH)
        .expect("after 1970")
        .as_secs()
}

fn is_empty(dir: &Path) -> bool {
    std::fs::read_dir(dir).expect("read dest").next().is_none()
}

fn quick_progress() -> VolumeCopyConfig {
    VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    }
}

/// What a restore is for: the bytes, the dates, and the permission bits come
/// out as they went in. A file selected alone takes the top-level dispatch, one
/// inside a folder the merge walker; both have to carry the metadata.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_out_lands_a_file_and_a_folder_with_their_dates_and_modes() {
    let _secrets = isolate_secrets();
    let engine = super::test_engine();
    let drive = backed_up_drive("restic-copy-out", &[NOTES, DOC, SCRIPT], &[]).await;
    drive.unlock().await;
    let (_dest_dir, dest_root, _dest_id, dest) = dest_drive("restic-copy-out-dest");

    let sources = vec![drive.backed_up.join("notes.txt"), drive.backed_up.join("docs")];
    let (source, route) = engine
        .resolve_source_volume(&drive.volume_id, sources.first())
        .await
        .expect("the source volume");
    assert_eq!(route, Some(RoutedKind::Restic));

    let events = Arc::new(CollectorEventSink::new());
    let result = copy_volumes_with_progress(
        events.clone(),
        "restic-copy-out-op",
        &Arc::new(new_operation_state(&engine, Duration::from_millis(50))),
        source,
        &sources,
        dest,
        &dest_root,
        &quick_progress(),
    )
    .await;
    assert!(result.is_ok(), "a copy out of a snapshot succeeds: {result:?}");

    for (landed, file) in [
        (dest_root.join("notes.txt"), &NOTES),
        (dest_root.join("docs/a.txt"), &DOC),
        (dest_root.join("docs/run.sh"), &SCRIPT),
    ] {
        assert_eq!(std::fs::read(&landed).expect("landed"), file.bytes, "{}", file.path);
        assert_eq!(mode_of(&landed), file.mode, "the mode of {}", file.path);
        assert_eq!(modified_of(&landed), file.modified, "the date of {}", file.path);
    }

    let complete = events.complete.lock().expect("events");
    assert_eq!(complete.len(), 1);
    assert_eq!(
        complete[0].bytes_processed,
        (NOTES.bytes.len() + DOC.bytes.len() + SCRIPT.bytes.len()) as u64,
        "real sizes, every inner file's bytes"
    );
}

/// A snapshot can be copied out of, never moved out of, and the refusal comes
/// before a byte lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_out_is_refused_before_a_byte_is_written() {
    let _secrets = isolate_secrets();
    let engine = super::test_engine();
    let drive = backed_up_drive("restic-move-out", &[NOTES], &[]).await;
    drive.unlock().await;
    let (_dest_dir, dest_root, dest_id, _dest) = dest_drive("restic-move-out-dest");

    let refused = engine
        .start_volume_move(
            Arc::new(CollectorEventSink::new()),
            drive.volume_id.clone(),
            vec![drive.backed_up.join("notes.txt")],
            dest_id,
            dest_root.to_string_lossy().into_owned(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    assert!(
        matches!(
            refused,
            Err(WriteOperationError::ReadOnlyDevice {
                side: ReadOnlySide::Source,
                ..
            })
        ),
        "{refused:?}"
    );
    assert!(is_empty(&dest_root), "nothing was written");
}

/// Copying a whole snapshot is a legal, enormous restore, and its pre-scan's
/// first listing inside the snapshot waits on the repo's index load: seconds on
/// a NAS, minutes on a big repo. A Cancel during that wait ends the copy at
/// once, with nothing landed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_the_pre_scan_of_a_whole_snapshot_ends_the_copy_with_nothing_written() {
    let _secrets = isolate_secrets();
    let engine = super::test_engine();
    let drive = backed_up_drive("restic-scan-cancel", &[NOTES, DOC, SCRIPT], &[]).await;
    drive.unlock().await;
    let (_dest_dir, dest_root, dest_id, _dest) = dest_drive("restic-scan-cancel-dest");

    drive.gate.close_on("index");
    let events = Arc::new(CollectorEventSink::new());
    let started = engine
        .start_volume_copy(
            events.clone(),
            drive.volume_id.clone(),
            vec![drive.repo.join("Browse snapshots/latest")],
            dest_id,
            dest_root.to_string_lossy().into_owned(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await
        .expect("a copy out starts");
    let gate = Arc::clone(&drive.gate);
    wait_until_async(Duration::from_secs(10), "the pre-scan waits on the index load", || {
        gate.held() > 0
    })
    .await;

    engine.cancel_write_operation(&started.operation_id.as_str().into(), false);
    wait_until_async(Duration::from_secs(5), "the copy ends on the cancel", || {
        !events.cancelled.lock().expect("events").is_empty() || !events.errors.lock().expect("events").is_empty()
    })
    .await;
    assert!(
        events.errors.lock().expect("events").is_empty(),
        "a cancel, not a failure: {:?}",
        events.errors.lock().expect("events")
    );
    assert!(events.complete.lock().expect("events").is_empty());
    assert!(is_empty(&dest_root), "nothing landed");
    drive.gate.open();
}

/// A repo whose password the hub doesn't hold (remember off, then a restart)
/// answers a copy out with the transfer's password prompt, the error the unlock
/// dialog's repo variant answers; once unlocked, the same copy goes through.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_out_with_no_password_asks_for_one_and_goes_through_after_the_unlock() {
    let _secrets = isolate_secrets();
    let engine = super::test_engine();
    let drive = backed_up_drive("restic-copy-password", &[NOTES], &[]).await;
    let (_dest_dir, dest_root, _dest_id, dest) = dest_drive("restic-copy-password-dest");
    let sources = vec![drive.backed_up.join("notes.txt")];

    let copy = || async {
        let (source, _route) = engine
            .resolve_source_volume(&drive.volume_id, sources.first())
            .await
            .expect("the source volume");
        copy_volumes_with_progress(
            Arc::new(CollectorEventSink::new()),
            "restic-copy-password-op",
            &Arc::new(new_operation_state(&engine, Duration::from_millis(50))),
            source,
            &sources,
            Arc::clone(&dest),
            &dest_root,
            &quick_progress(),
        )
        .await
    };

    let asked = copy().await;
    assert!(
        matches!(
            &asked,
            Err(failure) if matches!(failure.error, WriteOperationError::ArchiveNeedsPassword { container: LockedContainer::ResticRepo, wrong_attempt: false, .. })
        ),
        "{asked:?}"
    );
    assert!(is_empty(&dest_root), "nothing was written before the prompt");

    drive.unlock().await;
    let retried = copy().await;
    assert!(retried.is_ok(), "{retried:?}");
    assert_eq!(std::fs::read(dest_root.join("notes.txt")).expect("landed"), NOTES.bytes);
}

/// A fifo in a backup has no bytes to restore: the copy of its folder skips it
/// as one item and carries everything beside it, ❌ never aborts the whole copy.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_special_file_in_a_snapshot_is_skipped_and_its_siblings_land() {
    let _secrets = isolate_secrets();
    let engine = super::test_engine();
    let drive = backed_up_drive("restic-copy-fifo", &[DOC], &[NoBytes::Fifo("docs/pipe")]).await;
    drive.unlock().await;
    let (_dest_dir, dest_root, _dest_id, dest) = dest_drive("restic-copy-fifo-dest");

    let sources = vec![drive.backed_up.join("docs")];
    let (source, _route) = engine
        .resolve_source_volume(&drive.volume_id, sources.first())
        .await
        .expect("the source volume");
    let names: Vec<String> = source
        .list_directory(&sources[0], None)
        .await
        .expect("the folder lists")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert!(
        names.contains(&"pipe".to_string()),
        "the backup holds the fifo: {names:?}"
    );

    let events = Arc::new(CollectorEventSink::new());
    let result = copy_volumes_with_progress(
        events.clone(),
        "restic-copy-fifo-op",
        &Arc::new(new_operation_state(&engine, Duration::from_millis(50))),
        source,
        &sources,
        dest,
        &dest_root,
        &quick_progress(),
    )
    .await;
    assert!(result.is_ok(), "one special file doesn't abort the copy: {result:?}");
    assert_eq!(std::fs::read(dest_root.join("docs/a.txt")).expect("landed"), DOC.bytes);
    assert!(!dest_root.join("docs/pipe").exists(), "the fifo isn't recreated");
    assert_eq!(
        events.complete.lock().expect("events")[0].files_skipped,
        1,
        "the fifo is one skipped item"
    );
}

/// The restore: a link lands as the link, holding exactly what the backup
/// recorded, a target pointing out of the destination included. Nothing is
/// followed, so nothing lands where it points.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restore_lands_links_as_links_verbatim_nested_and_selected() {
    let _secrets = isolate_secrets();
    let engine = super::test_engine();
    let drive = backed_up_drive(
        "restic-restore-links",
        &[DOC],
        &[
            NoBytes::Symlink("docs/near", "a.txt"),
            NoBytes::Symlink("docs/out", "../../../outside"),
            NoBytes::Symlink("alone", "/somewhere/absolute"),
        ],
    )
    .await;
    drive.unlock().await;
    let (_dest_dir, dest_root, _dest_id, dest) = dest_drive("restic-restore-links-dest");
    let sources = vec![drive.backed_up.join("docs"), drive.backed_up.join("alone")];
    let (source, _route) = engine
        .resolve_source_volume(&drive.volume_id, sources.first())
        .await
        .expect("the source volume");
    let events = Arc::new(CollectorEventSink::new());

    let result = copy_volumes_with_progress(
        Arc::clone(&events) as _,
        "restic-restore-links-op",
        &Arc::new(new_operation_state(&engine, Duration::from_millis(50))),
        source,
        &sources,
        dest,
        &dest_root,
        &quick_progress(),
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    let target_of = |name: &str| std::fs::read_link(dest_root.join(name)).expect("a link landed");
    assert_eq!(target_of("docs/near"), Path::new("a.txt"));
    assert_eq!(target_of("docs/out"), Path::new("../../../outside"));
    assert_eq!(target_of("alone"), Path::new("/somewhere/absolute"));
    assert_eq!(std::fs::read(dest_root.join("docs/a.txt")).expect("landed"), DOC.bytes);
    assert_eq!(events.complete.lock().expect("events")[0].files_skipped, 0);
}

/// Onto a remote that can't hold a link, a restore has nothing to land: each is
/// one counted skip, selected or nested, ❌ never an I/O error, and the rest lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restore_onto_a_destination_with_no_links_counts_each_as_a_skip() {
    let _secrets = isolate_secrets();
    let engine = super::test_engine();
    let drive = backed_up_drive(
        "restic-restore-no-links",
        &[DOC],
        &[
            NoBytes::Symlink("docs/near", "a.txt"),
            NoBytes::Symlink("alone", "somewhere"),
        ],
    )
    .await;
    drive.unlock().await;
    let dest = Arc::new(cmdr_fs::volume::InMemoryVolume::new("No links").with_space_info(10_000_000, 10_000_000));
    dest.create_directory(Path::new("/inbox")).await.expect("inbox");

    for (tag, source_path, skips, lands) in [
        ("selected", drive.backed_up.join("alone"), 1, None),
        ("nested", drive.backed_up.join("docs"), 1, Some("/inbox/docs/a.txt")),
    ] {
        let sources = vec![source_path];
        let (source, _route) = engine
            .resolve_source_volume(&drive.volume_id, sources.first())
            .await
            .expect("the source volume");
        let events = Arc::new(CollectorEventSink::new());
        let result = copy_volumes_with_progress(
            Arc::clone(&events) as _,
            &format!("restic-restore-no-links-{tag}"),
            &Arc::new(new_operation_state(&engine, Duration::from_millis(50))),
            source,
            &sources,
            Arc::clone(&dest) as Arc<dyn Volume>,
            Path::new("/inbox"),
            &quick_progress(),
        )
        .await;
        assert!(result.is_ok(), "{tag}: {result:?}");
        assert_eq!(events.complete.lock().expect("events")[0].files_skipped, skips, "{tag}");
        if let Some(landed) = lands {
            assert!(dest.exists(Path::new(landed)).await, "{tag}");
        }
    }
    assert!(!dest.exists(Path::new("/inbox/alone")).await);
    assert!(!dest.exists(Path::new("/inbox/docs/near")).await);
}

/// A Mac's backup is full of symlinks, and the view never follows one. A folder
/// holding one still copies out: everything beside the link lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_symlink_in_a_snapshot_doesnt_stop_its_folder_from_copying_out() {
    let _secrets = isolate_secrets();
    let engine = super::test_engine();
    let drive = backed_up_drive("restic-copy-link", &[DOC], &[NoBytes::Symlink("docs/link", "a.txt")]).await;
    drive.unlock().await;
    let (_dest_dir, dest_root, _dest_id, dest) = dest_drive("restic-copy-link-dest");

    let sources = vec![drive.backed_up.join("docs")];
    let (source, _route) = engine
        .resolve_source_volume(&drive.volume_id, sources.first())
        .await
        .expect("the source volume");
    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "restic-copy-link-op",
        &Arc::new(new_operation_state(&engine, Duration::from_millis(50))),
        source,
        &sources,
        dest,
        &dest_root,
        &quick_progress(),
    )
    .await;
    assert!(result.is_ok(), "one symlink doesn't abort the copy: {result:?}");
    assert_eq!(std::fs::read(dest_root.join("docs/a.txt")).expect("landed"), DOC.bytes);
}
