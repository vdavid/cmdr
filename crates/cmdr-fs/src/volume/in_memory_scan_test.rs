//! `InMemoryVolume`'s scan surface: `scan_for_copy`, the batch default the trait
//! provides, space info, and `scan_for_conflicts`. Core CRUD is
//! `in_memory_test.rs`; streaming is `in_memory_stream_test.rs`.

use super::*;
use std::path::{Path, PathBuf};

// ============================================================================
// scan_for_copy tests
// ============================================================================

#[tokio::test]
async fn test_scan_for_copy_single_file() {
    let volume = InMemoryVolume::new("Test");
    volume
        .create_file(Path::new("/report.txt"), b"Hello, World!")
        .await
        .unwrap();

    let result = volume.scan_for_copy(Path::new("/report.txt")).await.unwrap();
    assert_eq!(result.file_count, 1);
    assert_eq!(result.dir_count, 0);
    assert_eq!(result.total_bytes, 13);
}

#[tokio::test]
async fn test_scan_for_copy_empty_directory() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/empty")).await.unwrap();

    let result = volume.scan_for_copy(Path::new("/empty")).await.unwrap();
    assert_eq!(result.file_count, 0);
    assert_eq!(result.dir_count, 0);
    assert_eq!(result.total_bytes, 0);
}

#[tokio::test]
async fn test_scan_for_copy_directory_with_files() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/docs")).await.unwrap();
    volume
        .create_file(Path::new("/docs/readme.txt"), b"Read me")
        .await
        .unwrap();
    volume
        .create_file(Path::new("/docs/notes.txt"), b"Notes here")
        .await
        .unwrap();

    let result = volume.scan_for_copy(Path::new("/docs")).await.unwrap();
    assert_eq!(result.file_count, 2);
    assert_eq!(result.dir_count, 0);
    assert_eq!(result.total_bytes, 17); // 7 + 10
}

#[tokio::test]
async fn test_scan_for_copy_nested_directory_tree() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/root")).await.unwrap();
    volume.create_directory(Path::new("/root/sub")).await.unwrap();
    volume.create_directory(Path::new("/root/sub/deep")).await.unwrap();
    volume.create_file(Path::new("/root/file1.txt"), b"AAA").await.unwrap();
    volume
        .create_file(Path::new("/root/sub/file2.txt"), b"BBBBB")
        .await
        .unwrap();
    volume
        .create_file(Path::new("/root/sub/deep/file3.txt"), b"C")
        .await
        .unwrap();

    let result = volume.scan_for_copy(Path::new("/root")).await.unwrap();
    assert_eq!(result.file_count, 3);
    assert_eq!(result.dir_count, 2); // sub + deep (root not counted)
    assert_eq!(result.total_bytes, 9); // 3 + 5 + 1
}

// ============================================================================
// scan_for_copy_batch tests (default implementation via Volume trait)
// ============================================================================

#[tokio::test]
async fn test_scan_for_copy_batch_multiple_files_same_dir() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/photos")).await.unwrap();
    volume.create_file(Path::new("/photos/a.jpg"), &[0; 100]).await.unwrap();
    volume.create_file(Path::new("/photos/b.jpg"), &[0; 200]).await.unwrap();
    volume.create_file(Path::new("/photos/c.jpg"), &[0; 300]).await.unwrap();

    let paths = vec![
        PathBuf::from("/photos/a.jpg"),
        PathBuf::from("/photos/b.jpg"),
        PathBuf::from("/photos/c.jpg"),
    ];
    let result = volume.scan_for_copy_batch(&paths).await.unwrap();
    assert_eq!(result.aggregate.file_count, 3);
    assert_eq!(result.aggregate.dir_count, 0);
    assert_eq!(result.aggregate.total_bytes, 600);
    assert_eq!(result.per_path.len(), 3);
}

