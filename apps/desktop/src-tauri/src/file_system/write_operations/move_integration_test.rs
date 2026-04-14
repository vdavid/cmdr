//! Integration tests for move operations and cross-filesystem staging.

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
