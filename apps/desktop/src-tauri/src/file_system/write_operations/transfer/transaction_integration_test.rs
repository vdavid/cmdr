//! Integration tests for CopyTransaction rollback behavior.

use std::fs;
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
