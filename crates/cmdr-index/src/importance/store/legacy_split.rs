//! Splitting an older build's combined importance store, once per volume, in
//! the background right after the index is built.
//!
//! An older build kept weights AND visits in one `importance-{id}.db` in the data
//! dir. This build keeps the visits in `visits-{id}.db` there and the weights in
//! the drive-index dir, out of backups. [`split_legacy_stores`] moves each old
//! file's visits into the new file and deletes the old file. The weights aren't
//! moved: the next full pass rebuilds them (David's call, 2026-11-03: a one-time
//! background pass per volume costs less than machinery to carry a cache across).
//!
//! **Crash-safe at every point, and never counts a visit twice.** Per volume:
//!
//! 1. Stamp a random token into the OLD file's `meta`, unless it has one.
//! 2. In ONE transaction on the NEW file: merge the old rows in, record the token
//!    in `split_from`, and record in `split_rows` how much of each row this token
//!    has contributed. Both files sync fully (`F_FULLFSYNC`), the new file's WAL is
//!    folded in, and its folder entry is synced, all BEFORE step 3.
//! 3. Fold the old file's WAL in, close it, and delete it.
//!
//! A crash before step 2 commits redoes the copy; after it, the re-run finds the
//! ledger says everything is accounted for, adds nothing, and deletes.
//!
//! **How a row merges**, which is what keeps the round trips honest:
//!
//! - **A token already in the ledger** (a re-run, an older build that kept writing
//!   into a file whose delete failed, a restored copy taken after the stamp): adds
//!   only what the row has beyond what that token contributed.
//! - **A new token, a row last visited at or before the latest split AND no more
//!   visited than an earlier file's ledger says it was**: history the visits file
//!   already holds (Time Machine restored a copy of the old store from before its
//!   split). Merged by `max`, so it adds nothing it already has.
//! - **Any other row of a new token**: visits recorded since, by an older build
//!   that started a fresh combined file. Added. The ledger test is what keeps a
//!   clock set back after a split from dropping those: wall clock alone isn't
//!   trusted.
//!
//! The other cases, per volume:
//!
//! - **The old file isn't a sound database**: deleted. Nothing in it can be read,
//!   which is what every earlier build did to such a file on open. Only damage
//!   found in the OLD file counts: damage in the new one leaves the old one whole.
//! - **It has no `visits` table**: in a separate drive-index dir it's a stray and
//!   is deleted; when both dirs are one folder it's this build's own weights store
//!   and is left alone.
//! - **Anything else goes wrong** (a full disk, a permission): left where it is
//!   with the reason, for the next launch. Until then a same-folder host can't
//!   recreate it ([`super::ImportanceStoreError::HoldsVisits`]) nor a rebuild delete
//!   it (`volume_files::remove`).

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use super::ImportanceStoreError;
use crate::indexing::store::normalize_for_comparison;
use crate::volume_files::{StoreDirs, VolumeStore};
use cmdr_fs::sqlite_util;

/// The old file's `meta` key for the token step 1 stamps.
const SPLIT_TOKEN_KEY: &str = "visits_split_token";

/// What [`split_legacy_stores`] did, volume by volume, for the host's log line.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct LegacySplit {
    /// Volumes whose visits moved to their own file, with how many rows changed
    /// the visits file (0 when an earlier, interrupted run had already copied them).
    pub(crate) split: Vec<(String, u64)>,
    /// Volumes whose old file was deleted with nothing to move: unsound, a stray,
    /// or only sidecars left by an interrupted delete.
    pub(crate) discarded: Vec<String>,
    /// Volumes whose old file stays until the next launch, with why.
    pub(crate) left: Vec<(String, String)>,
}

// ── The background run ───────────────────────────────────────────────────────

/// Run [`split_legacy_stores`] on a blocking thread, handing the report to
/// `report`. Marked as store work before it returns, so a caller that comes after
/// can't slip in ahead of it; the mark is a drop guard, so a panic in the split or
/// in `report` still clears it.
///
/// ⚠️ Safe only because the host holds the data dir's instance lock: no other
/// process has these files. In this one, everything that opens the visits or the
/// weights, and every removal, waits for the split first.
pub(crate) fn split_in_background(dirs: StoreDirs, report: impl FnOnce(LegacySplit) + Send + 'static) {
    let work = crate::volume_files::begin_store_work();
    crate::indexing::host::runtime::spawn_blocking(move || {
        let _work = work;
        report(split_legacy_stores(&dirs));
    });
}

/// Block until a background split has finished (`volume_files::wait_for_store_work`).
/// Called from blocking threads only: the writer registry, the launch probe, and
/// the pass's visit read.
pub(crate) fn wait_for_background_split() {
    crate::volume_files::wait_for_store_work();
}

// ── The split ────────────────────────────────────────────────────────────────

