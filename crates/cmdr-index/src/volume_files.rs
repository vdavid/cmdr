//! A volume's state on disk: every file the index keeps for one volume, and the
//! one way any of them is removed.
//!
//! Four stores each keep a file set per volume, all named after the volume id. Two
//! are rebuildable caches in the drive-index dir, which the app keeps out of
//! backups: the drive index (`index-{id}.db`) and importance's folder weights
//! (`importance-{id}.db`). Two hold what a scan can't bring back and stay in the
//! data dir: importance's navigation visits (`visits-{id}.db`) and the media index
//! (`media-{id}.db` plus the vector index it carries beside it). [`StoreDirs`] says
//! which folder each store is in. Each store opens and migrates its own database;
//! what they share is here, because it's what goes wrong when it's spread out:
//! the NAMES, and the decision of which stores go when a volume's files are
//! removed. Removal used to live in each caller, and each one unlinked
//! `index-{id}.db` and stopped: a forgotten share left its importance database
//! behind for good, and the sweep for retired IDs left the vector index.
//!
//! So a removal names its reason ([`Removal`]) and [`Removal::takes`] answers per
//! store in ONE exhaustive match. A fourth store is a new [`VolumeStore`]
//! variant, and the compiler then asks what every reason does with it.
//!
//! A leaf on purpose: it names files and deletes them, and imports no subsystem.
//! The stores delegate their path functions here, and a subsystem that holds a
//! store open says so through [`register_holder`] rather than being called by
//! name, so `indexing` never has to know what `importance` keeps alive.

use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::sqlite_util;

/// One of the per-volume stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VolumeStore {
    /// The drive index: `index-{id}.db`.
    Index,
    /// Folder importance's weights: `importance-{id}.db`. Rebuildable by a rescore.
    Importance,
    /// Folder importance's navigation visits: `visits-{id}.db`. NOT rebuildable:
    /// it records where the user actually went.
    Visits,
    /// The media index: `media-{id}.db`, and one vector index per ANN space.
    Media,
}

/// The CLIP vector space's file-name suffix, as in `media-{id}.clip.usearch`.
pub(crate) const ANN_SPACE_CLIP: &str = "clip";

/// Every ANN space a media store can keep a vector index for.
/// `media_index::ann::AnnSpace::suffix` answers from these, and
/// `media_index::ann`'s tests pin that every space is listed.
const ANN_SPACES: [&str; 1] = [ANN_SPACE_CLIP];

/// A vector index's file beside its media database.
pub(crate) const ANN_INDEX_EXTENSION: &str = "usearch";
/// The JSON sidecar pinning a vector index's format, model, dims, and rows.
pub(crate) const ANN_META_EXTENSION: &str = "usearch.meta";
/// The crash-detection marker a writer holds while it has unflushed ops.
pub(crate) const ANN_DIRTY_EXTENSION: &str = "usearch.dirty";

const ANN_EXTENSIONS: [&str; 3] = [ANN_INDEX_EXTENSION, ANN_META_EXTENSION, ANN_DIRTY_EXTENSION];

/// One of a vector index's files for `space`, beside the media database at
/// `media_db`: `media-{id}.{space}.{extension}`.
pub(crate) fn ann_file(media_db: &Path, space: &str, extension: &str) -> PathBuf {
    let stem = media_db.file_stem().and_then(|s| s.to_str()).unwrap_or("media");
    media_db.with_file_name(format!("{stem}.{space}.{extension}"))
}

/// The folders the per-volume stores live in: the rebuildable caches (the drive
/// index and importance's weights) in the drive-index dir, and what a scan can't
/// bring back (visits and media) in the data dir. One folder for both is fine
/// (tests, and a host that keeps everything together).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoreDirs {
    /// Importance's visits and the media index: backed up.
    pub(crate) data: PathBuf,
    /// The drive index and importance's weights: kept out of backups.
    pub(crate) drive_index: PathBuf,
}

impl StoreDirs {
    /// The folders the host configured, or why there are none.
    pub(crate) fn configured() -> Result<Self, crate::indexing::host::config::DataDirUnset> {
        Ok(Self {
            data: crate::indexing::host::config::data_dir()?,
            drive_index: crate::indexing::host::config::drive_index_dir()?,
        })
    }

