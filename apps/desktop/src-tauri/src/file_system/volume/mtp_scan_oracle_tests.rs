//! Integration tests for the fresh-listing oracle layered on top of
//! `MtpVolume::scan_for_copy_batch_with_boundary`.
//!
//! Two scenarios pinned:
//!
//! 1. **Oracle hit**: when the parent listing is watcher-backed (the device is connected and
//!    `LISTING_CACHE` holds the entries), the MTP batch scan reads child sizes from the cache and
//!    doesn't hit the device. We pin this with a test-only call counter on
//!    `MtpVolume::list_directory` (`super::backends::mtp::test_hooks`): zero calls after the scan.
//! 2. **Cold cache, parent-grouped**: when there's no cached listing, the existing parent-grouping
//!    optimization still runs. 4 children sharing parent `A` + 2 children sharing parent `B`
//!    collapse to exactly 2 `list_directory` calls, not 6. This is the load-bearing perf for the
//!    selected-many-photos-in-one-folder workflow.
//!
//! Both live behind the `virtual-mtp` feature so a real `MtpVolume` (with its
//! own override) runs end-to-end against a backing-dir-shaped virtual device.

#![cfg(feature = "virtual-mtp")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::file_system::listing::caching_test_support::{TestListing, TestListingGuard};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{MtpVolume, ScanBoundary, Volume, WatchCoverage};
use crate::mtp::connection::DeviceWatch;
use crate::mtp::connection::MtpDisconnectReason;
use crate::mtp::connection_manager;
use crate::mtp::virtual_device::{setup_virtual_mtp_device, virtual_device_test_lock};

use super::backends::mtp::test_hooks;

// `setup_virtual_mtp_device` gives every call its own temp backing root, so these
// tests don't contend on the filesystem. They still take
// `virtual_device_test_lock()` (via `VirtualDeviceGuard`): all virtual devices
// register under one serial, hence one Cmdr device id, which matters whenever
// several run in the SAME process (plain `cargo test`; nextest forks per test).

fn make_file_entry(name: &str, parent: &str, size: u64) -> FileEntry {
    FileEntry {
        size: Some(size),
        permissions: 0o644,
        owner: "test".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(
            name.to_string(),
            format!("{}/{}", parent.trim_end_matches('/'), name),
            false,
            false,
        )
    }
}

/// Seeds a cached listing owned by this test; the guard tears it down on drop,
/// unwind included.
fn insert_listing(tag: &str, volume_id: &str, path: &str, entries: Vec<FileEntry>) -> TestListingGuard {
    TestListing::new()
        .volume(volume_id)
        .path(path)
        .entries(entries)
        .sequence(1)
        .insert(tag)
}

/// Keeps a test's virtual device alive and exclusive: the process-wide lock (all
/// virtual devices share one serial, hence one Cmdr device id) plus the fixture
/// owning its temp backing dir. Held for the test body, released on drop.
struct VirtualDeviceGuard {
    _lock: tokio::sync::MutexGuard<'static, ()>,
    fixture: crate::mtp::virtual_device::VirtualDeviceFixture,
}

/// Connects the virtual MTP device, builds an `MtpVolume` for its first
/// storage, and returns `(device_id, volume, volume_id)`. The volume_id format
/// matches what `MtpVolume::new` computes internally
/// (`"{device_id}:{storage_id}"`); see `mtp/CLAUDE.md` § Volume IDs.
async fn connect_virtual_device() -> (String, Arc<MtpVolume>, String, VirtualDeviceGuard) {
    let guard = VirtualDeviceGuard {
        _lock: virtual_device_test_lock().lock().await,
        fixture: setup_virtual_mtp_device(),
    };
    let location_id = guard.fixture.location_id;
    // Derive the canonical device id from discovery, not `mtp-{location_id}`: the
    // virtual device reports a serial, so its id is serial-based
    // (`device_id_for`), and the connect path resolves by matching the live
    // enumeration's `.id`.
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");
    let info = connection_manager()
        .connect(&device_id, DeviceWatch::Off)
        .await
        .expect("virtual-mtp connect");
    let storage_id = info.storages.first().expect("at least one virtual storage").id;
    let vol = Arc::new(MtpVolume::new(
        Arc::clone(connection_manager()),
        &device_id,
        storage_id,
        "Test",
    ));
    let volume_id = format!("{}:{}", device_id, storage_id);
    (device_id, vol, volume_id, guard)
}