/// Split every older build's combined importance store in `dirs.data`.
///
/// Runs whether or not the two dirs are one folder: the visits have to come out
/// of the combined file either way.
pub(crate) fn split_legacy_stores(dirs: &StoreDirs) -> LegacySplit {
    let mut report = LegacySplit::default();
    for volume_id in VolumeStore::Importance.volume_ids_in(&dirs.data) {
        match split_one(dirs, &volume_id) {
            Outcome::Split(rows) => report.split.push((volume_id, rows)),
            Outcome::Current => {}
            Outcome::Discarded => report.discarded.push(volume_id),
            Outcome::Left(why) => report.left.push((volume_id, why)),
        }
    }
    report
}

enum Outcome {
    /// The visits moved over; this many rows changed the visits file this run.
    Split(u64),
    /// This build's own weights store, in a host that keeps one folder.
    Current,
    Discarded,
    Left(String),
}

/// Which file a copy failed on, which decides whether the OLD file is damaged.
#[derive(Debug)]
enum CopyError {
    Old(ImportanceStoreError),
    New(ImportanceStoreError),
}

fn split_one(dirs: &StoreDirs, volume_id: &str) -> Outcome {
    let old_db = VolumeStore::Importance.db_path(&dirs.data, volume_id);
    if !old_db.exists() {
        // Sidecars an interrupted delete left. They belong to no database, and in a
        // one-folder host SQLite would replay a stray `-wal` onto the weights store
        // created at this path next.
        return match sqlite_util::delete_sidecars(&old_db) {
            Ok(()) => Outcome::Discarded,
            Err(e) => Outcome::Left(format!("can't delete the old sidecars: {e}")),
        };
    }

    let old = match open_old(&old_db) {
        Ok(Some(conn)) => conn,
        Ok(None) if dirs.is_single() => return Outcome::Current,
        Ok(None) => return discard(&old_db),
        Err(e) if e.is_corruption() => {
            log::warn!(
                target: "importance",
                "the old importance store of '{volume_id}' isn't a sound database ({e}); its visits can't be read"
            );
            return discard(&old_db);
        }
        Err(e) => return Outcome::Left(format!("can't read the old store: {e}")),
    };

    let new_db = VolumeStore::Visits.db_path(&dirs.data, volume_id);
    let copied = match copy_visits(&old, &new_db, now_secs()) {
        Ok(copied) => copied,
        Err(CopyError::Old(e)) if e.is_corruption() => {
            log::warn!(
                target: "importance",
                "the old importance store of '{volume_id}' is damaged ({e}); its visits can't be read"
            );
            drop(old);
            return discard(&old_db);
        }
        Err(CopyError::Old(e)) => return Outcome::Left(format!("can't read the old visits: {e}")),
        Err(CopyError::New(e)) => return Outcome::Left(format!("can't write the new visits file: {e}")),
    };

    if let Err(why) = fold_the_wal_in_and_close(old) {
        return Outcome::Left(why);
    }
    match sqlite_util::delete_database(&old_db) {
        Ok(()) => Outcome::Split(copied),
        Err(e) => Outcome::Left(format!("can't delete the old store: {e}")),
    }
}

/// Every write here must be on disk before the old file goes: a full sync per
/// commit, and `F_FULLFSYNC` on macOS, where a plain `fsync` stops at the drive's
/// cache.
fn sync_fully(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA synchronous = FULL; PRAGMA fullfsync = ON; PRAGMA checkpoint_fullfsync = ON;")
}

/// Open the old file, or `None` when it has no `visits` table (nothing to move).
fn open_old(old_db: &Path) -> Result<Option<Connection>, ImportanceStoreError> {
    let conn = sqlite_util::open(old_db)?;
    // An older schema may still name the collation in a column definition.
    super::connection::register_collation(&conn)?;
    conn.execute_batch("PRAGMA busy_timeout = 5000;")?;
    sync_fully(&conn)?;
    let has_visits: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'visits')",
        [],
        |row| row.get(0),
    )?;
    Ok(has_visits.then_some(conn))
}

/// Open the new visits file for the copy, syncing fully.
fn open_new(new_db: &Path) -> Result<Connection, ImportanceStoreError> {
    let conn = super::visits::open_write_connection(new_db)?;
    sync_fully(&conn)?;
    Ok(conn)
}

/// Steps 1 and 2: stamp the old file's token, then merge its rows into the new
/// file under the ledger (module docs § How a row merges), durably. Returns how
/// many rows changed the visits file.
fn copy_visits(old: &Connection, new_db: &Path, now_secs: i64) -> Result<u64, CopyError> {
    let token = stamp_token(old).map_err(CopyError::Old)?;
    let rows = read_old_visits(old).map_err(CopyError::Old)?;

    let mut new = open_new(new_db).map_err(CopyError::New)?;
    let changed = merge_rows(&mut new, &token, &rows, now_secs).map_err(CopyError::New)?;
    // The commit is in the new WAL; fold it into the database file, then sync the
    // folder so the files' own entries survive a power loss too.
    let _ = new.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    drop(new);
    sync_folder_of(new_db).map_err(|e| CopyError::New(e.into()))?;
    Ok(changed)
}

