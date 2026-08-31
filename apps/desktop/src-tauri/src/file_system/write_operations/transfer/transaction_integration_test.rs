//! Integration tests for CopyTransaction rollback behavior.

use crate::file_system::write_operations::ledger::WrittenFile;
use crate::file_system::write_operations::reversal::ReversalGuard;
use crate::test_support::TestDir;
use std::fs;

// ============================================================================
// Test utilities
// ============================================================================

fn create_temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("write_integration_test_{}", name))
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

    tx.record_file(WrittenFile::local(file1.clone()));
    tx.record_file(WrittenFile::local(file2.clone()));

    assert_eq!(tx.created_files().len(), 2);
    assert!(tx.created_files().iter().any(|f| f.path == file1));
    assert!(tx.created_files().iter().any(|f| f.path == file2));
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
    tx.record_file(WrittenFile::local(file1.clone()));
    tx.record_file(WrittenFile::local(file2.clone()));

    // Verify files exist
    assert!(file1.exists());
    assert!(file2.exists());

    // Rollback
    tx.rollback(ReversalGuard::SkipDrifted);

    // Verify files deleted
    assert!(!file1.exists());
    assert!(!file2.exists());
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
    tx.rollback(ReversalGuard::SkipDrifted);

    // Verify dirs deleted
    assert!(!dir2.exists());
    assert!(!dir1.exists());
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
    tx.record_file(WrittenFile::local(file1.clone()));

    // Verify everything exists
    assert!(dir1.exists());
    assert!(file1.exists());

    // Rollback
    tx.rollback(ReversalGuard::SkipDrifted);

    // Files should be deleted first, then directories
    assert!(!file1.exists());
    assert!(!dir1.exists());
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
    tx.record_file(WrittenFile::local(file1.clone()));

    // Commit (should NOT delete)
    tx.commit();

    // File should still exist
    assert!(file1.exists());
}

// ============================================================================
// What a reversal refuses to touch
// ============================================================================

/// A destination file something else replaced after the copy wrote it is NOT
/// deleted when the copy is reversed. The reversal removes what this operation
/// put on disk; the file sitting there now is somebody else's.
#[test]
fn a_destination_changed_since_the_copy_wrote_it_survives_the_reversal() {
    use super::CopyTransaction;

    let temp_dir = create_temp_dir("reversal_drift");
    let ours = temp_dir.join("ours.txt");
    let theirs = temp_dir.join("theirs.txt");
    fs::write(&ours, b"what the copy wrote").unwrap();
    fs::write(&theirs, b"what the copy wrote").unwrap();

    let mut tx = CopyTransaction::new();
    tx.record_file(WrittenFile::local(ours.clone()));
    tx.record_file(WrittenFile::local(theirs.clone()));

    // Somebody else saves over one of them the way an editor does: write a temp,
    // rename it into place. Same length, different file.
    let incoming = temp_dir.join("theirs.incoming");
    fs::write(&incoming, b"what somebody else").unwrap();
    fs::rename(&incoming, &theirs).unwrap();

    tx.rollback(ReversalGuard::SkipDrifted);

    assert!(
        theirs.exists(),
        "a file replaced since the copy wrote it must survive the reversal"
    );
    assert!(
        !ours.exists(),
        "one drifted file must not stop the reversal removing its unchanged neighbours"
    );
}
