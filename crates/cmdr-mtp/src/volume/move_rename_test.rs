//! A rename that also changes folder: `MtpVolume::rename` across folders with a
//! new name, which MTP can only do as two calls (`MoveObject`, then a rename).
//!
//! ❗ Regression anchor: it moved first, under the ORIGINAL name. When the
//! destination folder already held that name, the device took the move as a
//! replace (Android's `MoveObject` is a filesystem rename, and so is the
//! virtual device's), so the user's file there was lost and the moved one then
//! renamed onto the new name.

use std::path::Path;

use cmdr_fs::volume::Volume;

use crate::testing::{connect_fixture, device_lock, test_connection_manager, volume_for};
use crate::virtual_device::setup_virtual_mtp_device;

const NOTES: &str = "Meeting notes: discuss MTP E2E test strategy.\n";

fn on_disk(root: &Path, relative: &str) -> Option<String> {
    std::fs::read_to_string(root.join("internal").join(relative)).ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_with_a_new_name_spares_a_file_holding_the_old_name_in_the_destination() {
    let _guard = device_lock().await;
    let fixture = setup_virtual_mtp_device();
    std::fs::write(fixture.root().join("internal/Music/notes.txt"), "bystander").expect("the bystander");
    let device = connect_fixture(test_connection_manager(), fixture).await;
    let volume = volume_for(test_connection_manager(), &device, None).await;

    volume
        .rename(
            Path::new("/Documents/notes.txt"),
            Path::new("/Music/2026 notes.txt"),
            false,
        )
        .await
        .expect("the move with a new name");

    let root = device.root();
    assert_eq!(on_disk(root, "Music/2026 notes.txt").as_deref(), Some(NOTES));
    assert_eq!(
        on_disk(root, "Music/notes.txt").as_deref(),
        Some("bystander"),
        "the file already holding the old name is untouched"
    );
    assert_eq!(on_disk(root, "Documents/notes.txt"), None, "moved, not copied");

    device.teardown(test_connection_manager()).await;
}

/// A name one past what a PTP string holds: `mtp-rs` refuses it before sending
/// anything (older releases garbled its length byte instead), and the refusal
/// reaches the caller typed, with the file untouched and the session still usable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_to_a_name_too_long_for_mtp_is_refused_as_an_invalid_name() {
    let _guard = device_lock().await;
    let fixture = setup_virtual_mtp_device();
    let device = connect_fixture(test_connection_manager(), fixture).await;
    let volume = volume_for(test_connection_manager(), &device, None).await;

    let long = format!("{}.txt", "a".repeat(mtp_rs::MAX_NAME_UNITS + 1 - ".txt".len()));
    let refused = volume
        .rename(
            Path::new("/Documents/notes.txt"),
            Path::new(&format!("/Documents/{long}")),
            false,
        )
        .await;
    assert!(
        matches!(refused, Err(cmdr_fs::volume::VolumeError::InvalidName(_))),
        "got {refused:?}"
    );

    let root = device.root();
    assert_eq!(
        on_disk(root, "Documents/notes.txt").as_deref(),
        Some(NOTES),
        "untouched"
    );
    volume
        .rename(
            Path::new("/Documents/notes.txt"),
            Path::new("/Documents/notes 2.txt"),
            false,
        )
        .await
        .expect("the session still takes a rename");

    device.teardown(test_connection_manager()).await;
}

async fn read(volume: &crate::volume::MtpVolume, path: &str) -> Result<Vec<u8>, cmdr_fs::volume::VolumeError> {
    let mut stream = volume.open_read_stream(Path::new(path)).await?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend(chunk?);
    }
    Ok(bytes)
}

/// A folder read through after it moved with a new name, and again after it
/// moved back: its children answer at each new path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_moved_with_a_new_name_and_back_reads_its_children_at_each_path() {
    let _guard = device_lock().await;
    let fixture = setup_virtual_mtp_device();
    std::fs::create_dir_all(fixture.root().join("internal/Documents/scans")).expect("a folder");
    std::fs::write(fixture.root().join("internal/Documents/scans/page.png"), "page").expect("a child");
    let device = connect_fixture(test_connection_manager(), fixture).await;
    let volume = volume_for(test_connection_manager(), &device, None).await;

    assert_eq!(
        read(&volume, "/Documents/scans/page.png").await.expect("before"),
        b"page"
    );
    volume
        .rename(Path::new("/Documents/scans"), Path::new("/Music/2026 scans"), false)
        .await
        .expect("away");
    assert_eq!(
        read(&volume, "/Music/2026 scans/page.png").await.expect("there"),
        b"page"
    );
    volume
        .rename(Path::new("/Music/2026 scans"), Path::new("/Documents/scans"), false)
        .await
        .expect("back");
    assert_eq!(
        read(&volume, "/Documents/scans/page.png").await.expect("back again"),
        b"page"
    );

    device.teardown(test_connection_manager()).await;
}

/// Both folders hold the other's name: the new name is taken where the item
/// starts, and its old name where it goes. It still lands as asked, touching
/// neither.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_with_a_new_name_taken_in_both_folders_spares_both_files() {
    let _guard = device_lock().await;
    let fixture = setup_virtual_mtp_device();
    std::fs::write(fixture.root().join("internal/Music/notes.txt"), "bystander").expect("the bystander");
    std::fs::write(fixture.root().join("internal/Documents/2026 notes.txt"), "neighbour").expect("the neighbour");
    let device = connect_fixture(test_connection_manager(), fixture).await;
    let volume = volume_for(test_connection_manager(), &device, None).await;

    volume
        .rename(
            Path::new("/Documents/notes.txt"),
            Path::new("/Music/2026 notes.txt"),
            false,
        )
        .await
        .expect("the move with a new name");

    let root = device.root();
    assert_eq!(on_disk(root, "Music/2026 notes.txt").as_deref(), Some(NOTES));
    assert_eq!(on_disk(root, "Music/notes.txt").as_deref(), Some("bystander"));
    assert_eq!(on_disk(root, "Documents/2026 notes.txt").as_deref(), Some("neighbour"));
    assert_eq!(on_disk(root, "Documents/notes.txt"), None);

    device.teardown(test_connection_manager()).await;
}
