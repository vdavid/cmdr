//! Tests for InMemoryVolume.

use super::*;
use std::path::Path;

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
            name: "test.txt".to_string(),
            path: "/test.txt".to_string(),
            is_directory: false,
            is_symlink: false,
            size: Some(1024),
            modified_at: Some(1_640_000_000),
            created_at: Some(1_639_000_000),
            added_at: None,
            opened_at: None,
            permissions: 0o644,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            icon_id: "ext:txt".to_string(),
            extended_metadata_loaded: true,
        },
        FileEntry {
            name: "folder".to_string(),
            path: "/folder".to_string(),
            is_directory: true,
            is_symlink: false,
            size: None,
            modified_at: Some(1_640_000_000),
            created_at: Some(1_639_000_000),
            added_at: None,
            opened_at: None,
            permissions: 0o755,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            icon_id: "dir".to_string(),
            extended_metadata_loaded: true,
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
        name: "test.txt".to_string(),
        path: "/test.txt".to_string(),
        is_directory: false,
        is_symlink: false,
        size: Some(100),
        modified_at: None,
        created_at: None,
        added_at: None,
        opened_at: None,
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        icon_id: "file".to_string(),
        extended_metadata_loaded: true,
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
        name: "test.txt".to_string(),
        path: "/test.txt".to_string(),
        is_directory: false,
        is_symlink: false,
        size: Some(1024),
        modified_at: Some(1_640_000_000),
        created_at: None,
        added_at: None,
        opened_at: None,
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        icon_id: "file".to_string(),
        extended_metadata_loaded: true,
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
            name: "zebra.txt".to_string(),
            path: "/zebra.txt".to_string(),
            is_directory: false,
            is_symlink: false,
            size: Some(100),
            modified_at: None,
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            icon_id: "file".to_string(),
            extended_metadata_loaded: true,
        },
        FileEntry {
            name: "alpha".to_string(),
            path: "/alpha".to_string(),
            is_directory: true,
            is_symlink: false,
            size: None,
            modified_at: None,
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0o755,
            owner: "user".to_string(),
            group: "group".to_string(),
            icon_id: "dir".to_string(),
            extended_metadata_loaded: true,
        },
        FileEntry {
            name: "apple.txt".to_string(),
            path: "/apple.txt".to_string(),
            is_directory: false,
            is_symlink: false,
            size: Some(50),
            modified_at: None,
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            icon_id: "file".to_string(),
            extended_metadata_loaded: true,
        },
        FileEntry {
            name: "beta".to_string(),
            path: "/beta".to_string(),
            is_directory: true,
            is_symlink: false,
            size: None,
            modified_at: None,
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0o755,
            owner: "user".to_string(),
            group: "group".to_string(),
            icon_id: "dir".to_string(),
            extended_metadata_loaded: true,
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
            name: "subdir".to_string(),
            path: "/subdir".to_string(),
            is_directory: true,
            is_symlink: false,
            size: None,
            modified_at: None,
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0o755,
            owner: "user".to_string(),
            group: "group".to_string(),
            icon_id: "dir".to_string(),
            extended_metadata_loaded: true,
        },
        FileEntry {
            name: "file_in_subdir.txt".to_string(),
            path: "/subdir/file_in_subdir.txt".to_string(),
            is_directory: false,
            is_symlink: false,
            size: Some(100),
            modified_at: None,
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            icon_id: "file".to_string(),
            extended_metadata_loaded: true,
        },
        FileEntry {
            name: "root_file.txt".to_string(),
            path: "/root_file.txt".to_string(),
            is_directory: false,
            is_symlink: false,
            size: Some(50),
            modified_at: None,
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            icon_id: "file".to_string(),
            extended_metadata_loaded: true,
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
                let _ = vol.list_directory(std::path::Path::new(""));
                let _ = vol.exists(std::path::Path::new("/file_000001.txt"));
                let _ = vol.get_metadata(std::path::Path::new("/file_000010.txt"));
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Volume should still be intact
    assert_eq!(volume.list_directory(std::path::Path::new("")).unwrap().len(), 1000);
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
                vol.create_file(std::path::Path::new(&path), b"content").unwrap();
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Should have all 100 files
    let entries = volume.list_directory(std::path::Path::new("")).unwrap();
    assert_eq!(entries.len(), 100);
}

#[test]
fn test_concurrent_create_delete() {
    use std::sync::Arc;
    use std::thread;

    let volume = Arc::new(InMemoryVolume::new("Test"));
    // Create a permanent file
    volume
        .create_file(std::path::Path::new("/permanent.txt"), b"keep")
        .unwrap();

    let mut handles = vec![];

    // Readers
    for _ in 0..5 {
        let vol = Arc::clone(&volume);
        handles.push(thread::spawn(move || {
            for _ in 0..50 {
                let _ = vol.list_directory(std::path::Path::new(""));
                let _ = vol.exists(std::path::Path::new("/permanent.txt"));
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
                let p = std::path::Path::new(&path);
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
    assert!(volume.exists(std::path::Path::new("/permanent.txt")));
}
