//! What a transfer does when its drive is REALLY pulled, on a synthetic HFS+
//! image (macOS).
//!
//! `#[ignore]`d: each attaches a real disk image. Hand-run with
//! `cargo nextest run -p cmdr --run-ignored only -E 'test(cmdr_operations::transfer::real_image::)'`,
//! or through `pnpm check disk-images`. Serialized in the `disk-image` nextest
//! group, and machine-wide by the harness's session lock.
//!
//! The unit tests answer the mount table from a hook (`move_vanished_tests.rs`,
//! `transfer_sides_tests.rs`); these prove the same gates against a real detach,
//! where the kernel picks the errno and the mount table really loses an entry.
//!
//! Each cell runs on an engine of its own (`testing::TestOperations`), whose
//! registry the drive coming back registers on, which is what wakes the ledger
//! waiting for it.

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::testing::disk_images::{DiskImage, DiskImageSession, ImageSpec, MountedVolume};

use cmdr_fs::testing::wait_until_async;

use super::chunked_copy::chunk_park;
use super::copy::copy_files_with_progress_inner;
use super::move_op::move_files_with_progress_inner;
use crate::event_sinks::CollectorEventSink;
use crate::host::testing::pane_order_host;
use crate::in_flight_temps::Ledger;
use crate::state::WriteOperationState;
use crate::testing::TestOperations;
use crate::transfer_sides::{TransferSide, TransferSides};
use crate::types::{ConflictResolution, TransferRole, WriteOperationConfig, WriteOperationError, WriteOperationType};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::Volume;
use cmdr_local::LocalPosixVolume;

/// The bytes of the file already on the drive, which an overwrite displaces.
/// Deliberately unlike the source's, so "the original survived" can't pass on
/// the replacement's bytes.
const ORIGINAL_BYTES: &[u8] = b"the user's own footage, already on the drive";

/// Big enough to span several 1 MiB chunks, small enough to write in a moment.
const SOURCE_SIZE: usize = 8 * 1024 * 1024;
/// Where the copy parks: two chunks in, so the file is provably partial.
const PARK_AFTER: u64 = 2 * 1024 * 1024;

/// A Mac-side source folder holding one multi-chunk file.
fn mac_source(name: &str) -> (TestDir, std::path::PathBuf) {
    let dir = TestDir::new(name);
    let file = dir.join("footage.mov");
    std::fs::write(&file, vec![7u8; SOURCE_SIZE]).expect("write the source file");
    (dir, file)
}

fn state_onto(
    engine: &TestOperations,
    ledger: &Ledger,
    image_root: &std::path::Path,
    image_name: &str,
    mac_root: &std::path::Path,
) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_sides(Some(TransferSides::new(
                TransferSide::new("root".to_string(), "Macintosh HD".to_string(), mac_root.to_path_buf()),
                TransferSide::new(
                    "vol-image".to_string(),
                    image_name.to_string(),
                    image_root.to_path_buf(),
                ),
            )))
            .with_in_flight_ledger(ledger.clone()),
    )
}

/// Runs `engine` on its own thread, force-detaches the image once its copy of
/// `source` has parked mid-file, and answers what the engine ended with.
fn detach_mid_transfer(
    image: &DiskImage,
    source: &std::path::Path,
    events: Arc<CollectorEventSink>,
    state: Arc<WriteOperationState>,
    engine: impl FnOnce(Arc<CollectorEventSink>, Arc<WriteOperationState>) -> Result<(), WriteOperationError>
    + Send
    + 'static,
) -> Result<(), WriteOperationError> {
    let park = chunk_park::park_after(source, PARK_AFTER);
    let worker = std::thread::spawn(move || engine(events, state));

    let written = park
        .wait_until_parked(Duration::from_secs(20))
        .expect("the copy parks mid-file");
    // allowed-pluralize-noun: a test's panic message, read by whoever debugs a red lane, and `written` is a byte count in the hundreds of thousands here
    assert!(written >= PARK_AFTER, "parked after {written} bytes");

    image.force_detach().expect("the drive is pulled");
    park.release();
    worker.join().expect("the engine thread finishes")
}

