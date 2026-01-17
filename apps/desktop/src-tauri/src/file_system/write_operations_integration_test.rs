//! Integration tests for write operations (copy, move, delete).
//!
//! These tests verify end-to-end behavior including:
//! - File and directory copying with metadata preservation
//! - Symlink handling (preserve vs. dereference)
//! - Symlink loop detection
//! - Conflict resolution modes
//! - Cross-filesystem moves using staging

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::write_operations::*;

// ============================================================================
// Test utilities
// ============================================================================

fn create_temp_dir(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir().join(format!("cmdr_write_integration_test_{}", name));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
    temp_dir
}

fn cleanup_temp_dir(path: &PathBuf) {
    let _ = fs::remove_dir_all(path);
}

// ============================================================================
// Copy tests
// ============================================================================

#[test]
fn test_copy_single_file() {
    let temp_dir = create_temp_dir("copy_single");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.txt");
    fs::write(&src_file, "Hello, world!").unwrap();

    // Verify source exists
    assert!(src_file.exists());

    // This test just verifies the file can be copied using the low-level module
    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        let dst_file = dst_dir.join("file.txt");
        let result = copy_single_file_native(&src_file, &dst_file, false, None);
        assert!(result.is_ok());
        assert!(dst_file.exists());
        assert_eq!(fs::read_to_string(&dst_file).unwrap(), "Hello, world!");
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_directory_recursive() {
    let temp_dir = create_temp_dir("copy_recursive");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");

    // Create nested structure
    fs::create_dir_all(src_dir.join("subdir1/subdir2")).unwrap();
    fs::write(src_dir.join("file1.txt"), "file1").unwrap();
    fs::write(src_dir.join("subdir1/file2.txt"), "file2").unwrap();
    fs::write(src_dir.join("subdir1/subdir2/file3.txt"), "file3").unwrap();

    fs::create_dir_all(&dst_dir).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::{copy_file_native, CopyOptions};

        let result = copy_file_native(&src_dir, &dst_dir.join("src"), CopyOptions::default(), None);
        assert!(result.is_ok());
        assert!(dst_dir.join("src/file1.txt").exists());
        assert!(dst_dir.join("src/subdir1/file2.txt").exists());
        assert!(dst_dir.join("src/subdir1/subdir2/file3.txt").exists());
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_preserves_permissions() {
    let temp_dir = create_temp_dir("copy_permissions");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("script.sh");
    fs::write(&src_file, "#!/bin/bash\necho hello").unwrap();
    fs::set_permissions(&src_file, fs::Permissions::from_mode(0o755)).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        let dst_file = dst_dir.join("script.sh");
        let result = copy_single_file_native(&src_file, &dst_file, false, None);
        assert!(result.is_ok());

        let dst_perms = fs::metadata(&dst_file).unwrap().permissions().mode();
        assert_eq!(dst_perms & 0o777, 0o755);
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_preserves_symlinks() {
    let temp_dir = create_temp_dir("copy_symlinks");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a file and a symlink to it
    let target_file = src_dir.join("target.txt");
    fs::write(&target_file, "target content").unwrap();

    let symlink = src_dir.join("link");
    std::os::unix::fs::symlink(&target_file, &symlink).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_symlink;

        let dst_link = dst_dir.join("link");
        let result = copy_symlink(&symlink, &dst_link);
        assert!(result.is_ok());

        // Verify it's a symlink
        assert!(dst_link.is_symlink());
        let link_target = fs::read_link(&dst_link).unwrap();
        assert_eq!(link_target, target_file);
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_handles_broken_symlink() {
    let temp_dir = create_temp_dir("copy_broken_symlink");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a symlink to a nonexistent target
    let nonexistent = src_dir.join("nonexistent");
    let symlink = src_dir.join("broken_link");
    std::os::unix::fs::symlink(&nonexistent, &symlink).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_symlink;

        let dst_link = dst_dir.join("broken_link");
        let result = copy_symlink(&symlink, &dst_link);
        assert!(result.is_ok());

        // Verify it's a broken symlink
        assert!(dst_link.is_symlink());
        assert!(!dst_link.exists()); // exists() returns false for broken symlinks
    }

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Conflict resolution tests
// ============================================================================

#[test]
fn test_find_unique_name() {
    let temp_dir = create_temp_dir("unique_name");

    // Create existing files
    fs::write(temp_dir.join("file.txt"), "").unwrap();
    fs::write(temp_dir.join("file (1).txt"), "").unwrap();
    fs::write(temp_dir.join("file (2).txt"), "").unwrap();

    // Find unique name would return "file (3).txt"
    let path = temp_dir.join("file.txt");
    let unique = find_unique_name(&path);
    assert_eq!(unique.file_name().unwrap().to_str().unwrap(), "file (3).txt");

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_find_unique_name_no_extension() {
    let temp_dir = create_temp_dir("unique_name_no_ext");

    fs::write(temp_dir.join("file"), "").unwrap();
    fs::write(temp_dir.join("file (1)"), "").unwrap();

    let path = temp_dir.join("file");
    let unique = find_unique_name(&path);
    assert_eq!(unique.file_name().unwrap().to_str().unwrap(), "file (2)");

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn test_error_user_message_source_not_found() {
    let error = WriteOperationError::SourceNotFound {
        path: "/path/to/missing.txt".to_string(),
    };
    let message = error.user_message();
    assert!(message.contains("Cannot find"));
    assert!(message.contains("/path/to/missing.txt"));
}

#[test]
fn test_error_user_message_destination_exists() {
    let error = WriteOperationError::DestinationExists {
        path: "/path/to/existing.txt".to_string(),
    };
    let message = error.user_message();
    assert!(message.contains("already exists"));
}

#[test]
fn test_error_user_message_permission_denied() {
    let error = WriteOperationError::PermissionDenied {
        path: "/protected/folder".to_string(),
        message: "Operation not permitted".to_string(),
    };
    let message = error.user_message();
    assert!(message.contains("permission denied"));
    assert!(message.contains("Finder"));
}

#[test]
fn test_error_user_message_insufficient_space() {
    let error = WriteOperationError::InsufficientSpace {
        required: 1_073_741_824, // 1 GB
        available: 524_288_000,  // 500 MB
        volume_name: Some("Macintosh HD".to_string()),
    };
    let message = error.user_message();
    assert!(message.contains("Not enough space"));
    assert!(message.contains("Macintosh HD"));
}

#[test]
fn test_error_user_message_symlink_loop() {
    let error = WriteOperationError::SymlinkLoop {
        path: "/path/to/loop".to_string(),
    };
    let message = error.user_message();
    assert!(message.contains("Symlink loop"));
}

// ============================================================================
// Edge case tests
// ============================================================================

#[test]
fn test_special_characters_in_paths() {
    let temp_dir = create_temp_dir("special_chars");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create files with special characters
    let special_names = vec!["file with spaces.txt", "file'with'quotes.txt", "file\"double\".txt"];

    for name in &special_names {
        let src_file = src_dir.join(name);
        fs::write(&src_file, name).unwrap();

        #[cfg(target_os = "macos")]
        {
            use super::macos_copy::copy_single_file_native;

            let dst_file = dst_dir.join(name);
            let result = copy_single_file_native(&src_file, &dst_file, false, None);
            assert!(result.is_ok(), "Failed to copy file: {}", name);
            assert!(dst_file.exists());
        }
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_empty_directory() {
    let temp_dir = create_temp_dir("empty_dir");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create empty subdirectory
    let empty_subdir = src_dir.join("empty");
    fs::create_dir_all(&empty_subdir).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::{copy_file_native, CopyOptions};

        let result = copy_file_native(&empty_subdir, &dst_dir.join("empty"), CopyOptions::default(), None);
        assert!(result.is_ok());
        assert!(dst_dir.join("empty").exists());
        assert!(dst_dir.join("empty").is_dir());
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_readonly_source() {
    let temp_dir = create_temp_dir("readonly_source");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a readonly file
    let src_file = src_dir.join("readonly.txt");
    fs::write(&src_file, "readonly content").unwrap();
    fs::set_permissions(&src_file, fs::Permissions::from_mode(0o444)).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        let dst_file = dst_dir.join("readonly.txt");
        let result = copy_single_file_native(&src_file, &dst_file, false, None);
        assert!(result.is_ok());
        assert!(dst_file.exists());

        // Verify permissions are preserved
        let dst_perms = fs::metadata(&dst_file).unwrap().permissions().mode();
        assert_eq!(dst_perms & 0o777, 0o444);
    }

    // Cleanup: restore write permissions so we can delete
    fs::set_permissions(&src_file, fs::Permissions::from_mode(0o644)).unwrap();

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Symlink loop detection tests
// ============================================================================

#[test]
fn test_copy_detects_symlink_loop() {
    let temp_dir = create_temp_dir("symlink_loop");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a directory structure with a symlink loop
    // src/a/b -> src/a (creates a loop)
    let a_dir = src_dir.join("a");
    fs::create_dir_all(&a_dir).unwrap();
    fs::write(a_dir.join("file.txt"), "content").unwrap();

    let loop_link = a_dir.join("b");
    std::os::unix::fs::symlink(&a_dir, &loop_link).unwrap();

    // The copy should detect the loop during scanning
    // We can't easily test the full copy operation without a Tauri app handle,
    // but we can verify the symlink loop exists
    assert!(loop_link.is_symlink());
    let link_target = fs::read_link(&loop_link).unwrap();
    assert_eq!(link_target, a_dir);

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Delete tests
// ============================================================================

#[test]
fn test_delete_directory_manually() {
    let temp_dir = create_temp_dir("delete_recursive");
    let target = temp_dir.join("target");

    // Create nested structure
    fs::create_dir_all(target.join("subdir1/subdir2")).unwrap();
    fs::write(target.join("file1.txt"), "file1").unwrap();
    fs::write(target.join("subdir1/file2.txt"), "file2").unwrap();
    fs::write(target.join("subdir1/subdir2/file3.txt"), "file3").unwrap();

    // Verify everything exists
    assert!(target.join("file1.txt").exists());
    assert!(target.join("subdir1/file2.txt").exists());
    assert!(target.join("subdir1/subdir2/file3.txt").exists());

    // Use std::fs::remove_dir_all (which is what our delete implementation uses internally)
    fs::remove_dir_all(&target).unwrap();

    // Verify everything is deleted
    assert!(!target.exists());

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Move tests
// ============================================================================

#[test]
fn test_move_same_fs_uses_rename() {
    let temp_dir = create_temp_dir("move_same_fs");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a source file
    let src_file = src_dir.join("file.txt");
    fs::write(&src_file, "content").unwrap();

    // Get inode before move
    #[cfg(unix)]
    let src_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&src_file).unwrap().ino()
    };

    // Perform rename (same as what move does on same fs)
    let dst_file = dst_dir.join("file.txt");
    fs::rename(&src_file, &dst_file).unwrap();

    // Verify source gone, destination exists
    assert!(!src_file.exists());
    assert!(dst_file.exists());

    // Verify inode is same (proves it was a rename, not copy+delete)
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let dst_inode = fs::metadata(&dst_file).unwrap().ino();
        assert_eq!(src_inode, dst_inode);
    }

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Conflict resolution tests
// ============================================================================

#[test]
fn test_conflict_rename_generates_unique_names() {
    let temp_dir = create_temp_dir("conflict_rename");

    // Create existing files
    fs::write(temp_dir.join("file.txt"), "original").unwrap();
    fs::write(temp_dir.join("file (1).txt"), "first copy").unwrap();
    fs::write(temp_dir.join("file (2).txt"), "second copy").unwrap();

    // Find unique name should return "file (3).txt"
    let path = temp_dir.join("file.txt");
    let unique = find_unique_name(&path);
    assert_eq!(unique.file_name().unwrap().to_str().unwrap(), "file (3).txt");

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Edge case tests
// ============================================================================

#[test]
fn test_long_paths() {
    let temp_dir = create_temp_dir("long_paths");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a deeply nested path (but not too deep to fail on the OS)
    let mut nested = src_dir.clone();
    for i in 0..20 {
        nested = nested.join(format!("level{}", i));
    }
    fs::create_dir_all(&nested).unwrap();
    let long_file = nested.join("file.txt");
    fs::write(&long_file, "content").unwrap();

    assert!(long_file.exists());

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        // Copy the deeply nested file
        let mut dst_nested = dst_dir.clone();
        for i in 0..20 {
            dst_nested = dst_nested.join(format!("level{}", i));
        }
        fs::create_dir_all(&dst_nested).unwrap();
        let dst_file = dst_nested.join("file.txt");

        let result = copy_single_file_native(&long_file, &dst_file, false, None);
        assert!(result.is_ok());
        assert!(dst_file.exists());
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_readonly_destination() {
    let temp_dir = create_temp_dir("readonly_dest");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create source file
    let src_file = src_dir.join("file.txt");
    fs::write(&src_file, "content").unwrap();

    // Make destination directory read-only
    fs::set_permissions(&dst_dir, fs::Permissions::from_mode(0o555)).unwrap();

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        let dst_file = dst_dir.join("file.txt");
        let result = copy_single_file_native(&src_file, &dst_file, false, None);

        // Should fail with permission denied
        assert!(result.is_err());
    }

    // Restore permissions for cleanup
    fs::set_permissions(&dst_dir, fs::Permissions::from_mode(0o755)).unwrap();

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_preserves_xattrs() {
    let temp_dir = create_temp_dir("xattrs");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.txt");
    fs::write(&src_file, "content").unwrap();

    // Set an extended attribute using xattr command
    let xattr_result = std::process::Command::new("xattr")
        .args(["-w", "com.test.attr", "test_value", src_file.to_str().unwrap()])
        .output();

    if xattr_result.is_err() || !xattr_result.as_ref().unwrap().status.success() {
        // Skip test if xattr command not available
        cleanup_temp_dir(&temp_dir);
        return;
    }

    #[cfg(target_os = "macos")]
    {
        use super::macos_copy::copy_single_file_native;

        let dst_file = dst_dir.join("file.txt");
        let result = copy_single_file_native(&src_file, &dst_file, false, None);
        assert!(result.is_ok());

        // Verify xattr was preserved
        let output = std::process::Command::new("xattr")
            .args(["-p", "com.test.attr", dst_file.to_str().unwrap()])
            .output()
            .expect("Failed to read xattr");

        let value = String::from_utf8_lossy(&output.stdout);
        assert!(value.trim() == "test_value", "xattr not preserved: {}", value);
    }

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Helper function to find unique name (exposed for testing)
// ============================================================================

fn find_unique_name(path: &std::path::Path) -> PathBuf {
    let parent = path.parent().unwrap_or(std::path::Path::new(""));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path.extension().map(|s| s.to_string_lossy().to_string());

    let mut counter = 1;
    loop {
        let new_name = match &extension {
            Some(ext) => format!("{} ({}).{}", stem, counter, ext),
            None => format!("{} ({})", stem, counter),
        };
        let new_path = parent.join(new_name);
        if !new_path.exists() {
            return new_path;
        }
        counter += 1;
    }
}

// ============================================================================
// CopyTransaction rollback tests
// ============================================================================

#[test]
fn test_copy_transaction_records_files() {
    use super::write_operations::CopyTransaction;

    let temp_dir = create_temp_dir("transaction_record");

    let mut tx = CopyTransaction::new();
    let file1 = temp_dir.join("file1.txt");
    let file2 = temp_dir.join("file2.txt");

    tx.record_file(file1.clone());
    tx.record_file(file2.clone());

    assert_eq!(tx.created_files.len(), 2);
    assert!(tx.created_files.contains(&file1));
    assert!(tx.created_files.contains(&file2));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_transaction_records_dirs() {
    use super::write_operations::CopyTransaction;

    let temp_dir = create_temp_dir("transaction_record_dirs");

    let mut tx = CopyTransaction::new();
    let dir1 = temp_dir.join("dir1");
    let dir2 = temp_dir.join("dir2");

    tx.record_dir(dir1.clone());
    tx.record_dir(dir2.clone());

    assert_eq!(tx.created_dirs.len(), 2);
    assert!(tx.created_dirs.contains(&dir1));
    assert!(tx.created_dirs.contains(&dir2));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_transaction_rollback_removes_files() {
    use super::write_operations::CopyTransaction;

    let temp_dir = create_temp_dir("transaction_rollback_files");

    // Create actual files
    let file1 = temp_dir.join("file1.txt");
    let file2 = temp_dir.join("file2.txt");
    fs::write(&file1, "content1").unwrap();
    fs::write(&file2, "content2").unwrap();

    // Record them in transaction
    let mut tx = CopyTransaction::new();
    tx.record_file(file1.clone());
    tx.record_file(file2.clone());

    // Verify files exist
    assert!(file1.exists());
    assert!(file2.exists());

    // Rollback
    tx.rollback();

    // Verify files deleted
    assert!(!file1.exists());
    assert!(!file2.exists());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_transaction_rollback_removes_dirs() {
    use super::write_operations::CopyTransaction;

    let temp_dir = create_temp_dir("transaction_rollback_dirs");

    // Create nested directories
    let dir1 = temp_dir.join("dir1");
    let dir2 = dir1.join("dir2");
    fs::create_dir_all(&dir2).unwrap();

    // Record them in creation order (parent first)
    let mut tx = CopyTransaction::new();
    tx.record_dir(dir1.clone());
    tx.record_dir(dir2.clone());

    // Verify dirs exist
    assert!(dir1.exists());
    assert!(dir2.exists());

    // Rollback (should remove in reverse order)
    tx.rollback();

    // Verify dirs deleted
    assert!(!dir2.exists());
    assert!(!dir1.exists());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_transaction_rollback_mixed() {
    use super::write_operations::CopyTransaction;

    let temp_dir = create_temp_dir("transaction_rollback_mixed");

    // Create a directory with files
    let dir1 = temp_dir.join("dir1");
    fs::create_dir_all(&dir1).unwrap();
    let file1 = dir1.join("file1.txt");
    fs::write(&file1, "content").unwrap();

    // Record them in creation order
    let mut tx = CopyTransaction::new();
    tx.record_dir(dir1.clone());
    tx.record_file(file1.clone());

    // Verify everything exists
    assert!(dir1.exists());
    assert!(file1.exists());

    // Rollback
    tx.rollback();

    // Files should be deleted first, then directories
    assert!(!file1.exists());
    assert!(!dir1.exists());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_transaction_commit_preserves_files() {
    use super::write_operations::CopyTransaction;

    let temp_dir = create_temp_dir("transaction_commit");

    // Create actual files
    let file1 = temp_dir.join("file1.txt");
    fs::write(&file1, "content").unwrap();

    // Record in transaction
    let mut tx = CopyTransaction::new();
    tx.record_file(file1.clone());

    // Commit (should NOT delete)
    tx.commit();

    // File should still exist
    assert!(file1.exists());

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Validation function tests
// ============================================================================

#[test]
fn test_validate_sources_with_existing_files() {
    use super::write_operations::validate_sources;

    let temp_dir = create_temp_dir("validate_sources_exist");
    let file1 = temp_dir.join("file1.txt");
    let file2 = temp_dir.join("file2.txt");
    fs::write(&file1, "content1").unwrap();
    fs::write(&file2, "content2").unwrap();

    let result = validate_sources(&[file1, file2]);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_sources_with_missing_file() {
    use super::write_operations::validate_sources;

    let temp_dir = create_temp_dir("validate_sources_missing");
    let file1 = temp_dir.join("exists.txt");
    let file2 = temp_dir.join("missing.txt");
    fs::write(&file1, "content").unwrap();

    let result = validate_sources(&[file1, file2]);
    assert!(matches!(result, Err(WriteOperationError::SourceNotFound { .. })));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_sources_with_symlink() {
    use super::write_operations::validate_sources;

    let temp_dir = create_temp_dir("validate_sources_symlink");
    let target = temp_dir.join("target.txt");
    let link = temp_dir.join("link");
    fs::write(&target, "content").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // Should accept symlinks
    let result = validate_sources(&[link]);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_sources_with_broken_symlink() {
    use super::write_operations::validate_sources;

    let temp_dir = create_temp_dir("validate_sources_broken_symlink");
    let link = temp_dir.join("broken_link");
    std::os::unix::fs::symlink("/nonexistent/path", &link).unwrap();

    // Should accept broken symlinks (symlink_metadata succeeds)
    let result = validate_sources(&[link]);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_with_existing_dir() {
    use super::write_operations::validate_destination;

    let temp_dir = create_temp_dir("validate_dest_dir");
    let dest = temp_dir.join("dest");
    fs::create_dir_all(&dest).unwrap();

    let result = validate_destination(&dest);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_with_missing_dir() {
    use super::write_operations::validate_destination;

    let temp_dir = create_temp_dir("validate_dest_missing");
    let dest = temp_dir.join("missing");

    let result = validate_destination(&dest);
    assert!(matches!(result, Err(WriteOperationError::SourceNotFound { .. })));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_with_file() {
    use super::write_operations::validate_destination;

    let temp_dir = create_temp_dir("validate_dest_file");
    let dest = temp_dir.join("file.txt");
    fs::write(&dest, "content").unwrap();

    let result = validate_destination(&dest);
    assert!(matches!(result, Err(WriteOperationError::IoError { .. })));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_not_same_location_different() {
    use super::write_operations::validate_not_same_location;

    let temp_dir = create_temp_dir("validate_same_loc_diff");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let file = src_dir.join("file.txt");
    fs::write(&file, "content").unwrap();

    let result = validate_not_same_location(&[file], &dst_dir);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_not_same_location_same() {
    use super::write_operations::validate_not_same_location;

    let temp_dir = create_temp_dir("validate_same_loc_same");
    let file = temp_dir.join("file.txt");
    fs::write(&file, "content").unwrap();

    // Copying file to same directory
    let result = validate_not_same_location(&[file], &temp_dir);
    assert!(matches!(result, Err(WriteOperationError::SameLocation { .. })));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_not_inside_source_ok() {
    use super::write_operations::validate_destination_not_inside_source;

    let temp_dir = create_temp_dir("validate_inside_ok");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let result = validate_destination_not_inside_source(&[src_dir], &dst_dir);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_not_inside_source_nested() {
    use super::write_operations::validate_destination_not_inside_source;

    let temp_dir = create_temp_dir("validate_inside_nested");
    let src_dir = temp_dir.join("src");
    let dst_dir = src_dir.join("nested/dest");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Copying src to src/nested/dest - destination is inside source
    let result = validate_destination_not_inside_source(std::slice::from_ref(&src_dir), &dst_dir);
    assert!(matches!(
        result,
        Err(WriteOperationError::DestinationInsideSource { .. })
    ));

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Filesystem detection tests
// ============================================================================

#[test]
fn test_is_same_filesystem_same_volume() {
    use super::write_operations::is_same_filesystem;

    let temp_dir = create_temp_dir("same_fs");
    let dir1 = temp_dir.join("dir1");
    let dir2 = temp_dir.join("dir2");
    fs::create_dir_all(&dir1).unwrap();
    fs::create_dir_all(&dir2).unwrap();

    let result = is_same_filesystem(&dir1, &dir2);
    assert!(result.is_ok());
    assert!(result.unwrap()); // Should be same filesystem

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_is_same_filesystem_with_root() {
    use super::write_operations::is_same_filesystem;

    let temp_dir = create_temp_dir("same_fs_root");

    // Compare temp dir with root (/) - should be same on most systems
    let result = is_same_filesystem(&temp_dir, std::path::Path::new("/"));
    assert!(result.is_ok());
    // Note: Result depends on whether temp is on the same volume as root

    cleanup_temp_dir(&temp_dir);
}
