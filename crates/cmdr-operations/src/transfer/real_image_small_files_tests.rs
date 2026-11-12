//! A many-small-file copy onto a real second volume pays a sync pass and a
//! device barrier on an interval and once at the end, never one per file
//! (#1145), and a sync that pass can't confirm keeps a move's sources and marks
//! a copy unconfirmed.
//!
//! A child of `real_image`, so `pnpm check disk-images` (whose filter names
//! `transfer::real_image::`) runs it. `#[ignore]`d like its parent. The elapsed
//! time is printed, which is where the numbers in `DETAILS.md` § Durability come
//! from: `cargo nextest run -p cmdr-operations --run-ignored only --no-capture
//! -E 'test(real_image::small_files::)'`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cmdr_fs::testing::TestDir;
use cmdr_fs::testing::disk_images::{DiskImage, DiskImageSession, ImageSpec};
use cmdr_fs::volume::Volume;
use cmdr_local::LocalPosixVolume;

use super::super::copy::copy_files_with_progress_inner;
use super::super::volume::copy_volumes_with_progress;
use crate::event_sinks::{CollectorEventSink, OperationEventSink};
use crate::host::testing::pane_order_host;
use crate::testing::TestOperations;
use crate::types::{VolumeCopyConfig, WriteOperationConfig};

/// How many files of what size, in how many folders.
#[derive(Debug, Clone, Copy)]
struct Shape {
    files: usize,
    size: usize,
    folders: usize,
}

/// Enough files that a per-file barrier dominates (about 4 ms each on an
/// internal SSD), few enough to write in seconds.
const SMALL: Shape = Shape {
    files: 3_000,
    size: 4 * 1024,
    folders: 10,
};

/// A run long enough to span several barrier intervals that still fits the
/// fixture image (about 64 MB free on APFS).
const LARGER: Shape = Shape {
    files: 10_000,
    size: 4 * 1024,
    folders: 20,
};

/// A Mac-side tree of `shape` small files.
fn small_file_tree(label: &str, shape: Shape) -> (TestDir, PathBuf) {
    let dir = TestDir::new(label);
    let root = dir.join("tree");
    for folder in 0..shape.folders {
        let sub = root.join(format!("folder-{folder}"));
        std::fs::create_dir_all(&sub).expect("create a source folder");
        for i in 0..shape.files / shape.folders {
            std::fs::write(sub.join(format!("file-{i:05}.txt")), vec![b'x'; shape.size]).expect("write a source file");
        }
    }
    (dir, root)
}

fn count_files(root: &Path) -> usize {
    std::fs::read_dir(root)
        .expect("list a copied folder")
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() { count_files(&path) } else { 1 }
        })
        .sum()
}

/// The local engine (`chunked_copy.rs` per file, `durability.rs` at the end)
/// from the Mac's disk onto an image volume of `spec`.
#[allow(
    clippy::print_stderr,
    reason = "a measurement prints its numbers, which `DETAILS.md` § Durability quotes"
)]
fn local_copy_onto(spec: ImageSpec, shape: Shape) {
    let engine = TestOperations::new();
    let session = DiskImageSession::acquire();
    let (_source_dir, source) = small_file_tree("small-files-local", shape);
    let image = DiskImage::attach(&session, spec).expect("attach an image");
    let volume = image.volumes()[0].clone();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(200)));
    let events = CollectorEventSink::new();

    let started = Instant::now();
    copy_files_with_progress_inner(
        &events,
        "op-small-files-local",
        &state,
        std::slice::from_ref(&source),
        &volume.mount_point,
        &WriteOperationConfig::default(),
    )
    .expect("the copy completes");
    let elapsed = started.elapsed();

    assert_eq!(count_files(&volume.mount_point.join("tree")), shape.files);
    let complete = events.complete.lock().expect("the event log isn't poisoned");
    assert!(complete[0].durability_unconfirmed.is_none());
    let barriers = state.barriers.barriers_issued();
    // allowed-pluralize-noun: a test's printout of constants
    eprintln!("local copy, {shape:?} onto {spec:?}: {elapsed:?}, {barriers} barriers");
    assert_barriers_per_interval(barriers, elapsed, shape);
}