/// Test 1: on oracle hit, the MTP override skips its `list_directory` call
/// for the (sole) watcher-backed parent. The cached sizes flow into the
/// aggregate; no MTP I/O happens for those entries.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtp_scan_uses_oracle_on_hit_skips_list_directory() {
    let (device_id, vol, vid, _guard) = connect_virtual_device().await;
    // Register the volume so the oracle's `VolumeManager::get(vid)` finds it
    // and the `listing_watch_coverage` gate reports coverage (device connected).
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // Pre-populate `LISTING_CACHE` for the parent with sizes that don't match
    // any real file on the virtual device. If the oracle short-circuit fails
    // (override calls `list_directory` and uses real sizes), `total_bytes`
    // would be the device's real numbers, not these cached ones.
    let cached = vec![
        make_file_entry("a.jpg", "/DCIM", 1000),
        make_file_entry("b.jpg", "/DCIM", 2000),
        make_file_entry("c.jpg", "/DCIM", 3000),
    ];
    let _listing = insert_listing("mtp-oracle-hit", &vid, "/DCIM", cached);

    // Sanity-check the oracle gate. Without this, an unrelated regression in
    // `listing_watch_coverage` would make the test claim the wrong cause.
    assert_eq!(
        vol.listing_watch_coverage(Path::new("/DCIM")),
        WatchCoverage::EveryWriter,
        "virtual device must report connected (listing covered)"
    );

    let paths = vec![
        PathBuf::from("/DCIM/a.jpg"),
        PathBuf::from("/DCIM/b.jpg"),
        PathBuf::from("/DCIM/c.jpg"),
    ];

    test_hooks::reset_list_directory_call_count();
    let result = vol
        .scan_for_copy_batch_with_boundary(&paths, &ScanBoundary::silent())
        .await
        .expect("oracle-served batch scan");

    assert_eq!(
        test_hooks::list_directory_call_count(),
        0,
        "expected zero MtpVolume::list_directory calls on oracle hit"
    );
    // Cached sizes (not device sizes) win.
    assert_eq!(result.aggregate.file_count, 3);
    assert_eq!(result.aggregate.total_bytes, 6000);
    assert_eq!(result.per_path.len(), 3);

    get_volume_manager().unregister(&vid);
    connection_manager()
        .disconnect(&device_id, MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect");
}

