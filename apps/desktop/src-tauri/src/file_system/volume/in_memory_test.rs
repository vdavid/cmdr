//! Tests for InMemoryVolume.

use super::*;
use std::path::{Path, PathBuf};

#[tokio::test]
async fn test_new_creates_empty_volume() {
    let volume = InMemoryVolume::new("Test");
    assert_eq!(volume.name(), "Test");
    assert_eq!(volume.root(), Path::new("/"));

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn test_with_entries_populates_volume() {
    let entries = vec![
        FileEntry {
            size: Some(1024),
            modified_at: Some(1_640_000_000),
            created_at: Some(1_639_000_000),
            permissions: 0o644,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("test.txt".to_string(), "/test.txt".to_string(), false, false)
        },
        FileEntry {
            modified_at: Some(1_640_000_000),
            created_at: Some(1_639_000_000),
            permissions: 0o755,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("folder".to_string(), "/folder".to_string(), true, false)
        },
    ];

    let volume = InMemoryVolume::with_entries("Test", entries);
    let result = volume.list_directory(Path::new(""), None).await.unwrap();

    assert_eq!(result.len(), 2);
    // Directories should be first (sorted)
    assert_eq!(result[0].name, "folder");
    assert!(result[0].is_directory);
    assert_eq!(result[1].name, "test.txt");
    assert!(!result[1].is_directory);
}

#[tokio::test]
async fn test_with_file_count_creates_correct_number() {
    let volume = InMemoryVolume::with_file_count("Test", 100);
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();

    assert_eq!(entries.len(), 100);
    assert!(entries[0].name.starts_with("file_"));
}

#[tokio::test]
async fn test_with_file_count_stress_test() {
    // Verify we can handle large file counts for stress testing
    let volume = InMemoryVolume::with_file_count("Test", 50_000);
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();

    assert_eq!(entries.len(), 50_000);
}

#[tokio::test]
async fn test_exists_returns_true_for_existing() {
    let entries = vec![FileEntry {
        size: Some(100),
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new("test.txt".to_string(), "/test.txt".to_string(), false, false)
    }];

    let volume = InMemoryVolume::with_entries("Test", entries);

    assert!(volume.exists(Path::new("/test.txt")).await);
    assert!(volume.exists(Path::new("test.txt")).await); // Relative path
}

#[tokio::test]
async fn test_exists_returns_false_for_nonexistent() {
    let volume = InMemoryVolume::new("Test");
    assert!(!volume.exists(Path::new("/nonexistent.txt")).await);
}

#[tokio::test]
async fn test_get_metadata_returns_correct_entry() {
    let entries = vec![FileEntry {
        size: Some(1024),
        modified_at: Some(1_640_000_000),
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new("test.txt".to_string(), "/test.txt".to_string(), false, false)
    }];

    let volume = InMemoryVolume::with_entries("Test", entries);
    let result = volume.get_metadata(Path::new("/test.txt")).await.unwrap();

    assert_eq!(result.name, "test.txt");
    assert_eq!(result.size, Some(1024));
}

#[tokio::test]
async fn test_get_metadata_nonexistent_returns_error() {
    let volume = InMemoryVolume::new("Test");
    let result = volume.get_metadata(Path::new("/nonexistent.txt")).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), VolumeError::NotFound(_)));
}

#[tokio::test]
async fn test_create_file_then_exists() {
    let volume = InMemoryVolume::new("Test");

    volume
        .create_file(Path::new("/test.txt"), b"Hello, World!")
        .await
        .unwrap();

    assert!(volume.exists(Path::new("/test.txt")).await);

    let metadata = volume.get_metadata(Path::new("/test.txt")).await.unwrap();
    assert_eq!(metadata.name, "test.txt");
    assert_eq!(metadata.size, Some(13)); // "Hello, World!" is 13 bytes
    assert!(!metadata.is_directory);
}

#[tokio::test]
async fn test_create_directory_then_exists() {
    let volume = InMemoryVolume::new("Test");

    volume.create_directory(Path::new("/mydir")).await.unwrap();

    assert!(volume.exists(Path::new("/mydir")).await);

    let metadata = volume.get_metadata(Path::new("/mydir")).await.unwrap();
    assert_eq!(metadata.name, "mydir");
    assert!(metadata.is_directory);
}

