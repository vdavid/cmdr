//! SMB watcher → archive-inner refresh wiring.
//!
//! The recursive share watch already refreshes the DIRECTORY listing showing a
//! changed `.zip`. These tests pin the added behavior: when the changed path is
//! a supported archive, `process_event_batch` ALSO refreshes any open listing
//! INSIDE the archive — the push-refresh a REMOTE parent otherwise never gets,
//! since `archive::watch` (the local-parent equivalent) can't arm without a local
//! `notify` transport. A non-archive change must NOT trigger the inner refresh
//! (the extension gate).
//!
//! The batch is driven directly (no live SMB session): the seeded inner listing
//! starts stale (missing an entry the on-disk zip already has), so a fired
//! refresh re-reads and adds it, and a suppressed refresh leaves it stale — a
//! deterministic observable either way.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use smb2::FileNotifyAction;

use super::process_event_batch;
use crate::file_system::get_volume_manager;
use crate::file_system::listing::FileEntry;
use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::volume::backends::InMemoryVolume;
use crate::file_system::volume::backends::archive::test_fixtures::{FixtureFile, build_zip, stored};

/// A temp directory with a `.zip` inside, cleaned up on drop. The temp dir plays
/// the role of the SMB mount root; the zip sits directly under it so a share-root
/// event names it as `bundle.zip`.
struct Fixture {
    _dir: tempfile::TempDir,
    mount_path: PathBuf,
    zip_path: PathBuf,
}

impl Fixture {
    fn new(entries: &[FixtureFile]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let mount_path = dir.path().to_path_buf();
        let zip_path = mount_path.join("bundle.zip");
        std::fs::write(&zip_path, build_zip(entries)).expect("write fixture zip");
        Self {
            _dir: dir,
            mount_path,
            zip_path,
        }
    }
}

/// A synthetic `FileEntry` for seeding a cached listing (a fired refresh replaces
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

/// Seeds a cached listing at `path` on `volume_id` with `entries`, returning its id.
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

/// Registers an `InMemoryVolume` parent (with local-fs access so the archive reads
/// the real temp zip) and resolves the zip so it's recognized as an archive.
async fn register_parent_and_resolve(volume_id: &str, zip_path: &Path) {
    get_volume_manager().register(
        volume_id,
        Arc::new(InMemoryVolume::new("parent").with_local_fs_access()),
    );
    let resolved = get_volume_manager().resolve(volume_id, zip_path).await;
    assert!(resolved.is_archive, "the zip path must resolve to an ArchiveVolume");
}

/// One `Modified` event naming `filename` under the mount root, ready for
/// `process_event_batch`.
fn modified_batch(
    mount_path: &Path,
    filename: &str,
) -> std::collections::HashMap<PathBuf, Vec<(FileNotifyAction, String)>> {
    let mut batch = std::collections::HashMap::new();
    batch.insert(
        mount_path.to_path_buf(),
        vec![(FileNotifyAction::Modified, filename.to_string())],
    );
    batch
}

/// A `Modified` event for the backing `.zip` refreshes an open archive-inner
/// listing, reflecting an entry added to the zip out of band.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_modified_zip_event_refreshes_the_inner_listing() {
    let fixture = Fixture::new(&[stored("a.txt", b"a".to_vec()), stored("b.txt", b"bb".to_vec())]);
    let volume_id = format!("test-vol-{}", uuid::Uuid::new_v4());
    register_parent_and_resolve(&volume_id, &fixture.zip_path).await;

    // The listing starts stale: it lists only a.txt, though the on-disk zip
    // already has a.txt + b.txt.
    let inner_listing = seed_listing(
        &volume_id,
        &fixture.zip_path,
        vec![stub_entry(&fixture.zip_path, "a.txt")],
    );

    process_event_batch(
        modified_batch(&fixture.mount_path, "bundle.zip"),
        &volume_id,
        &fixture.mount_path,
    )
    .await;

    let mut names = listing_names(&inner_listing);
    names.sort();
    assert_eq!(
        names,
        vec!["a.txt", "b.txt"],
        "a Modified event for the .zip must refresh the inner listing to reflect the new entry"
    );

    drop_listing(&inner_listing);
    get_volume_manager().unregister(&volume_id);
}

/// A `Modified` event for a NON-archive sibling leaves an open archive-inner
/// listing untouched (the extension gate holds).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_modified_non_archive_event_leaves_the_inner_listing_alone() {
    let fixture = Fixture::new(&[stored("a.txt", b"a".to_vec()), stored("b.txt", b"bb".to_vec())]);
    let volume_id = format!("test-vol-{}", uuid::Uuid::new_v4());
    register_parent_and_resolve(&volume_id, &fixture.zip_path).await;

    // Same stale seed as above (lists only a.txt).
    let inner_listing = seed_listing(
        &volume_id,
        &fixture.zip_path,
        vec![stub_entry(&fixture.zip_path, "a.txt")],
    );

    // A plain text sibling changed, not the archive: the inner refresh must not fire.
    process_event_batch(
        modified_batch(&fixture.mount_path, "notes.txt"),
        &volume_id,
        &fixture.mount_path,
    )
    .await;

    assert_eq!(
        listing_names(&inner_listing),
        vec!["a.txt"],
        "a non-archive change must not refresh the archive-inner listing"
    );

    drop_listing(&inner_listing);
    get_volume_manager().unregister(&volume_id);
}
