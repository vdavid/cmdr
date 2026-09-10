//! The destination free-space pre-flight: what a preview and a copy do with
//! each of the three answers a backend can give to `get_space_info`.
//!
//! ⚠️ Two correct-looking decisions collided here once and made every copy INTO
//! an SFTP server fail after ~500 ms with `IoError { message: "Operation not
//! supported by this volume type" }`, naming the destination path and nothing
//! else. The backend was right to answer `NotSupported` (its protocol really
//! can't ask), and the check was right to exist; what was missing is that
//! "can't tell" and "no room" are different answers.
//!
//! So the suite is a matrix, and both axes matter. The answers are: **can't
//! tell** (`NotSupported`, which must never read as "no room"), **a real
//! ceiling** (the only answer allowed to refuse a copy), and **no ceiling**
//! (used bytes are measured but nothing caps them, the live shape of a
//! quota-less Nextcloud account, which always fits). Each is asserted through
//! BOTH entry points, because `scan_for_volume_copy` and
//! `copy_volumes_with_progress` ask independently and a fix to one leaves the
//! other wrong.
//!
//! One question comes before space: whether the destination folder takes writes
//! at all (`Volume::write_access_at`). A folder that takes none is refused as
//! such, before anything is created or measured, and ❌ never reads as a full
//! one. A phone's `/` is the live case: a read-only system image reporting 0 free,
//! beside shared storage that takes the same copy.
//!
//! Shared fixtures `make_state` / `make_volumes` live in `volume/copy_tests/`
//! (`super::tests`) so they aren't duplicated.

use super::tests::make_state;
use super::*;
use crate::file_system::volume::{InMemoryVolume, UnwritableReason, WriteAccess};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;

// ========================================
// The destination can't tell
// ========================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_preview_of_a_destination_that_cant_report_space_still_scans() {
    // ❗ "Can't tell" is not "no room". A backend answers `NotSupported` here when
    // the protocol genuinely has no way to ask — SFTP is the live case, since
    // `statvfs@openssh.com` isn't reachable from its crate stack — and that
    // honest refusal must not become a preview the user can't open.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(1_000_000, 900_000));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));
    source.create_file(Path::new("/report.pdf"), b"content").await.unwrap();

    let result = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("/report.pdf")],
        dest.as_ref(),
        Path::new("/"),
        10,
    )
    .await
    .expect("a destination that can't report free space is still previewable");

    assert_eq!(result.file_count, 1);
    assert!(
        result.dest_space.is_none(),
        "a destination that can't tell must say so rather than report a made-up number, got {:?}",
        result.dest_space,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_that_cant_report_free_space_is_still_copyable_into() {
    // ❗ The cross-backend contract, not an SFTP quirk: ANY backend may answer
    // `NotSupported` to `get_space_info`, and the trait explicitly allows it. A
    // pre-flight that read the refusal as "no room" would make such a volume a
    // destination nothing can ever be written to, with an error message that
    // names neither the check nor the reason.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    // ❗ No `with_space_info`, so `get_space_info` answers `NotSupported` — the
    // same answer `SftpVolume` gives.
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));
    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-space-unknown",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await;

    let errors: Vec<String> = events
        .errors
        .lock()
        .unwrap()
        .iter()
        .map(|e| format!("{:?}", e.error))
        .collect();
    assert!(
        result.is_ok(),
        "a destination that can't report free space must still be copyable into; errors: {errors:?}",
    );

    // The bytes, not just the absence of an error.
    for (name, content) in [("/a.txt", &b"alpha"[..]), ("/b.txt", &b"bravo"[..])] {
        let landed = dest.read_range(Path::new(name), 0, 64).await.expect("the copy landed");
        assert_eq!(landed, content, "{name} must arrive byte for byte");
    }
}

// ========================================
// The destination reports a real ceiling
// ========================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_preview_of_a_destination_that_cant_hold_the_copy_is_refused() {
    let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
    source
        .create_file(Path::new("/big.bin"), &vec![0u8; 1000])
        .await
        .unwrap();
    let source = Arc::new(source);

    // Dest has only 500 bytes available
    let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(1000, 500));

    let result = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("/big.bin")],
        dest.as_ref(),
        Path::new("/"),
        10,
    )
    .await;

    assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_that_does_report_free_space_still_refuses_what_it_cant_hold() {
    // ❗ The other half, and the one an over-eager fix breaks: tolerating "can't
    // tell" must not turn into ignoring a real "no room". A volume that ANSWERS
    // keeps the check exactly as it was.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(1_000, 4));
    source
        .create_file(Path::new("/big.bin"), b"more than four bytes")
        .await
        .unwrap();

    let failure = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "test-op-space-too-small",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/big.bin")],
        Arc::clone(&dest),
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect_err("a destination that says it has 4 bytes free must refuse 20 bytes");

    assert!(
        matches!(&failure.error, WriteOperationError::InsufficientSpace { .. }),
        "the refusal must stay the typed InsufficientSpace the dialog renders, got {:?}",
        failure.error,
    );
}

