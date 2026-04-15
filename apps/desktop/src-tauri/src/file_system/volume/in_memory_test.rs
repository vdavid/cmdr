//! Tests for InMemoryVolume.

use super::*;
use std::path::{Path, PathBuf};

#[test]
fn test_new_creates_empty_volume() {
    let volume = InMemoryVolume::new("Test");
    assert_eq!(volume.name(), "Test");
    assert_eq!(volume.root(), Path::new("/"));

    let entries = volume.list_directory(Path::new("")).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_with_entries_populates_volume() {
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
    let result = volume.list_directory(Path::new("")).unwrap();

    assert_eq!(result.len(), 2);
    // Directories should be first (sorted)
    assert_eq!(result[0].name, "folder");
    assert!(result[0].is_directory);
    assert_eq!(result[1].name, "test.txt");
    assert!(!result[1].is_directory);
}

#[test]
fn test_with_file_count_creates_correct_number() {
    let volume = InMemoryVolume::with_file_count("Test", 100);
    let entries = volume.list_directory(Path::new("")).unwrap();

    assert_eq!(entries.len(), 100);
    assert!(entries[0].name.starts_with("file_"));
}

#[test]
fn test_with_file_count_stress_test() {
    // Verify we can handle large file counts for stress testing
    let volume = InMemoryVolume::with_file_count("Test", 50_000);
    let entries = volume.list_directory(Path::new("")).unwrap();

    assert_eq!(entries.len(), 50_000);
}

#[test]
fn test_exists_returns_true_for_existing() {
    let entries = vec![FileEntry {
        size: Some(100),
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new("test.txt".to_string(), "/test.txt".to_string(), false, false)
    }];

    let volume = InMemoryVolume::with_entries("Test", entries);

    assert!(volume.exists(Path::new("/test.txt")));
    assert!(volume.exists(Path::new("test.txt"))); // Relative path
}

#[test]
fn test_exists_returns_false_for_nonexistent() {
    let volume = InMemoryVolume::new("Test");
    assert!(!volume.exists(Path::new("/nonexistent.txt")));
}

#[test]
fn test_get_metadata_returns_correct_entry() {
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
    let result = volume.get_metadata(Path::new("/test.txt")).unwrap();

    assert_eq!(result.name, "test.txt");
    assert_eq!(result.size, Some(1024));
}

#[test]
fn test_get_metadata_nonexistent_returns_error() {
    let volume = InMemoryVolume::new("Test");
    let result = volume.get_metadata(Path::new("/nonexistent.txt"));

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), VolumeError::NotFound(_)));
}

#[test]
fn test_create_file_then_exists() {
    let volume = InMemoryVolume::new("Test");

    volume.create_file(Path::new("/test.txt"), b"Hello, World!").unwrap();

    assert!(volume.exists(Path::new("/test.txt")));

    let metadata = volume.get_metadata(Path::new("/test.txt")).unwrap();
    assert_eq!(metadata.name, "test.txt");
    assert_eq!(metadata.size, Some(13)); // "Hello, World!" is 13 bytes
    assert!(!metadata.is_directory);
}

#[test]
fn test_create_directory_then_exists() {
    let volume = InMemoryVolume::new("Test");

    volume.create_directory(Path::new("/mydir")).unwrap();

    assert!(volume.exists(Path::new("/mydir")));

    let metadata = volume.get_metadata(Path::new("/mydir")).unwrap();
    assert_eq!(metadata.name, "mydir");
    assert!(metadata.is_directory);
}

#[test]
fn test_delete_removes_entry() {
    let volume = InMemoryVolume::new("Test");

    volume.create_file(Path::new("/test.txt"), b"content").unwrap();
    assert!(volume.exists(Path::new("/test.txt")));

    volume.delete(Path::new("/test.txt")).unwrap();
    assert!(!volume.exists(Path::new("/test.txt")));
}

