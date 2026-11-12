//! The handle's own tests: the single-instance contract, and the acceptance scan
//! that proves the index runs with nothing app-side in the room.

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{ConnectionState, InMemoryVolume, Volume};

use crate::indexing::events::{EventSink, IndexEventKind, RecordingSink};
use crate::indexing::handle::{Index, IndexBuildError, StartOutcome};
use crate::indexing::host::volumes::FakeVolumeProvider;
use crate::indexing::read::coverage::{CoverageDimension, CoverageToken};

/// Building twice is not two indexes. The statics behind the seams are
/// process-wide, so a second `build()` reports [`IndexBuildError::AlreadyBuilt`]
/// rather than handing back a handle that would quietly share the first one's
/// state. The variant goes away when the statics move into the handle.
#[test]
fn a_second_build_reports_already_built_rather_than_a_second_index() {
    let _serialized = crate::indexing::handle::test_lock();
    let _released = Index::release_build_claim_for_test();
    let data = tempfile::tempdir().expect("index data dir");
    // `build()` is the real thing, so it installs the config PERMANENTLY —
    // `install_for_test`'s restore-on-drop is exactly what it doesn't do. Without
    // this guard the data dir stays set for the rest of the binary, and
    // `host::config::tests::an_unset_data_dir_is_an_error_not_an_empty_path` fails
    // for anyone unlucky enough to run after it.
    let _config = crate::indexing::host::config::install_data_dir_for_test(data.path());

    Index::builder()
        .data_dir(data.path())
        .build()
        .expect("the first build claims the process's index");

    match Index::builder().data_dir(data.path()).build() {
        Err(IndexBuildError::AlreadyBuilt) => {}
        Ok(_) => panic!("a second build must not hand back a second index"),
    }
}

/// Building the index splits an older build's combined importance store before
/// anything can open it: the visits land in their own file in the data dir, and
/// the old file leaves the data dir (and with it, the backups).
#[test]
fn building_splits_an_older_builds_importance_store() {
    let _serialized = crate::indexing::handle::test_lock();
    let _released = Index::release_build_claim_for_test();
    let root = tempfile::tempdir().expect("temp dir");
    let (data, cache) = (root.path().join("data"), root.path().join("cache"));
    std::fs::create_dir_all(&data).expect("data");
    // Restores the config `build()` installs for good (see the test above).
    let _config = crate::indexing::host::config::install_data_dir_for_test(&data);
    let old = data.join("importance-root.db");
    {
        let conn = cmdr_fs::sqlite_util::open(&old).expect("open");
        conn.execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO meta VALUES ('schema_version', '3');
             CREATE TABLE weights (path_folded TEXT PRIMARY KEY, path TEXT NOT NULL, score REAL NOT NULL,
                 signals TEXT NOT NULL, as_of_generation INTEGER NOT NULL);
             CREATE TABLE visits (path_folded TEXT PRIMARY KEY, path TEXT NOT NULL,
                 visit_count INTEGER NOT NULL DEFAULT 0, last_visit_secs INTEGER NOT NULL DEFAULT 0);
             INSERT INTO visits VALUES ('/users/me', '/Users/me', 4, 40);",
        )
        .expect("an older build's combined store");
    }

    Index::builder()
        .config(crate::host::config::IndexConfig {
            data_dir: data.clone(),
            drive_index_dir: cache.clone(),
            ..crate::host::config::IndexConfig::default()
        })
        .build()
        .expect("build");
    // It runs off the building thread; whatever needs the visits waits for it.
    crate::importance::store::wait_for_background_split();

    assert!(!old.exists(), "the combined store left the data dir");
    let visits: (String, i64) = cmdr_fs::sqlite_util::open(&data.join("visits-root.db"))
        .expect("visits")
        .query_row("SELECT path, visit_count FROM visits", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the visit came over");
    assert_eq!(visits, ("/Users/me".to_string(), 4));
    assert_eq!(
        crate::importance::store::importance_db_path(&data, "root"),
        cache.join("importance-root.db"),
        "the weights now live beside the drive index"
    );
}

/// Three files and a subdirectory on a volume that exists only in memory.
fn in_memory_share(root: &str) -> Arc<dyn Volume> {
    let entries = vec![
        FileEntry::new("sub".into(), format!("{root}/sub"), true, false),
        FileEntry {
            size: Some(11),
            ..FileEntry::new("a.txt".into(), format!("{root}/a.txt"), false, false)
        },
        FileEntry {
            size: Some(22),
            ..FileEntry::new("b.txt".into(), format!("{root}/b.txt"), false, false)
        },
        FileEntry {
            size: Some(33),
            ..FileEntry::new("c.txt".into(), format!("{root}/sub/c.txt"), false, false)
        },
    ];
    Arc::new(
        InMemoryVolume::with_entries("Acceptance", entries)
            .with_root(root)
            .with_connection_state(ConnectionState::Direct)
            .with_backend_kind(cmdr_fs::volume::BackendKind::Smb),
    )
}