#[tokio::test]
async fn test_scan_for_copy_batch_mixed_files_and_dirs() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/stuff")).await.unwrap();
    volume
        .create_file(Path::new("/stuff/readme.txt"), b"hello")
        .await
        .unwrap();
    volume.create_directory(Path::new("/stuff/subdir")).await.unwrap();
    volume
        .create_file(Path::new("/stuff/subdir/deep.txt"), &[0; 50])
        .await
        .unwrap();

    let paths = vec![PathBuf::from("/stuff/readme.txt"), PathBuf::from("/stuff/subdir")];
    let result = volume.scan_for_copy_batch(&paths).await.unwrap();
    assert_eq!(result.aggregate.file_count, 2); // readme.txt + deep.txt
    assert_eq!(result.aggregate.dir_count, 0); // subdir's children don't include extra dirs
    assert_eq!(result.aggregate.total_bytes, 55); // 5 + 50
    assert_eq!(result.per_path.len(), 2);
    // The file entry should report top_level_is_directory=false; the dir one true.
    let readme = result
        .per_path
        .iter()
        .find(|(p, _)| p == Path::new("/stuff/readme.txt"))
        .unwrap();
    assert!(!readme.1.top_level_is_directory);
    assert_eq!(readme.1.total_bytes, 5);
    let subdir = result
        .per_path
        .iter()
        .find(|(p, _)| p == Path::new("/stuff/subdir"))
        .unwrap();
    assert!(subdir.1.top_level_is_directory);
}

#[tokio::test]
async fn test_scan_for_copy_batch_empty_input() {
    let volume = InMemoryVolume::new("Test");
    let result = volume.scan_for_copy_batch(&[]).await.unwrap();
    assert_eq!(result.aggregate.file_count, 0);
    assert_eq!(result.aggregate.dir_count, 0);
    assert_eq!(result.aggregate.total_bytes, 0);
    assert!(result.per_path.is_empty());
}

#[tokio::test]
async fn test_scan_for_copy_batch_single_item_matches_single_scan() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/docs")).await.unwrap();
    volume.create_file(Path::new("/docs/a.txt"), b"data").await.unwrap();

    let single = volume.scan_for_copy(Path::new("/docs/a.txt")).await.unwrap();
    let batch = volume
        .scan_for_copy_batch(&[PathBuf::from("/docs/a.txt")])
        .await
        .unwrap();
    assert_eq!(single.file_count, batch.aggregate.file_count);
    assert_eq!(single.dir_count, batch.aggregate.dir_count);
    assert_eq!(single.total_bytes, batch.aggregate.total_bytes);
    assert_eq!(batch.per_path.len(), 1);
}

#[tokio::test]
async fn test_scan_for_copy_batch_files_from_different_dirs() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/a")).await.unwrap();
    volume.create_directory(Path::new("/b")).await.unwrap();
    volume.create_file(Path::new("/a/file1.txt"), &[0; 10]).await.unwrap();
    volume.create_file(Path::new("/b/file2.txt"), &[0; 20]).await.unwrap();

    let paths = vec![PathBuf::from("/a/file1.txt"), PathBuf::from("/b/file2.txt")];
    let result = volume.scan_for_copy_batch(&paths).await.unwrap();
    assert_eq!(result.aggregate.file_count, 2);
    assert_eq!(result.aggregate.total_bytes, 30);
    assert_eq!(result.per_path.len(), 2);
}

// ============================================================================
// get_space_info tests
// ============================================================================

#[tokio::test]
async fn test_get_space_info_not_supported_by_default() {
    let volume = InMemoryVolume::new("Test");
    assert!(matches!(volume.get_space_info().await, Err(VolumeError::NotSupported)));
}

#[tokio::test]
async fn test_get_space_info_with_configured_space() {
    let volume = InMemoryVolume::new("Test").with_space_info(1_000_000, 500_000);
    let space = volume.get_space_info().await.unwrap();
    assert_eq!(space.total_bytes, 1_000_000);
    assert_eq!(space.available_bytes, 500_000);
    assert_eq!(space.used_bytes, 500_000);
}

