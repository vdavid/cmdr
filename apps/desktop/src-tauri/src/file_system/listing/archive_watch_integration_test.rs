//! The app's half of the archive live-content watch: what a refresh DOES.
//!
//! These drive real temp `.zip` files through `VolumeManager::resolve` and the
//! listing cache, so they cover the second half of the refresh path: an archive
//! refresh → re-read through the re-resolved `ArchiveVolume` → update the pane
//! listing, plus what LRU eviction releases. They go through
//! [`AppListings`], the adapter a backend actually reaches, rather than
//! `caching::refresh_archive_listings` directly.
//!
//! The FIRST half — a real on-disk edit reaching the refresh seam at all, across
//! an inode swap, and what a watch that can't establish reports — belongs to the
//! backend and is asserted against a recording host in `watch/host_seam_test.rs`.
//! No FSEvents timing lives in this file.

use cmdr_fs::volume::host::listings::ListingHost;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::file_system::VolumeManager;
use crate::file_system::listing::FileEntry;
use crate::file_system::listing::caching_test_support::{TestListing, TestListingGuard};
use crate::file_system::listing::listing_host::AppListings;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::volume::Volume;
use crate::file_system::volume::WatchCoverage;
use crate::file_system::volume::manager::get_volume_manager;
use cmdr_archive::ArchiveVolume;

use cmdr_archive::test_fixtures::{FixtureFile, build_zip, stored};

/// Starts the content watch on a resolved archive volume. `VolumeManager::resolve`
/// only auto-starts the watch when an app handle is registered (production); a
/// headless test has none, so it starts the watch directly on the registered
/// `ArchiveVolume` (the same instance the registry and future re-resolves see).
fn start_watch_on(volume: &Arc<dyn Volume>, parent_volume_id: &str) {
    volume
        .as_any()
        .downcast_ref::<ArchiveVolume>()
        .expect("resolved volume is an ArchiveVolume")
        .start_content_watch(parent_volume_id, WatchCoverage::EveryWriter);
}

/// A temp directory with a `.zip` inside, cleaned up on drop. The zip lives in
/// its own directory so the parent-directory content watch is isolated.
struct ArchiveFixture {
    _dir: tempfile::TempDir,
    zip_path: PathBuf,
}

impl ArchiveFixture {
    fn new(entries: &[FixtureFile]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip_path = dir.path().join("bundle.zip");
        std::fs::write(&zip_path, build_zip(entries)).expect("write fixture zip");
        Self { _dir: dir, zip_path }
    }

    fn rewrite(&self, entries: &[FixtureFile]) {
        std::fs::write(&self.zip_path, build_zip(entries)).expect("rewrite fixture zip");
    }
}

/// A synthetic `FileEntry` for seeding a cached listing (the watcher replaces
/// these with freshly-read ones).
fn stub_entry(archive_path: &Path, inner_name: &str) -> FileEntry {
    let full = archive_path.join(inner_name);
    FileEntry {
        extended_metadata_loaded: true,
        ..FileEntry::new(
            inner_name.to_string(),
            full.to_string_lossy().into_owned(),
            false,
            false,
        )
    }
}

/// Seeds a cached listing at `path` on `volume_id` with `entries`. The returned
/// guard owns the entry and tears it down on drop, unwind included.
fn seed_listing(tag: &str, volume_id: &str, path: &Path, entries: Vec<FileEntry>) -> TestListingGuard {
    TestListing::new()
        .volume(volume_id)
        .path(path)
        .entries(entries)
        .insert(tag)
}

/// Re-reading through the re-resolved `ArchiveVolume` picks up an entry added to
/// the backing zip, and a listing NOT inside the archive is left untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_reflects_a_new_entry_and_leaves_outside_listings_alone() {
    let fixture = ArchiveFixture::new(&[stored("a.txt", b"a".to_vec())]);
    let volume_id = format!("test-vol-{}", uuid::Uuid::new_v4());
    get_volume_manager().register(
        &volume_id,
        Arc::new(InMemoryVolume::new("parent").with_local_fs_access()),
    );

    // Resolve once so the zip is recognized as an archive (this test drives the
    // refresh directly, so it needs no live watch).
    let resolved = get_volume_manager().resolve(&volume_id, &fixture.zip_path).await;
    assert!(resolved.is_archive, "the zip path must resolve to an ArchiveVolume");

    // A listing at the archive root, plus a sibling listing on the same drive
    // that is NOT inside the archive — the refresh must not touch the sibling.
    let inner_listing = seed_listing(
        "archive-refresh-inner",
        &volume_id,
        &fixture.zip_path,
        vec![stub_entry(&fixture.zip_path, "a.txt")],
    );
    let outside_dir = fixture.zip_path.parent().expect("parent").join("elsewhere");
    let outside_listing = seed_listing(
        "archive-refresh-outside",
        &volume_id,
        &outside_dir,
        vec![stub_entry(&outside_dir, "keep.txt")],
    );

    // Add a second entry to the zip, then refresh directly (deterministic; no
    // reliance on FSEvents timing).
    fixture.rewrite(&[stored("a.txt", b"a".to_vec()), stored("b.txt", b"bb".to_vec())]);
    AppListings
        .refresh_archive_listings(&volume_id, &fixture.zip_path)
        .await;

    let mut inner = inner_listing.entry_names();
    inner.sort();
    assert_eq!(
        inner,
        vec!["a.txt", "b.txt"],
        "the archive listing must reflect the new entry"
    );
    assert_eq!(
        outside_listing.entry_names(),
        vec!["keep.txt"],
        "a listing outside the archive must be left untouched"
    );

    get_volume_manager().unregister(&volume_id);
}

