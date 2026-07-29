//! Classifying a SQLite failure: what stops the index, what's worth retrying,
//! and the narrow corruption class that justifies deleting it.

use super::*;

// ── Fatal storage-error classification ───────────────────────────────

/// Build an `IndexStoreError` wrapping a SQLite failure with the given (extended)
/// result code, so the classifier can be exercised without a real dead disk.
fn sqlite_err(result_code: std::os::raw::c_int) -> IndexStoreError {
    IndexStoreError::Sqlite(rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(result_code),
        None,
    ))
}

#[test]
fn fatal_storage_errors_are_classified_as_fatal() {
    // The storage-death classes: a dead disk, a corrupt file, an unopenable or
    // full or read-only volume, and non-DB bytes. Each means every later
    // read/write fails the same way, so the index must stop, not retry forever.
    for code in [
        rusqlite::ffi::SQLITE_IOERR,
        rusqlite::ffi::SQLITE_CORRUPT,
        rusqlite::ffi::SQLITE_CANTOPEN,
        rusqlite::ffi::SQLITE_FULL,
        rusqlite::ffi::SQLITE_READONLY,
        rusqlite::ffi::SQLITE_NOTADB,
    ] {
        assert!(
            sqlite_err(code).is_fatal_storage_error(),
            "result code {code} must be fatal"
        );
    }
}

#[test]
fn extended_ioerr_codes_are_still_fatal_and_preserved() {
    // The incident's error was an EXTENDED `SQLITE_IOERR_*` code. It must classify
    // fatal on its primary `SQLITE_IOERR` low byte, and `as_index_failure` must
    // preserve the full extended code (which `Display` would have dropped).
    let err = sqlite_err(rusqlite::ffi::SQLITE_IOERR_WRITE);
    assert!(err.is_fatal_storage_error());
    let failure = err.as_index_failure().expect("a fatal error yields a typed failure");
    assert_eq!(
        failure.code,
        rusqlite::ffi::SQLITE_IOERR,
        "primary code is the low byte"
    );
    assert_eq!(
        failure.extended_code,
        rusqlite::ffi::SQLITE_IOERR_WRITE,
        "the extended code is preserved, not flattened"
    );
}

#[test]
fn transient_contention_is_not_fatal() {
    // BUSY/LOCKED are routine contention the busy handler already retries; failing
    // the whole index on them would be a regression, so they must NOT be fatal.
    assert!(!sqlite_err(rusqlite::ffi::SQLITE_BUSY).is_fatal_storage_error());
    assert!(!sqlite_err(rusqlite::ffi::SQLITE_LOCKED).is_fatal_storage_error());
    assert!(sqlite_err(rusqlite::ffi::SQLITE_BUSY).as_index_failure().is_none());
}

#[test]
fn primary_key_conflicts_are_told_apart_from_name_conflicts() {
    // 1555 is the writer's ID counter drifting behind `MAX(id)`: heal it by
    // resyncing and retrying under a fresh id. 2067 is a `(parent_id,
    // name_folded)` conflict: the name is already there, so a retry under a
    // fresh id would insert a duplicate row. Only the extended code separates
    // them; the primary code is `ConstraintViolation` for both.
    assert!(sqlite_err(rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY).is_primary_key_conflict());
    assert!(!sqlite_err(rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE).is_primary_key_conflict());
    assert!(!sqlite_err(rusqlite::ffi::SQLITE_BUSY).is_primary_key_conflict());
    // A constraint conflict is never a storage-death class either.
    assert!(!sqlite_err(rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY).is_fatal_storage_error());
}

#[test]
fn transient_lock_errors_are_retryable_and_never_delete() {
    // The contention family: another connection (or process) holds the lock, or
    // the WAL locking protocol needs another go. Retrying fixes all three, and
    // deleting a good index over one is the data loss we refuse.
    for code in [
        rusqlite::ffi::SQLITE_BUSY,
        rusqlite::ffi::SQLITE_LOCKED,
        rusqlite::ffi::SQLITE_PROTOCOL,
        rusqlite::ffi::SQLITE_BUSY_SNAPSHOT,
    ] {
        let err = sqlite_err(code);
        assert!(err.is_transient_lock_error(), "result code {code} must be transient");
        assert!(!err.indicates_corruption(), "result code {code} must not delete the DB");
    }
}

#[test]
fn only_corruption_codes_justify_deleting_the_index() {
    // Deleting is the destructive branch, so it needs positive proof the file is
    // unusable: the bytes aren't a DB, or the B-tree is broken.
    assert!(sqlite_err(rusqlite::ffi::SQLITE_CORRUPT).indicates_corruption());
    assert!(sqlite_err(rusqlite::ffi::SQLITE_NOTADB).indicates_corruption());
    assert!(sqlite_err(rusqlite::ffi::SQLITE_CORRUPT_VTAB).indicates_corruption());

    // The storage-death classes stop the index (`is_fatal_storage_error`) but are
    // NOT proof of corruption: a full disk, a read-only volume, a dead mount, or a
    // momentary I/O error all leave a perfectly good index on disk.
    for code in [
        rusqlite::ffi::SQLITE_IOERR,
        rusqlite::ffi::SQLITE_FULL,
        rusqlite::ffi::SQLITE_READONLY,
        rusqlite::ffi::SQLITE_CANTOPEN,
    ] {
        let err = sqlite_err(code);
        assert!(!err.indicates_corruption(), "result code {code} must not delete the DB");
        assert!(!err.is_transient_lock_error(), "result code {code} isn't contention");
    }
}

#[test]
fn non_sqlite_errors_have_no_code_and_are_not_fatal() {
    let io = IndexStoreError::Io(std::io::Error::other("broken pipe"));
    assert!(io.sqlite_code().is_none());
    assert!(!io.is_fatal_storage_error());
    assert!(!io.is_primary_key_conflict());
    assert!(!io.is_transient_lock_error());
    assert!(!io.indicates_corruption());
    assert!(io.as_index_failure().is_none());
}
