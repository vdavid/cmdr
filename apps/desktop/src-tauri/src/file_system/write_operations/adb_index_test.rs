//! A phone's drive index, end to end, against `cmdr_adb::testing::FakeAdbServer`:
//! the walk, a Cmdr copy and delete patching the index, an unplug mid-walk, and
//! another phone's path kept out.
//!
//! ❗ **The phone is dialed through the app** (`adb_transfer_test::dialed_phone_with`),
//! ❌ never `cmdr_adb::volume::testing::connect_fake`. A phone has no watcher, so
//! the only thing that keeps its index honest after a write is the patch the
//! volume reports to its listing host, and only the app's host forwards it to the
//! index (`caching::notify_directory_changed` → `Index::apply_directory_change`).
//! A recording host would pass every cell here without proving that hop.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_adb::testing::{FakeAdbServer, FakeTree};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{DirectoryChange, adb_app_root};
use cmdr_index::testing::host::test_lock;
use cmdr_index::{Freshness, Index, StartOutcome};

use super::adb_transfer_test::{dialed_phone_with, registered_local};
use super::event_sinks::CollectorEventSink;
use super::network_transfer_test_support::start_copy_by_id;
use super::state::WriteOperationState;
use super::types::{VolumeCopyConfig, WriteOperationConfig};
use crate::adb::device_provider::apply_device_list;
use crate::file_system::index_provider::AppVolumeProvider;
use crate::test_support::wait_until_async;

/// A backstop for a whole walk of the fake phone, far above the fraction of a
/// second it takes on loopback.
const WALK_BUDGET: Duration = Duration::from_secs(20);

/// A backstop for one patch to reach the index's writer and commit.
const PATCH_BUDGET: Duration = Duration::from_secs(10);

// ── Fixtures ─────────────────────────────────────────────────────────

/// The index, wired to the app's own volume registry the way `index_host::install`
/// wires it, over a data directory of the cell's own.
fn install_index(data: &Path) -> (Index, impl Sized) {
    Index::builder()
        .data_dir(data)
        .volumes(Arc::new(AppVolumeProvider))
        .install_for_test()
}

/// A phone with two photos and a note on its shared storage.
fn a_phone_with_photos() -> FakeTree {
    let mut tree = FakeTree::new();
    tree.add_file("/sdcard/DCIM/Camera/IMG_0001.jpg", &[7; 11])
        .add_file("/sdcard/DCIM/Camera/IMG_0002.jpg", &[7; 22])
        .add_file("/sdcard/Download/notes.txt", b"hello");
    tree
}

/// Turns indexing on the way the badge menu does and waits out the first walk,
/// which on a phone ends Stale with its completion recorded.
async fn index_it(index: &Index, volume_id: &str) {
    assert_eq!(
        index.start_volume(volume_id).await.expect("the enable is not an error"),
        StartOutcome::Started,
        "a dialed phone over ADB starts indexing"
    );
    wait_until_async(WALK_BUDGET, "the phone's first walk to finish", || {
        let status = index.volume_status(volume_id);
        status.scan_completed_at.is_some() && status.freshness == Some(Freshness::Stale)
    })
    .await;
}

