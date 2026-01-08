//! Tests for LocalPosixVolume.

use super::*;
use std::path::Path;

#[test]
fn test_new_creates_volume_with_correct_name_and_root() {
    let volume = LocalPosixVolume::new("Test Volume", "/tmp");
    assert_eq!(volume.name(), "Test Volume");
    assert_eq!(volume.root(), Path::new("/tmp"));
}

#[test]
fn test_resolve_empty_path_returns_root() {
    let volume = LocalPosixVolume::new("Test", "/tmp");
    assert_eq!(volume.resolve(Path::new("")), Path::new("/tmp"));
}

#[test]
fn test_resolve_dot_returns_root() {
    let volume = LocalPosixVolume::new("Test", "/tmp");
    assert_eq!(volume.resolve(Path::new(".")), Path::new("/tmp"));
}

#[test]
fn test_resolve_relative_path_joins_with_root() {
    let volume = LocalPosixVolume::new("Test", "/tmp");
    assert_eq!(
        volume.resolve(Path::new("subdir/file.txt")),
        Path::new("/tmp/subdir/file.txt")
    );
}

#[test]
fn test_resolve_absolute_path_treats_as_relative() {
    let volume = LocalPosixVolume::new("Test", "/tmp");
    // Absolute paths should be treated as relative to volume root
    assert_eq!(
        volume.resolve(Path::new("/subdir/file.txt")),
        Path::new("/tmp/subdir/file.txt")
    );
}

#[test]
fn test_exists_returns_true_for_root() {
    let volume = LocalPosixVolume::new("Test", "/tmp");
    assert!(volume.exists(Path::new("")));
    assert!(volume.exists(Path::new(".")));
}

#[test]
fn test_exists_returns_false_for_nonexistent() {
    let volume = LocalPosixVolume::new("Test", "/tmp");
    assert!(!volume.exists(Path::new("definitely_does_not_exist_12345")));
}

#[test]
fn test_list_directory_returns_entries() {
    // Use /tmp which should exist and have some contents on any POSIX system
    let volume = LocalPosixVolume::new("Temp", "/tmp");
    let result = volume.list_directory(Path::new(""));

    // Should succeed (even if empty)
    assert!(result.is_ok());
}

#[test]
fn test_list_directory_nonexistent_returns_error() {
    let volume = LocalPosixVolume::new("Test", "/definitely_does_not_exist_12345");
    let result = volume.list_directory(Path::new(""));

    assert!(result.is_err());
    match result.unwrap_err() {
        VolumeError::NotFound(_) | VolumeError::IoError(_) => (),
        other => panic!("Expected NotFound or IoError, got: {:?}", other),
    }
}

#[test]
fn test_get_metadata_returns_entry() {
    let volume = LocalPosixVolume::new("Temp", "/tmp");
    // /tmp itself exists on any POSIX system
    let result = volume.get_metadata(Path::new(""));

    assert!(result.is_ok());
    let entry = result.unwrap();
    assert!(entry.is_directory);
}

#[test]
fn test_get_metadata_nonexistent_returns_error() {
    let volume = LocalPosixVolume::new("Test", "/tmp");
    let result = volume.get_metadata(Path::new("definitely_does_not_exist_12345"));

    assert!(result.is_err());
}

#[test]
fn test_supports_watching_returns_true() {
    let volume = LocalPosixVolume::new("Test", "/tmp");
    assert!(volume.supports_watching());
}

#[test]
fn test_optional_methods_return_not_supported() {
    let volume = LocalPosixVolume::new("Test", "/tmp");

    let result = volume.create_file(Path::new("test.txt"), b"content");
    assert!(matches!(result, Err(VolumeError::NotSupported)));

    let result = volume.create_directory(Path::new("testdir"));
    assert!(matches!(result, Err(VolumeError::NotSupported)));

    let result = volume.delete(Path::new("test.txt"));
    assert!(matches!(result, Err(VolumeError::NotSupported)));
}

// ============================================================================
// Symlink edge case tests
// ============================================================================

#[test]
fn test_symlink_to_file_detected() {
    use std::fs;
    use std::os::unix::fs::symlink;

    // Create a test file and symlink in /tmp
    let test_dir = std::env::temp_dir().join("cmdr_symlink_file_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let target_file = test_dir.join("target.txt");
    let link_file = test_dir.join("link_to_file.txt");

    fs::write(&target_file, "content").unwrap();
    symlink(&target_file, &link_file).unwrap();

    let volume = LocalPosixVolume::new("Test", test_dir.to_str().unwrap());

    // The symlink should exist
    assert!(volume.exists(Path::new("link_to_file.txt")));

    // Get metadata - should report is_symlink=true, is_directory=false
    let metadata = volume.get_metadata(Path::new("link_to_file.txt")).unwrap();
    assert!(metadata.is_symlink);
    assert!(!metadata.is_directory);
    assert_eq!(metadata.name, "link_to_file.txt");

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_symlink_to_directory_detected() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let test_dir = std::env::temp_dir().join("cmdr_symlink_dir_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let target_dir = test_dir.join("target_dir");
    let link_to_dir = test_dir.join("link_to_dir");

    fs::create_dir(&target_dir).unwrap();
    symlink(&target_dir, &link_to_dir).unwrap();

    let volume = LocalPosixVolume::new("Test", test_dir.to_str().unwrap());

    // Get metadata - should report is_symlink=true AND is_directory=true
    let metadata = volume.get_metadata(Path::new("link_to_dir")).unwrap();
    assert!(metadata.is_symlink);
    assert!(metadata.is_directory); // Target is a directory
    assert_eq!(metadata.name, "link_to_dir");

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_broken_symlink_still_exists() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let test_dir = std::env::temp_dir().join("cmdr_broken_symlink_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let broken_link = test_dir.join("broken_link.txt");
    symlink("/definitely_does_not_exist_12345", &broken_link).unwrap();

    let volume = LocalPosixVolume::new("Test", test_dir.to_str().unwrap());

    // The broken symlink itself exists
    assert!(volume.exists(Path::new("broken_link.txt")));

    // Can get metadata for the broken symlink
    let metadata = volume.get_metadata(Path::new("broken_link.txt")).unwrap();
    assert!(metadata.is_symlink);
    assert!(!metadata.is_directory); // Target doesn't exist, so defaults to false

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_list_directory_includes_symlinks() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let test_dir = std::env::temp_dir().join("cmdr_symlink_list_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    // Create a regular file, a directory, and symlinks to each
    let file = test_dir.join("file.txt");
    let dir = test_dir.join("dir");
    let link_to_file = test_dir.join("link_to_file");
    let link_to_dir = test_dir.join("link_to_dir");

    fs::write(&file, "content").unwrap();
    fs::create_dir(&dir).unwrap();
    symlink(&file, &link_to_file).unwrap();
    symlink(&dir, &link_to_dir).unwrap();

    let volume = LocalPosixVolume::new("Test", test_dir.to_str().unwrap());
    let entries = volume.list_directory(Path::new("")).unwrap();

    // Should have 4 entries
    assert_eq!(entries.len(), 4);

    // Find the symlinks
    let link_file_entry = entries.iter().find(|e| e.name == "link_to_file").unwrap();
    assert!(link_file_entry.is_symlink);
    assert!(!link_file_entry.is_directory);

    let link_dir_entry = entries.iter().find(|e| e.name == "link_to_dir").unwrap();
    assert!(link_dir_entry.is_symlink);
    assert!(link_dir_entry.is_directory); // Points to directory

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}
