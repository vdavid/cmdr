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
fn test_supports_streaming_returns_false() {
    // LocalPosixVolume uses the default implementation which returns false.
    // Streaming is primarily for MTP-to-MTP transfers.
    let volume = LocalPosixVolume::new("Test", "/tmp");
    assert!(!volume.supports_streaming());
}

#[test]
fn test_write_operations() {
    use std::fs;

    // Create a temp directory for this test
    let test_dir = std::env::temp_dir().join("cmdr_write_ops_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let volume = LocalPosixVolume::new("Test", &test_dir);

    // Test create_file
    let result = volume.create_file(Path::new("test.txt"), b"hello world");
    assert!(result.is_ok());
    assert!(test_dir.join("test.txt").exists());
    assert_eq!(fs::read_to_string(test_dir.join("test.txt")).unwrap(), "hello world");

    // Test create_directory
    let result = volume.create_directory(Path::new("subdir"));
    assert!(result.is_ok());
    assert!(test_dir.join("subdir").is_dir());

    // Test delete file
    let result = volume.delete(Path::new("test.txt"));
    assert!(result.is_ok());
    assert!(!test_dir.join("test.txt").exists());

    // Test delete directory
    let result = volume.delete(Path::new("subdir"));
    assert!(result.is_ok());
    assert!(!test_dir.join("subdir").exists());

    // Test rename (force=false, no conflict)
    volume.create_file(Path::new("old.txt"), b"content").unwrap();
    let result = volume.rename(Path::new("old.txt"), Path::new("new.txt"), false);
    assert!(result.is_ok());
    assert!(!test_dir.join("old.txt").exists());
    assert!(test_dir.join("new.txt").exists());

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_rename_conflict_no_force() {
    use std::fs;

    let test_dir = std::env::temp_dir().join("cmdr_rename_conflict_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let volume = LocalPosixVolume::new("Test", &test_dir);
    volume.create_file(Path::new("source.txt"), b"source").unwrap();
    volume.create_file(Path::new("target.txt"), b"target").unwrap();

    let result = volume.rename(Path::new("source.txt"), Path::new("target.txt"), false);
    assert!(matches!(result, Err(VolumeError::AlreadyExists(_))));
    // Both files still intact
    assert!(test_dir.join("source.txt").exists());
    assert!(test_dir.join("target.txt").exists());

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_rename_force_overwrites() {
    use std::fs;

    let test_dir = std::env::temp_dir().join("cmdr_rename_force_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let volume = LocalPosixVolume::new("Test", &test_dir);
    volume.create_file(Path::new("source.txt"), b"new content").unwrap();
    volume.create_file(Path::new("target.txt"), b"old content").unwrap();

    let result = volume.rename(Path::new("source.txt"), Path::new("target.txt"), true);
    assert!(result.is_ok());
    assert!(!test_dir.join("source.txt").exists());
    assert_eq!(fs::read_to_string(test_dir.join("target.txt")).unwrap(), "new content");

    let _ = fs::remove_dir_all(&test_dir);
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

// ============================================================================
// Copy operation tests
// ============================================================================

#[test]
fn test_supports_export_returns_true() {
    let volume = LocalPosixVolume::new("Test", "/tmp");
    assert!(volume.supports_export());
}

#[test]
fn test_scan_for_copy_single_file() {
    use std::fs;

    let test_dir = std::env::temp_dir().join("cmdr_scan_copy_file_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    // Create a single file with known content
    fs::write(test_dir.join("test.txt"), "Hello, World!").unwrap();

    let volume = LocalPosixVolume::new("Test", test_dir.to_str().unwrap());
    let result = volume.scan_for_copy(Path::new("test.txt")).unwrap();

    assert_eq!(result.file_count, 1);
    assert_eq!(result.dir_count, 0);
    assert_eq!(result.total_bytes, 13); // "Hello, World!" is 13 bytes

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_scan_for_copy_directory() {
    use std::fs;

    let test_dir = std::env::temp_dir().join("cmdr_scan_copy_dir_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    // Create directory structure
    let subdir = test_dir.join("mydir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("file1.txt"), "123").unwrap();
    fs::write(subdir.join("file2.txt"), "456789").unwrap();
    let nested = subdir.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("file3.txt"), "A").unwrap();

    let volume = LocalPosixVolume::new("Test", test_dir.to_str().unwrap());
    let result = volume.scan_for_copy(Path::new("mydir")).unwrap();

    assert_eq!(result.file_count, 3);
    assert_eq!(result.dir_count, 1); // Just the nested dir (root not counted)
    assert_eq!(result.total_bytes, 10); // 3 + 6 + 1

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_export_to_local_single_file() {
    use std::fs;

    let src_dir = std::env::temp_dir().join("cmdr_export_src_test");
    let dst_dir = std::env::temp_dir().join("cmdr_export_dst_test");
    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    fs::write(src_dir.join("source.txt"), "Test content").unwrap();

    let volume = LocalPosixVolume::new("Test", src_dir.to_str().unwrap());
    let bytes = volume
        .export_to_local(Path::new("source.txt"), &dst_dir.join("dest.txt"))
        .unwrap();

    assert_eq!(bytes, 12); // "Test content" is 12 bytes
    assert_eq!(fs::read_to_string(dst_dir.join("dest.txt")).unwrap(), "Test content");

    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
}

#[test]
fn test_export_to_local_directory() {
    use std::fs;

    let src_dir = std::env::temp_dir().join("cmdr_export_dir_src_test");
    let dst_dir = std::env::temp_dir().join("cmdr_export_dir_dst_test");
    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create source directory with files
    let source_subdir = src_dir.join("sourcedir");
    fs::create_dir(&source_subdir).unwrap();
    fs::write(source_subdir.join("file1.txt"), "AAA").unwrap();
    fs::write(source_subdir.join("file2.txt"), "BBBBB").unwrap();

    let volume = LocalPosixVolume::new("Test", src_dir.to_str().unwrap());
    let bytes = volume
        .export_to_local(Path::new("sourcedir"), &dst_dir.join("destdir"))
        .unwrap();

    assert_eq!(bytes, 8); // 3 + 5 bytes
    assert!(dst_dir.join("destdir").is_dir());
    assert_eq!(fs::read_to_string(dst_dir.join("destdir/file1.txt")).unwrap(), "AAA");
    assert_eq!(fs::read_to_string(dst_dir.join("destdir/file2.txt")).unwrap(), "BBBBB");

    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
}

#[test]
fn test_import_from_local_single_file() {
    use std::fs;

    let local_dir = std::env::temp_dir().join("cmdr_import_local_test");
    let vol_dir = std::env::temp_dir().join("cmdr_import_vol_test");
    let _ = fs::remove_dir_all(&local_dir);
    let _ = fs::remove_dir_all(&vol_dir);
    fs::create_dir_all(&local_dir).unwrap();
    fs::create_dir_all(&vol_dir).unwrap();

    fs::write(local_dir.join("local.txt"), "Imported content").unwrap();

    let volume = LocalPosixVolume::new("Test", vol_dir.to_str().unwrap());
    let bytes = volume
        .import_from_local(&local_dir.join("local.txt"), Path::new("imported.txt"))
        .unwrap();

    assert_eq!(bytes, 16); // "Imported content" is 16 bytes
    assert_eq!(
        fs::read_to_string(vol_dir.join("imported.txt")).unwrap(),
        "Imported content"
    );

    let _ = fs::remove_dir_all(&local_dir);
    let _ = fs::remove_dir_all(&vol_dir);
}

#[test]
fn test_scan_for_conflicts_no_conflicts() {
    use std::fs;

    let test_dir = std::env::temp_dir().join("cmdr_conflicts_none_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let volume = LocalPosixVolume::new("Test", test_dir.to_str().unwrap());

    let source_items = vec![
        SourceItemInfo {
            name: "newfile.txt".to_string(),
            size: 100,
            modified: None,
        },
        SourceItemInfo {
            name: "another.txt".to_string(),
            size: 200,
            modified: None,
        },
    ];

    let conflicts = volume.scan_for_conflicts(&source_items, Path::new("")).unwrap();
    assert!(conflicts.is_empty());

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_scan_for_conflicts_with_conflicts() {
    use std::fs;

    let test_dir = std::env::temp_dir().join("cmdr_conflicts_some_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    // Create existing files
    fs::write(test_dir.join("existing.txt"), "Old content").unwrap();
    fs::write(test_dir.join("another.txt"), "Another old").unwrap();

    let volume = LocalPosixVolume::new("Test", test_dir.to_str().unwrap());

    let source_items = vec![
        SourceItemInfo {
            name: "existing.txt".to_string(),
            size: 100,
            modified: Some(1_700_000_000),
        },
        SourceItemInfo {
            name: "newfile.txt".to_string(),
            size: 200,
            modified: None,
        },
        SourceItemInfo {
            name: "another.txt".to_string(),
            size: 300,
            modified: Some(1_700_000_000),
        },
    ];

    let conflicts = volume.scan_for_conflicts(&source_items, Path::new("")).unwrap();
    assert_eq!(conflicts.len(), 2);

    // Verify conflict info
    let existing_conflict = conflicts.iter().find(|c| c.source_path == "existing.txt").unwrap();
    assert_eq!(existing_conflict.source_size, 100);
    assert_eq!(existing_conflict.dest_size, 11); // "Old content" is 11 bytes
    assert_eq!(existing_conflict.source_modified, Some(1_700_000_000));

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_get_space_info() {
    // Test against /tmp which should exist on any POSIX system
    let volume = LocalPosixVolume::new("Test", "/tmp");
    let space = volume.get_space_info().unwrap();

    // Basic sanity checks
    assert!(space.total_bytes > 0);
    assert!(space.available_bytes <= space.total_bytes);
    assert!(space.used_bytes <= space.total_bytes);
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