#[test]
fn test_delete_nonexistent_returns_error() {
    let volume = InMemoryVolume::new("Test");

    let result = volume.delete(Path::new("/nonexistent.txt"));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), VolumeError::NotFound(_)));
}

#[test]
fn test_list_directory_sorts_correctly() {
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
    let result = volume.list_directory(Path::new("")).unwrap();

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

#[test]
fn test_list_subdirectory() {
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
    let root_entries = volume.list_directory(Path::new("")).unwrap();
    assert_eq!(root_entries.len(), 2);

    // List subdir - should only show file_in_subdir.txt
    let subdir_entries = volume.list_directory(Path::new("/subdir")).unwrap();
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

#[test]
fn test_rename_success() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/old.txt"), b"content").unwrap();

    let result = volume.rename(Path::new("/old.txt"), Path::new("/new.txt"), false);
    assert!(result.is_ok());
    assert!(!volume.exists(Path::new("/old.txt")));
    assert!(volume.exists(Path::new("/new.txt")));

    let metadata = volume.get_metadata(Path::new("/new.txt")).unwrap();
    assert_eq!(metadata.name, "new.txt");
    assert_eq!(metadata.path, "/new.txt");
}

#[test]
fn test_rename_conflict_no_force() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/source.txt"), b"source").unwrap();
    volume.create_file(Path::new("/target.txt"), b"target").unwrap();

    let result = volume.rename(Path::new("/source.txt"), Path::new("/target.txt"), false);
    assert!(matches!(result, Err(VolumeError::AlreadyExists(_))));
    // Both entries still exist
    assert!(volume.exists(Path::new("/source.txt")));
    assert!(volume.exists(Path::new("/target.txt")));
}

#[test]
fn test_rename_force_overwrites() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/source.txt"), b"new").unwrap();
    volume.create_file(Path::new("/target.txt"), b"old").unwrap();

    let result = volume.rename(Path::new("/source.txt"), Path::new("/target.txt"), true);
    assert!(result.is_ok());
    assert!(!volume.exists(Path::new("/source.txt")));
    assert!(volume.exists(Path::new("/target.txt")));

    let metadata = volume.get_metadata(Path::new("/target.txt")).unwrap();
    assert_eq!(metadata.name, "target.txt");
}

#[test]
fn test_rename_nonexistent_source() {
    let volume = InMemoryVolume::new("Test");
    let result = volume.rename(Path::new("/missing.txt"), Path::new("/new.txt"), false);
    assert!(matches!(result, Err(VolumeError::NotFound(_))));
}

// ============================================================================
// Concurrency tests
// ============================================================================