    /// The folders for the volume whose index database is `index_db`: that file's own
    /// folder for the index, and the configured data dir for the rest, or the index's
    /// folder too when nothing is configured (every store in one folder, as a test
    /// lays them out).
    pub(crate) fn around_index_db(index_db: &Path) -> Self {
        let drive_index = index_db.parent().unwrap_or(Path::new("")).to_path_buf();
        let data = crate::indexing::host::config::data_dir().unwrap_or_else(|_| drive_index.clone());
        Self { data, drive_index }
    }

    /// The folders for the host whose data dir is `data_dir`: the configured ones
    /// when the host configured that data dir, else every store in `data_dir` (a
    /// test or a tool handing over a folder of its own).
    ///
    /// The data dir is what names an index instance across the crate's API (a
    /// host passes the one it configured), so this is how a store that isn't in
    /// the data dir is found from it without deriving one folder from another.
    pub(crate) fn for_data_dir(data_dir: &Path) -> Self {
        match Self::configured() {
            Ok(dirs) if dirs.data == data_dir => dirs,
            _ => Self::single(data_dir),
        }
    }

    /// Every store in `dir`.
    pub(crate) fn single(dir: &Path) -> Self {
        Self {
            data: dir.to_path_buf(),
            drive_index: dir.to_path_buf(),
        }
    }

    /// The folder `store` keeps its files in.
    pub(crate) fn of(&self, store: VolumeStore) -> &Path {
        match store {
            VolumeStore::Index | VolumeStore::Importance => &self.drive_index,
            VolumeStore::Visits | VolumeStore::Media => &self.data,
        }
    }

    /// `store`'s database for `volume_id`.
    pub(crate) fn db_path(&self, store: VolumeStore, volume_id: &str) -> PathBuf {
        store.db_path(self.of(store), volume_id)
    }

    /// Whether the data dir and the drive-index dir are one folder.
    pub(crate) fn is_single(&self) -> bool {
        self.data == self.drive_index
    }

    /// Each distinct folder once, so a scan over both never reads one twice.
    fn distinct(&self) -> Vec<&Path> {
        if self.data == self.drive_index {
            vec![&self.data]
        } else {
            vec![&self.data, &self.drive_index]
        }
    }
}

impl VolumeStore {
    /// Every store, in the order a removal works through them.
    pub(crate) const ALL: [VolumeStore; 4] = [
        VolumeStore::Index,
        VolumeStore::Importance,
        VolumeStore::Visits,
        VolumeStore::Media,
    ];

    /// What this store's file names start with, before the volume id.
    fn prefix(self) -> &'static str {
        match self {
            VolumeStore::Index => "index-",
            VolumeStore::Importance => "importance-",
            VolumeStore::Visits => "visits-",
            VolumeStore::Media => "media-",
        }
    }

    /// The store's database for `volume_id` in `dir`, which is this store's own
    /// folder ([`StoreDirs::of`]).
    pub(crate) fn db_path(self, dir: &Path, volume_id: &str) -> PathBuf {
        dir.join(format!("{}{volume_id}.db", self.prefix()))
    }

    /// The volume id a database file name belongs to, or `None` for anything that
    /// isn't this store's database (a sidecar, another store's file, an unrelated
    /// one). A volume id may itself contain `-` and `.`, so this strips the fixed
    /// prefix and suffix and never splits.
    pub(crate) fn volume_id_of(self, file_name: &str) -> Option<&str> {
        file_name.strip_prefix(self.prefix())?.strip_suffix(".db")
    }

    /// Every volume with any of this store's SQLite files in `dir`: its database,
    /// or a sidecar whose database is gone. Each id once; an unreadable `dir`
    /// answers none.
    pub(crate) fn volume_ids_in(self, dir: &Path) -> Vec<String> {
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
            if let Some(volume_id) = self.volume_id_of(db_name)
                && !volume_ids.iter().any(|seen| seen == volume_id)
            {
                volume_ids.push(volume_id.to_string());
            }
        }
        volume_ids
    }

    /// What the store keeps beside its database that SQLite doesn't own.
    fn derived_files(self, db_path: &Path) -> Vec<PathBuf> {
        match self {
            VolumeStore::Index | VolumeStore::Importance | VolumeStore::Visits => Vec::new(),
            VolumeStore::Media => ANN_SPACES
                .iter()
                .flat_map(|space| ANN_EXTENSIONS.map(|extension| ann_file(db_path, space, extension)))
                .collect(),
        }
    }

    /// Every file the store keeps for the volume whose database is `db_path`: the
    /// database, its WAL and SHM, and whatever the store derives from them.
    pub(crate) fn files(self, db_path: &Path) -> Vec<PathBuf> {
        let mut files = sqlite_util::database_files(db_path).to_vec();
        files.extend(self.derived_files(db_path));
        files
    }
}

