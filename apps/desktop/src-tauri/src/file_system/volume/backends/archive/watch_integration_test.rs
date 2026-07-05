//! Integration tests for the archive live-content watch.
//!
//! These drive real temp `.zip` files through `VolumeManager::resolve` (which
//! starts the watch) and the listing cache, so they exercise the whole
//! refresh path: an external edit → drop the stale index → re-read through the
//! re-resolved `ArchiveVolume` → update the pane listing. The real-notify test
//! polls a condition with a generous timeout rather than sleeping a fixed
//! duration (FSEvents latency is unpredictable).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use crate::file_system::listing::FileEntry;
use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE, refresh_archive_listings};
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::volume::Volume;
use crate::file_system::volume::backends::InMemoryVolume;
use crate::file_system::volume::backends::archive::{ArchiveVolume, active_watch_count};
use crate::file_system::{VolumeManager, get_volume_manager};

use super::test_fixtures::{FixtureFile, build_zip, stored};

/// Starts the content watch on a resolved archive volume. `VolumeManager::resolve`
/// only auto-starts the watch when an app handle is registered (production); a
/// headless test has none, so it starts the watch directly on the registered
/// `ArchiveVolume` (the same instance the registry and future re-resolves see).
fn start_watch_on(volume: &Arc<dyn Volume>, parent_volume_id: &str) {
    volume
        .as_any()
        .downcast_ref::<ArchiveVolume>()
        .expect("resolved volume is an ArchiveVolume")
        .start_content_watch(parent_volume_id);
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

    /// Rewrites the archive the way editors and safe-overwrites do: build a
    /// sibling temp file, then atomically rename it over the archive. This swaps
    /// the file's inode, which a file-pinned watch would miss — the parent-dir
    /// watch must still catch it.
    fn rewrite_via_temp_rename(&self, entries: &[FixtureFile]) {
        let tmp = self.zip_path.with_extension("zip.tmp");
        std::fs::write(&tmp, build_zip(entries)).expect("write temp zip");
        std::fs::rename(&tmp, &self.zip_path).expect("rename temp over zip");
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

/// Seeds a cached listing at `path` on `volume_id` with `entries` and returns
/// its listing id.
fn seed_listing(volume_id: &str, path: &Path, entries: Vec<FileEntry>) -> String {
    let listing_id = format!("listing-{}", uuid::Uuid::new_v4());
    let mut cache = LISTING_CACHE.write().expect("cache lock");
    cache.insert(
        listing_id.clone(),
        CachedListing {
            volume_id: volume_id.to_string(),
            path: path.to_path_buf(),
            entries,
            sort_by: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            directory_sort_mode: DirectorySortMode::LikeFiles,
            sequence: AtomicU64::new(0),
            created_at: Instant::now(),
            last_accessed_ms: AtomicU64::new(0),
        },
    );
    listing_id
}

fn listing_names(listing_id: &str) -> Vec<String> {
    let cache = LISTING_CACHE.read().expect("cache lock");
    cache
        .get(listing_id)
        .map(|l| l.entries.iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default()
}

fn drop_listing(listing_id: &str) {
    LISTING_CACHE.write().expect("cache lock").remove(listing_id);
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
        &volume_id,
        &fixture.zip_path,
        vec![stub_entry(&fixture.zip_path, "a.txt")],
    );
    let outside_dir = fixture.zip_path.parent().expect("parent").join("elsewhere");
    let outside_listing = seed_listing(&volume_id, &outside_dir, vec![stub_entry(&outside_dir, "keep.txt")]);

    // Add a second entry to the zip, then refresh directly (deterministic; no
    // reliance on FSEvents timing).
    fixture.rewrite(&[stored("a.txt", b"a".to_vec()), stored("b.txt", b"bb".to_vec())]);
    refresh_archive_listings(&volume_id, &fixture.zip_path).await;

    let mut inner = listing_names(&inner_listing);
    inner.sort();
    assert_eq!(
        inner,
        vec!["a.txt", "b.txt"],
        "the archive listing must reflect the new entry"
    );
    assert_eq!(
        listing_names(&outside_listing),
        vec!["keep.txt"],
        "a listing outside the archive must be left untouched"
    );

    drop_listing(&inner_listing);
    drop_listing(&outside_listing);
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
    refresh_archive_listings(&volume_id, &fixture.zip_path).await;

    let mut names = listing_names(&listing);
    names.sort();
    assert_eq!(
        names,
        vec!["a.txt", "b.txt"],
        "an unreadable mid-write archive must keep the last good listing, not blank it"
    );

    drop_listing(&listing);
    get_volume_manager().unregister(&volume_id);
}

/// End-to-end through the real notify machinery: an on-disk edit to the zip makes
/// the live watch fire, which refreshes the open listing. Polls a condition with
/// a generous timeout (FSEvents/inotify latency varies).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_watch_refreshes_the_listing_when_the_zip_changes_on_disk() {
    let fixture = ArchiveFixture::new(&[stored("a.txt", b"a".to_vec())]);
    let volume_id = format!("test-vol-{}", uuid::Uuid::new_v4());
    get_volume_manager().register(
        &volume_id,
        Arc::new(InMemoryVolume::new("parent").with_local_fs_access()),
    );

    // Resolve registers the archive; without an app handle it doesn't auto-start
    // the watch, so start it directly (see `start_watch_on`).
    let resolved = get_volume_manager().resolve(&volume_id, &fixture.zip_path).await;
    let archive = resolved.volume.expect("archive volume");
    assert!(
        !archive.listing_is_watched(&fixture.zip_path),
        "resolve must not auto-start the watch without an app handle"
    );
    start_watch_on(&archive, &volume_id);
    assert!(
        archive.listing_is_watched(&fixture.zip_path),
        "an archive with an established watch must report listing_is_watched"
    );
    assert!(
        active_watch_count() >= 1,
        "the live watch must count toward the active total"
    );

    let listing = seed_listing(
        &volume_id,
        &fixture.zip_path,
        vec![stub_entry(&fixture.zip_path, "a.txt")],
    );

    // Edit the zip on disk; the parent-directory watch should notice.
    fixture.rewrite(&[stored("a.txt", b"a".to_vec()), stored("b.txt", b"bb".to_vec())]);

    // Poll until the watch-driven refresh lands the new entry. 15 s stays below
    // the 20 s nextest cap for this test (`.config/nextest.toml`) so a merely-slow
    // FSEvents delivery fails cleanly here (and retries) instead of racing a
    // SIGTERM; a fully-starved event is absorbed by that override's retries.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut saw_b = false;
    while Instant::now() < deadline {
        if listing_names(&listing).iter().any(|n| n == "b.txt") {
            saw_b = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        saw_b,
        "the live watch never refreshed the listing after the zip changed"
    );

    drop_listing(&listing);
    get_volume_manager().unregister(&volume_id);
}

/// The editor-style inode swap: rewriting the archive via a sibling temp file +
/// atomic rename (a new inode) must still refresh the listing. A file-pinned
/// watch would miss this; the parent-directory watch catches it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_temp_rename_swap_refreshes_the_listing() {
    let fixture = ArchiveFixture::new(&[stored("a.txt", b"a".to_vec())]);
    let volume_id = format!("test-vol-{}", uuid::Uuid::new_v4());
    get_volume_manager().register(
        &volume_id,
        Arc::new(InMemoryVolume::new("parent").with_local_fs_access()),
    );

    let resolved = get_volume_manager().resolve(&volume_id, &fixture.zip_path).await;
    let archive = resolved.volume.expect("archive volume");
    start_watch_on(&archive, &volume_id);

    let listing = seed_listing(
        &volume_id,
        &fixture.zip_path,
        vec![stub_entry(&fixture.zip_path, "a.txt")],
    );

    // Swap the archive's inode via temp+rename (the editor / safe-overwrite path).
    fixture.rewrite_via_temp_rename(&[stored("a.txt", b"a".to_vec()), stored("b.txt", b"bb".to_vec())]);

    // Poll (15 s, below the 20 s nextest cap) for the refresh; see the sibling
    // real-notify test for the timeout rationale.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut saw_b = false;
    while Instant::now() < deadline {
        if listing_names(&listing).iter().any(|n| n == "b.txt") {
            saw_b = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        saw_b,
        "the parent-dir watch never refreshed after the temp+rename inode swap"
    );

    drop_listing(&listing);
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
    start_watch_on(&a, "root");
    assert!(a.listing_is_watched(&zip_a), "A's watch must be live while registered");
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
    assert!(
        a.listing_is_watched(&zip_a),
        "the still-held Arc keeps the watch alive until the last reference drops"
    );
    drop(a); // final reference gone → ArchiveVolume drops → watch stops
}