// ============================================================================
// scan_for_conflicts tests
// ============================================================================

#[tokio::test]
async fn test_scan_for_conflicts_no_conflicts() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/existing.txt"), b"data").await.unwrap();

    let source_items = vec![SourceItemInfo {
        name: "other.txt".to_string(),
        size: 100,
        modified: None,
        is_directory: false,
    }];

    let conflicts = volume.scan_for_conflicts(&source_items, Path::new("/")).await.unwrap();
    assert!(conflicts.is_empty());
}

#[tokio::test]
async fn test_scan_for_conflicts_detects_conflict() {
    let volume = InMemoryVolume::new("Test");
    volume
        .create_file(Path::new("/report.txt"), b"old content")
        .await
        .unwrap();

    let source_items = vec![SourceItemInfo {
        name: "report.txt".to_string(),
        size: 200,
        modified: Some(1_700_000_000),
        is_directory: false,
    }];

    let conflicts = volume.scan_for_conflicts(&source_items, Path::new("/")).await.unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].source_path, "report.txt");
    assert_eq!(conflicts[0].source_size, 200);
    assert_eq!(conflicts[0].dest_size, 11); // "old content"
    // File vs file: both flags false.
    assert!(!conflicts[0].source_is_directory);
    assert!(!conflicts[0].dest_is_directory);
}

#[tokio::test]
async fn test_scan_for_conflicts_populates_directory_flags() {
    let volume = InMemoryVolume::new("Test");
    // Dest holds a directory `photos` and a file `notes.txt`.
    volume.create_directory(Path::new("/photos")).await.unwrap();
    volume.create_file(Path::new("/notes.txt"), b"hello").await.unwrap();

    // Source offers: a dir `photos` (→ dir-vs-dir merge), a file `notes.txt`
    // (→ file-vs-file), and a file `photos.zip` that doesn't clash.
    let source_items = vec![
        SourceItemInfo {
            name: "photos".to_string(),
            size: 0,
            modified: None,
            is_directory: true,
        },
        SourceItemInfo {
            name: "notes.txt".to_string(),
            size: 99,
            modified: None,
            is_directory: false,
        },
        SourceItemInfo {
            name: "photos.zip".to_string(),
            size: 12,
            modified: None,
            is_directory: false,
        },
    ];

    let conflicts = volume.scan_for_conflicts(&source_items, Path::new("/")).await.unwrap();
    assert_eq!(conflicts.len(), 2);

    let dir_conflict = conflicts.iter().find(|c| c.source_path == "photos").unwrap();
    assert!(dir_conflict.source_is_directory, "source dir flag");
    assert!(dir_conflict.dest_is_directory, "dest dir flag");

    let file_conflict = conflicts.iter().find(|c| c.source_path == "notes.txt").unwrap();
    assert!(!file_conflict.source_is_directory);
    assert!(!file_conflict.dest_is_directory);
}

#[tokio::test]
async fn test_scan_for_conflicts_type_mismatch_flags() {
    let volume = InMemoryVolume::new("Test");
    // Dest `data` is a file; source `data` is a directory.
    volume.create_file(Path::new("/data"), b"x").await.unwrap();

    let source_items = vec![SourceItemInfo {
        name: "data".to_string(),
        size: 0,
        modified: None,
        is_directory: true,
    }];

    let conflicts = volume.scan_for_conflicts(&source_items, Path::new("/")).await.unwrap();
    assert_eq!(conflicts.len(), 1);
    assert!(conflicts[0].source_is_directory, "source is a dir");
    assert!(!conflicts[0].dest_is_directory, "dest is a file");
}

#[test]
fn test_supports_export() {
    let volume = InMemoryVolume::new("Test");
    assert!(volume.supports_export());
}