/// A phone over ADB, rooted where the app roots one (`adb://<serial>`), holding a
/// folder and two files.
fn in_memory_phone(root: &str) -> Arc<dyn Volume> {
    let entries = vec![
        FileEntry::new("sdcard".into(), format!("{root}/sdcard"), true, false),
        FileEntry {
            size: Some(11),
            ..FileEntry::new("a.jpg".into(), format!("{root}/sdcard/a.jpg"), false, false)
        },
        FileEntry {
            size: Some(22),
            ..FileEntry::new("b.jpg".into(), format!("{root}/sdcard/b.jpg"), false, false)
        },
    ];
    Arc::new(
        InMemoryVolume::with_entries("Phone", entries)
            .with_root(root)
            .with_backend_kind(cmdr_fs::volume::BackendKind::Adb),
    )
}

/// A phone's finished scan is honest about what it can't see: ADB reports no
/// change made on the phone itself, so the index lands Stale with its completion
/// recorded, never Fresh. `is_fresh` is what the operation log trusts an index on,
/// so a Fresh here would be a claim nothing backs.
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the process-wide seams for the whole scan; holding it across the awaits IS the point"
)]
async fn a_phone_over_adb_finishes_its_scan_stale_because_nothing_watches_it() {
    use crate::indexing::lifecycle::freshness::Freshness;

    let _serialized = crate::indexing::handle::test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let serial = "ACCEPTANCE1";
    let root = cmdr_fs::volume::adb_app_root(serial);
    let volume_id = cmdr_fs::volume::adb_volume_id(serial);

    let volumes = FakeVolumeProvider::shared();
    volumes.register(&volume_id, in_memory_phone(&root));

    let events = Arc::new(RecordingSink::new());
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .events(Arc::clone(&events) as Arc<dyn EventSink>)
        .install_for_test();

    assert_eq!(
        index.start_volume(&volume_id).await.expect("the phone starts indexing"),
        StartOutcome::Started
    );
    wait_until_async(Duration::from_secs(20), "the phone's scan to settle", || {
        matches!(
            index.volume_status(&volume_id).freshness,
            Some(Freshness::Fresh | Freshness::Stale)
        )
    })
    .await;

    let status = index.volume_status(&volume_id);
    assert_eq!(
        status.freshness,
        Some(Freshness::Stale),
        "nothing watches a phone over ADB"
    );
    assert!(status.scan_completed_at.is_some(), "and yet its scan did finish");
    assert!(!index.is_fresh(&volume_id), "so nothing may trust it as current");
    assert!(
        !status.live_watch,
        "and its status says nothing watches it, so the stale copy doesn't blame a disconnect"
    );
    let children = index
        .list_children(&volume_id, &format!("{root}/sdcard"))
        .expect("the scanned folder is readable")
        .expect("the scanned folder is in the index");
    assert_eq!(children.len(), 2, "the walk read both files: {children:?}");
}

/// ❗ A rescan of a drive with no manager running has to WALK. Starting it isn't
/// enough: a trait-scanned volume with a finished index on disk resumes into Stale
/// and waits for somebody to ask for a rescan, and this IS that somebody. Without
/// the walk, `indexing rescan` answered OK ("status now stale") having read nothing.
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the process-wide seams for the whole scan; holding it across the awaits IS the point"
)]
async fn a_rescan_of_a_volume_with_no_running_manager_walks_it() {
    let _serialized = crate::indexing::handle::test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let serial = "RESCANSTOPPED1";
    let root = cmdr_fs::volume::adb_app_root(serial);
    let volume_id = cmdr_fs::volume::adb_volume_id(serial);

    let volumes = FakeVolumeProvider::shared();
    volumes.register(&volume_id, in_memory_phone(&root));

    let events = Arc::new(RecordingSink::new());
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .events(Arc::clone(&events) as Arc<dyn EventSink>)
        .install_for_test();
    let scans = || {
        events
            .kinds_for(&volume_id)
            .iter()
            .filter(|kind| **kind == IndexEventKind::ScanComplete)
            .count()
    };

    index.start_volume(&volume_id).await.expect("the phone starts indexing");
    wait_until_async(Duration::from_secs(20), "the first scan to finish", || scans() == 1).await;

    index.disable_volume(&volume_id).expect("the phone stops indexing");

    assert_eq!(
        index.rescan_volume(&volume_id).await.expect("the rescan starts"),
        StartOutcome::Started
    );
    wait_until_async(Duration::from_secs(20), "the rescan's walk to finish", || scans() == 2).await;
}

