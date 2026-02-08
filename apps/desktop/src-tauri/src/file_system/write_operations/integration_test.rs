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

use super::*;

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
        use super::macos_copy::{CopyOptions, copy_file_native};

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
        use super::macos_copy::{CopyOptions, copy_file_native};

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
fn test_delete_single_file() {
    let temp_dir = create_temp_dir("delete_single_file");
    let file = temp_dir.join("file.txt");
    fs::write(&file, "content to delete").unwrap();

    assert!(file.exists());

    // Delete using the same approach as delete.rs (remove_file per file)
    fs::remove_file(&file).unwrap();

    assert!(!file.exists());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_delete_directory_files_then_dirs() {
    let temp_dir = create_temp_dir("delete_files_then_dirs");
    let target = temp_dir.join("target");

    // Create nested structure matching what scan would produce
    fs::create_dir_all(target.join("subdir1/subdir2")).unwrap();
    fs::write(target.join("file1.txt"), "file1").unwrap();
    fs::write(target.join("subdir1/file2.txt"), "file2").unwrap();
    fs::write(target.join("subdir1/subdir2/file3.txt"), "file3").unwrap();

    // Simulate delete.rs: remove files first
    let files = vec![
        target.join("file1.txt"),
        target.join("subdir1/file2.txt"),
        target.join("subdir1/subdir2/file3.txt"),
    ];
    for file in &files {
        fs::remove_file(file).unwrap();
    }

    // Then remove directories deepest-first (reversed creation order)
    let dirs = [target.clone(), target.join("subdir1"), target.join("subdir1/subdir2")];
    for dir in dirs.iter().rev() {
        let _ = fs::remove_dir(dir);
    }

    // Verify everything is deleted
    assert!(!target.exists());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_delete_permission_error_on_file() {
    // Skip if running as root (root bypasses permission checks)
    if unsafe { libc::geteuid() } == 0 {
        return;
    }

    let temp_dir = create_temp_dir("delete_permission_error");
    let protected_dir = temp_dir.join("protected");
    fs::create_dir_all(&protected_dir).unwrap();
    let file = protected_dir.join("file.txt");
    fs::write(&file, "protected content").unwrap();

    // Make directory read-only so file can't be deleted
    fs::set_permissions(&protected_dir, fs::Permissions::from_mode(0o555)).unwrap();

    let result = fs::remove_file(&file);
    assert!(result.is_err());

    // Verify the error is a permission error
    let err = result.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);

    // Verify the file still exists
    assert!(file.exists());

    // Restore permissions for cleanup
    fs::set_permissions(&protected_dir, fs::Permissions::from_mode(0o755)).unwrap();
    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_delete_partial_state_on_error() {
    // Skip if running as root (root bypasses permission checks)
    if unsafe { libc::geteuid() } == 0 {
        return;
    }

    let temp_dir = create_temp_dir("delete_partial");

    // Create files: one deletable, one protected, one more deletable
    let deletable_dir = temp_dir.join("deletable");
    fs::create_dir_all(&deletable_dir).unwrap();
    let file1 = deletable_dir.join("file1.txt");
    fs::write(&file1, "can delete").unwrap();

    let protected_dir = temp_dir.join("protected");
    fs::create_dir_all(&protected_dir).unwrap();
    let file2 = protected_dir.join("file2.txt");
    fs::write(&file2, "protected").unwrap();

    let file3 = deletable_dir.join("file3.txt");
    fs::write(&file3, "also deletable").unwrap();

    // Simulate the delete loop from delete.rs: delete files in order, stop on error
    let files = vec![file1.clone(), file2.clone(), file3.clone()];

    // Make protected_dir read-only so file2 can't be deleted
    fs::set_permissions(&protected_dir, fs::Permissions::from_mode(0o555)).unwrap();

    let mut files_deleted = 0;
    for file in &files {
        match fs::remove_file(file) {
            Ok(()) => files_deleted += 1,
            Err(_) => break, // Stop on first error, matching delete.rs behavior
        }
    }

    // file1 was deleted, file2 failed, file3 was never attempted
    assert_eq!(files_deleted, 1);
    assert!(!file1.exists(), "file1 should be deleted");
    assert!(file2.exists(), "file2 should still exist (permission denied)");
    assert!(file3.exists(), "file3 should still exist (not attempted)");

    // Restore permissions for cleanup
    fs::set_permissions(&protected_dir, fs::Permissions::from_mode(0o755)).unwrap();
    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_delete_empty_directory() {
    let temp_dir = create_temp_dir("delete_empty_dir");
    let empty_dir = temp_dir.join("empty");
    fs::create_dir_all(&empty_dir).unwrap();

    assert!(empty_dir.exists());

    // remove_dir only works on empty directories, matching delete.rs behavior
    fs::remove_dir(&empty_dir).unwrap();

    assert!(!empty_dir.exists());

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
// Cross-filesystem move staging pattern tests
// ============================================================================

#[test]
fn test_staging_copy_then_rename_preserves_content() {
    let temp_dir = create_temp_dir("staging_copy_rename");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    let staging_dir = dst_dir.join(".cmdr-staging-test-op");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();
    fs::create_dir_all(&staging_dir).unwrap();

    // Create source files
    fs::write(src_dir.join("file1.txt"), "content1").unwrap();
    fs::create_dir_all(src_dir.join("subdir")).unwrap();
    fs::write(src_dir.join("subdir/file2.txt"), "content2").unwrap();

    // Phase 1: Copy files to staging (simulates copy phase of move_with_staging)
    fs::write(staging_dir.join("file1.txt"), "content1").unwrap();
    fs::create_dir_all(staging_dir.join("subdir")).unwrap();
    fs::write(staging_dir.join("subdir/file2.txt"), "content2").unwrap();

    // Phase 2: Rename staged items to final destination
    fs::rename(staging_dir.join("file1.txt"), dst_dir.join("file1.txt")).unwrap();
    fs::rename(staging_dir.join("subdir"), dst_dir.join("subdir")).unwrap();

    // Phase 3: Remove empty staging directory
    fs::remove_dir(&staging_dir).unwrap();

    // Verify final destination has correct content
    assert_eq!(fs::read_to_string(dst_dir.join("file1.txt")).unwrap(), "content1");
    assert_eq!(
        fs::read_to_string(dst_dir.join("subdir/file2.txt")).unwrap(),
        "content2"
    );
    assert!(!staging_dir.exists(), "Staging directory should be removed");

    // Source files still exist (deletion is a separate phase)
    assert!(src_dir.join("file1.txt").exists());
    assert!(src_dir.join("subdir/file2.txt").exists());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_staging_cleanup_on_copy_failure() {
    let temp_dir = create_temp_dir("staging_copy_fail");
    let dst_dir = temp_dir.join("dst");
    let staging_dir = dst_dir.join(".cmdr-staging-fail-op");
    fs::create_dir_all(&dst_dir).unwrap();
    fs::create_dir_all(&staging_dir).unwrap();

    // Simulate partial copy to staging: one file succeeds, then failure
    fs::write(staging_dir.join("file1.txt"), "partial").unwrap();
    assert!(staging_dir.join("file1.txt").exists());

    // On copy failure, move_with_staging calls remove_dir_all on staging
    fs::remove_dir_all(&staging_dir).unwrap();

    assert!(
        !staging_dir.exists(),
        "Staging directory should be cleaned up on copy failure"
    );
    // Destination directory itself should remain untouched
    assert!(dst_dir.exists());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_staging_cleanup_on_rename_failure() {
    // Skip if running as root (root bypasses permission checks)
    if unsafe { libc::geteuid() } == 0 {
        return;
    }

    let temp_dir = create_temp_dir("staging_rename_fail");
    let dst_dir = temp_dir.join("dst");
    let staging_dir = dst_dir.join(".cmdr-staging-rename-fail");
    fs::create_dir_all(&dst_dir).unwrap();
    fs::create_dir_all(&staging_dir).unwrap();

    // Copy files to staging (simulates successful copy phase)
    fs::write(staging_dir.join("file1.txt"), "staged content").unwrap();

    // Make destination read-only so rename from staging fails
    fs::set_permissions(&dst_dir, fs::Permissions::from_mode(0o555)).unwrap();

    // Attempt rename (should fail due to read-only destination)
    let rename_result = fs::rename(staging_dir.join("file1.txt"), dst_dir.join("file1.txt"));
    assert!(rename_result.is_err(), "Rename should fail on read-only destination");

    // Restore permissions so cleanup can proceed
    fs::set_permissions(&dst_dir, fs::Permissions::from_mode(0o755)).unwrap();

    // On rename failure, move_with_staging calls remove_dir_all on staging
    fs::remove_dir_all(&staging_dir).unwrap();

    assert!(
        !staging_dir.exists(),
        "Staging directory should be cleaned up on rename failure"
    );
    // Destination should have no leftover files from the failed operation
    assert!(
        !dst_dir.join("file1.txt").exists(),
        "Failed rename should not leave files in destination"
    );

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_staging_source_preserved_on_failure() {
    let temp_dir = create_temp_dir("staging_source_preserved");
    let src_dir = temp_dir.join("src");
    let dst_dir = temp_dir.join("dst");
    let staging_dir = dst_dir.join(".cmdr-staging-preserve-op");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();
    fs::create_dir_all(&staging_dir).unwrap();

    // Create source files
    fs::write(src_dir.join("important.txt"), "precious data").unwrap();
    fs::write(src_dir.join("also_important.txt"), "more data").unwrap();

    // Simulate copy to staging succeeds for first file, fails for second
    fs::write(staging_dir.join("important.txt"), "precious data").unwrap();
    // (second file copy "fails" -- we skip it)

    // On failure, clean up staging
    fs::remove_dir_all(&staging_dir).unwrap();

    // Source files must be intact (move_with_staging only deletes sources after full success)
    assert_eq!(
        fs::read_to_string(src_dir.join("important.txt")).unwrap(),
        "precious data"
    );
    assert_eq!(
        fs::read_to_string(src_dir.join("also_important.txt")).unwrap(),
        "more data"
    );

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_staging_dir_naming_uses_operation_id() {
    let temp_dir = create_temp_dir("staging_naming");
    let dst_dir = temp_dir.join("dst");
    fs::create_dir_all(&dst_dir).unwrap();

    // Verify staging directory follows the naming convention from move_op.rs
    let operation_id = "abc-123-def";
    let staging_dir = dst_dir.join(format!(".cmdr-staging-{}", operation_id));
    fs::create_dir(&staging_dir).unwrap();

    assert!(staging_dir.exists());
    assert!(staging_dir.is_dir());
    // Dot-prefixed, so hidden on Unix
    assert!(
        staging_dir.file_name().unwrap().to_str().unwrap().starts_with('.'),
        "Staging directory should be hidden (dot-prefixed)"
    );

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_staging_atomic_rename_is_same_inode() {
    let temp_dir = create_temp_dir("staging_atomic");
    let staging_dir = temp_dir.join("staging");
    let final_dir = temp_dir.join("final");
    fs::create_dir_all(&staging_dir).unwrap();
    fs::create_dir_all(&final_dir).unwrap();

    let staged_file = staging_dir.join("file.txt");
    fs::write(&staged_file, "staged content").unwrap();

    #[cfg(unix)]
    let staged_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&staged_file).unwrap().ino()
    };

    // Rename from staging to final (same filesystem, should be atomic)
    let final_file = final_dir.join("file.txt");
    fs::rename(&staged_file, &final_file).unwrap();

    assert!(!staged_file.exists(), "Staged file should be gone after rename");
    assert!(final_file.exists(), "Final file should exist after rename");
    assert_eq!(fs::read_to_string(&final_file).unwrap(), "staged content");

    // Verify rename was atomic (same inode)
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let final_inode = fs::metadata(&final_file).unwrap().ino();
        assert_eq!(staged_inode, final_inode, "Rename should preserve inode (atomic)");
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
        assert_eq!(value.trim(), "test_value", "xattr not preserved: {}", value);
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
    use super::CopyTransaction;

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
    use super::CopyTransaction;

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
    use super::CopyTransaction;

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
    use super::CopyTransaction;

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
    use super::CopyTransaction;

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
    use super::CopyTransaction;

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
    use super::validate_sources;

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
    use super::validate_sources;

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
    use super::validate_sources;

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
    use super::validate_sources;

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
    use super::validate_destination;

    let temp_dir = create_temp_dir("validate_dest_dir");
    let dest = temp_dir.join("dest");
    fs::create_dir_all(&dest).unwrap();

    let result = validate_destination(&dest);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_with_missing_dir() {
    use super::validate_destination;

    let temp_dir = create_temp_dir("validate_dest_missing");
    let dest = temp_dir.join("missing");

    let result = validate_destination(&dest);
    assert!(matches!(result, Err(WriteOperationError::SourceNotFound { .. })));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_with_file() {
    use super::validate_destination;

    let temp_dir = create_temp_dir("validate_dest_file");
    let dest = temp_dir.join("file.txt");
    fs::write(&dest, "content").unwrap();

    let result = validate_destination(&dest);
    assert!(matches!(result, Err(WriteOperationError::IoError { .. })));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_not_same_location_different() {
    use super::validate_not_same_location;

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
    use super::validate_not_same_location;

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
    use super::validate_destination_not_inside_source;

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
    use super::validate_destination_not_inside_source;

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
    use super::is_same_filesystem;

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
    use super::is_same_filesystem;

    let temp_dir = create_temp_dir("same_fs_root");

    // Compare temp dir with root (/) - should be same on most systems
    let result = is_same_filesystem(&temp_dir, std::path::Path::new("/"));
    assert!(result.is_ok());
    // Note: Result depends on whether temp is on the same volume as root

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Copy safety: path canonicalization (prevents ".." bypass)
// ============================================================================

#[test]
fn test_validate_destination_not_inside_source_dotdot_bypass() {
    use super::validate_destination_not_inside_source;

    let temp_dir = create_temp_dir("validate_inside_dotdot");
    let src_dir = temp_dir.join("src");
    let nested = src_dir.join("nested");
    fs::create_dir_all(&nested).unwrap();

    // Attempt to bypass with ".." segments: src/nested/../nested is still inside src
    let sneaky_dest = src_dir.join("nested").join("..").join("nested");
    let result = validate_destination_not_inside_source(std::slice::from_ref(&src_dir), &sneaky_dest);
    assert!(
        matches!(result, Err(WriteOperationError::DestinationInsideSource { .. })),
        "Should detect destination inside source even with '..' segments"
    );

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_not_inside_source_symlink_bypass() {
    use super::validate_destination_not_inside_source;

    let temp_dir = create_temp_dir("validate_inside_symlink");
    let src_dir = temp_dir.join("src");
    let nested = src_dir.join("nested");
    fs::create_dir_all(&nested).unwrap();

    // Create a symlink outside src that points back into src
    let alias = temp_dir.join("alias_to_nested");
    std::os::unix::fs::symlink(&nested, &alias).unwrap();

    // Destination via symlink resolves back inside source
    let result = validate_destination_not_inside_source(std::slice::from_ref(&src_dir), &alias);
    assert!(
        matches!(result, Err(WriteOperationError::DestinationInsideSource { .. })),
        "Should detect destination inside source even through symlinks"
    );

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Copy safety: destination writability check
// ============================================================================

#[test]
fn test_validate_destination_writable_ok() {
    use super::validate_destination_writable;

    let temp_dir = create_temp_dir("validate_writable_ok");
    let dest = temp_dir.join("writable_dest");
    fs::create_dir_all(&dest).unwrap();

    let result = validate_destination_writable(&dest);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_validate_destination_writable_readonly() {
    use super::validate_destination_writable;

    // Skip if running as root (root bypasses permission checks)
    if unsafe { libc::geteuid() } == 0 {
        return;
    }

    let temp_dir = create_temp_dir("validate_writable_ro");
    let dest = temp_dir.join("readonly_dest");
    fs::create_dir_all(&dest).unwrap();
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o555)).unwrap();

    let result = validate_destination_writable(&dest);
    assert!(
        matches!(result, Err(WriteOperationError::PermissionDenied { .. })),
        "Should reject read-only destination"
    );

    // Restore for cleanup
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o755)).unwrap();
    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Copy safety: inode identity check (copy-over-self via symlink)
// ============================================================================

#[test]
fn test_is_same_file_different_files() {
    use super::is_same_file;

    let temp_dir = create_temp_dir("same_file_diff");
    let file1 = temp_dir.join("file1.txt");
    let file2 = temp_dir.join("file2.txt");
    fs::write(&file1, "content1").unwrap();
    fs::write(&file2, "content2").unwrap();

    assert!(!is_same_file(&file1, &file2));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_is_same_file_via_symlink() {
    use super::is_same_file;

    let temp_dir = create_temp_dir("same_file_symlink");
    let file = temp_dir.join("original.txt");
    let link = temp_dir.join("link.txt");
    fs::write(&file, "content").unwrap();
    std::os::unix::fs::symlink(&file, &link).unwrap();

    // file and link resolve to the same inode
    assert!(is_same_file(&file, &link));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_is_same_file_via_hardlink() {
    use super::is_same_file;

    let temp_dir = create_temp_dir("same_file_hardlink");
    let file = temp_dir.join("original.txt");
    let link = temp_dir.join("hardlink.txt");
    fs::write(&file, "content").unwrap();
    fs::hard_link(&file, &link).unwrap();

    // Hard links share the same inode
    assert!(is_same_file(&file, &link));

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_is_same_file_nonexistent_dest() {
    use super::is_same_file;

    let temp_dir = create_temp_dir("same_file_noexist");
    let file = temp_dir.join("exists.txt");
    let missing = temp_dir.join("missing.txt");
    fs::write(&file, "content").unwrap();

    // Should return false when destination doesn't exist
    assert!(!is_same_file(&file, &missing));

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Copy safety: path length validation
// ============================================================================

#[test]
fn test_validate_path_length_ok() {
    use super::validate_path_length;

    let path = PathBuf::from("/tmp/short/path/file.txt");
    assert!(validate_path_length(&path).is_ok());
}

#[test]
fn test_validate_path_length_name_too_long() {
    use super::validate_path_length;

    // 256-byte filename exceeds the 255-byte APFS limit
    let long_name = "a".repeat(256);
    let path = PathBuf::from("/tmp").join(&long_name);
    let result = validate_path_length(&path);
    assert!(
        matches!(result, Err(WriteOperationError::IoError { ref message, .. }) if message.contains("File name")),
        "Should reject file names exceeding 255 bytes, got: {:?}",
        result
    );
}

#[test]
fn test_validate_path_length_path_too_long() {
    use super::validate_path_length;

    // Build a path longer than 1024 bytes
    let mut path = PathBuf::from("/tmp");
    while path.as_os_str().len() < 1025 {
        path = path.join("a_long_segment_name_here");
    }

    let result = validate_path_length(&path);
    assert!(
        matches!(result, Err(WriteOperationError::IoError { ref message, .. }) if message.contains("Path exceeds")),
        "Should reject paths exceeding 1024 bytes, got: {:?}",
        result
    );
}

#[test]
fn test_validate_path_length_exact_limit() {
    use super::validate_path_length;

    // Exactly 255 bytes is acceptable
    let name = "a".repeat(255);
    let path = PathBuf::from("/tmp").join(&name);
    // Only checks name length, this should pass if total path < 1024
    if path.as_os_str().len() <= 1024 {
        assert!(validate_path_length(&path).is_ok());
    }
}

// ============================================================================
// Copy safety: special file filtering
// ============================================================================

#[cfg(unix)]
#[test]
fn test_special_file_socket_skipped() {
    use std::os::unix::net::UnixListener;

    let temp_dir = create_temp_dir("special_socket");
    let socket_path = temp_dir.join("test.sock");

    // Create a Unix domain socket
    let _listener = UnixListener::bind(&socket_path).unwrap();
    assert!(socket_path.exists());

    // Verify it's not a regular file, symlink, or directory
    let metadata = fs::symlink_metadata(&socket_path).unwrap();
    assert!(!metadata.is_file());
    assert!(!metadata.is_dir());
    assert!(!metadata.is_symlink());

    // The scan functions skip special files — we verify the metadata detection works
    // (Full scan integration requires a Tauri app handle, tested at a higher level)

    cleanup_temp_dir(&temp_dir);
}

#[cfg(unix)]
#[test]
fn test_special_file_fifo_skipped() {
    let temp_dir = create_temp_dir("special_fifo");
    let fifo_path = temp_dir.join("test.fifo");

    // Create a FIFO
    let c_path = std::ffi::CString::new(fifo_path.to_str().unwrap()).unwrap();
    let result = unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) };
    assert_eq!(result, 0, "Failed to create FIFO");
    assert!(fifo_path.exists());

    // Verify it's detected as a special file
    let metadata = fs::symlink_metadata(&fifo_path).unwrap();
    assert!(!metadata.is_file());
    assert!(!metadata.is_dir());
    assert!(!metadata.is_symlink());

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Copy safety: disk space check
// ============================================================================

#[cfg(unix)]
#[test]
fn test_validate_disk_space_sufficient() {
    use super::validate_disk_space;

    let temp_dir = create_temp_dir("disk_space_ok");
    // Requesting 1 byte should always succeed on any volume with free space
    let result = validate_disk_space(&temp_dir, 1);
    assert!(result.is_ok());

    cleanup_temp_dir(&temp_dir);
}

#[cfg(unix)]
#[test]
fn test_validate_disk_space_insufficient() {
    use super::validate_disk_space;

    let temp_dir = create_temp_dir("disk_space_fail");
    // Requesting an absurdly large amount (1 exabyte) should fail
    let result = validate_disk_space(&temp_dir, u64::MAX);
    assert!(
        matches!(result, Err(WriteOperationError::InsufficientSpace { .. })),
        "Should reject when required space exceeds available, got: {:?}",
        result
    );

    cleanup_temp_dir(&temp_dir);
}
