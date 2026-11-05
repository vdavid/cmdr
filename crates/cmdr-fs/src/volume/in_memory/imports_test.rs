use std::path::Path;

use super::super::InMemoryVolume;
use super::*;
use crate::volume::{Volume, VolumeError};

/// A store holding `/home` (a folder) and `/home/notes.md` (synthetic).
fn store() -> InMemoryVolume {
    InMemoryVolume::with_entries(
        "imports",
        vec![
            FileEntry::new("home".into(), "/home".into(), true, false),
            FileEntry::new("notes.md".into(), "/home/notes.md".into(), false, false),
        ],
    )
    .with_content("/home/notes.md", InMemoryContent::synthetic(7, 100))
}

async fn read_all(volume: &InMemoryVolume, path: &str) -> Vec<u8> {
    let size = volume
        .get_metadata(Path::new(path))
        .await
        .expect("held")
        .size
        .unwrap_or(0);
    volume
        .read_range(Path::new(path), 0, size as usize)
        .await
        .expect("readable")
}

#[tokio::test]
async fn an_imported_file_holds_its_real_bytes_beside_the_synthetic_ones() {
    let volume = store();
    volume
        .import_file(Path::new("/home/hello.txt"), b"hello".to_vec(), Some(1_700_000_000))
        .await
        .expect("imports");

    let entry = volume.get_metadata(Path::new("/home/hello.txt")).await.expect("listed");
    assert_eq!(entry.size, Some(5));
    assert_eq!(entry.modified_at, Some(1_700_000_000));
    assert!(!entry.is_directory);
    assert_eq!(read_all(&volume, "/home/hello.txt").await, b"hello");
    // The seeded file is untouched.
    assert_eq!(read_all(&volume, "/home/notes.md").await.len(), 100);
}

#[tokio::test]
async fn an_imported_folder_keeps_its_structure() {
    let volume = store();
    volume
        .import_folder(Path::new("/home/trip"), None)
        .await
        .expect("folder");
    volume
        .import_folder(Path::new("/home/trip/day 1"), None)
        .await
        .expect("subfolder");
    volume
        .import_file(Path::new("/home/trip/day 1/a.jpg"), vec![1, 2, 3], None)
        .await
        .expect("file");

    let listed = volume
        .list_directory(Path::new("/home/trip"), None)
        .await
        .expect("lists");
    assert_eq!(listed.len(), 1);
    assert!(listed[0].is_directory);
    assert_eq!(read_all(&volume, "/home/trip/day 1/a.jpg").await, vec![1, 2, 3]);
}

#[tokio::test]
async fn an_import_never_replaces_a_taken_name() {
    let volume = store();
    let file = volume
        .import_file(Path::new("/home/notes.md"), b"new".to_vec(), None)
        .await;
    assert!(matches!(file, Err(VolumeError::AlreadyExists(_))), "{file:?}");
    let folder = volume.import_folder(Path::new("/home/notes.md"), None).await;
    assert!(matches!(folder, Err(VolumeError::AlreadyExists(_))), "{folder:?}");
    assert_eq!(read_all(&volume, "/home/notes.md").await.len(), 100);
}

#[tokio::test]
async fn an_import_needs_its_parent_folder() {
    let volume = store();
    let refused = volume
        .import_file(Path::new("/nowhere/a.txt"), b"a".to_vec(), None)
        .await;
    assert!(matches!(refused, Err(VolumeError::NotFound(_))), "{refused:?}");
    // A file isn't a parent either.
    let under_file = volume.import_folder(Path::new("/home/notes.md/x"), None).await;
    assert!(matches!(under_file, Err(VolumeError::NotFound(_))), "{under_file:?}");
    // The root is.
    volume
        .import_folder(Path::new("/drops"), None)
        .await
        .expect("at the root");
}

#[tokio::test]
async fn an_import_the_disk_has_no_room_for_stores_nothing() {
    let volume = store().with_capacity(104);
    let refused = volume.import_file(Path::new("/home/big.bin"), vec![0; 5], None).await;
    assert!(matches!(refused, Err(VolumeError::StorageFull { .. })), "{refused:?}");
    assert!(volume.get_metadata(Path::new("/home/big.bin")).await.is_err());
    volume
        .import_file(Path::new("/home/small.bin"), vec![0; 4], None)
        .await
        .expect("fits");
}
