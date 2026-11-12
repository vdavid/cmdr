//! What a cross-filesystem move does when a drive leaves mid-flight.
//!
//! Phase 4 is the only place in Cmdr that deletes files the user didn't ask to
//! delete, so every "it's already gone" reading in it is checked against the
//! mount table first. These drive the engine with the table answered by
//! `transfer_sides::test_hook`, so no test needs a real drive.

use std::collections::HashSet;

use super::cross_fs::move_with_staging;
use super::source_sweep::{LandedOriginal, SourceSweep, delete_sources_after_move};
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::state::ScanResult;
use crate::testing::TestOperations;
use crate::transfer_sides::{MoveSourceCounts, TransferSide, TransferSides, test_hook};
use crate::types::TransferRole;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::TestDir;

/// An operation that knows both its volumes, the way every transfer the dialog
/// starts does.
fn state_between(engine: &TestOperations, source_root: &Path, dest_root: &Path) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(std::time::Duration::from_millis(200))
            .with_sides(Some(TransferSides::new(
                TransferSide::new(
                    "vol-mac".to_string(),
                    "Macintosh HD".to_string(),
                    source_root.to_path_buf(),
                ),
                TransferSide::new(
                    "vol-stick".to_string(),
                    "Tëstkamera".to_string(),
                    dest_root.to_path_buf(),
                ),
            ))),
    )
}

/// Two files to move, in a source folder that also IS the source volume's root.
struct TwoFileMove {
    _dir: TestDir,
    src: PathBuf,
    dst: PathBuf,
    sources: Vec<PathBuf>,
}

fn two_file_move(name: &str) -> TwoFileMove {
    let dir = TestDir::new(name);
    let src = dir.join("src");
    let dst = dir.join("dst");
    fs::create_dir_all(&src).expect("create src");
    fs::create_dir_all(&dst).expect("create dst");
    fs::write(src.join("a.txt"), b"one").expect("write a.txt");
    fs::write(src.join("b.txt"), b"two").expect("write b.txt");
    let sources = vec![src.join("a.txt"), src.join("b.txt")];
    TwoFileMove {
        _dir: dir,
        src,
        dst,
        sources,
    }
}

/// The M10 gate: a flush can answer `Ok` for writes the kernel took moments
/// before the drive left, so the mount table is asked once more between the
/// flush and the first delete. Without it, the sweep deletes the only copies of
/// files that never reached a disk.
#[test]
fn a_destination_that_left_before_phase_4_keeps_every_source() {
    let engine = TestOperations::new();
    let moving = two_file_move("move-vanished-destination");
    let events = Arc::new(CollectorEventSink::new());
    let state = state_between(&engine, &moving.src, &moving.dst);
    let _hook = test_hook::pull(&moving.dst);

    let result = move_with_staging(
        &*events,
        "op-vanished-dest",
        &state,
        &moving.sources,
        &moving.dst,
        &WriteOperationConfig::default(),
        0,
    );

    assert!(
        matches!(result, Err(WriteOperationError::DeviceDisconnected { .. })),
        "a destination that left reads as a disconnect, got {result:?}"
    );
    for source in &moving.sources {
        assert!(
            source.exists(),
            "{} must still be there: nothing proved its copy landed",
            source.display()
        );
    }

    let errors = events.errors.lock_ignore_poison();
    let event = errors.first().expect("the move says why it stopped");
    let WriteOperationError::DeviceDisconnected { side: Some(side), .. } = &event.error else {
        panic!("the event names the drive that left, got {:?}", event.error);
    };
    assert_eq!(side.role, TransferRole::Destination);
    assert_eq!(side.volume_name, "Tëstkamera", "the name captured when it started");
    assert_eq!(side.counterpart_name, "Macintosh HD");
}

/// The same move with both drives mounted: the gate is a gate, not a wall.
#[test]
fn a_destination_that_stayed_lets_the_move_finish() {
    let engine = TestOperations::new();
    let moving = two_file_move("move-mounted-destination");
    let events = Arc::new(CollectorEventSink::new());
    let state = state_between(&engine, &moving.src, &moving.dst);
    let _hook = test_hook::answer_each(vec![(moving.src.clone(), Some(true)), (moving.dst.clone(), Some(true))]);

    move_with_staging(
        &*events,
        "op-mounted-dest",
        &state,
        &moving.sources,
        &moving.dst,
        &WriteOperationConfig::default(),
        0,
    )
    .expect("the move finishes");

    for source in &moving.sources {
        assert!(!source.exists(), "{} moved, so it's gone", source.display());
    }
    assert!(moving.dst.join("a.txt").exists() && moving.dst.join("b.txt").exists());
}

/// One source, planned as landed, that isn't on disk any more.
fn sweep_of_one_missing_source(src: &Path) -> (Vec<PathBuf>, SourceSweep) {
    let sources = vec![src.join("a.txt")];
    let scan = ScanResult {
        files: Vec::new(),
        dirs: Vec::new(),
        file_count: 1,
        total_bytes: 0,
        dedup_bytes: 0,
        per_path: Vec::new(),
        tree: None,
    };
    let landed = LandedOriginal {
        path: src.join("a.txt"),
        stamp: None,
    };
    let sweep = SourceSweep::plan(&sources, vec![landed], &scan, HashSet::new());
    (sources, sweep)
}

/// A source that isn't there normally means this move already carried it. On a
/// drive that LEFT it means the mount is gone, and counting those originals as
/// deleted is how a person is told their files are safe at the destination when
/// they are in fact still on the drive in their hand.
#[test]
fn a_sweep_stops_when_the_source_drive_left_rather_than_reading_it_as_done() {
    let engine = TestOperations::new();
    let dir = TestDir::new("sweep-vanished-source");
    let src = dir.join("src");
    fs::create_dir_all(&src).expect("create src");
    let (sources, sweep) = sweep_of_one_missing_source(&src);
    let events = Arc::new(CollectorEventSink::new());
    let state = state_between(&engine, &src, &dir.join("dst"));
    let _hook = test_hook::pull(&src);

    let stopped = delete_sources_after_move(
        &*events,
        "op-sweep-vanished",
        &state,
        &sources,
        1,
        &sweep,
        &LockedPlan::none(),
    )
    .err()
    .expect("the sweep stops");

    assert!(
        matches!(stopped.error, WriteOperationError::DeviceDisconnected { .. }),
        "got {:?}",
        stopped.error
    );
    assert_eq!(
        stopped.counts,
        Some(MoveSourceCounts { removed: 0, left: 1 }),
        "nothing was removed, so everything is still the user's to find"
    );
}

/// The same missing source with the drive still mounted: something else removed
/// it and the destination has it, which is the case the sweep was built for.
#[test]
fn a_missing_source_on_a_mounted_drive_is_still_counted_done() {
    let engine = TestOperations::new();
    let dir = TestDir::new("sweep-mounted-source");
    let src = dir.join("src");
    fs::create_dir_all(&src).expect("create src");
    let (sources, sweep) = sweep_of_one_missing_source(&src);
    let events = Arc::new(CollectorEventSink::new());
    let state = state_between(&engine, &src, &dir.join("dst"));
    let _hook = test_hook::answer_each(vec![(src.clone(), Some(true))]);

    delete_sources_after_move(
        &*events,
        "op-sweep-mounted",
        &state,
        &sources,
        1,
        &sweep,
        &LockedPlan::none(),
    )
    .expect("the sweep finishes");
}