#[test]
fn test_concurrent_reads() {
    use std::sync::Arc;
    use std::thread;

    let volume = Arc::new(InMemoryVolume::with_file_count("Test", 1000));
    let mut handles = vec![];

    // Spawn 10 threads doing concurrent reads
    for _ in 0..10 {
        let vol = Arc::clone(&volume);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                let _ = vol.list_directory(Path::new(""));
                let _ = vol.exists(Path::new("/file_000001.txt"));
                let _ = vol.get_metadata(Path::new("/file_000010.txt"));
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Volume should still be intact
    assert_eq!(volume.list_directory(Path::new("")).unwrap().len(), 1000);
}

#[test]
fn test_concurrent_writes() {
    use std::sync::Arc;
    use std::thread;

    let volume = Arc::new(InMemoryVolume::new("Test"));
    let mut handles = vec![];

    // Spawn 10 threads each creating 10 files
    for i in 0..10 {
        let vol = Arc::clone(&volume);
        handles.push(thread::spawn(move || {
            for j in 0..10 {
                let path = format!("/file_{}_{}.txt", i, j);
                vol.create_file(Path::new(&path), b"content").unwrap();
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Should have all 100 files
    let entries = volume.list_directory(Path::new("")).unwrap();
    assert_eq!(entries.len(), 100);
}

#[test]
fn test_concurrent_create_delete() {
    use std::sync::Arc;
    use std::thread;

    let volume = Arc::new(InMemoryVolume::new("Test"));
    // Create a permanent file
    volume.create_file(Path::new("/permanent.txt"), b"keep").unwrap();

    let mut handles = vec![];

    // Readers
    for _ in 0..5 {
        let vol = Arc::clone(&volume);
        handles.push(thread::spawn(move || {
            for _ in 0..50 {
                let _ = vol.list_directory(Path::new(""));
                let _ = vol.exists(Path::new("/permanent.txt"));
                thread::yield_now();
            }
        }));
    }

    // Writers: create and delete temporary files
    for i in 0..5 {
        let vol = Arc::clone(&volume);
        handles.push(thread::spawn(move || {
            for j in 0..10 {
                let path = format!("/temp_{}_{}.txt", i, j);
                let p = Path::new(&path);
                vol.create_file(p, b"temp").unwrap();
                thread::yield_now();
                let _ = vol.delete(p); // May fail if another thread already deleted
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Permanent file should still exist
    assert!(volume.exists(Path::new("/permanent.txt")));
}

// ============================================================================
// scan_for_copy tests
// ============================================================================

#[test]
fn test_scan_for_copy_single_file() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/report.txt"), b"Hello, World!").unwrap();

    let result = volume.scan_for_copy(Path::new("/report.txt")).unwrap();
    assert_eq!(result.file_count, 1);
    assert_eq!(result.dir_count, 0);
    assert_eq!(result.total_bytes, 13);
}

#[test]
fn test_scan_for_copy_empty_directory() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/empty")).unwrap();

    let result = volume.scan_for_copy(Path::new("/empty")).unwrap();
    assert_eq!(result.file_count, 0);
    assert_eq!(result.dir_count, 0);
    assert_eq!(result.total_bytes, 0);
}

#[test]
fn test_scan_for_copy_directory_with_files() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/docs")).unwrap();
    volume.create_file(Path::new("/docs/readme.txt"), b"Read me").unwrap();
    volume.create_file(Path::new("/docs/notes.txt"), b"Notes here").unwrap();

    let result = volume.scan_for_copy(Path::new("/docs")).unwrap();
    assert_eq!(result.file_count, 2);
    assert_eq!(result.dir_count, 0);
    assert_eq!(result.total_bytes, 17); // 7 + 10
}

#[test]
fn test_scan_for_copy_nested_directory_tree() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/root")).unwrap();
    volume.create_directory(Path::new("/root/sub")).unwrap();
    volume.create_directory(Path::new("/root/sub/deep")).unwrap();
    volume.create_file(Path::new("/root/file1.txt"), b"AAA").unwrap();
    volume.create_file(Path::new("/root/sub/file2.txt"), b"BBBBB").unwrap();
    volume.create_file(Path::new("/root/sub/deep/file3.txt"), b"C").unwrap();

    let result = volume.scan_for_copy(Path::new("/root")).unwrap();
    assert_eq!(result.file_count, 3);
    assert_eq!(result.dir_count, 2); // sub + deep (root not counted)
    assert_eq!(result.total_bytes, 9); // 3 + 5 + 1
}

// ============================================================================
// scan_for_copy_batch tests (default implementation via Volume trait)
// ============================================================================

#[test]
fn test_scan_for_copy_batch_multiple_files_same_dir() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/photos")).unwrap();
    volume.create_file(Path::new("/photos/a.jpg"), &[0; 100]).unwrap();
    volume.create_file(Path::new("/photos/b.jpg"), &[0; 200]).unwrap();
    volume.create_file(Path::new("/photos/c.jpg"), &[0; 300]).unwrap();

    let paths = vec![
        PathBuf::from("/photos/a.jpg"),
        PathBuf::from("/photos/b.jpg"),
        PathBuf::from("/photos/c.jpg"),
    ];
    let result = volume.scan_for_copy_batch(&paths).unwrap();
    assert_eq!(result.file_count, 3);
    assert_eq!(result.dir_count, 0);
    assert_eq!(result.total_bytes, 600);
}

#[test]
fn test_scan_for_copy_batch_mixed_files_and_dirs() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/stuff")).unwrap();
    volume.create_file(Path::new("/stuff/readme.txt"), b"hello").unwrap();
    volume.create_directory(Path::new("/stuff/subdir")).unwrap();
    volume
        .create_file(Path::new("/stuff/subdir/deep.txt"), &[0; 50])
        .unwrap();

    let paths = vec![PathBuf::from("/stuff/readme.txt"), PathBuf::from("/stuff/subdir")];
    let result = volume.scan_for_copy_batch(&paths).unwrap();
    assert_eq!(result.file_count, 2); // readme.txt + deep.txt
    assert_eq!(result.dir_count, 0); // subdir's children don't include extra dirs
    assert_eq!(result.total_bytes, 55); // 5 + 50
}

#[test]
fn test_scan_for_copy_batch_empty_input() {
    let volume = InMemoryVolume::new("Test");
    let result = volume.scan_for_copy_batch(&[]).unwrap();
    assert_eq!(result.file_count, 0);
    assert_eq!(result.dir_count, 0);
    assert_eq!(result.total_bytes, 0);
}

#[test]
fn test_scan_for_copy_batch_single_item_matches_single_scan() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/docs")).unwrap();
    volume.create_file(Path::new("/docs/a.txt"), b"data").unwrap();

    let single = volume.scan_for_copy(Path::new("/docs/a.txt")).unwrap();
    let batch = volume.scan_for_copy_batch(&[PathBuf::from("/docs/a.txt")]).unwrap();
    assert_eq!(single.file_count, batch.file_count);
    assert_eq!(single.dir_count, batch.dir_count);
    assert_eq!(single.total_bytes, batch.total_bytes);
}

#[test]
fn test_scan_for_copy_batch_files_from_different_dirs() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/a")).unwrap();
    volume.create_directory(Path::new("/b")).unwrap();
    volume.create_file(Path::new("/a/file1.txt"), &[0; 10]).unwrap();
    volume.create_file(Path::new("/b/file2.txt"), &[0; 20]).unwrap();

    let paths = vec![PathBuf::from("/a/file1.txt"), PathBuf::from("/b/file2.txt")];
    let result = volume.scan_for_copy_batch(&paths).unwrap();
    assert_eq!(result.file_count, 2);
    assert_eq!(result.total_bytes, 30);
}

// ============================================================================
// get_space_info tests
// ============================================================================

#[test]
fn test_get_space_info_not_supported_by_default() {
    let volume = InMemoryVolume::new("Test");
    assert!(matches!(volume.get_space_info(), Err(VolumeError::NotSupported)));
}

#[test]
fn test_get_space_info_with_configured_space() {
    let volume = InMemoryVolume::new("Test").with_space_info(1_000_000, 500_000);
    let space = volume.get_space_info().unwrap();
    assert_eq!(space.total_bytes, 1_000_000);
    assert_eq!(space.available_bytes, 500_000);
    assert_eq!(space.used_bytes, 500_000);
}

// ============================================================================
// scan_for_conflicts tests
// ============================================================================

#[test]
fn test_scan_for_conflicts_no_conflicts() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/existing.txt"), b"data").unwrap();

    let source_items = vec![SourceItemInfo {
        name: "other.txt".to_string(),
        size: 100,
        modified: None,
    }];

    let conflicts = volume.scan_for_conflicts(&source_items, Path::new("/")).unwrap();
    assert!(conflicts.is_empty());
}

#[test]
fn test_scan_for_conflicts_detects_conflict() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/report.txt"), b"old content").unwrap();

    let source_items = vec![SourceItemInfo {
        name: "report.txt".to_string(),
        size: 200,
        modified: Some(1_700_000_000),
    }];

    let conflicts = volume.scan_for_conflicts(&source_items, Path::new("/")).unwrap();
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