/// The acceptance test for the whole extraction: a full scan, start to finish,
/// driven through the public handle over an `InMemoryVolume`, reporting into a
/// `RecordingSink`. Nothing app-side is in the room — no `AppHandle`, no
/// `VolumeManager`, no Tauri event, no real disk beneath the volume.
///
/// If a hidden global ever starts carrying something the handle should, this is
/// what fails: the scan would either not start or report into the wrong sink.
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the process-wide seams for the whole scan; holding it across the awaits IS the point"
)]
async fn a_handle_scans_an_in_memory_volume_and_reports_to_its_own_sink() {
    let _serialized = crate::indexing::handle::test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    // A platform-appropriate mount root. Read routing sends a path to a per-mount
    // index only when it sits under an external-mount prefix, and those differ per
    // OS (`/Volumes/` on macOS, `/mnt/` and `/media/` on Linux). A hardcoded
    // `/Volumes/…` routes back to `root`'s index on Linux, where nothing scanned it.
    #[cfg(target_os = "macos")]
    let root = "/Volumes/acceptance";
    #[cfg(not(target_os = "macos"))]
    let root = "/media/acceptance";
    let volume_id = "smb-acceptance-test";

    let volumes = FakeVolumeProvider::shared();
    volumes.register(volume_id, in_memory_share(root)).mark_network(root);

    let events = Arc::new(RecordingSink::new());
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .events(Arc::clone(&events) as Arc<dyn EventSink>)
        .install_for_test();

    assert_eq!(
        index.start_volume(volume_id).await.expect("the share starts indexing"),
        StartOutcome::Started
    );

    wait_until_async(
        Duration::from_secs(20),
        "the in-memory share's scan to complete",
        || events.kinds_for(volume_id).contains(&IndexEventKind::ScanComplete),
    )
    .await;

    let kinds = events.kinds_for(volume_id);
    let started_at = kinds
        .iter()
        .position(|k| *k == IndexEventKind::ScanStarted)
        .expect("a scan announces itself");
    let complete_at = kinds
        .iter()
        .position(|k| *k == IndexEventKind::ScanComplete)
        .expect("a completed scan reports ScanComplete");
    assert!(
        started_at < complete_at,
        "ScanComplete must follow ScanStarted, not lead it: {kinds:?}"
    );
    for event in events.events() {
        if let Some(vid) = event.volume_id() {
            assert_eq!(vid, volume_id, "the scan reported under the wrong volume: {event:?}");
        }
    }

    // And the index can answer for what it just walked, through the handle only.
    let children = index
        .list_children(volume_id, root)
        .expect("the scanned root is readable")
        .expect("the scanned root is in the index");
    let mut names: Vec<&str> = children.iter().map(|row| row.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["a.txt", "b.txt", "sub"], "the walk found the share's entries");

    // And it knows there is nothing left to walk. This is the end-to-end proof for
    // coverage: a real scan-start sequence stamped the exclusion policy (without it
    // NOTHING reads as covered), a real walk marked its directories listed, and the
    // descent rule reads both back. A frontier here would mean a search over this
    // freshly indexed share would re-walk it.
    let coverage = index
        .coverage(volume_id, root, CoverageDimension::Listing)
        .expect("the scanned share answers for its own coverage");
    assert!(
        coverage.frontier.is_empty() && coverage.permission_denied.is_empty() && coverage.declined.is_empty(),
        "a completed walk leaves nothing to cover: {coverage:?}"
    );
    assert_eq!(
        coverage.token,
        index.coverage_token(volume_id),
        "the token a coverage answer carries is the one the volume reports"
    );
    assert_ne!(
        coverage.token,
        CoverageToken::UNINDEXED,
        "an indexed volume must not report the no-index token"
    );

    index.forget_volume(volume_id).expect("tear the test index down");
}

/// A saved share that isn't connected has no instance, but its index is on disk,
/// and that file is the only place its last root and the date of what it holds
/// live. `status` reads them off it without standing anything up: the volume
/// still reads as not indexed, so the "registered is not indexed" trap stays shut.
#[test]
fn a_stopped_volumes_status_carries_its_last_scan_off_disk_without_registering_it() {
    use crate::indexing::store::IndexStore;

    let _serialized = crate::indexing::handle::test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed) = Index::builder().data_dir(data.path()).install_for_test();
    let volume_id = "smb-192-0-2-60-445-testshare";
    let db_path = data.path().join(format!("index-{volume_id}.db"));
    let store = IndexStore::open(&db_path).expect("write the index a past scan left");
    let conn = IndexStore::open_write_connection(&db_path).expect("write connection");
    IndexStore::update_meta(&conn, "volume_path", "/Volumes/testshare").expect("root");
    IndexStore::update_meta(&conn, "scan_completed_at", "1761400000").expect("stamp");
    drop((conn, store));

    let status = index.status(volume_id).expect("status answers");
    let last = status.index_status.expect("the last scan's facts, off disk");
    assert_eq!(last.volume_path.as_deref(), Some("/Volumes/testshare"));
    assert_eq!(last.scan_completed_at.as_deref(), Some("1761400000"));
    assert!(!status.initialized, "nothing was stood up to answer");
    assert!(
        !index.volume_status(volume_id).enabled,
        "and it still reads as not indexed"
    );
    assert!(index.read_pool(volume_id).is_none(), "no read pool was installed");
}

