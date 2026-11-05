use super::*;

const VOLUME_ID: &str = "smb-192-168-1-111-445-naspi-bbe2c537964d80d0";

#[test]
fn the_wider_removal_takes_every_store_either_takes() {
    let all = [Removal::IndexRebuild, Removal::Forgotten, Removal::Unreachable];
    for a in all {
        for b in all {
            let wider = a.wider(b);
            for store in VolumeStore::ALL {
                assert_eq!(
                    wider.takes(store),
                    a.takes(store) || b.takes(store),
                    "{a:?}.wider({b:?}) = {wider:?} on {store:?}"
                );
            }
        }
    }
}

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
        (
            Removal::Forgotten,
            vec![VolumeStore::Index, VolumeStore::Importance, VolumeStore::Visits],
        ),
        (Removal::IndexRebuild, vec![VolumeStore::Index, VolumeStore::Importance]),
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
    let visits_db = VolumeStore::Visits.db_path(dir.path(), VOLUME_ID);

    // What the holder saw when it was called: whether the database was still
    // on disk. Keyed on this test's own data dir, since the registry is
    // process-wide.
    let seen: std::sync::Arc<Mutex<Vec<bool>>> = std::sync::Arc::default();
    let (seen_by_holder, own_dir, held_db) = (seen.clone(), dir.path().to_path_buf(), visits_db.clone());
    register_holder(
        VolumeStore::Visits,
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
    assert!(!visits_db.exists());
}

#[test]
fn the_volumes_on_disk_are_read_from_every_store_the_removal_takes() {
    let dir = tempfile::tempdir().expect("temp dir");
    for name in [
        "index-root.db",
        "index-root.db-wal",
        "importance-root.db",
        "visits-root.db",
        "importance-smb-index-already-gone.db",
        "visits-smb-visits-only.db",
        "media-smb-media-only.db",
        "operation-log.db",
    ] {
        std::fs::write(dir.path().join(name), [0u8; 10]).expect("write");
    }

    let dirs = StoreDirs::single(dir.path());
    let mut forgettable = volume_ids_on_disk(&dirs, Removal::Forgotten);
    forgettable.sort();
    assert_eq!(forgettable, ["root", "smb-index-already-gone", "smb-visits-only"]);

    let mut rebuildable = volume_ids_on_disk(&dirs, Removal::IndexRebuild);
    rebuildable.sort();
    assert_eq!(
        rebuildable,
        ["root", "smb-index-already-gone"],
        "a rebuild never reads visits"
    );

    let mut unreachable = volume_ids_on_disk(&dirs, Removal::Unreachable);
    unreachable.sort();
    assert_eq!(
        unreachable,
        ["root", "smb-index-already-gone", "smb-media-only", "smb-visits-only"]
    );

    assert_eq!(bytes_on_disk(&dirs, "root", Removal::Forgotten), 40);
    assert_eq!(bytes_on_disk(&dirs, "root", Removal::IndexRebuild), 30);
}

/// A removal waits for background store work (the split recreating a visits
/// file), so a forget at launch can't delete files the work then brings back.
#[test]
fn a_removal_waits_for_background_store_work() {
    let dir = tempfile::tempdir().expect("temp dir");
    let dirs = StoreDirs::single(dir.path());
    let work = begin_store_work();
    let visits = dirs.db_path(VolumeStore::Visits, VOLUME_ID);
    let forget = {
        let dirs = dirs.clone();
        std::thread::spawn(move || remove(&dirs, VOLUME_ID, Removal::Forgotten))
    };

    // The work writes its file, then ends; only then may the forget run.
    std::fs::write(&visits, [0u8; 8]).expect("the work writes its file");
    drop(work);
    forget.join().expect("forget thread").expect("forget");

    assert!(!visits.exists(), "the forget ran after the work, so its file is gone");
}

/// A store-work guard dropped by a panic still ends the work.
#[test]
fn store_work_ends_when_its_guard_unwinds() {
    let panicked = std::thread::spawn(|| {
        let _work = begin_store_work();
        panic!("the work panics");
    })
    .join();
    assert!(panicked.is_err());
    let started = std::time::Instant::now();
    wait_for_store_work();
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// An older build's combined store (weights AND visits) at `path`.
fn write_combined(path: &Path) {
    let conn = sqlite_util::open(path).expect("open");
    conn.execute_batch(
        "CREATE TABLE visits (path_folded TEXT PRIMARY KEY, path TEXT NOT NULL,
             visit_count INTEGER NOT NULL, last_visit_secs INTEGER NOT NULL);
         INSERT INTO visits VALUES ('/users/me', '/Users/me', 4, 40);",
    )
    .expect("combined store");
}

