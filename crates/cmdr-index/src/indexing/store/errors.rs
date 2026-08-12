//! What can go wrong around an index DB, typed: the store's own error, the
//! fatal-storage classification the lifecycle fails a volume on, and the cause
//! recorded on a directory nothing is going to read into.
//!
//! Classify on the typed values here, never on a `Display` string.

/// Why nothing is going to read into a directory: the domain of
/// `entries.unreadable_cause`.
///
/// The absence of a cause is `None`, so the column's `0` means "something may yet
/// read this" — which is what every ordinary row carries and what a successful
/// listing restores (`mark_dirs_listed` clears the column).
///
/// It's a CAUSE rather than a flag because the two reach the user as different
/// sentences: one is a permission they can grant, the other is a decision Cmdr
/// made for them. Telling them apart from the paths alone would mean matching
/// folder names, which would break the moment a NAS vendor renamed a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnreadableCause {
    /// A walk tried to read it and the OS refused (permission denied). The
    /// durable, user-fixable case: on macOS, granting Full Disk Access and
    /// searching again heals it, because the successful listing clears the mark.
    Denied,
    /// No walk will read it at all, by Cmdr's own choice: a NAS snapshot
    /// directory, whose per-snapshot hardlinked copies both whole-volume scanners
    /// refuse on purpose (44 TB reported on a 10 TB volume). Nothing for the user
    /// to fix; it's recorded so a short answer can say why it's short.
    Declined,
}

impl UnreadableCause {
    /// The stored integer, with `0` for "no cause".
    pub(super) fn to_stored(cause: Option<Self>) -> i64 {
        match cause {
            None => 0,
            Some(Self::Denied) => 1,
            Some(Self::Declined) => 2,
        }
    }

    /// The cause a stored integer names. An unknown value reads as
    /// [`Denied`](Self::Denied): it can only come from a future schema this build
    /// doesn't know, and "a folder Cmdr can't read" is the truthful half of every
    /// cause there could be.
    pub fn from_stored(stored: i64) -> Option<Self> {
        match stored {
            0 => None,
            2 => Some(Self::Declined),
            _ => Some(Self::Denied),
        }
    }
}

/// Why an index database operation didn't work.
#[derive(Debug)]
pub enum IndexStoreError {
    /// SQLite refused the statement or the file.
    Sqlite(rusqlite::Error),
    /// The database file itself couldn't be read, written, or removed.
    Io(std::io::Error),
    /// The on-disk DB carries a different `schema_version` than this build. The
    /// cache is disposable, so `IndexStore::open` recreates the file fresh
    /// (delete + recreate, reclaiming disk) rather than migrating. Carries the
    /// found vs expected versions for a clean upgrade log (not a corruption
    /// warning). Raised by `try_open` before the store is constructed, so its
    /// connection drops before `delete_and_recreate` opens a new one.
    SchemaMismatch {
        /// The `schema_version` the file on disk carries.
        found: String,
        /// The one this build writes.
        expected: &'static str,
    },
}

impl From<rusqlite::Error> for IndexStoreError {
    fn from(err: rusqlite::Error) -> Self {
        IndexStoreError::Sqlite(err)
    }
}

impl From<std::io::Error> for IndexStoreError {
    fn from(err: std::io::Error) -> Self {
        IndexStoreError::Io(err)
    }
}

impl std::fmt::Display for IndexStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexStoreError::Sqlite(e) => write!(f, "SQLite error: {e}"),
            IndexStoreError::Io(e) => write!(f, "I/O error: {e}"),
            IndexStoreError::SchemaMismatch { found, expected } => {
                write!(f, "schema version mismatch (found {found}, expected {expected})")
            }
        }
    }
}

impl std::error::Error for IndexStoreError {}

/// A fatal storage failure that stopped a volume's index: the SQLite result codes
/// that classified the DB as unusable (a dead disk, a corrupt file, a full or
/// read-only volume). Carried on the `IndexPhase::Failed` phase (see `lifecycle/state.rs`)
/// and surfaced to the UI and logs so the failure is specific.
///
/// `code` is the primary SQLite result code (for example `SQLITE_IOERR` = 10);
/// `extended_code` is the extended code (for example `SQLITE_IOERR_WRITE`),
/// preserved because [`IndexStoreError`]'s `Display` flattens it away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexFailure {
    /// The primary SQLite result code, for example `SQLITE_IOERR` = 10.
    pub code: i32,
    /// The extended result code, for example `SQLITE_IOERR_WRITE`.
    pub extended_code: i32,
}

