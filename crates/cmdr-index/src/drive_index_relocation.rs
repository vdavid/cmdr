//! Moving the drive index's databases from one folder to another, once per
//! launch, before anything opens them.
//!
//! Building the index calls [`relocate_drive_index`] when the host keeps the
//! drive index apart from the data dir (`IndexConfig::drive_index_dir`), to adopt
//! what an older build left in the data dir. Only the drive index moves:
//! importance and media stay where they are, so their files are never touched
//! here.
//!
//! **Each volume moves as ONE rename of ONE file**, which is what makes a crash at
//! any point harmless. A WAL-mode database is three files, and three renames
//! can't be atomic together: a crash between them would strand the `-wal` that
//! holds the newest commits away from its database. So the WAL is folded into the
//! database first (a `TRUNCATE` checkpoint, then the close that deletes the empty
//! sidecars), and only the database file travels. A crash before the rename
//! leaves everything in the old folder for the next launch; after it, nothing is
//! left to do.
//!
//! The decisions per volume, in order:
//!
//! - **The new folder already has its database**: that one is what this build
//!   has been keeping, so the old copy is deleted. That's the downgrade round
//!   trip: an older build found no index in the data dir, built one there, and
//!   the user upgraded again.
//! - **Only sidecars in the old folder**: they belong to no database, so they're
//!   deleted, never moved.
//! - **The WAL can't be folded in** (another process holds the database, or it's
//!   unreadable): left where it is with the reason, for the next launch.
//! - **Otherwise**: any sidecar already in the new folder is deleted (it belongs to
//!   no database there, and SQLite would replay a stray `-wal` onto the one that
//!   arrives), then the database is renamed across. Across volumes a rename
//!   can't work, and a copy of gigabytes isn't worth it for a cache: the old copy
//!   is deleted and the volume rebuilds.

use std::io::ErrorKind;
use std::path::Path;

use cmdr_fs::sqlite_util;

use crate::volume_files::VolumeStore;

/// What [`relocate_drive_index`] did, volume by volume, for the host's log line.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct DriveIndexRelocation {
    /// Volumes whose index now lives in the new folder.
    pub(crate) moved: Vec<String>,
    /// Volumes whose old copy was thrown away instead: the new folder already had
    /// one, or the old one couldn't be moved whole. Each rebuilds like any index
    /// that's missing.
    pub(crate) discarded: Vec<String>,
    /// Volumes whose old files are still in the old folder, with why. The next
    /// launch tries again.
    pub(crate) left: Vec<(String, String)>,
}

/// Move every drive-index database in `from` to `to`, creating `to` if needed.
///
/// ⚠️ Nothing may have either folder's index databases open: the caller holds the
/// data dir's instance lock and runs this before the index starts.
pub(crate) fn relocate_drive_index(from: &Path, to: &Path) -> DriveIndexRelocation {
    let mut report = DriveIndexRelocation::default();
    if from == to {
        return report;
    }
    let volume_ids = drive_index_volume_ids(from);
    if volume_ids.is_empty() {
        return report;
    }
    if let Err(e) = std::fs::create_dir_all(to) {
        let why = format!("can't create {}: {e}", to.display());
        report.left = volume_ids.into_iter().map(|id| (id, why.clone())).collect();
        return report;
    }
    for volume_id in volume_ids {
        match relocate_one(from, to, &volume_id) {
            Outcome::Moved => report.moved.push(volume_id),
            Outcome::Discarded => report.discarded.push(volume_id),
            Outcome::Left(why) => report.left.push((volume_id, why)),
        }
    }
    report
}

enum Outcome {
    Moved,
    Discarded,
    Left(String),
}

/// Every volume with any drive-index file in `dir`: its database, or a sidecar
/// whose database is gone.
fn drive_index_volume_ids(dir: &Path) -> Vec<String> {
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
        if let Some(volume_id) = VolumeStore::Index.volume_id_of(db_name)
            && !volume_ids.iter().any(|seen| seen == volume_id)
        {
            volume_ids.push(volume_id.to_string());
        }
    }
    volume_ids
}

fn relocate_one(from: &Path, to: &Path, volume_id: &str) -> Outcome {
    let old_db = VolumeStore::Index.db_path(from, volume_id);
    let new_db = VolumeStore::Index.db_path(to, volume_id);

    if new_db.exists() || !old_db.exists() {
        return discard(&old_db);
    }
    if let Err(why) = fold_the_wal_in(&old_db) {
        return Outcome::Left(why);
    }
    if let Err(e) = delete_sidecars(&new_db) {
        return Outcome::Left(format!("can't clear stray files beside {}: {e}", new_db.display()));
    }
    match std::fs::rename(&old_db, &new_db) {
        Ok(()) => {
            // The close in `fold_the_wal_in` deleted them; this only catches a
            // filesystem that kept an empty pair.
            let _ = delete_sidecars(&old_db);
            Outcome::Moved
        }
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            log::info!(
                target: "indexing",
                "the drive index of '{volume_id}' is on another volume than {}; rebuilding it there instead of copying",
                to.display()
            );
            discard(&old_db)
        }
        Err(e) => Outcome::Left(format!("can't move {}: {e}", old_db.display())),
    }
}

/// Checkpoint every frame of `db`'s WAL into the database, then close, which
/// deletes the emptied `-wal` and `-shm`. After this the database file alone is
/// the whole database.
fn fold_the_wal_in(db: &Path) -> Result<(), String> {
    let conn = sqlite_util::open(db).map_err(|e| format!("can't open {}: {e}", db.display()))?;
    // `(busy, log frames, checkpointed frames)`; busy means another connection
    // kept part of the log, so it can't all be folded in.
    let (busy, _, _): (i64, i64, i64) = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| format!("can't checkpoint {}: {e}", db.display()))?;
    drop(conn);
    if busy != 0 {
        return Err(format!("{} is in use by another process", db.display()));
    }
    let [_, wal, _] = sqlite_util::database_files(db);
    match std::fs::metadata(&wal) {
        Ok(meta) if meta.len() > 0 => Err(format!("{} still has a write-ahead log", db.display())),
        _ => Ok(()),
    }
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
        Err(e) => Outcome::Left(format!("can't delete {}: {e}", old_db.display())),
    }
}

#[cfg(test)]
#[path = "drive_index_relocation_tests.rs"]
mod tests;