fn merge_rows(
    new: &mut Connection,
    token: &str,
    rows: &[(String, i64, i64)],
    now_secs: i64,
) -> Result<u64, ImportanceStoreError> {
    let tx = new.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let known = tx
        .query_row("SELECT 1 FROM split_from WHERE token = ?1", [token], |_| Ok(()))
        .optional()?
        .is_some();
    let latest_split: Option<i64> = tx.query_row("SELECT max(at_secs) FROM split_from", [], |row| row.get(0))?;
    let mut changed = 0;
    {
        let mut contributed =
            tx.prepare_cached("SELECT visit_count FROM split_rows WHERE token = ?1 AND path_folded = ?2")?;
        // The most any EARLIER file contributed for a path: what a restored copy of
        // one could at most carry again.
        let mut covered =
            tx.prepare_cached("SELECT max(visit_count) FROM split_rows WHERE path_folded = ?1 AND token != ?2")?;
        let mut add = tx.prepare_cached(
            "INSERT INTO visits (path_folded, path, visit_count, last_visit_secs) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path_folded) DO UPDATE SET
                 visit_count = visit_count + excluded.visit_count,
                 last_visit_secs = max(last_visit_secs, excluded.last_visit_secs)",
        )?;
        let mut keep_the_larger = tx.prepare_cached(
            "INSERT INTO visits (path_folded, path, visit_count, last_visit_secs) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path_folded) DO UPDATE SET
                 visit_count = max(visit_count, excluded.visit_count),
                 last_visit_secs = max(last_visit_secs, excluded.last_visit_secs)
             WHERE excluded.visit_count > visit_count OR excluded.last_visit_secs > last_visit_secs",
        )?;
        let mut account = tx.prepare_cached(
            "INSERT INTO split_rows (token, path_folded, visit_count) VALUES (?1, ?2, ?3)
             ON CONFLICT(token, path_folded) DO UPDATE SET visit_count = max(visit_count, excluded.visit_count)",
        )?;
        for (path, count, last) in rows {
            let folded = normalize_for_comparison(path);
            let written = if known {
                let already: i64 = contributed
                    .query_row(rusqlite::params![token, folded], |row| row.get(0))
                    .optional()?
                    .unwrap_or(0);
                let extra = count - already;
                if extra > 0 {
                    add.execute(rusqlite::params![folded, path, extra, last])?
                } else {
                    0
                }
            } else if latest_split.is_some_and(|at| *last <= at)
                && covered
                    .query_row(rusqlite::params![folded, token], |row| row.get::<_, Option<i64>>(0))?
                    .is_some_and(|seen| seen >= *count)
            {
                keep_the_larger.execute(rusqlite::params![folded, path, count, last])?
            } else {
                add.execute(rusqlite::params![folded, path, count, last])?
            };
            changed += written as u64;
            account.execute(rusqlite::params![token, folded, count])?;
        }
    }
    tx.execute(
        "INSERT OR IGNORE INTO split_from (token, rows, at_secs) VALUES (?1, ?2, ?3)",
        rusqlite::params![token, rows.len() as i64, now_secs],
    )?;
    tx.commit()?;
    Ok(changed)
}

/// Sync `file`'s folder, so a file created in it survives a power loss.
fn sync_folder_of(file: &Path) -> std::io::Result<()> {
    match file.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => std::fs::File::open(dir)?.sync_all(),
        _ => Ok(()),
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The old file's split token, stamping a fresh one if it has none.
fn stamp_token(old: &Connection) -> Result<String, ImportanceStoreError> {
    old.execute_batch("CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
    old.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES (?1, lower(hex(randomblob(16))))",
        [SPLIT_TOKEN_KEY],
    )?;
    Ok(
        old.query_row("SELECT value FROM meta WHERE key = ?1", [SPLIT_TOKEN_KEY], |row| {
            row.get(0)
        })?,
    )
}

/// Every row of the old `visits` table, as `(path, count, last_visit_secs)`. Read
/// by the verbatim `path`, which every schema had, and folded again on the way
/// in, so an older file's key shape doesn't matter.
fn read_old_visits(old: &Connection) -> Result<Vec<(String, i64, i64)>, ImportanceStoreError> {
    let mut stmt = old.prepare("SELECT path, visit_count, last_visit_secs FROM visits")?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Checkpoint every frame of the old WAL into its database, then close, which
/// deletes the emptied sidecars. After this the delete can't strand a log with
/// commits in it, whichever of the three unlinks a crash interrupts.
fn fold_the_wal_in_and_close(old: Connection) -> Result<(), String> {
    let (busy, _, _): (i64, i64, i64) = old
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| format!("can't checkpoint the old store: {e}"))?;
    drop(old);
    if busy != 0 {
        return Err("the old store is in use by another process".to_string());
    }
    Ok(())
}

fn discard(old_db: &Path) -> Outcome {
    match sqlite_util::delete_database(old_db) {
        Ok(()) => Outcome::Discarded,
        Err(e) => Outcome::Left(format!("can't delete the old store: {e}")),
    }
}

#[cfg(test)]
#[path = "legacy_split_tests.rs"]
mod tests;