/// With one folder for both dirs, the weights path IS an older build's combined
/// store until its split lands. A rebuild keeps visits, so it must not delete
/// that file as if it were only weights.
#[test]
fn a_rebuild_never_deletes_a_weights_path_still_holding_visits() {
    let dir = tempfile::tempdir().expect("temp dir");
    let dirs = StoreDirs::single(dir.path());
    let combined = dirs.db_path(VolumeStore::Importance, VOLUME_ID);
    write_combined(&combined);

    remove(&dirs, VOLUME_ID, Removal::IndexRebuild).expect("rebuild");
    assert!(combined.exists(), "the visits in it are kept");

    remove(&dirs, VOLUME_ID, Removal::Forgotten).expect("forget");
    assert!(!combined.exists(), "a forget takes the visits anyway");
}

/// Forgetting a volume takes an older build's combined store left in the data
/// dir too (a split that hasn't landed yet), or the next split would bring the
/// forgotten history back. A rebuild leaves it for the split.
#[test]
fn forgetting_takes_an_older_builds_combined_store_too() {
    let root = tempfile::tempdir().expect("temp dir");
    let dirs = StoreDirs {
        data: root.path().join("data"),
        drive_index: root.path().join("cache"),
    };
    std::fs::create_dir_all(&dirs.data).expect("data");
    let combined = VolumeStore::Importance.db_path(&dirs.data, VOLUME_ID);
    write_combined(&combined);

    remove(&dirs, VOLUME_ID, Removal::IndexRebuild).expect("rebuild");
    assert!(combined.exists());

    remove(&dirs, VOLUME_ID, Removal::Forgotten).expect("forget");
    assert!(
        sqlite_util::database_files(&combined).iter().all(|f| !f.exists()),
        "gone with its sidecars"
    );
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
    std::fs::write(dirs.drive_index.join("visits-stray.db"), [0u8; 4]).expect("stray");
    std::fs::write(dirs.data.join("index-left-behind.db"), [0u8; 4]).expect("stray");
    std::fs::write(dirs.data.join("importance-left-behind.db"), [0u8; 4]).expect("stray");

    assert_eq!(volume_ids_on_disk(&dirs, Removal::Unreachable), [VOLUME_ID]);
    assert_eq!(
        bytes_on_disk(&dirs, VOLUME_ID, Removal::Unreachable),
        16 * written.len() as u64
    );
    // The rebuildable caches stay out of backups; what a scan can't bring back
    // stays in them.
    for store in [VolumeStore::Index, VolumeStore::Importance] {
        assert!(
            dirs.db_path(store, VOLUME_ID).starts_with(&dirs.drive_index),
            "{store:?}"
        );
    }
    for store in [VolumeStore::Visits, VolumeStore::Media] {
        assert!(dirs.db_path(store, VOLUME_ID).starts_with(&dirs.data), "{store:?}");
    }

    remove(&dirs, VOLUME_ID, Removal::Unreachable).expect("remove");
    for (_, file) in &written {
        assert!(!file.exists(), "{} must go", file.display());
    }
}

/// #1001: where there's no image engine, every `media-*.db` was written by something
/// other than one (the test fake that used to be Linux's production backend). They all
/// go, with their vector-index sidecars, and every other store stays.
#[test]
fn removing_every_media_store_takes_each_volumes_media_files_and_nothing_else() {
    let dir = tempfile::tempdir().expect("temp dir");
    let dirs = StoreDirs::single(dir.path());
    let written: Vec<_> = ["root", VOLUME_ID]
        .iter()
        .flat_map(|volume_id| write_every_file(&dirs, volume_id))
        .collect();

    remove_every_media_store(&dirs).expect("remove media");

    for (store, file) in &written {
        assert_eq!(
            file.exists(),
            *store != VolumeStore::Media,
            "{} after the media purge",
            file.display()
        );
    }
}
