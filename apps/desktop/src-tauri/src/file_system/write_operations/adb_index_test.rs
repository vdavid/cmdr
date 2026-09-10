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
/// second it takes on loopback, and under nextest's slow-test cutoff so a stuck
/// wait fails with its own message rather than a bare timeout.
const WALK_BUDGET: Duration = Duration::from_secs(6);

/// A backstop for one patch to reach the index's writer and commit.
const PATCH_BUDGET: Duration = Duration::from_secs(5);

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

/// How many requests the fake server has received so far. ❗ Every request, not
/// only `sync:`: with answers held, a new socket parks on its first request
/// (`host:transport:<serial>`) and never gets to ask for the sync service.
fn requests_seen(fake: &FakeAdbServer) -> usize {
    fake.requests().len()
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

    wait_until_async(PATCH_BUDGET, "the copied file's row to reach the phone's index", || {
        names_under(&index, &scratch).contains(&"uploaded.bin".to_string())
    })
    .await;
    assert_eq!(
        names_under(&index, &scratch),
        vec!["uploaded.bin"],
        "the copy is one row under its own name, and no staging name it passed through is left as a row"
    );
    wait_until_async(PATCH_BUDGET, "the copied bytes to reach the folder's size", || {
        size_under(&index, &scratch) == Some(4096)
    })
    .await;

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
    wait_until_async(PATCH_BUDGET, "the deleted bytes to leave the folder's size", || {
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
    let requests_before = requests_seen(&phone.fake);
    phone.fake.hold_answers();
    assert_eq!(
        index
            .start_volume(&phone.volume_id)
            .await
            .expect("the enable is not an error"),
        StartOutcome::Started
    );
    wait_until_async(WALK_BUDGET, "the walk to ask the phone for a listing", || {
        requests_seen(&phone.fake) > requests_before
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

// ── What a phone's walk covers ───────────────────────────────────────

/// A phone's walk indexes its storage once, under the `/sdcard` spelling the pane
/// browses, plus its SD cards, and never descends the rest of the device. The
/// kernel's views, a `/sys` symlink loop, private data, and the second paths
/// Android mounts onto the same storage each keep a row with nothing under it, and a
/// link inside storage stays the one row it is.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the index's process-wide seams for the whole cell; holding it across the awaits IS the point"
)]
async fn a_phone_walk_indexes_its_storage_once_and_never_its_system_trees() {
    let _serialized = test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed) = install_index(data.path());
    let serial = "R58M-Index-Layout";
    let mut tree = FakeTree::android_layout();
    tree.add_file("/storage/emulated/0/DCIM/Camera/IMG_0001.jpg", &[7; 11])
        .add_symlink("/storage/emulated/0/DCIM/again", "/sdcard/DCIM")
        .add_file("/storage/emulated/0/Download/notes.txt", b"hello")
        .add_file("/storage/1234-5678/Music/song.mp3", &[3; 7])
        .add_file("/proc/1/status", b"Name: init")
        .add_symlink("/proc/self", "/proc/1")
        .add_file("/sys/devices/soc/uevent", b"DRIVER=soc")
        .add_symlink("/sys/devices/soc/subsystem", "/sys/class/soc")
        .add_symlink("/sys/class/soc", "/sys/devices/soc")
        .add_file("/mnt/user/0/emulated/0/DCIM/Camera/IMG_0001.jpg", &[7; 11])
        .add_file("/data/local/tmp/scratch.bin", &[1; 64]);
    let fake = crate::adb::test_support::a_listed_phone(serial, tree).await;
    let (volume_id, _volume) = crate::adb::test_support::dial(&fake, serial, "adb-index-layout").await;

    index_it(&index, &volume_id).await;

    let root = adb_app_root(serial);
    let at = |device: &str| format!("{root}{device}");
    let at_root = names_under(&index, &root);
    for kept_out in ["/proc", "/sys", "/mnt", "/data", "/storage/emulated", "/storage/self"] {
        assert!(
            names_under(&index, &at(kept_out)).is_empty(),
            "{kept_out} keeps its row and nothing beneath it"
        );
    }
    for listed in ["proc", "sys", "mnt", "data", "sdcard", "storage"] {
        assert!(
            at_root.contains(&listed.to_string()),
            "{listed} still lists at the root: {at_root:?}"
        );
    }
    assert_eq!(names_under(&index, &at("/sdcard/DCIM/Camera")), vec!["IMG_0001.jpg"]);
    assert!(
        names_under(&index, &at("/sdcard/DCIM/again")).is_empty(),
        "a link inside storage is the one row it is"
    );
    assert_eq!(names_under(&index, &at("/storage/1234-5678/Music")), vec!["song.mp3"]);
    wait_until_async(PATCH_BUDGET, "the folder sizes the pane shows to roll up", || {
        size_under(&index, &at("/sdcard/DCIM")) == Some(11)
    })
    .await;
    wait_until_async(
        PATCH_BUDGET,
        "the phone's total to count each file once: the photo, the note, and the song",
        || size_under(&index, &root) == Some(11 + 5 + 7),
    )
    .await;

    let _ = index.forget_volume(&volume_id);
    crate::adb::test_support::retire_phone(serial);
    apply_device_list(Vec::new());
}

/// A change Cmdr reports under a tree the phone's walk keeps out never lands, so a
/// live patch can't put back rows no walk produces. The phone's total is what
/// shows it: an upsert under `/data` would add its byte there even though `/data`
/// lists nothing, while the change under storage, sent second, does land.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the index's process-wide seams for the whole cell; holding it across the awaits IS the point"
)]
async fn a_change_under_a_tree_the_walk_keeps_out_never_lands() {
    let _serialized = test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed) = install_index(data.path());
    let serial = "R58M-Index-Kept-Out";
    let mut tree = FakeTree::android_layout();
    tree.add_file("/storage/emulated/0/Download/notes.txt", b"hello")
        .add_file("/data/local/tmp/scratch.bin", &[1; 64]);
    let fake = crate::adb::test_support::a_listed_phone(serial, tree).await;
    let (volume_id, _volume) = crate::adb::test_support::dial(&fake, serial, "adb-index-kept-out").await;
    index_it(&index, &volume_id).await;

    let root = adb_app_root(serial);
    let data_dir = format!("{root}/data");
    let download = format!("{root}/sdcard/Download");
    // The hop a listing host takes, with the volume id a phone's patch carries.
    let index_host = crate::index_host::index();
    index_host.apply_directory_change(
        &volume_id,
        Path::new(&data_dir),
        &DirectoryChange::Added(a_file(&data_dir, "pushed.bin")),
    );
    index_host.apply_directory_change(
        &volume_id,
        Path::new(&download),
        &DirectoryChange::Added(a_file(&download, "copied.txt")),
    );

    wait_until_async(PATCH_BUDGET, "the change under storage to land", || {
        names_under(&index, &download).contains(&"copied.txt".to_string())
    })
    .await;
    // The writer applies changes in order, so the `/data` one is settled by now.
    wait_until_async(
        PATCH_BUDGET,
        "the phone's total to hold the note and the copied byte, and nothing pushed under /data",
        || size_under(&index, &root) == Some(5 + 1),
    )
    .await;

    let _ = index.forget_volume(&volume_id);
    crate::adb::test_support::retire_phone(serial);
    apply_device_list(Vec::new());
}
