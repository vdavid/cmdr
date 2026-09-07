//! Tests for `overwrite.rs`: the staged file landing, the folder landing, and
//! what `displace_with_directory` does with the entry it moved aside.
//!
//! A `#[path]` child of `overwrite.rs`, so `super::` here is `overwrite` and
//! `super::super::` is `write_operations`.

use super::super::ledger::CopyTransaction;
use super::super::state::WriteOperationState;
use super::super::types::WriteOperationError;
use super::*;
use crate::test_support::TestDir;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

/// Creates a temporary test directory with a unique name.
fn create_temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("write_test_{}", name))
}

// ============================================================================
// safe_overwrite_file tests
// ============================================================================


/// A running operation to land a write under.
fn overwrite_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

/// `stage_and_land_file` with the platform's default byte-copier, which is what
/// the copy strategies hand it for a same-volume local copy.
fn safe_overwrite_file(source: &Path, dest: &Path) -> Result<u64, WriteOperationError> {
    stage_and_land_file(&overwrite_state(), dest, true, |target| {
        #[cfg(target_os = "macos")]
        {
            super::super::transfer::macos_copy::copy_single_file_native(source, target, false, None)
        }
        #[cfg(not(target_os = "macos"))]
        {
            use super::super::error_classification::IoResultExt;
            fs::copy(source, target).with_path(source)
        }
    })
}

#[test]
fn test_safe_overwrite_basic() {
    let temp_dir = create_temp_dir("safe_overwrite_basic");
    let source = temp_dir.join("source.txt");
    let dest = temp_dir.join("dest.txt");

    fs::write(&source, "new-data!!").unwrap();
    fs::write(&dest, "old-data").unwrap();

    let result = safe_overwrite_file(&source, &dest);

    let bytes = result.expect("safe_overwrite_file should succeed");
    assert_eq!(bytes, 10, "should report 10 bytes copied");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "new-data!!");
    assert!(source.exists(), "source should still exist");

    // No leftover temp / set-aside files
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(!name.contains(".cmdr-tmp-"), "temp file should be cleaned up: {name}");
        assert!(!name.contains(".cmdr-temp-"), "aside file should be cleaned up: {name}");
    }
}

#[test]
fn test_safe_overwrite_preserves_dest_on_missing_source() {
    let temp_dir = create_temp_dir("safe_overwrite_missing_src");
    let source = temp_dir.join("nonexistent.txt");
    let dest = temp_dir.join("dest.txt");

    fs::write(&dest, "old-data").unwrap();

    let result = safe_overwrite_file(&source, &dest);

    assert!(result.is_err(), "should fail when source doesn't exist");
    assert_eq!(
        fs::read_to_string(&dest).unwrap(),
        "old-data",
        "original dest content must be untouched"
    );
}

#[test]
fn test_safe_overwrite_dest_has_new_content_after_completion() {
    let temp_dir = create_temp_dir("safe_overwrite_atomic");
    let source = temp_dir.join("source.txt");
    let dest = temp_dir.join("dest.txt");

    let new_content = "replacement-content-here";
    fs::write(&source, new_content).unwrap();
    fs::write(&dest, "original").unwrap();

    let result = safe_overwrite_file(&source, &dest);

    result.expect("safe_overwrite_file should succeed");

    // After completion, reading dest returns the full new content (no partial writes)
    assert_eq!(fs::read_to_string(&dest).unwrap(), new_content);
}

#[test]
fn test_safe_overwrite_different_sizes() {
    let temp_dir = create_temp_dir("safe_overwrite_sizes");

    // Case 1: source much larger than dest
    let source_large = temp_dir.join("large_source.txt");
    let dest_small = temp_dir.join("small_dest.txt");
    let large_content = "x".repeat(100_000);
    fs::write(&source_large, &large_content).unwrap();
    fs::write(&dest_small, "tiny").unwrap();

    let result = safe_overwrite_file(&source_large, &dest_small);

    result.expect("large-to-small overwrite should succeed");
    assert_eq!(fs::read_to_string(&dest_small).unwrap(), large_content);

    // Case 2: source much smaller than dest
    let source_small = temp_dir.join("small_source.txt");
    let dest_large = temp_dir.join("large_dest.txt");
    let large_dest_content = "y".repeat(100_000);
    fs::write(&source_small, "tiny").unwrap();
    fs::write(&dest_large, &large_dest_content).unwrap();

    let result = safe_overwrite_file(&source_small, &dest_large);

    result.expect("small-to-large overwrite should succeed");
    assert_eq!(fs::read_to_string(&dest_large).unwrap(), "tiny");
}