/// Copies `source` onto the image's volume, parks mid-file, pulls the drive, and
/// answers what the engine ended with: the opening move of every copy cell here.
fn copy_until_the_drive_is_pulled(
    engine: &TestOperations,
    ledger: &Ledger,
    image: &DiskImage,
    volume: &MountedVolume,
    op_id: &'static str,
    source: &std::path::Path,
    events: &Arc<CollectorEventSink>,
) -> Result<(), WriteOperationError> {
    let state = state_onto(
        engine,
        ledger,
        &volume.mount_point,
        &volume.name,
        std::path::Path::new("/"),
    );
    engine.status().register(op_id, WriteOperationType::Copy, Vec::new());
    let sources = vec![source.to_path_buf()];
    let destination = volume.mount_point.clone();
    detach_mid_transfer(image, source, Arc::clone(events), state, move |events, state| {
        copy_files_with_progress_inner(
            &*events,
            op_id,
            &state,
            &sources,
            &destination,
            &WriteOperationConfig::default(),
        )
    })
}

/// The whole point of M10: a real detach mid-copy is reported as the drive
/// leaving, named, with how far it got — and the Mac original is untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
async fn a_copy_onto_a_drive_that_is_pulled_mid_file_names_the_drive_and_keeps_the_original() {
    let engine = TestOperations::new();
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach an HFS+ image");
    let volume = image.volumes()[0].clone();
    let (mac_dir, source) = mac_source("real-image-copy");

    let events = Arc::new(CollectorEventSink::new());
    let op_id = "op-real-image-copy";
    let result = copy_until_the_drive_is_pulled(&engine, &Ledger::for_test(), &image, &volume, op_id, &source, &events);

    match &result {
        Err(WriteOperationError::DeviceDisconnected { side, .. }) => {
            let side = side.as_ref().expect("a real detach names the drive that left");
            assert_eq!(side.role, TransferRole::Destination);
            assert_eq!(side.volume_name, volume.name);
        }
        other => panic!("a pulled drive must read as a disconnect, got {other:?}"),
    }

    let errors = events.errors.lock_ignore_poison();
    let event = errors.first().expect("the copy says why it stopped");
    let progress = event
        .progress_at_stop
        .as_ref()
        .expect("the status cache still had the operation when the event was built");
    assert_eq!(progress.files_total, 1, "one file was asked for");

    assert_eq!(
        std::fs::metadata(&source).expect("the Mac original is untouched").len(),
        SOURCE_SIZE as u64,
        "nothing on the Mac side may be touched by a copy"
    );
    drop(mac_dir);
    engine.status().unregister(op_id);
}

/// Every `.cmdr-` scratch name sitting in `dir`, so a cell can say what the
/// drive is carrying rather than guessing at one path.
fn scratch_in(dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| cmdr_fs::staging::is_cmdr_scratch_name(name))
        .collect()
}

/// The ASIDE records the ledger is holding right now.
///
/// `.cmdr-temp-` is the marker only a displaced ORIGINAL wears, so this counts
/// exactly the records whose file is the user's own. ❗ It's the evidence that
/// separates "the sweep put the original back" from "the detach rolled the
/// aside rename back in the journal and the sweep never ran": a rollback leaves
/// the record DEFERRED, because the aside it names isn't on disk to rename.
fn aside_records(ledger: &Ledger) -> Vec<std::path::PathBuf> {
    ledger
        .live_paths()
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(cmdr_fs::staging::STAGING_ASIDE_MARKER))
        })
        .collect()
}

/// Stands in for the drive coming back: the volume registry takes the image's
/// new mount point under the id the transfer was started with, which is what
/// announces the arrival the ledger has been waiting on.
fn drive_is_back(engine: &TestOperations, volume: &MountedVolume) {
    engine.volumes().register(
        "vol-image",
        Arc::new(LocalPosixVolume::local_folder(
            volume.name.clone(),
            &volume.mount_point,
            pane_order_host(),
        )) as Arc<dyn Volume>,
    )
}