/// The database a past walk of a share left behind, rooted at `/Volumes/testshare`:
/// `documents/invoices` listed, holding one file, with its recursive totals;
/// `documents/unlisted` holding a row nothing listed. Returns the share's id.
fn a_stopped_share_on_disk(data: &std::path::Path) -> &'static str {
    use crate::indexing::store::{DirStatsById, IndexStore, ROOT_ID};

    let volume_id = "smb-192-0-2-61-445-testshare";
    let db_path = data.join(format!("index-{volume_id}.db"));
    let store = IndexStore::open(&db_path).expect("write the index a past scan left");
    let conn = IndexStore::open_write_connection(&db_path).expect("write connection");
    IndexStore::update_meta(&conn, "volume_path", "/Volumes/testshare").expect("root");
    let dir = |parent, name| {
        IndexStore::insert_entry_v2(&conn, parent, name, true, false, None, None, None, None).expect("insert dir")
    };
    let papers = dir(ROOT_ID, "documents");
    let invoices = dir(papers, "invoices");
    let unlisted = dir(papers, "unlisted");
    IndexStore::insert_entry_v2(
        &conn,
        invoices,
        "hetzner.pdf",
        false,
        false,
        Some(10),
        Some(10),
        None,
        None,
    )
    .expect("insert file");
    IndexStore::insert_entry_v2(&conn, unlisted, "seen.txt", false, false, Some(1), Some(1), None, None)
        .expect("insert file");
    IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, papers, invoices], 1).expect("mark listed");
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[DirStatsById {
            entry_id: invoices,
            recursive_logical_size: 10,
            recursive_physical_size: 10,
            recursive_file_count: 1,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 1,
        }],
    )
    .expect("stats");
    drop((conn, store));
    volume_id
}

/// A share that isn't mounted has no instance, so its folders answer from the
/// database its last walk left: rows, totals, and the same "not listed" honesty
/// a live index has. Nothing is stood up to answer.
#[test]
fn a_stopped_volume_lists_its_folders_from_its_database_on_disk() {
    let _serialized = crate::indexing::handle::test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed) = Index::builder().data_dir(data.path()).install_for_test();
    let volume_id = a_stopped_share_on_disk(data.path());

    let children = index
        .list_children(volume_id, "/Volumes/testshare/documents/invoices")
        .expect("the stored index reads")
        .expect("its last walk listed the folder");
    let names: Vec<&str> = children.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, ["hetzner.pdf"]);
    assert!(
        index
            .list_children(volume_id, "/Volumes/testshare/documents/unlisted")
            .expect("reads")
            .is_none(),
        "a folder nothing listed has no contents to report, offline too"
    );
    assert!(
        index
            .list_children(volume_id, "/Volumes/other/documents")
            .expect("reads")
            .is_none(),
        "a path outside the root the walk recorded isn't on this volume"
    );

    let stats = index
        .dir_stats(volume_id, "/Volumes/testshare/documents/invoices")
        .expect("reads")
        .expect("the folder's totals are on disk");
    assert_eq!(stats.recursive_size, 10);
    assert!(stats.recursive_size_complete);
    let batch = index
        .dir_stats_batch(
            volume_id,
            &[
                "/Volumes/testshare/documents/invoices".to_string(),
                "/Volumes/testshare/documents/unlisted".to_string(),
            ],
        )
        .expect("reads");
    assert_eq!(batch.iter().map(Option::is_some).collect::<Vec<_>>(), [true, false]);

    assert!(index.read_pool(volume_id).is_none(), "no read pool was installed");
    assert!(
        !index.volume_status(volume_id).enabled,
        "and it still reads as not indexed"
    );
}

/// A MOUNTED volume whose indexing is off keeps its old database, and that file
/// must never answer for it: a pane's sizes would come from a walk the user
/// turned off. Only a volume the host doesn't serve reads its stored index.
#[test]
fn a_mounted_volume_with_no_index_running_never_reads_its_leftover_database() {
    let _serialized = crate::indexing::handle::test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let volumes = FakeVolumeProvider::shared();
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .install_for_test();
    let volume_id = a_stopped_share_on_disk(data.path());
    volumes.register(volume_id, in_memory_share("/Volumes/testshare"));

    let path = "/Volumes/testshare/documents/invoices";
    assert!(index.list_children(volume_id, path).expect("reads").is_none());
    assert!(index.dir_stats(volume_id, path).expect("reads").is_none());
}
