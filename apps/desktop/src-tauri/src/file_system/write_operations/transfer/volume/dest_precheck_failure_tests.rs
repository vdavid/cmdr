//! What every engine does when the destination won't say whether a name is
//! taken.
//!
//! The top-level conflict pre-check is one `get_metadata` per source, and it is
//! the ONLY thing standing between a Skip or Stop policy and a write. A stat
//! that fails is not an absent file: on a flaky share or a phone mid-session-
//! reset it is a `ConnectionTimeout`, a `DeviceSessionReset`, or a
//! `PermissionDenied`, and reading any of them as "nothing is there" runs no
//! resolver, consults no policy, and lets the landing clear whatever the probe
//! was asked about.
//!
//! So the rule these cells pin, for all four sites (serial copy, concurrent
//! copy, cross-volume move, same-volume move): only `NotFound` means free, and
//! anything else fails THAT item with the error at the DESTINATION path. Same
//! discipline `merge.rs::what_the_destination_holds` follows for a merge child.
//!
//! `VolumeError::ConnectionTimeout` is the fault throughout because it maps to
//! a `WriteOperationError::ConnectionInterrupted` nothing downstream produces,
//! so the assertion proves the failure came from the PROBE rather than from a
//! write that shouldn't have been reached.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::faulty_volume::{FaultyOp, FaultyVolume};
use super::{copy_volumes_with_progress, move_volumes_with_progress, move_within_same_volume_with_progress};
use crate::file_system::volume::{InMemoryVolume, Volume, VolumeError};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::WriteOperationState;
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig, WriteOperationError};

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

const THE_USERS_BYTES: &[u8] = b"the user's own copy";

fn timeout() -> VolumeError {
    VolumeError::ConnectionTimeout("the share stopped answering".to_string())
}

/// A destination holding one file the incoming source would land on, wrapped so
/// its first `get_metadata` — the top-level pre-check — times out.
fn flaky_dest(inner: Arc<InMemoryVolume>, nth: usize) -> Arc<FaultyVolume<InMemoryVolume>> {
    FaultyVolume::wrapping(inner)
        .failing_call(FaultyOp::GetMetadata, nth, timeout())
        .arc()
}

async fn seeded_dest(names: &[&str]) -> Arc<InMemoryVolume> {
    let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest.create_directory(Path::new("/inbox")).await.unwrap();
    for name in names {
        dest.create_file(&PathBuf::from(format!("/inbox/{name}")), THE_USERS_BYTES)
            .await
            .unwrap();
    }
    dest
}

async fn seeded_source(names: &[&str]) -> (Arc<InMemoryVolume>, Vec<PathBuf>) {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let mut paths = Vec::new();
    for name in names {
        let path = PathBuf::from(format!("/{name}"));
        source.create_file(&path, b"the incoming bytes").await.unwrap();
        paths.push(path);
    }
    (source, paths)
}

async fn bytes_at(volume: &Arc<InMemoryVolume>, path: &str) -> Option<Vec<u8>> {
    let mut stream = volume.open_read_stream(&PathBuf::from(path)).await.ok()?;
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        out.extend_from_slice(&chunk);
    }
    Some(out)
}

/// The SERIAL copy driver (one source, so `copy.rs` takes the sequential path).
/// Policy Skip: the user said "don't touch what's already there", and a stat
/// that can't answer must not turn that into an overwrite.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_serial_copy_fails_the_item_whose_destination_stat_times_out() {
    let (source, paths) = seeded_source(&["doc.txt"]).await;
    let dest_inner = seeded_dest(&["doc.txt"]).await;
    let dest = flaky_dest(Arc::clone(&dest_inner), 1);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };
    let result = copy_volumes_with_progress(
        events,
        "op-precheck-serial-copy",
        &state,
        Arc::clone(&source) as Arc<dyn Volume>,
        &paths,
        Arc::clone(&dest) as Arc<dyn Volume>,
        Path::new("/inbox"),
        &config,
    )
    .await;

    assert!(
        dest.fault_fired(FaultyOp::GetMetadata),
        "the injected stat failure never reached the pre-check, so this cell proves nothing"
    );
    let failure = result.expect_err("a destination that won't answer must fail the item");
    assert!(
        matches!(failure.error, WriteOperationError::ConnectionInterrupted { .. }),
        "the failure has to be the PROBE's, at the destination: {:?}",
        failure.error
    );
    assert_eq!(
        bytes_at(&dest_inner, "/inbox/doc.txt").await.as_deref(),
        Some(THE_USERS_BYTES),
        "and under Skip the user's file must be exactly as it was"
    );
}

