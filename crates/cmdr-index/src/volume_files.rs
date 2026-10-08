//! A volume's state on disk: every file the index keeps for one volume, and the
//! one way any of them is removed.
//!
//! Three stores each keep a file set per volume, all named after the volume id:
//! the drive index (`index-{id}.db`, in the drive-index dir, which the app keeps
//! out of backups), folder importance (`importance-{id}.db`), and the media index
//! (`media-{id}.db` plus the vector index it carries beside it), both in the data
//! dir. [`StoreDirs`] says which folder each store is in. Each store opens and migrates its own database;
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
use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::sqlite_util;

/// One of the per-volume stores in the data dir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VolumeStore {
    /// The drive index: `index-{id}.db`.
    Index,
    /// Folder importance: `importance-{id}.db`.
    Importance,
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

/// The folders the per-volume stores live in: the drive index in its own,
/// importance and media in the data dir. One folder for both is fine (tests, and
/// a host that keeps everything together).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoreDirs {
    /// Importance and media.
    pub(crate) data: PathBuf,
    /// The drive index.
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

    /// Every store in `dir`.
    #[cfg(test)]
    pub(crate) fn single(dir: &Path) -> Self {
        Self {
            data: dir.to_path_buf(),
            drive_index: dir.to_path_buf(),
        }
    }

    /// The folder `store` keeps its files in.
    pub(crate) fn of(&self, store: VolumeStore) -> &Path {
        match store {
            VolumeStore::Index => &self.drive_index,
            VolumeStore::Importance | VolumeStore::Media => &self.data,
        }
    }

    /// `store`'s database for `volume_id`.
    pub(crate) fn db_path(&self, store: VolumeStore, volume_id: &str) -> PathBuf {
        store.db_path(self.of(store), volume_id)
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
    pub(crate) const ALL: [VolumeStore; 3] = [VolumeStore::Index, VolumeStore::Importance, VolumeStore::Media];

    /// What this store's file names start with, before the volume id.
    fn prefix(self) -> &'static str {
        match self {
            VolumeStore::Index => "index-",
            VolumeStore::Importance => "importance-",
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

    /// What the store keeps beside its database that SQLite doesn't own.
    fn derived_files(self, db_path: &Path) -> Vec<PathBuf> {
        match self {
            VolumeStore::Index | VolumeStore::Importance => Vec::new(),
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
    /// again: it failed, or its rows predate this build's exclusion policy.
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
            // Importance scores the folders of an index that's going, so it goes
            // with a forgotten volume. A rebuild keeps it: the volume is staying,
            // the next completed scan rescores it in full, and the visit history
            // in it isn't something a scan can bring back.
            (Removal::Forgotten | Removal::Unreachable, VolumeStore::Importance) => true,
            (Removal::IndexRebuild, VolumeStore::Importance) => false,
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

/// Remove the files `why` takes for `volume_id`, each store's from its own folder
/// in `dirs`.
///
/// Per store: ask its holders to let go, then delete the database through
/// [`sqlite_util::delete_database`] (which retires the read connections threads
/// have cached to it, on both sides of the unlink), then the files derived from
/// it. A file that isn't there is fine. Every store is attempted even when one
/// refuses, and the first refusal is what comes back.
///
/// ⚠️ The caller has already made sure nothing reads or writes the INDEX database:
/// its holders are the lifecycle's, not this module's.
pub(crate) fn remove(dirs: &StoreDirs, volume_id: &str, why: Removal) -> Result<(), String> {
    let mut first_refusal = None;
    for store in why.stores() {
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
    first_refusal.map_or(Ok(()), Err)
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
                log::warn!(target: "indexing::retention", "cannot read {}: {e}", dir.display());
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
mod tests {
    use super::*;

    const VOLUME_ID: &str = "smb-192-168-1-111-445-naspi-bbe2c537964d80d0";

    /// Write every file every store keeps for `volume_id`, and hand them back by
    /// store.
    fn write_every_file(dirs: &StoreDirs, volume_id: &str) -> Vec<(VolumeStore, PathBuf)> {
        let mut written = Vec::new();
        for store in VolumeStore::ALL {
            for file in store.files(&dirs.db_path(store, volume_id)) {
                std::fs::write(&file, [0u8; 16]).expect("write a store file");
                written.push((store, file));
            }
        }
        written
    }

    #[test]
    fn a_store_names_its_database_after_the_volume_and_reads_the_id_back() {
        let dir = Path::new("/data");
        assert_eq!(
            VolumeStore::Index.db_path(dir, "root"),
            PathBuf::from("/data/index-root.db")
        );
        assert_eq!(
            VolumeStore::Importance.db_path(dir, VOLUME_ID),
            PathBuf::from(format!("/data/importance-{VOLUME_ID}.db"))
        );
        assert_eq!(
            VolumeStore::Media.db_path(dir, "root"),
            PathBuf::from("/data/media-root.db")
        );

        assert_eq!(VolumeStore::Index.volume_id_of("index-root.db"), Some("root"));
        // A volume id containing `-` and `:` (an MTP serial) survives the strip.
        assert_eq!(
            VolumeStore::Index.volume_id_of("index-mtp-AABBCC-1:65537.db"),
            Some("mtp-AABBCC-1:65537")
        );
        // Sidecars, other stores' files, and unrelated ones are nobody's database.
        assert_eq!(VolumeStore::Index.volume_id_of("index-root.db-wal"), None);
        assert_eq!(VolumeStore::Index.volume_id_of("importance-root.db"), None);
        assert_eq!(VolumeStore::Importance.volume_id_of("index-root.db"), None);
        assert_eq!(VolumeStore::Media.volume_id_of("media-root.clip.usearch"), None);
        assert_eq!(VolumeStore::Index.volume_id_of("operation-log.db"), None);
    }

    #[test]
    fn the_media_store_carries_its_vector_index_files() {
        let files = VolumeStore::Media.files(Path::new("/data/media-root.db"));
        let names: Vec<&str> = files
            .iter()
            .map(|file| file.file_name().and_then(|n| n.to_str()).expect("name"))
            .collect();
        assert_eq!(
            names,
            [
                "media-root.db",
                "media-root.db-wal",
                "media-root.db-shm",
                "media-root.clip.usearch",
                "media-root.clip.usearch.meta",
                "media-root.clip.usearch.dirty",
            ]
        );
    }

    /// Which stores each reason takes, pinned as the table it is. A change here is
    /// a change to what a user loses, so it should fail a test and be read.
    #[test]
    fn each_removal_takes_the_stores_it_names_and_no_others() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cases = [
            (Removal::Forgotten, vec![VolumeStore::Index, VolumeStore::Importance]),
            (Removal::IndexRebuild, vec![VolumeStore::Index]),
            (Removal::Unreachable, VolumeStore::ALL.to_vec()),
        ];
        let dirs = StoreDirs::single(dir.path());
        for (why, taken) in cases {
            let written = write_every_file(&dirs, VOLUME_ID);

            remove(&dirs, VOLUME_ID, why).expect("remove");

            for (store, file) in &written {
                assert_eq!(file.exists(), !taken.contains(store), "{why:?} and {}", file.display());
            }
        }
    }

    /// Another volume's files are never touched, including one whose id this
    /// volume's id is a prefix of.
    #[test]
    fn removing_a_volume_leaves_every_other_volumes_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let dirs = StoreDirs::single(dir.path());
        write_every_file(&dirs, "smb-nas");
        let neighbours = write_every_file(&dirs, "smb-nas.local-photos");
        let root = write_every_file(&dirs, "root");

        remove(&dirs, "smb-nas", Removal::Unreachable).expect("remove");

        for (_, file) in neighbours.iter().chain(&root) {
            assert!(file.exists(), "{} belongs to another volume", file.display());
        }
    }

    /// A holder lets go BEFORE the files are unlinked, and is told which data dir
    /// and volume it is about.
    #[test]
    fn a_stores_holder_is_asked_to_let_go_before_its_files_are_deleted() {
        let dir = tempfile::tempdir().expect("temp dir");
        let dirs = StoreDirs::single(dir.path());
        write_every_file(&dirs, VOLUME_ID);
        let importance_db = VolumeStore::Importance.db_path(dir.path(), VOLUME_ID);

        // What the holder saw when it was called: whether the database was still
        // on disk. Keyed on this test's own data dir, since the registry is
        // process-wide.
        let seen: std::sync::Arc<Mutex<Vec<bool>>> = std::sync::Arc::default();
        let (seen_by_holder, own_dir, held_db) = (seen.clone(), dir.path().to_path_buf(), importance_db.clone());
        register_holder(
            VolumeStore::Importance,
            Box::new(move |data_dir, volume_id| {
                if data_dir == own_dir && volume_id == VOLUME_ID {
                    seen_by_holder.lock_ignore_poison().push(held_db.exists());
                }
            }),
        );

        remove(&dirs, VOLUME_ID, Removal::IndexRebuild).expect("rebuild");
        assert!(
            seen.lock_ignore_poison().is_empty(),
            "a removal that keeps the store doesn't disturb whoever holds it"
        );

        remove(&dirs, VOLUME_ID, Removal::Forgotten).expect("forget");
        assert_eq!(
            *seen.lock_ignore_poison(),
            [true],
            "asked once, while the database was still there"
        );
        assert!(!importance_db.exists());
    }

    #[test]
    fn the_volumes_on_disk_are_read_from_every_store_the_removal_takes() {
        let dir = tempfile::tempdir().expect("temp dir");
        for name in [
            "index-root.db",
            "index-root.db-wal",
            "importance-root.db",
            "importance-smb-index-already-gone.db",
            "media-smb-media-only.db",
            "operation-log.db",
        ] {
            std::fs::write(dir.path().join(name), [0u8; 10]).expect("write");
        }

        let dirs = StoreDirs::single(dir.path());
        let mut forgettable = volume_ids_on_disk(&dirs, Removal::Forgotten);
        forgettable.sort();
        assert_eq!(forgettable, ["root", "smb-index-already-gone"]);

        let mut unreachable = volume_ids_on_disk(&dirs, Removal::Unreachable);
        unreachable.sort();
        assert_eq!(unreachable, ["root", "smb-index-already-gone", "smb-media-only"]);

        assert_eq!(bytes_on_disk(&dirs, "root", Removal::Forgotten), 30);
        assert_eq!(bytes_on_disk(&dirs, "root", Removal::IndexRebuild), 20);
    }

    /// With the drive index in a folder of its own, each store is found, measured,
    /// and removed in ITS folder, and a stray file of one store sitting in the
    /// other store's folder names no volume.
    #[test]
    fn a_drive_index_kept_apart_is_found_and_removed_in_its_own_folder() {
        let root = tempfile::tempdir().expect("temp dir");
        let dirs = StoreDirs {
            data: root.path().join("data"),
            drive_index: root.path().join("cache"),
        };
        std::fs::create_dir_all(&dirs.data).expect("data");
        std::fs::create_dir_all(&dirs.drive_index).expect("cache");
        let written = write_every_file(&dirs, VOLUME_ID);
        std::fs::write(dirs.drive_index.join("importance-stray.db"), [0u8; 4]).expect("stray");
        std::fs::write(dirs.data.join("index-left-behind.db"), [0u8; 4]).expect("stray");

        assert_eq!(volume_ids_on_disk(&dirs, Removal::Unreachable), [VOLUME_ID]);
        assert_eq!(
            bytes_on_disk(&dirs, VOLUME_ID, Removal::Unreachable),
            16 * written.len() as u64
        );
        assert!(
            dirs.db_path(VolumeStore::Index, VOLUME_ID)
                .starts_with(&dirs.drive_index)
        );

        remove(&dirs, VOLUME_ID, Removal::Unreachable).expect("remove");
        for (_, file) in &written {
            assert!(!file.exists(), "{} must go", file.display());
        }
    }
}
