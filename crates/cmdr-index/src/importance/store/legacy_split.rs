//! Splitting an older build's combined importance store, once per volume, at
//! build time, before anything opens it.
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
//! 1. Stamp a random token into the OLD file's `meta`, unless it has one. A crash
//!    after this changed nothing a reader cares about.
//! 2. In ONE transaction on the NEW file: add the old rows (merged into any the
//!    new file already has) AND record the token in `split_from`. Both land or
//!    neither does, so a crash leaves the copy either done and marked, or not
//!    started. A re-run finds the token marked and skips the copy.
//! 3. Fold the old file's WAL in, close it, and delete it. A crash here leaves a
//!    file the next run deletes without copying again (its token is marked), or
//!    sidecars with no database, which the next run deletes.
//!
//! The token, rather than a plain "split done" flag, is what keeps a DOWNGRADE
//! round trip honest: an older build finds no combined file, starts a fresh one
//! and records visits in it, and the next upgrade splits THAT file too (a new
//! token), merging its visits in. A flag would skip it and lose them.
//!
//! The other cases, per volume:
//!
//! - **The old file isn't a sound database**: deleted. Nothing in it can be read,
//!   which is what every earlier build did to such a file on open.
//! - **It has no `visits` table**: in a separate drive-index dir it's a stray and
//!   is deleted; when both dirs are one folder it's this build's own weights store
//!   and is left alone.
//! - **Anything else goes wrong** (a full disk, a permission): left where it is
//!   with the reason, for the next launch. Until then a same-folder host can't
//!   recreate it either ([`super::ImportanceStoreError::HoldsVisits`]).

use std::io::ErrorKind;
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
    /// Volumes whose visits moved to their own file, with how many rows came over
    /// (0 when an earlier, interrupted run had already copied them).
    pub(crate) split: Vec<(String, u64)>,
    /// Volumes whose old file was deleted with nothing to move: unsound, a stray,
    /// or only sidecars left by an interrupted delete.
    pub(crate) discarded: Vec<String>,
    /// Volumes whose old file stays until the next launch, with why.
    pub(crate) left: Vec<(String, String)>,
}

/// Split every older build's combined importance store in `dirs.data`.
///
/// Runs whether or not the two dirs are one folder: the visits have to come out
/// of the combined file either way.
///
/// ⚠️ Nothing may have these files open: the caller holds the data dir's instance
/// lock and runs this before the importance scheduler starts.
pub(crate) fn split_legacy_stores(dirs: &StoreDirs) -> LegacySplit {
    let mut report = LegacySplit::default();
    for volume_id in importance_volume_ids(&dirs.data) {
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
    /// The visits moved over; this many rows were copied this run.
    Split(u64),
    /// This build's own weights store, in a host that keeps one folder.
    Current,
    Discarded,
    Left(String),
}

/// Every volume with an importance file in `dir`: its database, or a sidecar
/// whose database is gone.
fn importance_volume_ids(dir: &Path) -> Vec<String> {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut volume_ids: Vec<String> = Vec::new();
    for entry in read_dir.flatten() {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let db_name = file_name
            .strip_suffix("-wal")
            .or_else(|| file_name.strip_suffix("-shm"))
            .unwrap_or(file_name);
        if let Some(volume_id) = VolumeStore::Importance.volume_id_of(db_name)
            && !volume_ids.iter().any(|seen| seen == volume_id)
        {
            volume_ids.push(volume_id.to_string());
        }
    }
    volume_ids
}

fn split_one(dirs: &StoreDirs, volume_id: &str) -> Outcome {
    let old_db = VolumeStore::Importance.db_path(&dirs.data, volume_id);
    if !old_db.exists() {
        // Sidecars an interrupted delete left. They belong to no database, and in a
        // one-folder host SQLite would replay a stray `-wal` onto the weights store
        // created at this path next.
        return match delete_sidecars(&old_db) {
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

    let copied = match copy_visits(&old, &VolumeStore::Visits.db_path(&dirs.data, volume_id)) {
        Ok(copied) => copied,
        Err(e) if e.is_corruption() => {
            log::warn!(
                target: "importance",
                "the old importance store of '{volume_id}' is damaged ({e}); its visits can't be read"
            );
            drop(old);
            return discard(&old_db);
        }
        Err(e) => return Outcome::Left(format!("can't move the visits out: {e}")),
    };

    if let Err(why) = fold_the_wal_in_and_close(old) {
        return Outcome::Left(why);
    }
    match sqlite_util::delete_database(&old_db) {
        Ok(()) => Outcome::Split(copied),
        Err(e) => Outcome::Left(format!("can't delete the old store: {e}")),
    }
}

/// Open the old file, or `None` when it has no `visits` table (nothing to move).
fn open_old(old_db: &Path) -> Result<Option<Connection>, ImportanceStoreError> {
    let conn = sqlite_util::open(old_db)?;
    // An older schema may still name the collation in a column definition.
    super::connection::register_collation(&conn)?;
    conn.execute_batch("PRAGMA busy_timeout = 5000;")?;
    let has_visits: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'visits')",
        [],
        |row| row.get(0),
    )?;
    Ok(has_visits.then_some(conn))
}

/// Steps 1 and 2: stamp the old file's token, then copy its rows and mark the
/// token in the new file in one transaction. Returns the rows copied (0 when the
/// token was already marked).
fn copy_visits(old: &Connection, new_db: &Path) -> Result<u64, ImportanceStoreError> {
    let token = stamp_token(old)?;
    let rows = read_old_visits(old)?;

    let mut new = super::visits::open_write_connection(new_db)?;
    let tx = new.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let already: Option<i64> = tx
        .query_row("SELECT rows FROM split_from WHERE token = ?1", [&token], |row| {
            row.get(0)
        })
        .optional()?;
    if already.is_some() {
        return Ok(0);
    }
    {
        // Merged, never replaced: the new file may hold visits of its own (a
        // downgrade round trip recorded some in each).
        let mut insert = tx.prepare_cached(
            "INSERT INTO visits (path_folded, path, visit_count, last_visit_secs) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path_folded) DO UPDATE SET
                 visit_count = visit_count + excluded.visit_count,
                 last_visit_secs = max(last_visit_secs, excluded.last_visit_secs)",
        )?;
        for (path, count, last) in &rows {
            insert.execute(rusqlite::params![normalize_for_comparison(path), path, count, last])?;
        }
    }
    tx.execute(
        "INSERT INTO split_from (token, rows) VALUES (?1, ?2)",
        rusqlite::params![token, rows.len() as i64],
    )?;
    tx.commit()?;
    Ok(rows.len() as u64)
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

fn delete_sidecars(db: &Path) -> std::io::Result<()> {
    let [_, wal, shm] = sqlite_util::database_files(db);
    for file in [wal, shm] {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
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