impl IndexStoreError {
    /// The SQLite `(primary ErrorCode, extended result code)` when this wraps a
    /// `SqliteFailure`. `None` for non-SQLite errors and for `rusqlite` errors that
    /// carry no ffi code (for example `QueryReturnedNoRows`).
    ///
    /// Classify on THIS, never on the `Display` string:
    /// `Display` writes only `SQLite error: {e}` and drops the numeric extended
    /// code that distinguishes a transient lock from a dead disk.
    pub fn sqlite_code(&self) -> Option<(rusqlite::ErrorCode, i32)> {
        match self {
            IndexStoreError::Sqlite(rusqlite::Error::SqliteFailure(e, _)) => Some((e.code, e.extended_code)),
            _ => None,
        }
    }

    /// Whether this is a FATAL storage-class error: the DB is unusable and every
    /// subsequent read and write will fail the same way, so the index must stop and
    /// fail rather than retry forever (the 12,700-warning livelock this guards
    /// against). Transient contention (`SQLITE_BUSY` / `SQLITE_LOCKED`) is
    /// deliberately NOT fatal: the busy handler already backs those off. Classified
    /// on the typed primary `ErrorCode`.
    pub fn is_fatal_storage_error(&self) -> bool {
        use rusqlite::ErrorCode::{CannotOpen, DatabaseCorrupt, DiskFull, NotADatabase, ReadOnly, SystemIoFailure};
        matches!(
            self.sqlite_code(),
            Some((
                SystemIoFailure     // SQLITE_IOERR*
                    | DatabaseCorrupt   // SQLITE_CORRUPT
                    | CannotOpen        // SQLITE_CANTOPEN
                    | DiskFull          // SQLITE_FULL
                    | ReadOnly          // SQLITE_READONLY
                    | NotADatabase, // SQLITE_NOTADB
                _
            ))
        )
    }

    /// Whether this is TRANSIENT lock contention: another connection (or another
    /// process) held the lock, or the WAL locking protocol needs another attempt.
    /// The DB itself is fine, so the only correct response is to back off and try
    /// again, never to throw the index away. `IndexStore::open` retries on this.
    pub fn is_transient_lock_error(&self) -> bool {
        use rusqlite::ErrorCode::{DatabaseBusy, DatabaseLocked, FileLockingProtocolFailed};
        matches!(
            self.sqlite_code(),
            Some((
                DatabaseBusy        // SQLITE_BUSY
                    | DatabaseLocked    // SQLITE_LOCKED
                    | FileLockingProtocolFailed, // SQLITE_PROTOCOL
                _
            ))
        )
    }

    /// Whether the file is positively PROVEN unusable: the bytes aren't a SQLite
    /// database (`SQLITE_NOTADB`) or its B-tree is broken (`SQLITE_CORRUPT*`).
    ///
    /// This is the ONLY class that justifies `IndexStore::open` deleting the DB.
    /// It's deliberately narrower than [`is_fatal_storage_error`]: a full disk, a
    /// read-only volume, or a momentary `SQLITE_IOERR` also stop the index, but
    /// they leave a perfectly good index on disk that a later launch can reuse, so
    /// deleting on those would destroy a 6.9M-entry index (tens of minutes to
    /// rebuild) over an environment problem. Anything unrecognized fails loudly
    /// rather than deleting.
    ///
    /// [`is_fatal_storage_error`]: Self::is_fatal_storage_error
    pub fn indicates_corruption(&self) -> bool {
        use rusqlite::ErrorCode::{DatabaseCorrupt, NotADatabase};
        matches!(self.sqlite_code(), Some((DatabaseCorrupt | NotADatabase, _)))
    }

    /// Whether this is a PRIMARY KEY conflict on `entries.id`
    /// (`SQLITE_CONSTRAINT_PRIMARYKEY`, extended code 1555): the writer's shared
    /// ID counter fell behind the table's real `MAX(id)`, so the id it handed out
    /// is already taken. That's self-healing (resync the counter from the DB and
    /// retry with a fresh id), which is exactly why it's classified apart from
    /// `SQLITE_CONSTRAINT_UNIQUE` (2067), the `(parent_id, name_folded)` conflict.
    /// A UNIQUE conflict means the NAME is already in the table (a real duplicate,
    /// a case-folding collision, a racing writer); retrying that one under a fresh
    /// id would insert a duplicate row, so it must never heal. Both share the
    /// primary `ErrorCode::ConstraintViolation`, so only the extended code tells
    /// them apart.
    pub fn is_primary_key_conflict(&self) -> bool {
        matches!(
            self.sqlite_code(),
            Some((rusqlite::ErrorCode::ConstraintViolation, extended))
                if extended == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
        )
    }

    /// The typed [`IndexFailure`] for the `Failed` phase, if this is a fatal
    /// storage error (else `None`). The primary code is the low byte of the
    /// extended code, matching SQLite's `SQLITE_IOERR == extended & 0xFF`.
    pub fn as_index_failure(&self) -> Option<IndexFailure> {
        if !self.is_fatal_storage_error() {
            return None;
        }
        self.sqlite_code().map(|(_, extended_code)| IndexFailure {
            code: extended_code & 0xFF,
            extended_code,
        })
    }
}