/// Test 2: no cached listing → the cold-cache parent-grouping optimization
/// runs. Two unique parents, multiple children under each → exactly 2
/// `list_directory` calls.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtp_scan_cold_cache_still_uses_parent_grouping() {
    let (device_id, vol, vid, _guard) = connect_virtual_device().await;
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // MTP needs the parent's path-handle cached before it can list any path
    // (`resolve_path_to_handle` is cache-only; only `/` is auto-known). Walk
    // root first so `/Documents` and `/DCIM` get into the path-handle cache.
    // We don't care about the entries here, just the side effect on the cache.
    let root = vol.list_directory(Path::new("/"), None).await.expect("listing /");
    assert!(
        root.iter().any(|e| e.name == "Documents") && root.iter().any(|e| e.name == "DCIM"),
        "expected Documents/ and DCIM/ at root of virtual device fixture"
    );

    // Sanity-verify the children exist before relying on them in the scan.
    // These two listings also seed the listing cache (5 s TTL) for `/Documents`
    // and `/DCIM`, so we clear that cache below before the actual scan to
    // ensure the cold path runs.
    let documents = vol
        .list_directory(Path::new("/Documents"), None)
        .await
        .expect("listing /Documents");
    let dcim = vol
        .list_directory(Path::new("/DCIM"), None)
        .await
        .expect("listing /DCIM");
    assert!(
        documents.iter().any(|e| e.name == "report.txt") && documents.iter().any(|e| e.name == "notes.txt"),
        "expected report.txt and notes.txt in /Documents fixture"
    );
    assert!(
        dcim.iter().any(|e| e.name == "photo-001.jpg"),
        "expected photo-001.jpg in /DCIM fixture"
    );

    // Clear the mtp-rs listing cache so the override's `list_directory`
    // calls actually hit USB (rather than the cache) — the override invokes
    // `MtpVolume::list_directory` which counts via `test_hooks`, but the
    // assertion is structural ("called exactly twice"), not "did real I/O".
    // The path-handle cache stays primed; only the listing cache is dropped.
    connection_manager().clear_all_listing_caches().await;
    test_hooks::reset_list_directory_call_count();

    // 4 children under /Documents (duplicates are intentional: even a
    // 100-photo-pick should produce one parent listing, not 100), 2 under
    // /DCIM. Total: 6 input paths, 2 unique parents.
    let paths = vec![
        PathBuf::from("/Documents/report.txt"),
        PathBuf::from("/Documents/notes.txt"),
        PathBuf::from("/Documents/report.txt"),
        PathBuf::from("/Documents/notes.txt"),
        PathBuf::from("/DCIM/photo-001.jpg"),
        PathBuf::from("/DCIM/photo-001.jpg"),
    ];

    let result = vol
        .scan_for_copy_batch_with_boundary(&paths, &ScanBoundary::silent())
        .await
        .expect("cold batch scan");

    assert_eq!(
        test_hooks::list_directory_call_count(),
        2,
        "expected exactly 2 MtpVolume::list_directory calls (one per unique parent)"
    );
    // Sanity: every unique input resolved.
    let unique_inputs: std::collections::HashSet<&Path> = paths.iter().map(|p| p.as_path()).collect();
    assert_eq!(
        result.per_path.len(),
        unique_inputs.len(),
        "per_path should have one entry per unique input path"
    );

    get_volume_manager().unregister(&vid);
    connection_manager()
        .disconnect(&device_id, MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect");
}

/// The shared stop assertion, against the virtual MTP device.
///
/// MTP is the one backend with no real hardware in any lane, so this stands in
/// for it. A cold `/DCIM/Camera` listing is ~17 s of USB round trips on a real
/// phone; the seams are per parent group (before that listing), per selected
/// child, and per entry inside the subtree recursion in
/// `mtp/connection/bulk_ops.rs`.
///
/// Shares the virtual device with the two cells above, so it takes the same
/// `virtual_device_test_lock`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtp_batch_scan_stops_when_it_is_told_to() {
    let (device_id, vol, vid, _guard) = connect_virtual_device().await;
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);
    // MTP resolves a path through a cache that browsing populates, so walk root
    // first or `/DCIM` isn't addressable yet.
    vol.list_directory(Path::new("/"), None).await.expect("listing /");

    cmdr_fs::volume::conformance::assert_batch_scan_stops_when_told(vol.as_ref(), Path::new("/DCIM")).await;

    get_volume_manager().unregister(&vid);
    connection_manager()
        .disconnect(&device_id, MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect");
}

/// The boundary is asked INSIDE the subtree recursion, not once per source path:
/// `/DCIM` holds `photo-001.jpg` plus a `Burst/` directory with its own child, so
/// a backend asking only per path would come up short here.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtp_batch_scan_asks_its_boundary_inside_the_walk() {
    let (device_id, vol, vid, _guard) = connect_virtual_device().await;
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);
    vol.list_directory(Path::new("/"), None).await.expect("listing /");

    cmdr_fs::volume::conformance::assert_batch_scan_asks_inside_the_walk(vol.as_ref(), Path::new("/DCIM"), 3).await;

    get_volume_manager().unregister(&vid);
    connection_manager()
        .disconnect(&device_id, MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect");
}