/// M11's end of the story, against a real detach: a copy's partial on a drive
/// that was pulled is not forgotten. The record survives the disconnect, waits
/// for the drive rather than resolving against a mount point that isn't there,
/// and is settled the moment the drive is back — in the SAME session.
///
/// ❗ Asserts on what the drive carries, not on one path: a force-detach mid-write
/// may or may not have got the partial's own bytes onto the platter, and either
/// way nothing of Cmdr's may be left behind once the sweep has been.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
async fn a_partial_left_on_a_pulled_drive_is_settled_when_that_drive_comes_back() {
    let engine = TestOperations::new();
    let session = DiskImageSession::acquire();
    let mut image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach an HFS+ image");
    let volume = image.volumes()[0].clone();
    let (mac_dir, source) = mac_source("real-image-arrival-copy");
    let ledger_dir = TestDir::new("real-image-arrival-ledger");
    let ledger = Ledger::recording_in(&ledger_dir);

    let events = Arc::new(CollectorEventSink::new());
    let op_id = "op-real-image-arrival";
    let result = copy_until_the_drive_is_pulled(&engine, &ledger, &image, &volume, op_id, &source, &events);
    assert!(result.is_err(), "a copy onto a pulled drive can't succeed");

    // The engine has unwound. Its partial could not be removed — the drive was
    // gone — so the record has to have survived rather than being read as
    // "already gone" and dropped.
    assert!(
        ledger
            .live_paths()
            .iter()
            .any(|path| path.to_string_lossy().contains(".cmdr-tmp-")),
        "the ledger must still be holding the partial the pulled drive kept"
    );

    image.reattach().expect("the drive is plugged back in");
    let back = image.volumes()[0].clone();
    drive_is_back(&engine, &back);

    wait_until_async(
        Duration::from_secs(20),
        "the returned drive's scratch to be settled",
        || scratch_in(&back.mount_point).is_empty(),
    )
    .await;

    assert_eq!(
        std::fs::metadata(&source).expect("the Mac original is untouched").len(),
        SOURCE_SIZE as u64,
        "and the Mac side is whole, since a copy never touches it"
    );
    drop(ledger);
    drop(mac_dir);
    engine.status().unregister(op_id);
}

/// The window the whole milestone exists for, against a real detach: the user's
/// original is sitting under a scratch name, the replacement hasn't landed, and
/// the drive is pulled right there. When it comes back, those bytes are still
/// findable — at their own name or beside it — and ❌ never removed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
async fn an_original_set_aside_when_the_drive_was_pulled_is_still_there_when_it_comes_back() {
    let engine = TestOperations::new();
    let session = DiskImageSession::acquire();
    let mut image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach an HFS+ image");
    let volume = image.volumes()[0].clone();
    let (mac_dir, source) = mac_source("real-image-aside");
    let ledger_dir = TestDir::new("real-image-aside-ledger");
    let ledger = Ledger::recording_in(&ledger_dir);

    // The file the user already has on the drive, which the copy will replace.
    let original = volume.mount_point.join("footage.mov");
    std::fs::write(&original, ORIGINAL_BYTES).expect("the user's file is on the drive");

    let events = Arc::new(CollectorEventSink::new());
    let state = state_onto(
        &engine,
        &ledger,
        &volume.mount_point,
        &volume.name,
        std::path::Path::new("/"),
    );
    let op_id = "op-real-image-aside";
    engine.status().register(op_id, WriteOperationType::Copy, Vec::new());

    let park = state.scope().aside_park().park_next_overwrite();
    let sources = vec![source.clone()];
    let destination = volume.mount_point.clone();
    let worker = {
        let events = Arc::clone(&events);
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            copy_files_with_progress_inner(
                &*events,
                op_id,
                &state,
                &sources,
                &destination,
                &WriteOperationConfig {
                    conflict_resolution: ConflictResolution::Overwrite,
                    ..WriteOperationConfig::default()
                },
            )
        })
    };

    assert!(
        park.wait_until_parked(Duration::from_secs(30)),
        "the overwrite must park with the original set aside"
    );
    image.force_detach().expect("the drive is pulled mid-overwrite");
    park.release();
    let result = worker.join().expect("the engine thread finishes");
    assert!(result.is_err(), "a copy onto a pulled drive can't succeed");

    // The drive went while the original was under a scratch name and its
    // replacement hadn't landed, so nothing could put it back. The record is the
    // only thing that still knows where the user's file is.
    assert_eq!(
        aside_records(&ledger).len(),
        1,
        "the pulled drive left the original set aside, and the ledger has to be holding it"
    );

    image.reattach().expect("the drive is plugged back in");
    let back = image.volumes()[0].clone();
    drive_is_back(&engine, &back);

    // ❗ Waits on the RECORD, ❌ not on the drive looking clean. A force-detach
    // can roll the aside rename back in the HFS+ journal, and then the bytes sit
    // at their own name with the sweep never having run — a pass that proves
    // nothing. The record only retires once the sweep reached the aside and
    // acted on it; a rollback leaves it deferred and this wait fails loudly.
    wait_until_async(Duration::from_secs(20), "the sweep to settle the aside", || {
        aside_records(&ledger).is_empty()
    })
    .await;
    assert!(
        scratch_in(&back.mount_point).is_empty(),
        "and the sweep leaves nothing of Cmdr's on the drive: {:?}",
        scratch_in(&back.mount_point)
    );

    // The bytes are the point, not the name: the sweep puts them back at
    // `footage.mov` when that name is free and beside it when something else
    // took it. ❌ What it may never do is remove them.
    let landed = back.mount_point.join("footage.mov");
    let recovered = back.mount_point.join("footage (recovered).mov");
    let found = [landed, recovered]
        .into_iter()
        .filter_map(|path| std::fs::read(&path).ok())
        .any(|bytes| bytes == ORIGINAL_BYTES);
    assert!(
        found,
        "the user's original must still be on the drive, at its own name or beside it; the drive holds {:?}",
        std::fs::read_dir(&back.mount_point)
            .map(|entries| entries.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
            .unwrap_or_default()
    );
    drop(ledger);
    drop(mac_dir);
    engine.status().unregister(op_id);
}

