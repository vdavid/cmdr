//! A many-small-file copy onto a real second volume pays a device barrier on an
//! interval and once at the end, never one per file (#1145).
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

/// Enough files that a per-file barrier dominates (about 4 ms each on an
/// internal SSD), few enough to write in seconds.
const FILE_COUNT: usize = 3_000;
const FILE_SIZE: usize = 4 * 1024;
const FOLDERS: usize = 10;

/// A Mac-side tree of [`FILE_COUNT`] small files in [`FOLDERS`] folders.
fn small_file_tree(label: &str) -> (TestDir, PathBuf) {
    let dir = TestDir::new(label);
    let root = dir.join("tree");
    for folder in 0..FOLDERS {
        let sub = root.join(format!("folder-{folder}"));
        std::fs::create_dir_all(&sub).expect("create a source folder");
        for i in 0..FILE_COUNT / FOLDERS {
            std::fs::write(sub.join(format!("file-{i:04}.txt")), vec![b'x'; FILE_SIZE]).expect("write a source file");
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
fn local_copy_onto(spec: ImageSpec) {
    let engine = TestOperations::new();
    let session = DiskImageSession::acquire();
    let (_source_dir, source) = small_file_tree("small-files-local");
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

    assert_eq!(count_files(&volume.mount_point.join("tree")), FILE_COUNT);
    let complete = events.complete.lock().expect("the event log isn't poisoned");
    assert!(complete[0].durability_unconfirmed.is_none());
    let barriers = state.barriers.barriers_issued();
    // allowed-pluralize-noun: FILE_COUNT is a 3,000 constant, and the barrier count is a test's printout
    eprintln!("local copy, {FILE_COUNT} x {FILE_SIZE} B onto {spec:?}: {elapsed:?}, {barriers} barriers");
    assert_barriers_per_interval(barriers, elapsed);
}

#[test]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
fn a_local_small_file_copy_onto_apfs_barriers_per_interval_not_per_file() {
    local_copy_onto(ImageSpec::Apfs);
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_local_small_file_copy_onto_hfs_barriers_per_interval_not_per_file() {
    local_copy_onto(ImageSpec::Hfs);
}

/// The volume engine (`write_from_stream` per file, `Volume::flush_writes` at
/// the end), from the Mac's disk onto an image volume.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
#[allow(
    clippy::print_stderr,
    reason = "a measurement prints its numbers, which `DETAILS.md` § Durability quotes"
)]
async fn a_volume_small_file_copy_onto_another_volume_barriers_per_interval_not_per_file() {
    let engine = TestOperations::new();
    let session = DiskImageSession::acquire();
    let (source_dir, source) = small_file_tree("small-files-volume");
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

    assert_eq!(count_files(&volume.mount_point.join("tree")), FILE_COUNT);
    drop(source);
    let barriers = dest_local.barriers_issued();
    // allowed-pluralize-noun: FILE_COUNT is a 3,000 constant, and the barrier count is a test's printout
    eprintln!("volume copy, {FILE_COUNT} x {FILE_SIZE} B onto APFS image: {elapsed:?}, {barriers} barriers");
    assert_barriers_per_interval(barriers, elapsed);
}

/// At least the closing barrier, and no more than one per elapsed interval
/// plus the close: thousands of files, a handful of barriers.
fn assert_barriers_per_interval(issued: u64, elapsed: Duration) {
    let intervals = (elapsed.as_secs_f64() / cmdr_fs::durable::BARRIER_INTERVAL.as_secs_f64()).ceil() as u64;
    assert!(
        (1..=intervals + 1).contains(&issued),
        // allowed-pluralize-noun: a test's panic message, and FILE_COUNT is a 3,000 constant
        "{issued} barriers over {elapsed:?} for {FILE_COUNT} files: one per interval plus the close"
    );
}