/// The CONCURRENT copy driver (three sources and a remote peer). Its pre-check
/// normally comes from the one destination listing, so the listing fails here
/// too: that is the shape in which the per-source `get_metadata` probe runs at
/// all (`copy_concurrent_source.rs::existing_dest_entry`, `DestLookup::Unknown`
/// or no index).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_concurrent_copy_fails_the_item_whose_destination_stat_times_out() {
    let (source, paths) = seeded_source(&["doc-0.txt", "doc-1.txt", "doc-2.txt"]).await;
    let dest_inner = seeded_dest(&["doc-0.txt"]).await;
    let dest = FaultyVolume::wrapping(Arc::clone(&dest_inner))
        // The Phase 0.6 listing, so no `DestNameIndex` is built and every source
        // asks the backend directly.
        .failing_call(FaultyOp::ListDirectory, 1, timeout())
        .failing_call(FaultyOp::GetMetadata, 1, timeout())
        .arc();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };
    let result = copy_volumes_with_progress(
        events,
        "op-precheck-concurrent-copy",
        &state,
        Arc::clone(&source) as Arc<dyn Volume>,
        &paths,
        Arc::clone(&dest) as Arc<dyn Volume>,
        Path::new("/inbox"),
        &config,
    )
    .await;

    assert!(
        dest.fault_fired(FaultyOp::GetMetadata),
        "the injected stat failure never reached the pre-check, so this cell proves nothing"
    );
    let failure = result.expect_err("a destination that won't answer must fail the item");
    assert!(
        matches!(failure.error, WriteOperationError::ConnectionInterrupted { .. }),
        "the failure has to be the PROBE's, at the destination: {:?}",
        failure.error
    );
    assert_eq!(
        bytes_at(&dest_inner, "/inbox/doc-0.txt").await.as_deref(),
        Some(THE_USERS_BYTES),
        "and under Skip the user's file must be exactly as it was"
    );
}

/// The CROSS-VOLUME move, whose serial driver writes its own copy of the same
/// fetcher (`move_cross.rs`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_volume_move_fails_the_item_whose_destination_stat_times_out() {
    let (source, paths) = seeded_source(&["doc.txt"]).await;
    let dest_inner = seeded_dest(&["doc.txt"]).await;
    let dest = flaky_dest(Arc::clone(&dest_inner), 1);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };
    let result = move_volumes_with_progress(
        events,
        "op-precheck-cross-move",
        &state,
        Arc::clone(&source) as Arc<dyn Volume>,
        &paths,
        Arc::clone(&dest) as Arc<dyn Volume>,
        Path::new("/inbox"),
        &config,
    )
    .await;

    assert!(
        dest.fault_fired(FaultyOp::GetMetadata),
        "the injected stat failure never reached the pre-check, so this cell proves nothing"
    );
    let failure = result.expect_err("a destination that won't answer must fail the item");
    assert!(
        matches!(failure.error, WriteOperationError::ConnectionInterrupted { .. }),
        "the failure has to be the PROBE's, at the destination: {:?}",
        failure.error
    );
    assert_eq!(
        bytes_at(&dest_inner, "/inbox/doc.txt").await.as_deref(),
        Some(THE_USERS_BYTES),
        "and under Skip the user's file must be exactly as it was"
    );
    assert!(
        source.exists(Path::new("/doc.txt")).await,
        "a move whose item failed keeps its source"
    );
}

/// The SAME-VOLUME move, whose pre-check is the fourth copy of the fetcher
/// (`move_same.rs`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_same_volume_move_fails_the_item_whose_destination_stat_times_out() {
    let inner = Arc::new(InMemoryVolume::new("One").with_space_info(10_000_000, 10_000_000));
    inner.create_directory(Path::new("/inbox")).await.unwrap();
    inner
        .create_file(Path::new("/doc.txt"), b"the incoming bytes")
        .await
        .unwrap();
    inner
        .create_file(Path::new("/inbox/doc.txt"), THE_USERS_BYTES)
        .await
        .unwrap();
    let volume = flaky_dest(Arc::clone(&inner), 1);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };
    let result = move_within_same_volume_with_progress(
        events,
        "op-precheck-same-move",
        &state,
        Arc::clone(&volume) as Arc<dyn Volume>,
        &[PathBuf::from("/doc.txt")],
        Path::new("/inbox"),
        &config,
    )
    .await;

    assert!(
        volume.fault_fired(FaultyOp::GetMetadata),
        "the injected stat failure never reached the pre-check, so this cell proves nothing"
    );
    let error = result.expect_err("a destination that won't answer must fail the item");
    assert!(
        matches!(error, WriteOperationError::ConnectionInterrupted { .. }),
        "the failure has to be the PROBE's, at the destination: {error:?}"
    );
    assert_eq!(
        bytes_at(&inner, "/inbox/doc.txt").await.as_deref(),
        Some(THE_USERS_BYTES),
        "and under Skip the user's file must be exactly as it was"
    );
    assert!(
        inner.exists(Path::new("/doc.txt")).await,
        "a move whose item failed keeps its source"
    );
}