/// The same pull during a move. The originals are the only other copy of the
/// data until Phase 4, and a move that can't finish keeps every one of them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
async fn a_move_onto_a_drive_that_is_pulled_mid_file_keeps_every_mac_source() {
    let engine = TestOperations::new();
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach an HFS+ image");
    let volume = image.volumes()[0].clone();
    let (mac_dir, source) = mac_source("real-image-move");

    let events = Arc::new(CollectorEventSink::new());
    let state = state_onto(
        &engine,
        &Ledger::for_test(),
        &volume.mount_point,
        &volume.name,
        std::path::Path::new("/"),
    );
    let op_id = "op-real-image-move";
    engine.status().register(op_id, WriteOperationType::Move, Vec::new());

    let sources = vec![source.clone()];
    let destination = volume.mount_point.clone();
    let result = detach_mid_transfer(
        &image,
        &source,
        Arc::clone(&events),
        Arc::clone(&state),
        move |events, state| {
            move_files_with_progress_inner(
                &*events,
                op_id,
                &state,
                &sources,
                &destination,
                &WriteOperationConfig::default(),
            )
        },
    );

    assert!(result.is_err(), "a move onto a pulled drive can't succeed");
    assert!(
        source.exists(),
        "the source is the only other copy of the data: a move that didn't finish keeps it"
    );
    assert_eq!(
        std::fs::metadata(&source).expect("the source is readable").len(),
        SOURCE_SIZE as u64,
        "and keeps all of its bytes"
    );
    drop(mac_dir);
    engine.status().unregister(op_id);
}

/// A copy onto another volume takes the chunked arm, which sets birth and mtime in
/// one call (HFS+, like APFS, would otherwise drag birth down to the earlier
/// mtime) and puts Locked on after the landing rename it would refuse. The one
/// cell of the metadata matrix (`metadata_matrix_tests.rs`) that crosses a real
/// volume boundary.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
async fn a_copy_onto_another_volume_keeps_birth_time_and_locked() {
    let engine = TestOperations::new();
    use cmdr_fs::testing::metadata_fixture::{
        BIRTH_SECS, LOCKED_FLAG, MODIFIED_SECS, MetadataFixture, birth_secs, flags, modified_secs,
    };

    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach an HFS+ image");
    let volume = image.volumes()[0].clone();
    let fixture = MetadataFixture::new("real-image-birth");
    let source = fixture.rich_file("f.txt");
    let locked = fixture.locked_file("locked.txt");

    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
    let result = copy_files_with_progress_inner(
        &*events,
        "op-real-image-birth",
        &state,
        &[source, locked],
        &volume.mount_point,
        &WriteOperationConfig::default(),
    );
    assert!(result.is_ok(), "the copy lands: {result:?}");

    let landed = volume.mount_point.join("f.txt");
    assert_eq!(modified_secs(&landed), MODIFIED_SECS, "mtime is kept");
    assert_eq!(birth_secs(&landed), BIRTH_SECS, "and birth");
    let landed_locked = volume.mount_point.join("locked.txt");
    assert_eq!(flags(&landed_locked) & LOCKED_FLAG, LOCKED_FLAG, "the copy is Locked");
    assert_eq!(birth_secs(&landed_locked), BIRTH_SECS);
    // The image detaches with the Locked copy on it, which `hdiutil` doesn't mind.
}