/// Why a volume's files are being removed, which decides which stores go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Removal {
    /// The volume is leaving the index: the user forgot it, cleared every index, or
    /// the retention cap evicted it as abandoned.
    Forgotten,
    /// The volume stays and its index database is being thrown away to be built
    /// again: it failed, or its rows predate this build's exclusion policy. The
    /// weights scored from it go too; the visits stay.
    IndexRebuild,
    /// Nothing can ever open these files again: they are keyed by a volume ID from
    /// a retired scheme.
    Unreachable,
}

impl Removal {
    /// Whether this removal takes `store`'s files.
    ///
    /// ❗ **The ONE place that decides it.** A caller names a reason and never a
    /// list of stores, so a store can't be forgotten by a path that was written
    /// before it existed.
    pub(crate) fn takes(self, store: VolumeStore) -> bool {
        match (self, store) {
            (Removal::Forgotten | Removal::IndexRebuild | Removal::Unreachable, VolumeStore::Index) => true,
            // The weights score the folders of the index that's going, so they go
            // with it every time: a rebuild's next completed scan rescores in full.
            (Removal::Forgotten | Removal::IndexRebuild | Removal::Unreachable, VolumeStore::Importance) => true,
            // The visits are the user's navigation history, which no scan can bring
            // back: a forgotten volume takes them, a rebuild keeps them (the volume
            // is staying, and its next scores still want them).
            (Removal::Forgotten | Removal::Unreachable, VolumeStore::Visits) => true,
            (Removal::IndexRebuild, VolumeStore::Visits) => false,
            // ⚠️ Forgetting a volume's INDEX deliberately keeps its media index.
            // Those rows are hours of OCR and embedding work, the media subsystem
            // deletes rows on exactly four named paths and never on a volume's
            // absence (`media_index/DETAILS.md` § GC safety argument), and turning
            // the drive's indexing back on picks them straight up. Whether a
            // forgotten drive should take them along is David's call; until then
            // only an unreachable id does.
            (Removal::Unreachable, VolumeStore::Media) => true,
            (Removal::Forgotten | Removal::IndexRebuild, VolumeStore::Media) => false,
        }
    }

    /// Whichever of two removals takes more, for two clears landing on one
    /// teardown: the volume loses everything either would have taken. The store
    /// sets nest (a rebuild's inside a forget's inside an unreachable id's), which
    /// `tests::the_wider_removal_takes_every_store_either_takes` pins.
    pub(crate) fn wider(self, other: Removal) -> Removal {
        let reach = |removal: Removal| match removal {
            Removal::IndexRebuild => 0,
            Removal::Forgotten => 1,
            Removal::Unreachable => 2,
        };
        if reach(other) > reach(self) { other } else { self }
    }

    /// The stores this removal takes.
    fn stores(self) -> impl Iterator<Item = VolumeStore> {
        VolumeStore::ALL.into_iter().filter(move |store| self.takes(*store))
    }
}

/// A subsystem's way of letting go of a store it holds open for a volume, given
/// the data dir and the volume id.
type Release = Box<dyn Fn(&Path, &str) + Send + Sync>;

/// Who holds a store's files open outside the lifecycle registry. Process-global
/// and append-only, like the subsystem stop hooks: a holder registers once at
/// startup and lives for the process.
static HOLDERS: Mutex<Vec<(VolumeStore, Release)>> = Mutex::new(Vec::new());

/// Register something that keeps `store`'s database open for a volume (a writer
/// thread, a resident cache), so a removal can ask it to let go FIRST.
///
/// `release` runs inline on the removing thread, before the files are unlinked,
/// and must leave nothing of its own open on them when it returns: a file deleted
/// under an open handle keeps its blocks until the handle closes, and a writer
/// left running would carry on into the unlinked inode. It may block for as long
/// as that takes. It is handed the data dir as well as the volume id and ignores
/// one that isn't its own, since two hosts in one process (tests) can share an id.
///
/// The index database's own holders (the manager, its writer, the read pool) are
/// NOT registered here: the lifecycle teardown drains them itself, in an order
/// that is load-bearing (`indexing/lifecycle/state/teardown.rs`).
pub(crate) fn register_holder(store: VolumeStore, release: Release) {
    HOLDERS.lock_ignore_poison().push((store, release));
}