/// The names the index holds under `dir`, sorted; empty when it holds none.
fn names_under(index: &Index, dir: &str) -> Vec<String> {
    let mut names: Vec<String> = index
        .list_children(dir)
        .ok()
        .flatten()
        .map(|rows| rows.into_iter().map(|row| row.name).collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// The recursive logical size the index holds for `dir`, if it holds one.
fn size_under(index: &Index, dir: &str) -> Option<u64> {
    index.dir_stats(dir).ok().flatten().map(|stats| stats.recursive_size)
}

/// How many sync sessions the phone has been asked to open so far.
fn sync_sessions(fake: &FakeAdbServer) -> usize {
    fake.requests().iter().filter(|request| *request == "sync:").count()
}

/// A file entry for a change a listing host would report.
fn a_file(parent: &str, name: &str) -> FileEntry {
    FileEntry {
        size: Some(1),
        ..FileEntry::new(name.to_string(), format!("{parent}/{name}"), false, false)
    }
}

// ── The walk ─────────────────────────────────────────────────────────

/// Turning indexing on for a phone walks its whole tree: every file gets a row,
/// every folder a recursive size, and the phone's paths route to its own index.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the index's process-wide seams for the whole cell; holding it across the awaits IS the point"
)]
async fn turning_indexing_on_for_a_phone_indexes_every_file_on_it() {
    let _serialized = test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed) = install_index(data.path());
    let serial = "R58M-Index-Walk";
    let phone = dialed_phone_with(serial, "index-walk", a_phone_with_photos()).await;

    index_it(&index, &phone.volume_id).await;

    let root = adb_app_root(serial);
    let camera = format!("{root}/sdcard/DCIM/Camera");
    assert_eq!(names_under(&index, &camera), vec!["IMG_0001.jpg", "IMG_0002.jpg"]);
    assert_eq!(
        names_under(&index, &format!("{root}/sdcard/Download")),
        vec!["notes.txt"]
    );
    wait_until_async(PATCH_BUDGET, "the phone's folder sizes to roll up", || {
        size_under(&index, &format!("{root}/sdcard/DCIM")) == Some(33)
    })
    .await;
    assert_eq!(
        index.volume_id_for_path(&camera),
        phone.volume_id,
        "the phone's paths route to its own index, never the Mac's boot disk"
    );

    let _ = index.forget_volume(&phone.volume_id);
}

// ── Cmdr's own writes ────────────────────────────────────────────────

/// Nothing watches a phone, so a Cmdr write reaches its index only through the
/// patch the volume reports. A copy onto it lands as one row under its own name,
/// with no row left behind for the staging names it passed through, and a delete
/// takes that row and its bytes back out.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the index's process-wide seams for the whole cell; holding it across the awaits IS the point"
)]
async fn a_cmdr_copy_onto_a_phone_and_a_delete_on_it_patch_its_index() {
    let _serialized = test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed) = install_index(data.path());
    let phone = dialed_phone_with("R58M-Index-Patch", "index-patch", a_phone_with_photos()).await;
    index_it(&index, &phone.volume_id).await;
    let scratch = phone.dir.to_string_lossy().into_owned();
    assert!(
        names_under(&index, &scratch).is_empty(),
        "precondition: the scratch folder is indexed and empty"
    );

    let local = registered_local("adb_index_copy_onto_phone");
    std::fs::write(local.dir.join("uploaded.bin"), vec![1_u8; 4096]).expect("seeding the local file");
    let running = start_copy_by_id(
        "copy-onto-an-indexed-phone",
        local.volume_id.clone(),
        vec![PathBuf::from("uploaded.bin")],
        phone.volume_id.clone(),
        scratch.clone(),
        VolumeCopyConfig::default(),
    )
    .await;
    running.settle().await;
    running.assert_no_errors();

    wait_until_async(PATCH_BUDGET, "the copy's patch to reach the phone's index", || {
        size_under(&index, &scratch) == Some(4096)
    })
    .await;
    assert_eq!(
        names_under(&index, &scratch),
        vec!["uploaded.bin"],
        "the copy is one row under its own name, and no staging name it passed through is left as a row"
    );

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    super::delete_volume_files_for_test(
        Arc::clone(&phone.volume),
        &phone.volume_id,
        &events,
        "adb-index-delete",
        &state,
        &[phone.dir.join("uploaded.bin")],
        &WriteOperationConfig::default(),
    )
    .await
    .expect("the delete on the phone");

    wait_until_async(PATCH_BUDGET, "the delete's patch to reach the phone's index", || {
        names_under(&index, &scratch).is_empty()
    })
    .await;
    wait_until_async(PATCH_BUDGET, "the delete's bytes to leave the folder's size", || {
        size_under(&index, &scratch) == Some(0)
    })
    .await;

    let _ = index.forget_volume(&phone.volume_id);
}

// ── An unplug mid-walk ───────────────────────────────────────────────

