//! Copying OUT of a repo's virtual `.git` trees, end to end through the write-op
//! routing and the transfer engine: `resolve_source_volume` picks the portal, the
//! cross-volume engine reads every byte through `open_read_stream`, and a folder
//! comes out as a folder.
//!
//! The zip twin is `copy_extract_out_tests.rs`; the two run the same engine over
//! the two read-only routed volumes, so a change that breaks one usually breaks
//! the other.
//!
//! Shared fixture `make_state` lives in `volume/copy_tests/mod.rs` (`super::tests`).

use super::tests::make_state;
use super::*;
use crate::file_system::git;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::resolve_source_volume;
use cmdr_git::test_fixtures::{EntryKind, Fixture, cleanup, temp_dir};

/// A repo whose `main` snapshot holds a top-level file and a two-file folder,
/// registered as the plain local volume a pane would be browsing.
fn repo_registered_as_the_local_drive(name: &str) -> PathBuf {
    let dir = temp_dir("copy_snapshot_out", name);
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_files_with_modes(
        &[
            ("readme.txt", b"hello", EntryKind::Blob),
            ("docs/a.txt", b"aaa", EntryKind::Blob),
            ("docs/b.txt", b"bbb", EntryKind::Blob),
        ],
        "initial",
        1_700_000_000,
    );
    // (nextest isolates the process-global manager per test.)
    get_volume_manager().register("root", Arc::new(LocalPosixVolume::new("Root", dir.to_str().unwrap())));
    git::wiring::set_virtual_portal_enabled(true);
    dir
}

async fn read_dest_file(dest: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
    let mut stream = dest
        .open_read_stream(Path::new(path))
        .await
        .unwrap_or_else(|e| panic!("dest missing {path}: {e:?}"));
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk.expect("chunk"));
    }
    out
}

/// The one thing the routed shape exists for. Left on the parent drive, these
/// paths take the local-to-local fast path against files with no inode, and the
/// transfer dialog sits on "Verifying before copy" until the user gives up.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_out_of_a_snapshot_carries_a_file_and_a_folder_byte_for_byte() {
    let dir = repo_registered_as_the_local_drive("file_and_folder");

    // What a pane sends: full paths inside the branch snapshot.
    let sources = vec![
        dir.join(".git/branches/main/readme.txt"),
        dir.join(".git/branches/main/docs"),
    ];
    let (source, route) = resolve_source_volume("root", sources.first())
        .await
        .expect("the source volume");
    assert_eq!(
        route,
        Some(crate::file_system::volume::manager::RoutedKind::GitPortal),
        "the snapshot is the portal's"
    );

    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "snapshot-copy-out-op",
        &make_state(),
        Arc::clone(&source),
        &sources,
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "a copy out of a snapshot should succeed: {result:?}");

    assert_eq!(read_dest_file(&dest, "/readme.txt").await, b"hello");
    assert_eq!(read_dest_file(&dest, "/docs/a.txt").await, b"aaa");
    assert_eq!(read_dest_file(&dest, "/docs/b.txt").await, b"bbb");

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1, "one completion event");
    // `files_processed` counts TOP-LEVEL source items (the file + the folder);
    // `bytes_processed` is the full-transfer measure.
    assert_eq!(complete[0].files_processed, 2, "two top-level sources");
    assert_eq!(complete[0].bytes_processed, 5 + 3 + 3, "every inner file's bytes");
    drop(complete);

    cleanup(&dir);
}

/// A snapshot can be copied out of, never moved out of: there is no file to
/// remove once the copy lands. The refusal comes BEFORE anything is written, so
/// the destination stays untouched rather than holding half a move.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_out_of_a_snapshot_is_refused_before_a_byte_is_written() {
    use crate::file_system::write_operations::{ReadOnlySide, WriteOperationError, start_volume_move};
    use crate::operation_log::types::Initiator;

    let dir = repo_registered_as_the_local_drive("move_refused");
    let dest_dir = temp_dir("copy_snapshot_out", "move_refused_dest");
    std::fs::create_dir_all(&dest_dir).expect("dest dir");
    get_volume_manager().register(
        "dest",
        Arc::new(LocalPosixVolume::new("Dest", dest_dir.to_str().unwrap())),
    );

    let err = start_volume_move(
        Arc::new(CollectorEventSink::new()),
        "root".to_string(),
        vec![dir.join(".git/branches/main/readme.txt")],
        "dest".to_string(),
        dest_dir.to_string_lossy().into_owned(),
        VolumeCopyConfig::default(),
        Initiator::User,
        None,
    )
    .await
    .expect_err("a move out of a snapshot must be refused");

    // The SIDE is the whole point of the refusal's wording: the destination was
    // fine, and pointing this user at it would send them to fix the wrong half.
    assert!(
        matches!(
            err,
            WriteOperationError::ReadOnlyDevice {
                side: ReadOnlySide::Source,
                ..
            }
        ),
        "the typed read-only refusal naming the SOURCE, ❌ never a started-then-broken transfer: {err:?}"
    );
    assert!(
        std::fs::read_dir(&dest_dir).expect("read dest").next().is_none(),
        "nothing was written"
    );

    cleanup(&dir);
    cleanup(&dest_dir);
}

/// A drag-drop INTO a snapshot has nowhere to land: every mutation on the portal
/// volume answers `NotSupported`, so the copy is refused up front instead of
/// starting and dying on its first write.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_into_a_snapshot_is_refused_up_front() {
    use crate::file_system::write_operations::{ReadOnlySide, WriteOperationError, start_volume_copy};
    use crate::operation_log::types::Initiator;

    let dir = repo_registered_as_the_local_drive("drop_refused");
    std::fs::write(dir.join("dropped.txt"), b"dropped").expect("write the dragged file");

    let err = start_volume_copy(
        Arc::new(CollectorEventSink::new()),
        "root".to_string(),
        vec![dir.join("dropped.txt")],
        "root".to_string(),
        dir.join(".git/branches/main/docs").to_string_lossy().into_owned(),
        VolumeCopyConfig::default(),
        Initiator::User,
        None,
    )
    .await
    .expect_err("a drop into a snapshot must be refused");

    assert!(
        matches!(
            err,
            WriteOperationError::ReadOnlyDevice {
                side: ReadOnlySide::Destination,
                ..
            }
        ),
        "the typed read-only refusal naming the DESTINATION: {err:?}"
    );

    cleanup(&dir);
}
