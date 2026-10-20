//! What a same-volume Overwrite leaves behind when the rename that replaces the
//! destination doesn't land.
//!
//! A same-volume move replaces by renaming, and a rename can't clobber
//! (`force = false`, and MTP's `force = true` doesn't delete an existing dest
//! either), so the destination has to go out of the way first. Deleting it is
//! what makes the window fatal: the rename is a SEPARATE call that an SMB
//! `STATUS_SHARING_VIOLATION`, an MTP `MoveObject` refusal, or a session blip
//! can fail after the delete succeeded, and then the destination is gone and the
//! source hasn't moved. So it is renamed ASIDE and put back.
//!
//! Both sites: the top-level file→file Overwrite (`move_same.rs`'s resolver) and
//! the deep rename-merge child (`rename_merge.rs::apply_child_decision`).

use super::super::faulty_volume::{FaultyOp, FaultyVolume};
use super::super::move_same::move_within_same_volume_with_progress;
use super::test_support::make_state;
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::types::ConflictResolution;
use cmdr_fs::volume::{InMemoryVolume, VolumeError};

const THE_USERS_BYTES: &[u8] = b"the user's own copy";

fn refused() -> VolumeError {
    VolumeError::IoError {
        message: "the share refused the rename".into(),
        raw_os_error: None,
    }
}

async fn bytes_at(volume: &Arc<InMemoryVolume>, path: &str) -> Option<Vec<u8>> {
    let mut stream = volume.open_read_stream(&PathBuf::from(path)).await.ok()?;
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        out.extend_from_slice(&chunk);
    }
    Some(out)
}

fn overwrite_config() -> VolumeCopyConfig {
    VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    }
}

/// Top-level file→file Overwrite whose replacing rename is refused. The user
/// keeps BOTH files: the destination they were replacing and the source that
/// never moved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_rename_leaves_the_destination_the_overwrite_was_replacing() {
    let engine = TestOperations::new();
    let inner = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    inner.create_directory(Path::new("/inbox")).await.unwrap();
    inner
        .create_file(Path::new("/doc.txt"), b"the incoming bytes")
        .await
        .unwrap();
    inner
        .create_file(Path::new("/inbox/doc.txt"), THE_USERS_BYTES)
        .await
        .unwrap();
    let volume = FaultyVolume::wrapping(Arc::clone(&inner))
        .failing_call(FaultyOp::Rename, 1, refused())
        .arc();

    let result = move_within_same_volume_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-same-move-overwrite-rename-refused",
        &make_state(&engine),
        Arc::clone(&volume) as Arc<dyn Volume>,
        &[PathBuf::from("/doc.txt")],
        Path::new("/inbox"),
        &overwrite_config(),
    )
    .await;

    assert!(
        volume.fault_fired(FaultyOp::Rename),
        "the injected rename failure never fired, so this cell proves nothing"
    );
    assert!(result.is_err(), "a refused rename must fail the move: {result:?}");
    assert_eq!(
        bytes_at(&inner, "/inbox/doc.txt").await.as_deref(),
        Some(THE_USERS_BYTES),
        "the destination the Overwrite was replacing must still be there, byte for byte"
    );
    assert_eq!(
        bytes_at(&inner, "/doc.txt").await.as_deref(),
        Some(b"the incoming bytes".as_slice()),
        "and the source never moved, so it stays too"
    );
}

/// The DEEP rename-merge child has the same shape: a dir-vs-dir merge whose
/// clashing child is an Overwrite, and whose rename is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_child_rename_leaves_the_merged_destination_child() {
    let engine = TestOperations::new();
    let inner = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    inner.create_directory(Path::new("/album")).await.unwrap();
    inner
        .create_file(Path::new("/album/photo.jpg"), b"the incoming bytes")
        .await
        .unwrap();
    inner.create_directory(Path::new("/inbox")).await.unwrap();
    inner.create_directory(Path::new("/inbox/album")).await.unwrap();
    inner
        .create_file(Path::new("/inbox/album/photo.jpg"), THE_USERS_BYTES)
        .await
        .unwrap();
    let volume = FaultyVolume::wrapping(Arc::clone(&inner))
        .failing_call(FaultyOp::Rename, 1, refused())
        .arc();

    let result = move_within_same_volume_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-same-merge-child-rename-refused",
        &make_state(&engine),
        Arc::clone(&volume) as Arc<dyn Volume>,
        &[PathBuf::from("/album")],
        Path::new("/inbox"),
        &overwrite_config(),
    )
    .await;

    assert!(
        volume.fault_fired(FaultyOp::Rename),
        "the injected rename failure never fired, so this cell proves nothing"
    );
    assert!(result.is_err(), "a refused child rename must fail the move: {result:?}");
    assert_eq!(
        bytes_at(&inner, "/inbox/album/photo.jpg").await.as_deref(),
        Some(THE_USERS_BYTES),
        "the merged destination's child must still be there, byte for byte"
    );
    assert_eq!(
        bytes_at(&inner, "/album/photo.jpg").await.as_deref(),
        Some(b"the incoming bytes".as_slice()),
        "and the source child never moved, so it stays too"
    );
}

/// A cross-type Overwrite (a FILE moving onto the user's FOLDER), answered on
/// the prompt, whose replacing rename is refused. The folder comes back, children
/// and all: the resolver renamed it aside rather than deleting it.
///
/// Rename #1 is that aside; #2 is the one that would have replaced it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_rename_puts_back_the_folder_a_file_was_replacing() {
    let engine = TestOperations::new();
    let inner = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    inner.create_directory(Path::new("/inbox")).await.unwrap();
    inner
        .create_file(Path::new("/clash"), b"the incoming bytes")
        .await
        .unwrap();
    inner.create_directory(Path::new("/inbox/clash")).await.unwrap();
    inner
        .create_file(Path::new("/inbox/clash/precious.txt"), THE_USERS_BYTES)
        .await
        .unwrap();
    let volume = FaultyVolume::wrapping(Arc::clone(&inner))
        .failing_call(FaultyOp::Rename, 2, refused())
        .arc();
    let state = make_state(&engine);
    let events = Arc::new(
        super::super::super::conflict_responder_test_support::ConflictResponderSink::new(
            &state,
            ConflictResolution::Overwrite,
            false,
        ),
    );

    let result = move_within_same_volume_with_progress(
        events,
        "op-same-move-cross-type-rename-refused",
        &state,
        Arc::clone(&volume) as Arc<dyn Volume>,
        &[PathBuf::from("/clash")],
        Path::new("/inbox"),
        &VolumeCopyConfig {
            // Only an answered prompt may cross types.
            conflict_resolution: ConflictResolution::Stop,
            ..VolumeCopyConfig::default()
        },
    )
    .await;

    assert!(
        volume.fault_fired(FaultyOp::Rename),
        "the injected rename failure never fired, so this cell proves nothing"
    );
    assert!(result.is_err(), "a refused rename must fail the move: {result:?}");
    assert_eq!(
        bytes_at(&inner, "/inbox/clash/precious.txt").await.as_deref(),
        Some(THE_USERS_BYTES),
        "the folder the Overwrite was replacing must be back, children and all"
    );
    assert_eq!(
        bytes_at(&inner, "/clash").await.as_deref(),
        Some(b"the incoming bytes".as_slice()),
        "and the source never moved"
    );
}
