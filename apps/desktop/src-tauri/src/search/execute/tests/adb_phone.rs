//! Searching a phone over ADB, end to end against `cmdr_adb::testing::FakeAdbServer`.
//!
//! A search started from a phone pane carries an `adb://<serial>/…` scope. It has
//! to reach THAT phone's index, never the Mac's boot disk, and every row has to
//! come back as an app path a pane can open. A phone nobody indexed is walked
//! live, the way any drive is.
//!
//! ❗ The phone is dialed through the app (`crate::adb::test_support`), so its
//! index is built by the same transport a user's enable starts, and the walk
//! reads the phone over the same socket protocol a real one speaks.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cmdr_adb::testing::{FakeAdbServer, FakeTree};
use cmdr_fs::volume::adb_app_root;
use cmdr_index::testing::host::test_lock;
use cmdr_index::{Freshness, Index, StartOutcome};

use super::live_drive::pattern_query_for;
use super::*;
use crate::adb::device_provider::apply_device_list;
use crate::adb::test_support::{a_listed_phone, dial, retire_phone};
use crate::file_system::index_provider::AppVolumeProvider;
use crate::search::live;
use crate::test_support::wait_until_async;

/// A backstop for a whole walk of the fake phone: far above the fraction of a
/// second it takes on loopback, and under nextest's slow-test cutoff.
const WALK_BUDGET: Duration = Duration::from_secs(6);

/// A phone with two photos and a note on its shared storage.
fn a_phone_with_photos() -> FakeTree {
    let mut tree = FakeTree::new();
    tree.add_file("/sdcard/DCIM/Camera/IMG_0001.jpg", &[7; 11])
        .add_file("/sdcard/DCIM/Camera/IMG_0002.jpg", &[7; 22])
        .add_file("/sdcard/Download/notes.txt", b"hello");
    tree
}

/// A phone the app has dialed, retired on drop.
struct Phone {
    _fake: FakeAdbServer,
    serial: &'static str,
    volume_id: String,
}

impl Drop for Phone {
    fn drop(&mut self) {
        retire_phone(self.serial);
        apply_device_list(Vec::new());
    }
}

async fn dialed_phone(serial: &'static str) -> Phone {
    let fake = a_listed_phone(serial, a_phone_with_photos()).await;
    let (volume_id, _volume) = dial(&fake, serial, &format!("search-{serial}")).await;
    Phone {
        _fake: fake,
        serial,
        volume_id,
    }
}

/// The index, wired to the app's own volume registry, over a data directory the
/// search reads too.
fn install_index(data: &Path) -> (Index, impl Sized, impl Sized) {
    let (index, installed) = Index::builder()
        .data_dir(data)
        .volumes(Arc::new(AppVolumeProvider))
        .install_for_test();
    (index, installed, volumes::install_data_dir_for_test(data))
}

/// The paths a search answered with, sorted.
fn paths_of(entries: &[SearchResultEntry]) -> Vec<String> {
    let mut paths: Vec<String> = entries.iter().map(|entry| entry.path.clone()).collect();
    paths.sort();
    paths
}

/// A phone the user indexed answers a search from a phone pane out of its own
/// index, on both paths a search takes: the index-only answer and the live run
/// the dialog and MCP use. The rows are `adb://` paths, so a pane can open them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the index's process-wide seams for the whole cell; holding it across the awaits IS the point"
)]
async fn a_search_scoped_to_an_indexed_phone_finds_its_files_as_phone_paths() {
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed, _search_data) = install_index(data.path());
    let serial = "R58M-Search-Indexed";
    let phone = dialed_phone(serial).await;

    assert_eq!(
        index.start_volume(&phone.volume_id).await.expect("the enable"),
        StartOutcome::Started
    );
    wait_until_async(WALK_BUDGET, "the phone's first walk to finish", || {
        let status = index.volume_status(&phone.volume_id);
        status.scan_completed_at.is_some() && status.freshness == Some(Freshness::Stale)
    })
    .await;

    let sdcard = format!("{}/sdcard", adb_app_root(serial));
    let expected = vec![format!("{sdcard}/DCIM/Camera/IMG_0001.jpg")];

    let query = pattern_query_for(&sdcard, "IMG_0001");
    let result = tokio::task::spawn_blocking(move || run_blocking(query))
        .await
        .expect("the search thread")
        .expect("the search");
    assert_eq!(
        result.target_volume_id, phone.volume_id,
        "the phone's own index answers"
    );
    assert!(result.uncovered_scopes.is_empty(), "an indexed phone is covered");
    assert_eq!(
        paths_of(&result.entries),
        expected,
        "rows are phone paths a pane can open"
    );

    let query = pattern_query_for(&sdcard, "IMG_0001");
    let answer = tokio::task::spawn_blocking(move || run_live_collected(query, WALK_BUDGET))
        .await
        .expect("the search thread")
        .expect("the live search");
    assert_eq!(answer.target_volume_id, phone.volume_id);
    assert_eq!(
        paths_of(&answer.entries),
        expected,
        "the live run answers the same rows"
    );

    volumes::forget_volume_for_test(&phone.volume_id);
    let _ = index.forget_volume(&phone.volume_id);
}

/// A phone nobody indexed is walked live from a phone pane's scope, exactly as an
/// unindexed local drive is, and the walk's rows are `adb://` paths too.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the index's process-wide seams for the whole cell; holding it across the awaits IS the point"
)]
async fn a_live_search_over_a_phone_nobody_indexed_walks_it() {
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed, _search_data) = install_index(data.path());
    let serial = "R58M-Search-Walked";
    let phone = dialed_phone(serial).await;

    let sdcard = format!("{}/sdcard", adb_app_root(serial));
    let query = pattern_query_for(&sdcard, "notes");
    let answer = tokio::task::spawn_blocking(move || run_live_collected(query, WALK_BUDGET))
        .await
        .expect("the search thread")
        .expect("the live search");

    assert_eq!(
        answer.target_volume_id, phone.volume_id,
        "the phone is the one volume searched"
    );
    assert_eq!(
        paths_of(&answer.entries),
        vec![format!("{sdcard}/Download/notes.txt")],
        "the walk found the phone's file, as a phone path"
    );

    volumes::forget_volume_for_test(&phone.volume_id);
    let _ = index.forget_volume(&phone.volume_id);
}