/// A cable pulled while the walk waits on the phone stops the walk and leaves
/// an honest status: the index stays (with whatever it had read), reads Stale,
/// and never claims the walk finished.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the index's process-wide seams for the whole cell; holding it across the awaits IS the point"
)]
async fn a_phone_unplugged_mid_walk_stops_the_walk_and_reads_stale() {
    let _serialized = test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed) = install_index(data.path());
    let mut tree = a_phone_with_photos();
    for folder in 0..20 {
        tree.add_file(&format!("/sdcard/Album{folder:02}/photo.jpg"), b"x");
    }
    let phone = dialed_phone_with("R58M-Index-Unplug", "index-unplug", tree).await;

    // Park every answer, so the walk is provably waiting on the phone when the
    // cable comes out. Nothing before the walk talks to the phone: the scan's
    // disk-usage probe asks the Mac about an `adb://` path and gets nothing.
    let sessions_before = sync_sessions(&phone.fake);
    phone.fake.hold_answers();
    assert_eq!(
        index
            .start_volume(&phone.volume_id)
            .await
            .expect("the enable is not an error"),
        StartOutcome::Started
    );
    wait_until_async(WALK_BUDGET, "the walk to ask the phone for a listing", || {
        sync_sessions(&phone.fake) > sessions_before
    })
    .await;

    // The cable comes out: the server stops listing the phone, the tracker's
    // callback retires its volume, and every socket to it dies.
    phone.fake.push_devices(Vec::new());
    apply_device_list(Vec::new());
    phone.fake.drop_connections();
    phone.fake.release_answers();

    wait_until_async(WALK_BUDGET, "the unplugged walk to stop", || {
        index.volume_status(&phone.volume_id).freshness != Some(Freshness::Scanning)
    })
    .await;
    let status = index.volume_status(&phone.volume_id);
    assert_eq!(
        status.freshness,
        Some(Freshness::Stale),
        "a walk the unplug cut short is honest about it"
    );
    assert!(status.enabled, "the phone keeps its index and what the walk had read");
    assert!(status.scan_completed_at.is_none(), "and never claims the walk finished");

    let _ = index.forget_volume(&phone.volume_id);
}

// ── Another phone's paths ────────────────────────────────────────────

/// A change reported under another phone's path never lands in this phone's
/// index, even when that phone's serial merely extends this one's. A change under
/// this phone's own path does land, which is also what proves the writer had the
/// other two in hand before the assertion reads.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the index's process-wide seams for the whole cell; holding it across the awaits IS the point"
)]
async fn a_change_under_another_phones_path_never_lands_in_this_phones_index() {
    let _serialized = test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed) = install_index(data.path());
    let serial = "R58M-Index-Own";
    let phone = dialed_phone_with(serial, "index-own", a_phone_with_photos()).await;
    index_it(&index, &phone.volume_id).await;

    let own_sdcard = format!("{}/sdcard", adb_app_root(serial));
    let other_sdcard = format!("{}/sdcard", adb_app_root("R58M-Index-Other"));
    let lookalike_sdcard = format!("{}/sdcard", adb_app_root("R58M-Index-Own0"));
    // The hop a listing host takes, with the volume id a phone's patch carries.
    let index_host = crate::index_host::index();
    index_host.apply_directory_change(
        &phone.volume_id,
        Path::new(&other_sdcard),
        &DirectoryChange::Added(a_file(&other_sdcard, "from-another-phone.txt")),
    );
    index_host.apply_directory_change(
        &phone.volume_id,
        Path::new(&lookalike_sdcard),
        &DirectoryChange::Added(a_file(&lookalike_sdcard, "from-a-lookalike-serial.txt")),
    );
    index_host.apply_directory_change(
        &phone.volume_id,
        Path::new(&own_sdcard),
        &DirectoryChange::Added(a_file(&own_sdcard, "from-this-phone.txt")),
    );

    wait_until_async(PATCH_BUDGET, "this phone's own change to land", || {
        names_under(&index, &own_sdcard).contains(&"from-this-phone.txt".to_string())
    })
    .await;
    let names = names_under(&index, &own_sdcard);
    assert!(
        !names.contains(&"from-another-phone.txt".to_string()),
        "another phone's path must never land here: {names:?}"
    );
    assert!(
        !names.contains(&"from-a-lookalike-serial.txt".to_string()),
        "a serial that merely extends this one's is another phone: {names:?}"
    );

    let _ = index.forget_volume(&phone.volume_id);
}