#[tokio::test]
async fn test_delete_removes_entry() {
    let volume = InMemoryVolume::new("Test");

    volume.create_file(Path::new("/test.txt"), b"content").await.unwrap();
    assert!(volume.exists(Path::new("/test.txt")).await);

    volume.delete(Path::new("/test.txt")).await.unwrap();
    assert!(!volume.exists(Path::new("/test.txt")).await);
}

#[tokio::test]
async fn test_delete_nonexistent_returns_error() {
    let volume = InMemoryVolume::new("Test");

    let result = volume.delete(Path::new("/nonexistent.txt")).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), VolumeError::NotFound(_)));
}

#[tokio::test]
async fn test_list_directory_sorts_correctly() {
    let entries = vec![
        FileEntry {
            size: Some(100),
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("zebra.txt".to_string(), "/zebra.txt".to_string(), false, false)
        },
        FileEntry {
            permissions: 0o755,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("alpha".to_string(), "/alpha".to_string(), true, false)
        },
        FileEntry {
            size: Some(50),
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("apple.txt".to_string(), "/apple.txt".to_string(), false, false)
        },
        FileEntry {
            permissions: 0o755,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("beta".to_string(), "/beta".to_string(), true, false)
        },
    ];

    let volume = InMemoryVolume::with_entries("Test", entries);
    let result = volume.list_directory(Path::new(""), None).await.unwrap();

    // Expected order: directories first (alpha, beta), then files (apple.txt, zebra.txt)
    assert_eq!(result[0].name, "alpha");
    assert!(result[0].is_directory);
    assert_eq!(result[1].name, "beta");
    assert!(result[1].is_directory);
    assert_eq!(result[2].name, "apple.txt");
    assert!(!result[2].is_directory);
    assert_eq!(result[3].name, "zebra.txt");
    assert!(!result[3].is_directory);
}

#[tokio::test]
async fn test_list_subdirectory() {
    let entries = vec![
        FileEntry {
            permissions: 0o755,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("subdir".to_string(), "/subdir".to_string(), true, false)
        },
        FileEntry {
            size: Some(100),
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new(
                "file_in_subdir.txt".to_string(),
                "/subdir/file_in_subdir.txt".to_string(),
                false,
                false,
            )
        },
        FileEntry {
            size: Some(50),
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("root_file.txt".to_string(), "/root_file.txt".to_string(), false, false)
        },
    ];

    let volume = InMemoryVolume::with_entries("Test", entries);

    // List root - should only show subdir and root_file.txt
    let root_entries = volume.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(root_entries.len(), 2);

    // List subdir - should only show file_in_subdir.txt
    let subdir_entries = volume.list_directory(Path::new("/subdir"), None).await.unwrap();
    assert_eq!(subdir_entries.len(), 1);
    assert_eq!(subdir_entries[0].name, "file_in_subdir.txt");
}

#[test]
fn test_supports_watching_returns_false() {
    let volume = InMemoryVolume::new("Test");
    assert!(!volume.supports_watching());
}

// ============================================================================
// Rename tests
// ============================================================================

#[tokio::test]
async fn test_rename_success() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/old.txt"), b"content").await.unwrap();

    let result = volume.rename(Path::new("/old.txt"), Path::new("/new.txt"), false).await;
    assert!(result.is_ok());
    assert!(!volume.exists(Path::new("/old.txt")).await);
    assert!(volume.exists(Path::new("/new.txt")).await);

    let metadata = volume.get_metadata(Path::new("/new.txt")).await.unwrap();
    assert_eq!(metadata.name, "new.txt");
    assert_eq!(metadata.path, "/new.txt");
}

#[tokio::test]
async fn test_rename_conflict_no_force() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/source.txt"), b"source").await.unwrap();
    volume.create_file(Path::new("/target.txt"), b"target").await.unwrap();

    let result = volume
        .rename(Path::new("/source.txt"), Path::new("/target.txt"), false)
        .await;
    assert!(matches!(result, Err(VolumeError::AlreadyExists(_))));
    // Both entries still exist
    assert!(volume.exists(Path::new("/source.txt")).await);
    assert!(volume.exists(Path::new("/target.txt")).await);
}

#[tokio::test]
async fn test_rename_force_overwrites() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/source.txt"), b"new").await.unwrap();
    volume.create_file(Path::new("/target.txt"), b"old").await.unwrap();

    let result = volume
        .rename(Path::new("/source.txt"), Path::new("/target.txt"), true)
        .await;
    assert!(result.is_ok());
    assert!(!volume.exists(Path::new("/source.txt")).await);
    assert!(volume.exists(Path::new("/target.txt")).await);

    let metadata = volume.get_metadata(Path::new("/target.txt")).await.unwrap();
    assert_eq!(metadata.name, "target.txt");
}