// ============================================================================
// An Overwrite never removes a folder a drive is mounted inside
// ============================================================================

/// A file landing on a folder that holds a mounted drive (a cross-type
/// Overwrite the user answered): the landing would set the folder aside and
/// `remove_dir_all` it, emptying the drive. Refused before the temp is written.
#[test]
#[ignore = "attaches a real disk image; run via `pnpm check disk-images`"]
fn a_file_overwriting_a_folder_that_holds_a_mounted_drive_refuses_and_removes_nothing() {
    use crate::host::Announced;
    use crate::mount_boundary::test_support::{NestedMount, assert_refused_at_the_mount};
    use crate::overwrite::{PostLanding, Staged, stage_and_land_file};

    let nested = NestedMount::mount();
    let state = Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(50)));
    let folder = nested.folder();

    let result = stage_and_land_file(&state, Announced::assumed(&folder), true, PostLanding::Apply, |temp| {
        std::fs::write(temp, b"NEW").expect("write the temp");
        Ok(Staged::bytes(3))
    });

    assert_refused_at_the_mount(result.map(|landed| landed.bytes));
    nested.assert_untouched();
}

/// A folder landing on a folder that holds a mounted drive through the
/// set-aside-and-materialize path (`safe_overwrite_dir`, which a move and a
/// symlink copy take): refused before the folder is set aside.
#[test]
#[ignore = "attaches a real disk image; run via `pnpm check disk-images`"]
fn a_safe_overwrite_of_a_folder_that_holds_a_mounted_drive_refuses_and_removes_nothing() {
    use crate::mount_boundary::test_support::{NestedMount, assert_refused_at_the_mount};
    use crate::overwrite::safe_overwrite_dir;

    let nested = NestedMount::mount();
    let state = Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(50)));

    let result = safe_overwrite_dir(&state, &nested.folder(), |dest| {
        std::fs::create_dir(dest).expect("the replacement folder");
        Ok(())
    });

    assert_refused_at_the_mount(result);
    nested.assert_untouched();
}

/// The cross-volume twin: a `LocalPosixVolume` destination's folder set aside
/// for an Overwrite goes as a tree (`cleanup::remove_tree`) once the
/// replacement lands, so the refusal comes before the set-aside.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "attaches a real disk image; run via `pnpm check disk-images`"]
async fn a_volume_overwrite_of_a_folder_that_holds_a_mounted_drive_refuses_and_removes_nothing() {
    use crate::mount_boundary::test_support::{NestedMount, assert_refused_at_the_mount};
    use crate::transfer::volume::displace_destination;

    let nested = NestedMount::mount();
    let state = Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(50)));
    let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "V",
        nested.dir.to_path_buf(),
        pane_order_host(),
    ));

    let result = displace_destination(&state, &volume, std::path::Path::new("folder"), true).await;

    assert_refused_at_the_mount(
        result
            .map(|displaced| displaced.is_some())
            .map_err(|e| e.into_write_error()),
    );
    nested.assert_untouched();
}

// ============================================================================
// A move never carries a mounted drive's files off it
// ============================================================================

/// A move onto another drive copies, then deletes what it copied, so moving a
/// folder that holds a mounted drive would move that drive's files off it.
/// Refused before anything is copied, for the folder and for the mount point.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "attaches a real disk image; run via `pnpm check disk-images`"]
async fn a_move_onto_another_drive_of_a_folder_that_holds_a_mounted_drive_refuses_and_moves_nothing() {
    use crate::mount_boundary::test_support::{NestedMount, assert_refused_at_the_mount};

    let session = DiskImageSession::acquire();
    let nested = NestedMount::mount_in(&session);
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the destination");
    let destination = image.volumes()[0].mount_point.clone();
    let state = Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(50)));

    for source in [nested.folder(), nested.folder().join("mnt")] {
        let result = move_files_with_progress_inner(
            &CollectorEventSink::new(),
            "op-move-nested-mount",
            &state,
            std::slice::from_ref(&source),
            &destination,
            &WriteOperationConfig::default(),
        );

        assert_refused_at_the_mount(result);
        nested.assert_untouched();
        let landed = destination.join(source.file_name().expect("a named source"));
        assert!(!landed.exists(), "nothing was copied: {}", landed.display());
    }
}