// ============================================================================
// safe_overwrite_file: cross-type overwrites (file source → folder dest)
// ============================================================================

#[test]
fn test_safe_overwrite_file_replaces_existing_folder() {
    // Source = file. Dest = existing folder with contents. After overwrite the
    // dest path holds the source's bytes and the old folder tree is gone, with
    // no stray cmdr-temp artifacts left behind. Pre-fix the caller did a direct
    // `fs::remove_dir_all` before the copy, so a crash mid-delete would lose
    // the folder forever.
    let temp_dir = create_temp_dir("safe_overwrite_file_over_folder");
    let source = temp_dir.join("source.txt");
    let dest = temp_dir.join("dest");

    fs::write(&source, "I am a file now").unwrap();
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("inner-a.txt"), "inner a").unwrap();
    fs::write(dest.join("inner-b.txt"), "inner b").unwrap();
    fs::create_dir_all(dest.join("sub")).unwrap();
    fs::write(dest.join("sub").join("deep.txt"), "deep").unwrap();

    let result = safe_overwrite_file(&source, &dest);

    result.expect("safe_overwrite_file should succeed when dest is an existing folder");

    // Dest is now a file with the source's contents
    let dest_meta = fs::symlink_metadata(&dest).unwrap();
    assert!(dest_meta.is_file(), "dest should be a file after overwrite");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "I am a file now");

    // No cmdr-temp / cmdr-tmp artifacts remain
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            !name.contains(".cmdr-tmp-") && !name.contains(".cmdr-temp-"),
            "no temp / aside artifacts should remain: {name}"
        );
    }
}

// ============================================================================
// safe_overwrite_dir tests (folder materialized over existing file or folder)
// ============================================================================


// ============================================================================
// displace_with_directory: the aside outlives the directory that took its place
// ============================================================================

#[test]
fn displace_with_directory_keeps_the_file_until_the_transaction_answers() {
    // The distinction from `safe_overwrite_dir`: a folder→file Overwrite isn't
    // finished when the directory appears, so the displaced file must still be
    // on disk (under its `.cmdr-temp-` name) once this returns, and only the
    // transaction decides which way it goes.
    let temp_dir = create_temp_dir("displace_with_directory");
    let dest = temp_dir.join("thing");
    fs::write(&dest, "the user's only copy").unwrap();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));

    let displaced = displace_with_directory(&state, &dest).expect("the file should go aside");

    assert!(
        fs::symlink_metadata(&dest).unwrap().is_dir(),
        "a fresh directory stands where the file was"
    );
    let asides: Vec<String> = fs::read_dir(&temp_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.contains(".cmdr-temp-"))
        .collect();
    assert_eq!(asides.len(), 1, "the displaced file is still on disk: {asides:?}");
    assert_eq!(
        fs::read_to_string(temp_dir.join(&asides[0])).unwrap(),
        "the user's only copy",
        "and it still holds every byte"
    );

    // A reversal removes the directory first, then asks for the original back.
    fs::remove_dir(&dest).unwrap();
    let mut transaction = CopyTransaction::new();
    transaction.record_displaced(displaced);
    transaction.restore_displaced();

    assert!(fs::symlink_metadata(&dest).unwrap().is_file(), "the file is back");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "the user's only copy");
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(!name.contains(".cmdr-temp-"), "the aside is gone: {name}");
    }
}

#[test]
fn a_committed_transaction_drops_what_it_displaced() {
    let temp_dir = create_temp_dir("displace_commit");
    let dest = temp_dir.join("thing");
    fs::write(&dest, "replaced").unwrap();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));

    let mut transaction = CopyTransaction::new();
    transaction.record_displaced(displace_with_directory(&state, &dest).expect("displace"));
    transaction.commit();

    assert!(
        fs::symlink_metadata(&dest).unwrap().is_dir(),
        "the directory that replaced it stays"
    );
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(!name.contains(".cmdr-temp-"), "commit drops the aside: {name}");
    }
}