// ========================================
// The destination spans several filesystems
// ========================================

/// A phone-shaped destination: the volume's own figure is a full system image,
/// and `/sdcard` is a second filesystem with room.
async fn phone_with_room_under_sdcard() -> Arc<dyn Volume> {
    let dest = InMemoryVolume::new("Phone")
        .with_space_info(1_000, 0)
        .with_space_info_under("/sdcard", 10_000_000, 9_000_000);
    dest.create_directory(Path::new("/sdcard")).await.unwrap();
    dest.create_directory(Path::new("/sdcard/Download")).await.unwrap();
    Arc::new(dest)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_is_judged_by_the_filesystem_its_destination_folder_is_on() {
    // ❗ A phone over ADB is the live case: its `/` is a read-only system image
    // reporting 0 free, and asking the VOLUME refused a 920 KB copy into a Pixel's
    // shared storage with "only has 0 bytes available". Both pre-flights ask
    // about the destination folder.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source
        .create_file(Path::new("/photo.jpg"), b"a photo's worth of bytes")
        .await
        .unwrap();
    let dest = phone_with_room_under_sdcard().await;

    let preview = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("/photo.jpg")],
        dest.as_ref(),
        Path::new("/sdcard/Download"),
        10,
    )
    .await
    .expect("the shared storage has room, so the preview opens");
    assert_eq!(preview.dest_space.and_then(|s| s.available_bytes()), Some(9_000_000));

    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "test-op-space-per-folder",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/photo.jpg")],
        Arc::clone(&dest),
        Path::new("/sdcard/Download"),
        &VolumeCopyConfig::default(),
    )
    .await;
    assert!(
        result.is_ok(),
        "the destination folder's filesystem has room, got {:?}",
        result.err().map(|f| format!("{:?}", f.error)),
    );
    assert!(dest.exists(Path::new("/sdcard/Download/photo.jpg")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_full_destination_folder_refuses_even_when_the_volume_has_room() {
    // The other half: the folder's filesystem is what decides, in both directions.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source
        .create_file(Path::new("/big.bin"), b"more than four bytes")
        .await
        .unwrap();
    let dest = InMemoryVolume::new("Phone")
        .with_space_info(10_000_000, 9_000_000)
        .with_space_info_under("/storage/card", 1_000, 4);
    dest.create_directory(Path::new("/storage")).await.unwrap();
    dest.create_directory(Path::new("/storage/card")).await.unwrap();
    let dest: Arc<dyn Volume> = Arc::new(dest);

    let failure = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "test-op-space-full-folder",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/big.bin")],
        Arc::clone(&dest),
        Path::new("/storage/card"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect_err("a folder whose filesystem has 4 bytes free must refuse 20 bytes");
    assert!(
        matches!(
            &failure.error,
            WriteOperationError::InsufficientSpace { available: 4, .. }
        ),
        "got {:?}",
        failure.error,
    );
}

// ========================================
// The destination measures but has no ceiling
// ========================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_with_no_ceiling_accepts_a_copy_bigger_than_anything_it_holds() {
    // ❗ The third answer, and the one that shipped broken: a destination that
    // MEASURED and has no ceiling. A quota-less Nextcloud account is the live
    // case, and it is the DEFAULT state of a real account, so reading it as
    // "no room" would refuse every legitimate copy to every stock Nextcloud user.
    // The volume here holds far more than it is being sent, which is exactly the
    // arithmetic that would trip a check keying off `used_bytes`.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_unbounded_space_info(64 * 1024 * 1024));
    source
        .create_file(Path::new("/big.bin"), b"more than four bytes")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-space-unbounded",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/big.bin")],
        Arc::clone(&dest),
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await;

    assert!(
        result.is_ok(),
        "storage with no ceiling is the one destination a copy always fits into, got {:?}",
        result.err().map(|f| format!("{:?}", f.error)),
    );
    assert!(dest.exists(Path::new("/big.bin")).await, "the file has to be there");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_preview_of_a_destination_with_no_ceiling_carries_what_it_holds() {
    // The dialog's half of the same answer: the preview opens, AND it carries the
    // used figure the server gave rather than dropping it the way `NotSupported`
    // would. ❗ `available_bytes()` staying `None` is what keeps the pane from
    // drawing a bar and the warning bands from firing.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(1_000_000, 900_000));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_unbounded_space_info(67_108_864));
    source.create_file(Path::new("/report.pdf"), b"content").await.unwrap();

    let result = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("/report.pdf")],
        dest.as_ref(),
        Path::new("/"),
        10,
    )
    .await
    .expect("a destination with no ceiling is still previewable");

    assert_eq!(result.dest_space, Some(SpaceInfo::Unbounded { used_bytes: 67_108_864 }));
}