/// A mid-write, truncated archive keeps the previous listing rather than blanking
/// the pane, and surfaces no error on the refresh path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_truncated_midwrite_archive_keeps_the_previous_listing() {
    let fixture = ArchiveFixture::new(&[stored("a.txt", b"a".to_vec()), stored("b.txt", b"bb".to_vec())]);
    let volume_id = format!("test-vol-{}", uuid::Uuid::new_v4());
    get_volume_manager().register(
        &volume_id,
        Arc::new(InMemoryVolume::new("parent").with_local_fs_access()),
    );
    get_volume_manager().resolve(&volume_id, &fixture.zip_path).await;

    let listing = seed_listing(
        "archive-truncated",
        &volume_id,
        &fixture.zip_path,
        vec![
            stub_entry(&fixture.zip_path, "a.txt"),
            stub_entry(&fixture.zip_path, "b.txt"),
        ],
    );

    // Simulate a writer mid-rewrite: a local header signature but no central
    // directory / EOCD yet — an unreadable archive.
    std::fs::write(&fixture.zip_path, b"PK\x03\x04half-written-no-central-directory").expect("truncate");
    AppListings
        .refresh_archive_listings(&volume_id, &fixture.zip_path)
        .await;

    let mut names = listing.entry_names();
    names.sort();
    assert_eq!(
        names,
        vec!["a.txt", "b.txt"],
        "an unreadable mid-write archive must keep the last good listing, not blank it"
    );

    get_volume_manager().unregister(&volume_id);
}

/// LRU eviction releases the evicted archive (and thus its watch): the registry
/// drops its reference, so the only remaining strong count is the test's own.
/// Uses a private `VolumeManager` so the eviction is deterministic.
#[tokio::test]
async fn lru_eviction_releases_the_archive_and_its_watch() {
    let base = tempfile::tempdir().expect("tempdir");
    let manager = VolumeManager::new();
    manager.register("root", Arc::new(InMemoryVolume::new("root").with_local_fs_access()));

    // Resolve archive A, start its watch, and hold its Arc.
    let zip_a = base.path().join("a.zip");
    std::fs::write(&zip_a, build_zip(&[stored("x.txt", b"x".to_vec())])).expect("write a.zip");
    let a = manager.resolve("root", &zip_a).await.volume.expect("archive a");
    // `resolve` registers the archive but gates the watch on a registered app
    // handle, which a headless test has none of — so no real OS watch starts
    // behind our back, and this test starts one itself.
    assert_eq!(
        a.listing_watch_coverage(&zip_a),
        WatchCoverage::None,
        "resolve must not auto-start the watch without an app handle"
    );
    start_watch_on(&a, "root");
    assert_eq!(
        a.listing_watch_coverage(&zip_a),
        WatchCoverage::EveryWriter,
        "A's watch must be live while registered"
    );
    assert_eq!(Arc::strong_count(&a), 2, "the registry and the test each hold one Arc");

    // Resolve well past the LRU cap so A is evicted (cap is 16; 20 clears it).
    for i in 0..20 {
        let zip = base.path().join(format!("more-{i}.zip"));
        std::fs::write(&zip, build_zip(&[stored("y.txt", b"y".to_vec())])).expect("write filler zip");
        manager.resolve("root", &zip.join("inner")).await;
    }

    // The registry has dropped A: nothing but the test's own Arc remains, so
    // dropping it stops the watch — no leaked watcher.
    assert_eq!(
        Arc::strong_count(&a),
        1,
        "eviction must release the registry's Arc, leaving only the test's"
    );
    assert_eq!(
        a.listing_watch_coverage(&zip_a),
        WatchCoverage::EveryWriter,
        "the still-held Arc keeps the watch alive until the last reference drops"
    );
    drop(a); // final reference gone → ArchiveVolume drops → watch stops
}