#[test]
fn test_safe_overwrite_dir_materializes_folder_over_existing_file() {
    // Source intent = folder. Dest = existing file. After materialization the
    // dest path is a folder containing the materialized contents, and the
    // original file is gone with no cmdr-temp artifact left.
    let temp_dir = create_temp_dir("safe_overwrite_dir_over_file");
    let dest = temp_dir.join("dest");
    fs::write(&dest, "I am the existing file").unwrap();

    let result = safe_overwrite_dir(&dest, |target| {
        fs::create_dir_all(target).map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("create_dir_all: {e}"),
        })?;
        fs::write(target.join("a.txt"), "a").map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("write a: {e}"),
        })?;
        fs::write(target.join("b.txt"), "b").map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("write b: {e}"),
        })?;
        Ok(())
    });
    result.expect("safe_overwrite_dir should materialize the folder");

    let dest_meta = fs::symlink_metadata(&dest).unwrap();
    assert!(dest_meta.is_dir(), "dest should be a directory after overwrite");
    assert_eq!(fs::read_to_string(dest.join("a.txt")).unwrap(), "a");
    assert_eq!(fs::read_to_string(dest.join("b.txt")).unwrap(), "b");

    // No cmdr-temp artifacts remain in parent
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            !name.contains(".cmdr-temp-") && !name.contains(".cmdr-tmp-"),
            "no aside artifacts should remain: {name}"
        );
    }
}

#[test]
fn test_safe_overwrite_dir_restores_original_on_materialize_failure() {
    // Cancellation / materialize failure must restore the original dest. No
    // data loss when the closure returns an error after the rename-aside step.
    let temp_dir = create_temp_dir("safe_overwrite_dir_restore");
    let dest = temp_dir.join("dest_folder");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("keep-me.txt"), "do not lose this").unwrap();
    fs::write(dest.join("also-keep.txt"), "also important").unwrap();

    let result: Result<(), WriteOperationError> = safe_overwrite_dir(&dest, |target| {
        // Pretend the caller got partway through and then was cancelled.
        fs::create_dir_all(target).ok();
        fs::write(target.join("partial.txt"), "half-written").ok();
        Err(WriteOperationError::Cancelled {
            message: "user cancelled".to_string(),
        })
    });

    assert!(result.is_err(), "should propagate the materialize failure");
    assert!(
        matches!(result.unwrap_err(), WriteOperationError::Cancelled { .. }),
        "should preserve the Cancelled variant"
    );

    // Original dest survives untouched
    let dest_meta = fs::symlink_metadata(&dest).unwrap();
    assert!(dest_meta.is_dir(), "dest should still be the original folder");
    assert_eq!(
        fs::read_to_string(dest.join("keep-me.txt")).unwrap(),
        "do not lose this"
    );
    assert_eq!(
        fs::read_to_string(dest.join("also-keep.txt")).unwrap(),
        "also important"
    );
    assert!(
        !dest.join("partial.txt").exists(),
        "partial materialize artifact should be gone after restore"
    );

    // No cmdr-temp aside remains
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            !name.contains(".cmdr-temp-"),
            "aside should be rolled back, not left on disk: {name}"
        );
    }
}

#[test]
fn test_safe_overwrite_dir_over_folder_dest_replaces_contents() {
    // Source intent = folder. Dest = existing folder with different contents.
    // After successful materialization, the dest path holds the newly
    // materialized contents (no merge), and the original folder is gone with
    // no cmdr-temp artifacts left.
    let temp_dir = create_temp_dir("safe_overwrite_dir_over_folder");
    let dest = temp_dir.join("dest");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("old.txt"), "old").unwrap();

    let result = safe_overwrite_dir(&dest, |target| {
        fs::create_dir_all(target).map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("create_dir_all: {e}"),
        })?;
        fs::write(target.join("new.txt"), "new").map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("write new: {e}"),
        })?;
        Ok(())
    });
    result.expect("safe_overwrite_dir should succeed over existing folder");

    assert!(dest.is_dir(), "dest should be a directory");
    assert!(dest.join("new.txt").exists(), "new file should be present");
    assert!(!dest.join("old.txt").exists(), "old file should be gone (no merge)");

    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            !name.contains(".cmdr-temp-"),
            "no aside artifacts should remain: {name}"
        );
    }
}