// ========================================
// The destination folder takes no writes
// ========================================

/// A phone-shaped destination: a read-only system image at `/` reporting 0 free,
/// beside shared storage under `/sdcard` that takes writes and has room.
async fn phone_with_a_read_only_root() -> Arc<dyn Volume> {
    let dest = InMemoryVolume::new("Phone")
        .with_space_info(1_000, 0)
        .with_space_info_under("/sdcard", 10_000_000, 9_000_000)
        .with_write_access_under(
            "/",
            WriteAccess::Unwritable {
                reason: UnwritableReason::ReadOnlyFilesystem,
            },
        )
        .with_write_access_under("/sdcard", WriteAccess::Writable);
    dest.create_directory(Path::new("/sdcard")).await.unwrap();
    Arc::new(dest)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_into_a_folder_that_takes_no_writes_is_refused_as_such_before_anything_is_created_or_measured() {
    // ❗ The live case: a copy onto a Pixel's `/` said "Not enough space: the
    // destination needs X but only has 0 bytes available". `df` was right about
    // the 0 and wrong as the reason, since nothing lands there at any size. The
    // write-access answer comes before Phase 0.5 creates the folder and before
    // the space check.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source
        .create_file(Path::new("/photo.jpg"), b"a photo's worth of bytes")
        .await
        .unwrap();
    let dest = phone_with_a_read_only_root().await;

    let failure = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "test-op-unwritable-dest",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/photo.jpg")],
        Arc::clone(&dest),
        Path::new("/system/New"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect_err("nothing can land in a read-only folder");

    assert!(
        matches!(
            &failure.error,
            WriteOperationError::DestinationNotWritable {
                path,
                reason: UnwritableReason::ReadOnlyFilesystem,
            } if path == "/system/New"
        ),
        "a read-only destination must never read as a full one, got {:?}",
        failure.error,
    );
    assert!(
        !dest.exists(Path::new("/system/New")).await,
        "the refusal comes before Phase 0.5 creates the destination folder"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_preview_of_a_folder_that_takes_no_writes_says_so_instead_of_refusing_for_space() {
    // The dialog's half: the preview describes the destination (it takes no
    // writes) rather than failing with a space shortfall that isn't the reason.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(1_000_000, 900_000));
    source
        .create_file(Path::new("/photo.jpg"), b"a photo's worth of bytes")
        .await
        .unwrap();
    let dest = phone_with_a_read_only_root().await;

    let preview = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("/photo.jpg")],
        dest.as_ref(),
        Path::new("/"),
        10,
    )
    .await
    .expect("the preview describes an unwritable destination rather than refusing for space");

    assert_eq!(
        preview.dest_write_access,
        WriteAccess::Unwritable {
            reason: UnwritableReason::ReadOnlyFilesystem,
        },
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_that_takes_writes_beside_a_read_only_root_still_copies() {
    // The other half, and the one an over-eager fix breaks: the answer is about
    // the FOLDER. The shared storage beside the read-only root takes the copy.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source
        .create_file(Path::new("/photo.jpg"), b"a photo's worth of bytes")
        .await
        .unwrap();
    let dest = phone_with_a_read_only_root().await;

    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "test-op-writable-beside-read-only",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/photo.jpg")],
        Arc::clone(&dest),
        Path::new("/sdcard/New"),
        &VolumeCopyConfig::default(),
    )
    .await;

    assert!(
        result.is_ok(),
        "the shared storage takes writes, got {:?}",
        result.err().map(|f| format!("{:?}", f.error)),
    );
    assert!(dest.exists(Path::new("/sdcard/New/photo.jpg")).await);
}
