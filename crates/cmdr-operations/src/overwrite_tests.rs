//! Tests for `overwrite.rs`: the staged file landing, the folder landing, and
//! what `displace_with_directory` does with the entry it moved aside.
//!
//! A `#[path]` child of `overwrite.rs`, so `super::` here is `overwrite` and
//! `super::super::` is `cmdr-operations`.

use super::super::ledger::CopyTransaction;
use super::super::state::WriteOperationState;
use super::super::testing::TestOperations;
use super::super::types::WriteOperationError;
use super::*;
use cmdr_fs::testing::TestDir;
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

/// A running operation to land a write under, on an engine of its own (the
/// state's scope holds everything it reaches).
fn overwrite_state() -> Arc<WriteOperationState> {
    Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(50)))
}

/// `stage_and_land_file` with the platform's default byte-copier, which is what
/// the copy strategies hand it for a same-volume local copy.
fn safe_overwrite_file(source: &Path, dest: &Path) -> Result<u64, WriteOperationError> {
    copy_through_staging(source, dest, true)
}

fn copy_through_staging(source: &Path, dest: &Path, replacing: bool) -> Result<u64, WriteOperationError> {
    stage_and_land_file(
        &overwrite_state(),
        Announced::assumed(dest),
        replacing,
        PostLanding::Apply,
        |target| {
            #[cfg(target_os = "macos")]
            let bytes = super::super::transfer::macos_copy::copy_single_file_native(source, target, false, None);
            #[cfg(not(target_os = "macos"))]
            let bytes = {
                use super::super::error_classification::IoResultExt;
                fs::copy(source, target).with_path(source)
            };
            bytes.map(Staged::bytes)
        },
    )
    .map(|landed| landed.bytes)
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

/// Every name in `dir` that's one of Cmdr's scratch files.
fn scratch_left_in(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| cmdr_fs::staging::is_staging_temp_name(name))
        .collect()
}

/// ERR-9JY7Q's shape on the local path: a name already at the 255-byte limit
/// APFS and ext4 share. The temp's suffix used to push it over, so the copy
/// failed with "name too long" on a name the destination holds fine. Both
/// landings, fresh and over an existing file, and a two-byte script too.
#[test]
fn a_name_at_the_filesystem_limit_lands() {
    let temp_dir = create_temp_dir("name_at_limit");
    let source = temp_dir.join("source.epub");
    fs::write(&source, "new-data").unwrap();
    let dest_dir = temp_dir.join("dest");
    fs::create_dir(&dest_dir).unwrap();

    for name in [
        format!("{}.epub", "a".repeat(250)),
        format!("{}.epub", "αβγδε".repeat(25)),
    ] {
        assert_eq!(name.len(), 255);
        let dest = dest_dir.join(&name);

        copy_through_staging(&source, &dest, false).unwrap_or_else(|e| panic!("fresh landing: {e:?}"));
        assert_eq!(fs::read_to_string(&dest).unwrap(), "new-data");

        fs::write(&dest, "old-data").unwrap();
        safe_overwrite_file(&source, &dest).unwrap_or_else(|e| panic!("overwrite: {e:?}"));
        assert_eq!(fs::read_to_string(&dest).unwrap(), "new-data");
    }
    assert_eq!(scratch_left_in(&dest_dir), Vec::<String>::new());
}

/// A name the destination really can't hold still says so, about the user's
/// name: the shortened temp takes the bytes, and the landing is what refuses.
/// `NameTooLong` is what tells the user to pick a shorter name; a generic IO
/// failure would offer a Retry that meets the same refusal.
#[test]
fn a_name_past_the_filesystem_limit_reports_it_as_too_long() {
    let temp_dir = create_temp_dir("name_past_limit");
    let source = temp_dir.join("source.txt");
    fs::write(&source, "new-data").unwrap();
    let dest = temp_dir.join(format!("{}.txt", "a".repeat(296)));

    let error = copy_through_staging(&source, &dest, false).expect_err("a 300-byte name can't land");

    assert!(
        matches!(&error, WriteOperationError::NameTooLong { path } if *path == dest.display().to_string()),
        "got {error:?}"
    );
    assert_eq!(scratch_left_in(&temp_dir), Vec::<String>::new());
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
    let state = Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(0)));

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
    let state = Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(0)));

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

    let result = safe_overwrite_dir(&overwrite_state(), &dest, |target| {
        fs::create_dir_all(target).map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("create_dir_all: {e}").into(),
        })?;
        fs::write(target.join("a.txt"), "a").map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("write a: {e}").into(),
        })?;
        fs::write(target.join("b.txt"), "b").map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("write b: {e}").into(),
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

    let result: Result<(), WriteOperationError> = safe_overwrite_dir(&overwrite_state(), &dest, |target| {
        // Pretend the caller got partway through and then was cancelled.
        fs::create_dir_all(target).ok();
        fs::write(target.join("partial.txt"), "half-written").ok();
        Err(WriteOperationError::Cancelled {
            message: "user cancelled".into(),
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

/// A transiently unreadable destination has not proved that the name is free.
/// The original must stay aside rather than replacing an entry the failed stat
/// could not see.
#[test]
fn failed_destination_stat_stops_the_restore_without_replacing_anything() {
    let temp_dir = create_temp_dir("restore_stat_failure");
    let aside = temp_dir.join("dest.cmdr-temp-original");
    let dest = temp_dir.join("dest");
    fs::write(&aside, "the original").unwrap();
    fs::write(&dest, "a new arrival").unwrap();

    let err = restore_after_materialize_failure(&aside, &dest, |_| Err(std::io::Error::from_raw_os_error(libc::EIO)))
        .expect_err("a stat that could not answer must stop the restore");

    assert_eq!(err.raw_os_error(), Some(libc::EIO), "the stat refusal must surface");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "a new arrival");
    assert_eq!(fs::read_to_string(&aside).unwrap(), "the original");
}

/// A name can become occupied after the stat says `NotFound`. The restore's
/// rename must still refuse rather than replace the new arrival.
#[test]
fn restore_refuses_a_name_that_became_occupied_after_the_stat() {
    let temp_dir = create_temp_dir("restore_race");
    let aside = temp_dir.join("dest.cmdr-temp-original");
    let dest = temp_dir.join("dest");
    fs::write(&aside, "the original").unwrap();
    fs::write(&dest, "a new arrival").unwrap();

    let err = restore_after_materialize_failure(&aside, &dest, |_| {
        Err(std::io::Error::from(std::io::ErrorKind::NotFound))
    })
    .expect_err("an occupied name must stop the restore despite the stale stat");

    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(&dest).unwrap(), "a new arrival");
    assert_eq!(fs::read_to_string(&aside).unwrap(), "the original");
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

    let result = safe_overwrite_dir(&overwrite_state(), &dest, |target| {
        fs::create_dir_all(target).map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("create_dir_all: {e}").into(),
        })?;
        fs::write(target.join("new.txt"), "new").map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("write new: {e}").into(),
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