#[test]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
fn a_local_small_file_copy_onto_apfs_barriers_per_interval_not_per_file() {
    local_copy_onto(ImageSpec::Apfs, SMALL);
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_local_small_file_copy_onto_hfs_barriers_per_interval_not_per_file() {
    local_copy_onto(ImageSpec::Hfs, SMALL);
}

#[test]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
fn a_larger_local_small_file_copy_onto_apfs_barriers_per_interval_not_per_file() {
    local_copy_onto(ImageSpec::Apfs, LARGER);
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_larger_local_small_file_copy_onto_hfs_barriers_per_interval_not_per_file() {
    local_copy_onto(ImageSpec::Hfs, LARGER);
}

/// The volume engine (`write_from_stream` per file, `Volume::flush_writes` at
/// the end), from the Mac's disk onto an image volume.
#[allow(
    clippy::print_stderr,
    reason = "a measurement prints its numbers, which `DETAILS.md` § Durability quotes"
)]
async fn volume_copy_onto(shape: Shape) {
    let engine = TestOperations::new();
    let session = DiskImageSession::acquire();
    let (source_dir, source) = small_file_tree("small-files-volume", shape);
    let image = DiskImage::attach(&session, ImageSpec::Apfs).expect("attach an image");
    let volume = image.volumes()[0].clone();
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "Mac",
        source_dir.to_path_buf(),
        pane_order_host(),
    ));
    let dest_local = Arc::new(LocalPosixVolume::local_folder(
        "Image",
        volume.mount_point.clone(),
        pane_order_host(),
    ));
    let dest_volume: Arc<dyn Volume> = Arc::clone(&dest_local) as Arc<dyn Volume>;
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());

    let started = Instant::now();
    copy_volumes_with_progress(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        "op-small-files-volume",
        &state,
        source_volume,
        &[PathBuf::from("/tree")],
        dest_volume,
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect("the copy completes");
    let elapsed = started.elapsed();

    assert_eq!(count_files(&volume.mount_point.join("tree")), shape.files);
    drop(source);
    let barriers = dest_local.barriers_issued();
    // allowed-pluralize-noun: a test's printout of constants
    eprintln!("volume copy, {shape:?} onto APFS image: {elapsed:?}, {barriers} barriers");
    assert_barriers_per_interval(barriers, elapsed, shape);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
async fn a_volume_small_file_copy_onto_another_volume_barriers_per_interval_not_per_file() {
    volume_copy_onto(SMALL).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
async fn a_larger_volume_small_file_copy_onto_another_volume_barriers_per_interval_not_per_file() {
    volume_copy_onto(LARGER).await;
}

/// A small tree copied (or moved) by the local engine onto an HFS+ image with
/// one file's sync scripted to fail at the pass, after it counted and landed.
fn local_transfer_with_one_unconfirmed_sync(
    move_it: bool,
) -> (
    TestDir,
    PathBuf,
    Result<(), crate::types::WriteOperationError>,
    CollectorEventSink,
) {
    let engine = TestOperations::new();
    let session = DiskImageSession::acquire();
    let shape = Shape {
        files: 40,
        size: 4 * 1024,
        folders: 2,
    };
    let (source_dir, source) = small_file_tree("small-files-unconfirmed", shape);
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach an image");
    let volume = image.volumes()[0].clone();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(200)));
    state.barriers.script_syncs(|path| {
        path.ends_with("folder-1/file-00007.txt")
            .then(|| std::io::Error::from_raw_os_error(libc::EIO))
    });
    let events = CollectorEventSink::new();
    let sources = [source.clone()];
    let config = WriteOperationConfig::default();
    let result = if move_it {
        super::super::move_op::move_files_with_progress_inner(
            &events,
            "op-unconfirmed-move",
            &state,
            &sources,
            &volume.mount_point,
            &config,
        )
    } else {
        copy_files_with_progress_inner(
            &events,
            "op-unconfirmed-copy",
            &state,
            &sources,
            &volume.mount_point,
            &config,
        )
    };
    assert!(
        state
            .barriers
            .synced_paths()
            .iter()
            .any(|path| path.ends_with("folder-1/file-00007.txt")),
        "the failing file's sync ran"
    );
    assert_eq!(
        count_files(&volume.mount_point.join("tree")),
        shape.files,
        "every copy landed"
    );
    (source_dir, source, result, events)
}

/// The move: a sync the pass couldn't confirm keeps EVERY source, typed.
#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_local_move_whose_pass_cant_confirm_a_file_keeps_every_source() {
    let (_dir, source, result, _events) = local_transfer_with_one_unconfirmed_sync(true);
    let error = result.expect_err("an unconfirmed sync stops the move");
    assert!(
        matches!(error, crate::types::WriteOperationError::MoveNotConfirmed { .. }),
        "{:?}",
        error
    );
    assert_eq!(count_files(&source), 40, "every original stays");
}

/// The copy: it deletes nothing, so it completes, saying its output isn't
/// confirmed durable.
#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_local_copy_whose_pass_cant_confirm_a_file_completes_unconfirmed() {
    let (_dir, _source, result, events) = local_transfer_with_one_unconfirmed_sync(false);
    result.expect("a copy's flush failure doesn't stop it");
    let complete = events.complete.lock().expect("the event log isn't poisoned");
    assert!(complete[0].durability_unconfirmed.is_some());
}

/// At least the closing barrier, and no more than one per elapsed interval
/// plus the close: thousands of files, a handful of barriers.
fn assert_barriers_per_interval(issued: u64, elapsed: Duration, shape: Shape) {
    let intervals = (elapsed.as_secs_f64() / cmdr_fs::durable::BARRIER_INTERVAL.as_secs_f64()).ceil() as u64;
    assert!(
        (1..=intervals + 1).contains(&issued),
        // allowed-pluralize-noun: a test's panic message about constants
        "{issued} barriers over {elapsed:?} for {shape:?}: one per interval plus the close"
    );
}