/// The cross-volume engine's twin: a `LocalPosixVolume` source moving onto a
/// volume of another kind copies and deletes per file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "attaches a real disk image; run via `pnpm check disk-images`"]
async fn a_volume_move_of_a_folder_that_holds_a_mounted_drive_refuses_and_moves_nothing() {
    use crate::mount_boundary::test_support::{NestedMount, assert_refused_at_the_mount};
    use crate::transfer::volume::move_volumes_with_progress;
    use crate::types::VolumeCopyConfig;
    use cmdr_fs::volume::InMemoryVolume;

    let nested = NestedMount::mount();
    let state = Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(50)));
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "V",
        nested.dir.to_path_buf(),
        pane_order_host(),
    ));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(1 << 30, 1 << 30));

    for source_path in ["folder", "folder/mnt"] {
        let result = move_volumes_with_progress(
            Arc::new(CollectorEventSink::new()),
            "op-volume-move-nested-mount",
            &state,
            Arc::clone(&source),
            &[std::path::PathBuf::from(source_path)],
            Arc::clone(&dest),
            std::path::Path::new("/"),
            &VolumeCopyConfig::default(),
        )
        .await;

        assert_refused_at_the_mount(result.map_err(|failure| failure.error));
        nested.assert_untouched();
        for copied in ["/folder", "/mnt", "/mine.txt", "/precious.txt"] {
            assert!(
                !dest.exists(std::path::Path::new(copied)).await,
                "nothing was copied: {copied}"
            );
        }
    }
}

/// The into-archive twin: a move INTO a zip adds each tree, then deletes what
/// it carried, so a folder holding a mounted drive would leave the drive's
/// files in the zip and gone from the drive. Refused before the zip is opened.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "attaches a real disk image; run via `pnpm check disk-images`"]
async fn a_move_into_a_zip_of_a_folder_that_holds_a_mounted_drive_refuses_and_moves_nothing() {
    use std::io::Write;

    use crate::event_sinks::OperationEventSink;
    use crate::mount_boundary::test_support::{NestedMount, assert_refused_at_the_mount};

    let nested = NestedMount::mount();
    let archive_dir = TestDir::new("nested_mount_into_zip");
    let archive = archive_dir.join("a.zip");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).expect("create the zip"));
    writer
        .start_file("placeholder.txt", zip::write::SimpleFileOptions::default())
        .expect("start an entry");
    writer.write_all(b"x").expect("write the entry");
    writer.finish().expect("finish the zip");
    let archive_before = std::fs::read(&archive).expect("read the zip");

    let engine = TestOperations::new();
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "V",
        nested.dir.to_path_buf(),
        pane_order_host(),
    ));

    for source_path in ["folder", "folder/mnt"] {
        let events = Arc::new(CollectorEventSink::new());
        engine
            .route_archive_copy_into(
                Arc::clone(&events) as Arc<dyn OperationEventSink>,
                Arc::clone(&source),
                vec![std::path::PathBuf::from(source_path)],
                archive.clone(),
                format!("lane-{}", uuid::Uuid::new_v4()),
                ConflictResolution::Overwrite,
                0,
                true, // is_move
                None,
                None,
            )
            .await
            .expect("start the move into the zip");
        wait_until_async(Duration::from_secs(30), "the move to settle", || {
            !events.settled.lock_ignore_poison().is_empty()
        })
        .await;

        let refusal = events
            .errors
            .lock_ignore_poison()
            .first()
            .map(|event| event.error.clone());
        assert_refused_at_the_mount(refusal.map_or(Ok(()), Err));
        nested.assert_untouched();
        assert_eq!(
            std::fs::read(&archive).expect("read the zip"),
            archive_before,
            "nothing was added to the zip"
        );
    }
}

/// The many-small-file barrier cells (#1145), a child so the disk-image lane runs them.
#[path = "real_image_small_files_tests.rs"]
mod small_files;