#[tokio::test]
async fn test_rename_nonexistent_source() {
    let volume = InMemoryVolume::new("Test");
    let result = volume
        .rename(Path::new("/missing.txt"), Path::new("/new.txt"), false)
        .await;
    assert!(matches!(result, Err(VolumeError::NotFound(_))));
}

// ============================================================================
// Concurrency tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_reads() {
    use std::sync::Arc;

    let volume = Arc::new(InMemoryVolume::with_file_count("Test", 1000));
    let mut handles = vec![];

    // Spawn 10 tasks doing concurrent reads
    for _ in 0..10 {
        let vol = Arc::clone(&volume);
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let _ = vol.list_directory(Path::new(""), None).await;
                let _ = vol.exists(Path::new("/file_000001.txt")).await;
                let _ = vol.get_metadata(Path::new("/file_000010.txt")).await;
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Volume should still be intact
    assert_eq!(volume.list_directory(Path::new(""), None).await.unwrap().len(), 1000);
}

#[tokio::test]
async fn test_concurrent_writes() {
    use std::sync::Arc;

    let volume = Arc::new(InMemoryVolume::new("Test"));
    let mut handles = vec![];

    // Spawn 10 tasks each creating 10 files
    for i in 0..10 {
        let vol = Arc::clone(&volume);
        handles.push(tokio::spawn(async move {
            for j in 0..10 {
                let path = format!("/file_{}_{}.txt", i, j);
                vol.create_file(Path::new(&path), b"content").await.unwrap();
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Should have all 100 files
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(entries.len(), 100);
}

#[tokio::test]
async fn test_concurrent_create_delete() {
    use std::sync::Arc;

    let volume = Arc::new(InMemoryVolume::new("Test"));
    // Create a permanent file
    volume.create_file(Path::new("/permanent.txt"), b"keep").await.unwrap();

    let mut handles = vec![];

    // Readers
    for _ in 0..5 {
        let vol = Arc::clone(&volume);
        handles.push(tokio::spawn(async move {
            for _ in 0..50 {
                let _ = vol.list_directory(Path::new(""), None).await;
                let _ = vol.exists(Path::new("/permanent.txt")).await;
                tokio::task::yield_now().await;
            }
        }));
    }

    // Writers: create and delete temporary files
    for i in 0..5 {
        let vol = Arc::clone(&volume);
        handles.push(tokio::spawn(async move {
            for j in 0..10 {
                let path = format!("/temp_{}_{}.txt", i, j);
                let p = Path::new(&path);
                vol.create_file(p, b"temp").await.unwrap();
                tokio::task::yield_now().await;
                let _ = vol.delete(p).await; // May fail if another task already deleted
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Permanent file should still exist
    assert!(volume.exists(Path::new("/permanent.txt")).await);
}

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
    assert_eq!(result.file_count, 3);
    assert_eq!(result.dir_count, 0);
    assert_eq!(result.total_bytes, 600);
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
    assert_eq!(result.file_count, 2); // readme.txt + deep.txt
    assert_eq!(result.dir_count, 0); // subdir's children don't include extra dirs
    assert_eq!(result.total_bytes, 55); // 5 + 50
}

#[tokio::test]
async fn test_scan_for_copy_batch_empty_input() {
    let volume = InMemoryVolume::new("Test");
    let result = volume.scan_for_copy_batch(&[]).await.unwrap();
    assert_eq!(result.file_count, 0);
    assert_eq!(result.dir_count, 0);
    assert_eq!(result.total_bytes, 0);
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
    assert_eq!(single.file_count, batch.file_count);
    assert_eq!(single.dir_count, batch.dir_count);
    assert_eq!(single.total_bytes, batch.total_bytes);
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
    assert_eq!(result.file_count, 2);
    assert_eq!(result.total_bytes, 30);
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
    }];

    let conflicts = volume.scan_for_conflicts(&source_items, Path::new("/")).await.unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].source_path, "report.txt");
    assert_eq!(conflicts[0].source_size, 200);
    assert_eq!(conflicts[0].dest_size, 11); // "old content"
}

#[test]
fn test_supports_export() {
    let volume = InMemoryVolume::new("Test");
    assert!(volume.supports_export());
}

// ============================================================================
// Layer 1: Streaming tests (open_read_stream + write_from_stream)
// ============================================================================

// ============================================================================
// Layer 4: Delete, export/import, and edge case tests
// ============================================================================

#[tokio::test]
async fn test_delete_multiple_files() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/dir")).await.unwrap();
    volume.create_file(Path::new("/dir/a.txt"), b"a").await.unwrap();
    volume.create_file(Path::new("/dir/b.txt"), b"b").await.unwrap();
    volume.create_file(Path::new("/dir/c.txt"), b"c").await.unwrap();

    volume.delete(Path::new("/dir/b.txt")).await.unwrap();
    assert!(volume.exists(Path::new("/dir/a.txt")).await);
    assert!(!volume.exists(Path::new("/dir/b.txt")).await);
    assert!(volume.exists(Path::new("/dir/c.txt")).await);
}

#[tokio::test]
async fn test_export_to_local_creates_file() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/data.txt"), b"export me").await.unwrap();

    let temp = std::env::temp_dir().join("cmdr_inmem_export_test");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();

    let bytes = volume
        .export_to_local(Path::new("/data.txt"), &temp.join("data.txt"), &|_, _| {
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();
    assert_eq!(bytes, 9);
    assert_eq!(std::fs::read_to_string(temp.join("data.txt")).unwrap(), "export me");

    let _ = std::fs::remove_dir_all(&temp);
}

#[tokio::test]
async fn test_import_from_local_creates_entry() {
    let volume = InMemoryVolume::new("Test");

    let temp = std::env::temp_dir().join("cmdr_inmem_import_test");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::write(temp.join("src.txt"), "import me").unwrap();

    let bytes = volume
        .import_from_local(&temp.join("src.txt"), Path::new("/imported.txt"), &|_, _| {
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();
    assert_eq!(bytes, 9);
    assert!(volume.exists(Path::new("/imported.txt")).await);

    // Verify via streaming
    let mut stream = volume.open_read_stream(Path::new("/imported.txt")).await.unwrap();
    let chunk = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(chunk, b"import me");

    let _ = std::fs::remove_dir_all(&temp);
}

#[tokio::test]
async fn test_export_not_found() {
    let volume = InMemoryVolume::new("Test");
    let result = volume
        .export_to_local(
            Path::new("/nope.txt"),
            &std::env::temp_dir().join("cmdr_nope"),
            &|_, _| std::ops::ControlFlow::Continue(()),
        )
        .await;
    assert!(matches!(result, Err(VolumeError::NotFound(_))));
}

#[tokio::test]
async fn test_round_trip_export_import() {
    let source = InMemoryVolume::new("Source");
    let dest = InMemoryVolume::new("Dest");

    let data: Vec<u8> = (0..=255).cycle().take(50_000).collect();
    source.create_file(Path::new("/payload.bin"), &data).await.unwrap();

    let temp = std::env::temp_dir().join("cmdr_roundtrip_test");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();

    // Export from source → local
    source
        .export_to_local(Path::new("/payload.bin"), &temp.join("payload.bin"), &|_, _| {
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();
    // Import from local → dest
    dest.import_from_local(&temp.join("payload.bin"), Path::new("/payload.bin"), &|_, _| {
        std::ops::ControlFlow::Continue(())
    })
    .await
    .unwrap();

    // Verify content integrity via streaming
    let mut stream = dest.open_read_stream(Path::new("/payload.bin")).await.unwrap();
    let mut reassembled = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        reassembled.extend_from_slice(&chunk);
    }
    assert_eq!(reassembled, data);

    let _ = std::fs::remove_dir_all(&temp);
}

// ============================================================================
// Layer 1: Streaming tests (open_read_stream + write_from_stream)
// ============================================================================

#[test]
fn test_supports_streaming() {
    let volume = InMemoryVolume::new("Test");
    assert!(volume.supports_streaming());
}

#[tokio::test]
async fn test_open_read_stream_small_file() {
    let volume = InMemoryVolume::new("Test");
    volume
        .create_file(Path::new("/hello.txt"), b"Hello, world!")
        .await
        .unwrap();

    let mut stream = volume.open_read_stream(Path::new("/hello.txt")).await.unwrap();
    assert_eq!(stream.total_size(), 13);
    assert_eq!(stream.bytes_read(), 0);

    let chunk = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(chunk, b"Hello, world!");
    assert_eq!(stream.bytes_read(), 13);
    assert!(stream.next_chunk().await.is_none());
}

#[tokio::test]
async fn test_open_read_stream_empty_file() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/empty.txt"), b"").await.unwrap();

    let mut stream = volume.open_read_stream(Path::new("/empty.txt")).await.unwrap();
    assert_eq!(stream.total_size(), 0);
    assert!(stream.next_chunk().await.is_none());
}

#[tokio::test]
async fn test_open_read_stream_multi_chunk() {
    let volume = InMemoryVolume::new("Test");
    // Create a file larger than IN_MEMORY_STREAM_CHUNK_SIZE (64 KB)
    let data: Vec<u8> = (0..=255).cycle().take(100_000).collect();
    volume.create_file(Path::new("/big.bin"), &data).await.unwrap();

    let mut stream = volume.open_read_stream(Path::new("/big.bin")).await.unwrap();
    assert_eq!(stream.total_size(), 100_000);

    let mut reassembled = Vec::new();
    let mut chunk_count = 0;
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        reassembled.extend_from_slice(&chunk);
        chunk_count += 1;
    }
    assert_eq!(reassembled, data);
    assert!(chunk_count > 1, "expected multiple chunks, got {}", chunk_count);
    assert_eq!(stream.bytes_read(), 100_000);
}

#[tokio::test]
async fn test_open_read_stream_not_found() {
    let volume = InMemoryVolume::new("Test");
    let result = volume.open_read_stream(Path::new("/nope.txt")).await;
    assert!(matches!(result, Err(VolumeError::NotFound(_))));
}

#[tokio::test]
async fn test_open_read_stream_directory_fails() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/dir")).await.unwrap();
    let result = volume.open_read_stream(Path::new("/dir")).await;
    assert!(matches!(result, Err(VolumeError::IoError { .. })));
}

#[tokio::test]
async fn test_write_from_stream_creates_file() {
    let source = InMemoryVolume::new("Source");
    let dest = InMemoryVolume::new("Dest");
    source
        .create_file(Path::new("/data.bin"), b"source content")
        .await
        .unwrap();

    let stream = source.open_read_stream(Path::new("/data.bin")).await.unwrap();
    let no_progress = &|_: u64, _: u64| std::ops::ControlFlow::Continue(());
    let bytes = dest
        .write_from_stream(Path::new("/data.bin"), 14, stream, no_progress)
        .await
        .unwrap();

    assert_eq!(bytes, 14);
    // Verify content arrived correctly
    let mut verify = dest.open_read_stream(Path::new("/data.bin")).await.unwrap();
    let chunk = verify.next_chunk().await.unwrap().unwrap();
    assert_eq!(chunk, b"source content");
}

#[tokio::test]
async fn test_write_from_stream_progress_callback() {
    let source = InMemoryVolume::new("Source");
    let dest = InMemoryVolume::new("Dest");
    // 100 KB = 2 chunks at 64 KB chunk size
    let data = vec![0xAB; 100_000];
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();

    let progress_calls = std::sync::atomic::AtomicUsize::new(0);
    let last_bytes = std::sync::atomic::AtomicU64::new(0);

    let stream = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let bytes = dest
        .write_from_stream(Path::new("/big.bin"), 100_000, stream, &|bytes_done, total| {
            progress_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            last_bytes.store(bytes_done, std::sync::atomic::Ordering::Relaxed);
            assert_eq!(total, 100_000);
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();

    assert_eq!(bytes, 100_000);
    assert!(
        progress_calls.load(std::sync::atomic::Ordering::Relaxed) >= 2,
        "expected at least 2 progress calls for 100 KB at 64 KB chunks"
    );
    assert_eq!(last_bytes.load(std::sync::atomic::Ordering::Relaxed), 100_000);
}

#[tokio::test]
async fn test_write_from_stream_cancel_via_progress() {
    let source = InMemoryVolume::new("Source");
    let dest = InMemoryVolume::new("Dest");
    // 200 KB = 4 chunks, cancel after first
    let data = vec![0xCD; 200_000];
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();

    let call_count = std::sync::atomic::AtomicUsize::new(0);
    let stream = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let result = dest
        .write_from_stream(Path::new("/big.bin"), 200_000, stream, &|_, _| {
            let n = call_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n >= 1 {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        })
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, VolumeError::IoError { ref message, .. } if message.contains("cancelled")),
        "expected cancellation error, got: {:?}",
        err
    );
    // File should NOT exist at destination (write was cancelled before create_file)
    assert!(!dest.exists(Path::new("/big.bin")).await);
}
