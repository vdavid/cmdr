//! Integration tests for the fresh-listing oracle layered on top of
//! `MtpVolume::scan_for_copy_batch_with_progress`.
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
use std::sync::atomic::{AtomicU64, Ordering};

use crate::file_system::get_volume_manager;
use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::volume::{MtpVolume, Volume};
use crate::mtp::connection::{MtpDisconnectReason, connection_manager};
use crate::mtp::virtual_device::setup_virtual_mtp_device;

use super::backends::mtp::test_hooks;

// `setup_virtual_mtp_device` wipes and recreates a shared backing-dir fixture
// root at `/tmp/cmdr-mtp-e2e-fixtures`. Tests in this module + the existing
// `test_listing_is_watched_flips_with_connection` in `mtp.rs` would clobber
// each other if nextest scheduled them concurrently. They run inside the
// `virtual-mtp` test-group (max-threads = 1) per `.config/nextest.toml`.

/// Unique-per-test counter so parallel tests don't collide in the listing cache.
fn unique(suffix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "mtp_oracle_{}_{}_{}",
        suffix,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

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

fn insert_listing(id: &str, volume_id: &str, path: &str, entries: Vec<FileEntry>) {
    let mut cache = LISTING_CACHE.write().expect("LISTING_CACHE lock poisoned by a panicked test thread");
    cache.insert(
        id.to_string(),
        CachedListing {
            volume_id: volume_id.to_string(),
            path: PathBuf::from(path),
            entries,
            sort_by: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            directory_sort_mode: DirectorySortMode::LikeFiles,
            sequence: AtomicU64::new(1),
            created_at: std::time::Instant::now(),
            last_accessed_ms: AtomicU64::new(0),
        },
    );
}

fn remove_listing(id: &str) {
    let mut cache = LISTING_CACHE.write().expect("LISTING_CACHE lock poisoned by a panicked test thread");
    cache.remove(id);
}

/// Connects the virtual MTP device, builds an `MtpVolume` for its first
/// storage, and returns `(device_id, volume, volume_id)`. The volume_id format
/// matches what `MtpVolume::new` computes internally
/// (`"{device_id}:{storage_id}"`); see `mtp/CLAUDE.md` § Volume IDs.
async fn connect_virtual_device() -> (String, Arc<MtpVolume>, String) {
    let location_id = setup_virtual_mtp_device();
    let device_id = format!("mtp-{}", location_id);
    let info = connection_manager()
        .connect(&device_id, None)
        .await
        .expect("virtual-mtp connect");
    let storage_id = info.storages.first().expect("at least one virtual storage").id;
    let vol = Arc::new(MtpVolume::new(&device_id, storage_id, "Test"));
    let volume_id = format!("{}:{}", device_id, storage_id);
    (device_id, vol, volume_id)
}

/// Test 1: on oracle hit, the MTP override skips its `list_directory` call
/// for the (sole) watcher-backed parent. The cached sizes flow into the
/// aggregate; no MTP I/O happens for those entries.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtp_scan_uses_oracle_on_hit_skips_list_directory() {
    let (device_id, vol, vid) = connect_virtual_device().await;
    // Register the volume so the oracle's `VolumeManager::get(vid)` finds it
    // and the `listing_is_watched` gate returns true (device connected).
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // Pre-populate `LISTING_CACHE` for the parent with sizes that don't match
    // any real file on the virtual device. If the oracle short-circuit fails
    // (override calls `list_directory` and uses real sizes), `total_bytes`
    // would be the device's real numbers, not these cached ones.
    let lid = unique("hit");
    let cached = vec![
        make_file_entry("a.jpg", "/DCIM", 1000),
        make_file_entry("b.jpg", "/DCIM", 2000),
        make_file_entry("c.jpg", "/DCIM", 3000),
    ];
    insert_listing(&lid, &vid, "/DCIM", cached);

    // Sanity-check the oracle gate. Without this, an unrelated regression in
    // `listing_is_watched` would make the test claim the wrong cause.
    assert!(
        vol.listing_is_watched(Path::new("/DCIM")),
        "virtual device must report connected (listing watched)"
    );

    let paths = vec![
        PathBuf::from("/DCIM/a.jpg"),
        PathBuf::from("/DCIM/b.jpg"),
        PathBuf::from("/DCIM/c.jpg"),
    ];

    test_hooks::reset_list_directory_call_count();
    let result = vol
        .scan_for_copy_batch_with_progress(&paths, None)
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

    remove_listing(&lid);
    get_volume_manager().unregister(&vid);
    connection_manager()
        .disconnect(&device_id, None, MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect");
}

/// Test 2: no cached listing → the cold-cache parent-grouping optimization
/// runs. Two unique parents, multiple children under each → exactly 2
/// `list_directory` calls.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtp_scan_cold_cache_still_uses_parent_grouping() {
    let (device_id, vol, vid) = connect_virtual_device().await;
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
        .scan_for_copy_batch_with_progress(&paths, None)
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
        .disconnect(&device_id, None, MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect");
}
