//! Integration tests for delete operations.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

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