fn release_holders(store: VolumeStore, data_dir: &Path, volume_id: &str) {
    for (held, release) in HOLDERS.lock_ignore_poison().iter() {
        if *held == store {
            release(data_dir, volume_id);
        }
    }
}

/// How much background work is rewriting store files right now (today, only
/// importance's split of an older build's combined store). A removal waits for it,
/// or it could delete files the work recreates a moment later.
static STORE_WORK: Mutex<usize> = Mutex::new(0);
static STORE_WORK_DONE: Condvar = Condvar::new();

/// How long a caller waits for store work before going ahead: far more than the
/// split takes (about 100 KB per volume), short enough that a stuck disk can't
/// hold a writer or a removal forever.
const STORE_WORK_WAIT_CAP: Duration = Duration::from_secs(30);

/// Marks background store work until dropped, a panic included.
#[must_use = "the work is over when this drops"]
pub(crate) struct StoreWork(());

impl Drop for StoreWork {
    fn drop(&mut self) {
        let mut running = STORE_WORK.lock_ignore_poison();
        *running = running.saturating_sub(1);
        STORE_WORK_DONE.notify_all();
    }
}

/// Mark background store work as running, until the returned guard drops.
pub(crate) fn begin_store_work() -> StoreWork {
    *STORE_WORK.lock_ignore_poison() += 1;
    StoreWork(())
}

/// Block until no background store work runs (at most 30 s). Instant when none
/// does. Blocking threads only.
pub(crate) fn wait_for_store_work() {
    let running = STORE_WORK.lock_ignore_poison();
    let (_running, timeout) = STORE_WORK_DONE
        .wait_timeout_while(running, STORE_WORK_WAIT_CAP, |running| *running > 0)
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if timeout.timed_out() {
        log::warn!(target: "indexing::retention", "background store work still running after {STORE_WORK_WAIT_CAP:?}; going ahead");
    }
}

/// Remove the files `why` takes for `volume_id`, each store's from its own folder
/// in `dirs`, once any background store work has finished ([`wait_for_store_work`]).
///
/// Per store: ask its holders to let go, then delete the database through
/// [`sqlite_util::delete_database`] (which retires the read connections threads
/// have cached to it, on both sides of the unlink), then the files derived from
/// it. A file that isn't there is fine. Every store is attempted even when one
/// refuses, and the first refusal is what comes back.
///
/// An older build's combined importance store (weights AND visits in one
/// `importance-{id}.db` in the data dir, until `importance/store/legacy_split.rs`
/// splits it) belongs to BOTH importance stores, so:
///
/// - A removal that keeps the visits never deletes a weights path that still holds
///   visit rows (one folder for both dirs, where it IS the weights path).
/// - A removal that takes the visits takes it too, wherever it is, or the next
///   split would bring a forgotten volume's history back.
///
/// ⚠️ The caller has already made sure nothing reads or writes the INDEX database:
/// its holders are the lifecycle's, not this module's.
pub(crate) fn remove(dirs: &StoreDirs, volume_id: &str, why: Removal) -> Result<(), String> {
    wait_for_store_work();
    let mut first_refusal = None;
    for store in why.stores() {
        let db_path = dirs.db_path(store, volume_id);
        if store == VolumeStore::Importance && !why.takes(VolumeStore::Visits) && holds_visit_rows(&db_path) {
            log::warn!(
                target: "indexing::retention",
                "keeping {}: it still holds visits an older build kept there, and this removal keeps visits",
                LogPrivate::file_path(&db_path)
            );
            continue;
        }
        delete_store(dirs, store, volume_id, &mut first_refusal);
    }
    if why.takes(VolumeStore::Visits) {
        let combined = VolumeStore::Importance.db_path(&dirs.data, volume_id);
        if let Err(e) = sqlite_util::delete_database(&combined) {
            first_refusal.get_or_insert(format!("Failed to delete {}: {e}", combined.display()));
        }
    }
    first_refusal.map_or(Ok(()), Err)
}

