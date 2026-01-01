//! Integration tests using InMemoryVolume.
//!
//! These tests verify that the volume abstraction works correctly
//! without touching the real file system.

use super::FileEntry;
use super::volume::{InMemoryVolume, Volume};
use std::path::Path;

/// Creates a sample file entry for testing.
fn create_test_entry(name: &str, is_dir: bool) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: format!("/{}", name),
        is_directory: is_dir,
        is_symlink: false,
        size: if is_dir { None } else { Some(1024) },
        modified_at: Some(1_640_000_000),
        created_at: Some(1_639_000_000),
        added_at: None,
        opened_at: None,
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        icon_id: if is_dir { "dir".to_string() } else { "file".to_string() },
        extended_metadata_loaded: true,
    }
}

#[test]
fn test_inmemory_volume_full_workflow() {
    // Create volume with some entries
    let entries = vec![
        create_test_entry("documents", true),
        create_test_entry("photo.jpg", false),
        create_test_entry("notes.txt", false),
    ];

    let volume = InMemoryVolume::with_entries("Test Volume", entries);

    // Verify volume properties
    assert_eq!(volume.name(), "Test Volume");
    assert_eq!(volume.root(), Path::new("/"));

    // List directory
    let listed = volume.list_directory(Path::new("")).unwrap();
    assert_eq!(listed.len(), 3);

    // Verify sorting (directories first)
    assert_eq!(listed[0].name, "documents");
    assert!(listed[0].is_directory);

    // Create a new file
    volume.create_file(Path::new("/new_file.txt"), b"Hello World").unwrap();

    // Verify it exists
    assert!(volume.exists(Path::new("/new_file.txt")));

    // Get metadata
    let metadata = volume.get_metadata(Path::new("/new_file.txt")).unwrap();
    assert_eq!(metadata.name, "new_file.txt");
    assert_eq!(metadata.size, Some(11)); // "Hello World" is 11 bytes

    // Delete the file
    volume.delete(Path::new("/new_file.txt")).unwrap();
    assert!(!volume.exists(Path::new("/new_file.txt")));
}

#[test]
fn test_inmemory_volume_stress_test_50k_entries() {
    // Create volume with 50,000 entries
    let volume = InMemoryVolume::with_file_count("Stress Test", 50_000);

    // List directory
    let start = std::time::Instant::now();
    let entries = volume.list_directory(Path::new("")).unwrap();
    let duration = start.elapsed();

    // Verify count
    assert_eq!(entries.len(), 50_000);

    // Verify performance (should be well under 1 sec, but we say 5 sec so it isn't flaky on CI)
    assert!(duration.as_millis() < 5000, "Listing 50k entries took {:?}", duration);
}

#[test]
fn test_inmemory_volume_nested_directories() {
    let entries = vec![
        create_test_entry("level1", true),
        FileEntry {
            name: "level2".to_string(),
            path: "/level1/level2".to_string(),
            is_directory: true,
            ..create_test_entry("", true)
        },
        FileEntry {
            name: "file.txt".to_string(),
            path: "/level1/level2/file.txt".to_string(),
            is_directory: false,
            ..create_test_entry("", false)
        },
    ];

    let volume = InMemoryVolume::with_entries("Nested", entries);

    // List root - should only show level1
    let root_entries = volume.list_directory(Path::new("")).unwrap();
    assert_eq!(root_entries.len(), 1);
    assert_eq!(root_entries[0].name, "level1");

    // List level1 - should only show level2
    let level1_entries = volume.list_directory(Path::new("/level1")).unwrap();
    assert_eq!(level1_entries.len(), 1);
    assert_eq!(level1_entries[0].name, "level2");

    // List level2 - should only show file.txt
    let level2_entries = volume.list_directory(Path::new("/level1/level2")).unwrap();
    assert_eq!(level2_entries.len(), 1);
    assert_eq!(level2_entries[0].name, "file.txt");
}

#[test]
fn test_volume_create_and_list_sequence() {
    let volume = InMemoryVolume::new("Empty Volume");

    // Start empty
    let entries = volume.list_directory(Path::new("")).unwrap();
    assert_eq!(entries.len(), 0);

    // Create a directory
    volume.create_directory(Path::new("/docs")).unwrap();

    // Create some files
    volume.create_file(Path::new("/readme.md"), b"# README").unwrap();
    volume
        .create_file(Path::new("/docs/guide.txt"), b"Guide content")
        .unwrap();

    // List root
    let root_entries = volume.list_directory(Path::new("")).unwrap();
    assert_eq!(root_entries.len(), 2); // docs/ and readme.md

    // Directories should be first
    assert_eq!(root_entries[0].name, "docs");
    assert!(root_entries[0].is_directory);
    assert_eq!(root_entries[1].name, "readme.md");
    assert!(!root_entries[1].is_directory);

    // List docs
    let docs_entries = volume.list_directory(Path::new("/docs")).unwrap();
    assert_eq!(docs_entries.len(), 1);
    assert_eq!(docs_entries[0].name, "guide.txt");

    // Delete readme.md
    volume.delete(Path::new("/readme.md")).unwrap();

    // List root again
    let root_entries = volume.list_directory(Path::new("")).unwrap();
    assert_eq!(root_entries.len(), 1);
    assert_eq!(root_entries[0].name, "docs");
}

#[test]
fn test_volume_manager_with_inmemory() {
    use super::volume_manager::VolumeManager;
    use std::sync::Arc;

    let manager = VolumeManager::new();

    // Create two in-memory volumes
    let home_entries = vec![create_test_entry("Documents", true), create_test_entry("Desktop", true)];

    let dropbox_entries = vec![create_test_entry("Work", true), create_test_entry("Personal", true)];

    let home = Arc::new(InMemoryVolume::with_entries("Home", home_entries));
    let dropbox = Arc::new(InMemoryVolume::with_entries("Dropbox", dropbox_entries));

    // Register volumes
    manager.register("home", home.clone());
    manager.register("dropbox", dropbox.clone());
    manager.set_default("home");

    // Verify we can retrieve them
    let retrieved_home = manager.get("home").unwrap();
    assert_eq!(retrieved_home.name(), "Home");

    let retrieved_dropbox = manager.get("dropbox").unwrap();
    assert_eq!(retrieved_dropbox.name(), "Dropbox");

    // Verify default
    let default = manager.default_volume().unwrap();
    assert_eq!(default.name(), "Home");

    // List from both volumes
    let home_files = retrieved_home.list_directory(Path::new("")).unwrap();
    assert_eq!(home_files.len(), 2);

    let dropbox_files = retrieved_dropbox.list_directory(Path::new("")).unwrap();
    assert_eq!(dropbox_files.len(), 2);
    assert_eq!(dropbox_files[0].name, "Personal"); // Alphabetical order
    assert_eq!(dropbox_files[1].name, "Work");
}