/// Remove every volume's media store in `dirs`, for a platform with no on-device image
/// engine: whatever a `media-*.db` holds there was written by something other than
/// one, and nothing can ever enrich it honestly (#1001). Holders let go first, then
/// the database and its vector-index sidecars go, as in [`remove`]. Every other store
/// stays. Every volume is attempted even when one refuses; the first refusal comes back.
pub(crate) fn remove_every_media_store(dirs: &StoreDirs) -> Result<(), String> {
    let mut first_refusal = None;
    for volume_id in VolumeStore::Media.volume_ids_in(dirs.of(VolumeStore::Media)) {
        delete_store(dirs, VolumeStore::Media, &volume_id, &mut first_refusal);
    }
    first_refusal.map_or(Ok(()), Err)
}

/// Delete one store's files for `volume_id`: ask its holders to let go, then the
/// database (through [`sqlite_util::delete_database`]), then what it derives. A file
/// that isn't there is fine; a refusal lands in `first_refusal` unless one already did.
fn delete_store(dirs: &StoreDirs, store: VolumeStore, volume_id: &str, first_refusal: &mut Option<String>) {
    let db_path = dirs.db_path(store, volume_id);
    release_holders(store, &dirs.data, volume_id);
    if let Err(e) = sqlite_util::delete_database(&db_path) {
        first_refusal.get_or_insert(format!("Failed to delete {}: {e}", db_path.display()));
    }
    for file in store.derived_files(&db_path) {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                first_refusal.get_or_insert(format!("Failed to delete {}: {e}", file.display()));
            }
        }
    }
}

/// Whether the database at `db_path` has a `visits` table with at least one row.
/// A file that's missing or can't be read answers no: nothing in it can be kept.
pub(crate) fn holds_visit_rows(db_path: &Path) -> bool {
    if !db_path.exists() {
        return false;
    }
    let Ok(conn) = sqlite_util::open(db_path) else {
        return false;
    };
    conn.query_row("SELECT EXISTS (SELECT 1 FROM visits)", [], |row| row.get::<_, bool>(0))
        .unwrap_or(false)
}

/// Ask every thread to close the read connections it caches to `volume_id`'s
/// databases, for a volume that stopped and keeps its files.
///
/// A stopped share's connections would otherwise sit in each blocking thread's
/// cache until something else pushed them out. The files stay, so a later read
/// (an offline importance lookup, a restart of the volume) reopens.
/// `sqlite_util::retire_read_connections` says how far a request like this reaches.
pub(crate) fn retire_read_connections(dirs: &StoreDirs, volume_id: &str) {
    for store in VolumeStore::ALL {
        sqlite_util::retire_read_connections(&dirs.db_path(store, volume_id));
    }
}

/// The id of every volume that has a database in `dirs` that `why` would take,
/// in no particular order and each one once.
///
/// Read from the files of EVERY such store and never from the index's alone: a
/// store's database can outlive the index beside it (a share forgotten before the
/// stores were removed together), and a sweep that only looked for indexes would
/// never find it.
pub(crate) fn volume_ids_on_disk(dirs: &StoreDirs, why: Removal) -> Vec<String> {
    let mut volume_ids: Vec<String> = Vec::new();
    for dir in dirs.distinct() {
        let read_dir = match std::fs::read_dir(dir) {
            Ok(read_dir) => read_dir,
            // A drive-index dir nothing has written yet, or one a cleaner emptied.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                log::warn!(target: "indexing::retention", "cannot read {}: {e}", LogPrivate::dir_path(dir));
                continue;
            }
        };
        for entry in read_dir.flatten() {
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            // Only a store that lives in THIS folder names a volume here.
            let Some(volume_id) = why
                .stores()
                .filter(|store| dirs.of(*store) == dir)
                .find_map(|store| store.volume_id_of(file_name))
            else {
                continue;
            };
            if !volume_ids.iter().any(|seen| seen == volume_id) {
                volume_ids.push(volume_id.to_string());
            }
        }
    }
    volume_ids
}

/// How many bytes [`remove`] would give back for `volume_id` under `why`. A file
/// that can't be read counts as zero.
pub(crate) fn bytes_on_disk(dirs: &StoreDirs, volume_id: &str, why: Removal) -> u64 {
    why.stores()
        .flat_map(|store| store.files(&dirs.db_path(store, volume_id)))
        .map(|file| std::fs::metadata(file).map(|m| m.len()).unwrap_or(0))
        .sum()
}

#[cfg(test)]
#[path = "volume_files_tests.rs"]
mod tests;
