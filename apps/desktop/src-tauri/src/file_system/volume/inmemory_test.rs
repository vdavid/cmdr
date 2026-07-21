//! Integration tests using InMemoryVolume.
//!
//! These tests verify that the volume abstraction works correctly
//! without touching the real file system.

use super::{InMemoryVolume, Volume};
use crate::file_system::listing::FileEntry;
use std::path::Path;

/// Creates a sample file entry for testing.
fn create_test_entry(name: &str, is_dir: bool) -> FileEntry {
    FileEntry {
        size: if is_dir { None } else { Some(1024) },
        modified_at: Some(1_640_000_000),
        created_at: Some(1_639_000_000),
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(name.to_string(), format!("/{}", name), is_dir, false)
    }
}

#[tokio::test]
async fn test_inmemory_volume_full_workflow() {
    // Create volume with some entries
    let entries = vec![
        create_test_entry("documents", true),
        create_test_entry("photo.jpg", false),
        create_test_entry("notes.txt", false),
    ];

    let volume = InMemoryVolume::with_entries("Test Volume", entries);

    // Verify volume properties
    assert_eq!(volume.name(), "Test Volume");
    assert_eq!(volume.root(), Path::new("/"));

    // List directory
    let listed = volume.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(listed.len(), 3);

    // Verify sorting (directories first)
    assert_eq!(listed[0].name, "documents");
    assert!(listed[0].is_directory);

    // Create a new file
    volume
        .create_file(Path::new("/new_file.txt"), b"Hello World")
        .await
        .unwrap();

    // Verify it exists
    assert!(volume.exists(Path::new("/new_file.txt")).await);

    // Get metadata
    let metadata = volume.get_metadata(Path::new("/new_file.txt")).await.unwrap();
    assert_eq!(metadata.name, "new_file.txt");
    assert_eq!(metadata.size, Some(11)); // "Hello World" is 11 bytes

    // Delete the file
    volume.delete(Path::new("/new_file.txt")).await.unwrap();
    assert!(!volume.exists(Path::new("/new_file.txt")).await);
}

#[tokio::test]
async fn test_inmemory_volume_stress_test_50k_entries() {
    // Create volume with 50,000 entries
    let volume = InMemoryVolume::with_file_count("Stress Test", 50_000);

    // List directory
    let start = std::time::Instant::now();
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let duration = start.elapsed();

    // Verify count
    assert_eq!(entries.len(), 50_000);

    // Verify performance (should be well under 1 sec, but we say 5 sec so it isn't flaky on CI)
    assert!(duration.as_millis() < 5000, "Listing 50k entries took {:?}", duration);
}

#[tokio::test]
async fn test_inmemory_volume_nested_directories() {
    let entries = vec![
        create_test_entry("level1", true),
        FileEntry {
            name: "level2".to_string(),
            path: "/level1/level2".to_string(),
            ..create_test_entry("", true)
        },
        FileEntry {
            name: "file.txt".to_string(),
            path: "/level1/level2/file.txt".to_string(),
            is_directory: false,
            ..create_test_entry("", false)
        },
    ];

    let volume = InMemoryVolume::with_entries("Nested", entries);

    // List root - should only show level1
    let root_entries = volume.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(root_entries.len(), 1);
    assert_eq!(root_entries[0].name, "level1");

    // List level1 - should only show level2
    let level1_entries = volume.list_directory(Path::new("/level1"), None).await.unwrap();
    assert_eq!(level1_entries.len(), 1);
    assert_eq!(level1_entries[0].name, "level2");

    // List level2 - should only show file.txt
    let level2_entries = volume.list_directory(Path::new("/level1/level2"), None).await.unwrap();
    assert_eq!(level2_entries.len(), 1);
    assert_eq!(level2_entries[0].name, "file.txt");
}

#[tokio::test]
async fn test_volume_create_and_list_sequence() {
    let volume = InMemoryVolume::new("Empty Volume");

    // Start empty
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(entries.len(), 0);

    // Create a directory
    volume.create_directory(Path::new("/docs")).await.unwrap();

    // Create some files
    volume.create_file(Path::new("/readme.md"), b"# README").await.unwrap();
    volume
        .create_file(Path::new("/docs/guide.txt"), b"Guide content")
        .await
        .unwrap();

    // List root
    let root_entries = volume.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(root_entries.len(), 2); // docs/ and readme.md

    // Directories should be first
    assert_eq!(root_entries[0].name, "docs");
    assert!(root_entries[0].is_directory);
    assert_eq!(root_entries[1].name, "readme.md");
    assert!(!root_entries[1].is_directory);

    // List docs
    let docs_entries = volume.list_directory(Path::new("/docs"), None).await.unwrap();
    assert_eq!(docs_entries.len(), 1);
    assert_eq!(docs_entries[0].name, "guide.txt");

    // Delete readme.md
    volume.delete(Path::new("/readme.md")).await.unwrap();

    // List root again
    let root_entries = volume.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(root_entries.len(), 1);
    assert_eq!(root_entries[0].name, "docs");
}

#[tokio::test]
async fn test_volume_manager_with_inmemory() {
    use super::manager::VolumeManager;
    use std::sync::Arc;

    let manager = VolumeManager::new();

    // Create two in-memory volumes
    let home_entries = vec![create_test_entry("Documents", true), create_test_entry("Desktop", true)];

    let dropbox_entries = vec![create_test_entry("Work", true), create_test_entry("Personal", true)];

    let home = Arc::new(InMemoryVolume::with_entries("Home", home_entries));
    let dropbox = Arc::new(InMemoryVolume::with_entries("Dropbox", dropbox_entries));

    // Register volumes
    manager.register("home", home.clone());
    manager.register("dropbox", dropbox.clone());
    manager.set_default("home");

    // Verify we can retrieve them
    let retrieved_home = manager.get("home").unwrap();
    assert_eq!(retrieved_home.name(), "Home");

    let retrieved_dropbox = manager.get("dropbox").unwrap();
    assert_eq!(retrieved_dropbox.name(), "Dropbox");

    // Verify default
    let default = manager.default_volume().unwrap();
    assert_eq!(default.name(), "Home");

    // List from both volumes
    let home_files = retrieved_home.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(home_files.len(), 2);

    let dropbox_files = retrieved_dropbox.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(dropbox_files.len(), 2);
    assert_eq!(dropbox_files[0].name, "Personal"); // Alphabetical order
    assert_eq!(dropbox_files[1].name, "Work");
}

// ============================================================================
// Streaming state management integration tests
// ============================================================================

#[test]
fn test_streaming_state_lifecycle() {
    use crate::file_system::listing::cancel_listing;
    use crate::file_system::listing::streaming::{STREAMING_STATE, StreamingListingState};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Create and register a streaming state
    let listing_id = "integration-test-lifecycle";
    let state = Arc::new(StreamingListingState {
        cancelled: Arc::new(AtomicBool::new(false)),
        cancel_notify: tokio::sync::Notify::new(),
    });

    // Insert into cache
    {
        let mut cache = STREAMING_STATE.write().unwrap();
        cache.insert(listing_id.to_string(), Arc::clone(&state));
    }

    // Verify it exists in cache
    {
        let cache = STREAMING_STATE.read().unwrap();
        assert!(cache.contains_key(listing_id));
    }

    // Cancel it
    cancel_listing(listing_id);
    assert!(state.cancelled.load(Ordering::Relaxed));

    // Cleanup (simulate what the streaming task does)
    {
        let mut cache = STREAMING_STATE.write().unwrap();
        cache.remove(listing_id);
    }

    // Verify it's gone
    {
        let cache = STREAMING_STATE.read().unwrap();
        assert!(!cache.contains_key(listing_id));
    }
}

#[test]
fn test_multiple_concurrent_streaming_states() {
    use crate::file_system::listing::cancel_listing;
    use crate::file_system::listing::streaming::{STREAMING_STATE, StreamingListingState};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Create multiple streaming states
    let ids = ["stream-1", "stream-2", "stream-3"];
    let states: Vec<Arc<StreamingListingState>> = ids
        .iter()
        .map(|_| {
            Arc::new(StreamingListingState {
                cancelled: Arc::new(AtomicBool::new(false)),
                cancel_notify: tokio::sync::Notify::new(),
            })
        })
        .collect();

    // Insert all into cache
    {
        let mut cache = STREAMING_STATE.write().unwrap();
        for (id, state) in ids.iter().zip(states.iter()) {
            cache.insert(id.to_string(), Arc::clone(state));
        }
    }

    // Verify all exist
    {
        let cache = STREAMING_STATE.read().unwrap();
        assert!(cache.len() >= 3); // May have other tests' entries
    }

    // Cancel only the second one
    cancel_listing("stream-2");

    // Verify only second is cancelled
    assert!(!states[0].cancelled.load(Ordering::Relaxed));
    assert!(states[1].cancelled.load(Ordering::Relaxed));
    assert!(!states[2].cancelled.load(Ordering::Relaxed));

    // Cleanup
    {
        let mut cache = STREAMING_STATE.write().unwrap();
        for id in ids.iter() {
            cache.remove(*id);
        }
    }
}

#[test]
fn test_streaming_entries_are_sorted() {
    use crate::file_system::listing::sorting::{DirectorySortMode, sort_entries};
    use crate::file_system::listing::{SortColumn, SortOrder};

    // Create unsorted entries
    let mut entries = vec![
        create_test_entry("zebra.txt", false),
        create_test_entry("aardvark", true),
        create_test_entry("banana.txt", false),
        create_test_entry("zoo", true),
    ];

    // Sort by name ascending (default)
    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    // Directories should come first, sorted alphabetically
    assert_eq!(entries[0].name, "aardvark");
    assert!(entries[0].is_directory);
    assert_eq!(entries[1].name, "zoo");
    assert!(entries[1].is_directory);

    // Then files, sorted alphabetically
    assert_eq!(entries[2].name, "banana.txt");
    assert!(!entries[2].is_directory);
    assert_eq!(entries[3].name, "zebra.txt");
    assert!(!entries[3].is_directory);
}

// ============================================================================
// `Volume::create_directory_all` (recursive mkdir -p) trait-default tests.
//
// `create_directory_all` is the volume-aware transfer pipelines' destination
// gate: a copy/move into a not-yet-existing folder creates it (and ancestors)
// on every backend. The default impl walks ancestors leaf→root until one
// already exists, then creates the missing ones shallowest-first, pre-checking
// existence so it never re-creates (or, on MTP, duplicates) an existing level.
// ============================================================================

use super::{ListingProgress, VolumeError};
use crate::ignore_poison::{IgnorePoison, RwLockIgnorePoison};
use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Mutex;
use std::sync::RwLock as StdRwLock;

#[tokio::test]
async fn create_directory_all_creates_deeply_nested_missing_dest() {
    let volume = InMemoryVolume::new("Dest");

    volume
        .create_directory_all(Path::new("/a/b/c/d"))
        .await
        .expect("recursive create should land a deeply-nested missing dest");

    // Every ancestor now exists as a real directory.
    for dir in ["/a", "/a/b", "/a/b/c", "/a/b/c/d"] {
        assert!(volume.exists(Path::new(dir)).await, "{dir} should exist");
        assert!(
            volume
                .is_directory(Path::new(dir))
                .await
                .expect("ancestor should be statable"),
            "{dir} should be a directory"
        );
    }

    // A file can be written into the freshly-created leaf.
    volume
        .create_file(Path::new("/a/b/c/d/file.txt"), b"hi")
        .await
        .expect("file should land in the created dest");
    assert!(volume.exists(Path::new("/a/b/c/d/file.txt")).await);
}

#[tokio::test]
async fn create_directory_all_is_idempotent_on_existing_dest() {
    let volume = InMemoryVolume::new("Dest");

    volume
        .create_directory_all(Path::new("/x/y"))
        .await
        .expect("first create");

    // Re-running against an already-existing dest is a no-op, never an error
    // (a merge into an existing folder must not fail the gate).
    volume
        .create_directory_all(Path::new("/x/y"))
        .await
        .expect("re-running against an existing dest is a no-op");

    assert!(
        volume
            .is_directory(Path::new("/x/y"))
            .await
            .expect("dest should be statable")
    );
}

#[tokio::test]
async fn create_directory_all_only_creates_the_missing_tail() {
    let volume = InMemoryVolume::new("Dest");
    // Pre-existing ancestors (a merge target).
    volume.create_directory(Path::new("/a")).await.unwrap();
    volume.create_directory(Path::new("/a/b")).await.unwrap();

    // Only `/a/b/c` and `/a/b/c/d` are missing; the existing levels are left
    // alone (InMemory's `create_directory` would have errored `AlreadyExists`
    // had the helper blindly re-created them).
    volume
        .create_directory_all(Path::new("/a/b/c/d"))
        .await
        .expect("should create only the missing tail");

    assert!(
        volume
            .is_directory(Path::new("/a/b/c"))
            .await
            .expect("created level should be statable")
    );
    assert!(
        volume
            .is_directory(Path::new("/a/b/c/d"))
            .await
            .expect("created level should be statable")
    );
}

#[tokio::test]
async fn create_directory_all_surfaces_create_failure_as_typed_error() {
    // A backend whose `create_directory` fails at a specific level: the helper
    // must surface the typed `VolumeError`, never silently swallow it.
    let volume = MockDirVolume::new(/* errors_on_existing */ true);
    volume.fail_create_at(Path::new("/a/b"));

    let err = volume
        .create_directory_all(Path::new("/a/b/c"))
        .await
        .expect_err("a create failure mid-walk must surface");
    assert!(matches!(err, VolumeError::IoError { .. }), "got {err:?}");
}

#[tokio::test]
async fn create_directory_all_pre_checks_existence_on_mtp_like_backend() {
    // MTP-shaped backend: `create_directory` does NOT error on an existing dir
    // (it would make a duplicate sibling), and `create_directory_errors_on_existing_dir()`
    // is false. The helper must pre-check existence and call `create_directory`
    // ONLY for the truly-missing levels, so a pre-existing ancestor is never
    // duplicated.
    let volume = MockDirVolume::new(/* errors_on_existing */ false);
    // `/photos` already on the device.
    volume.seed_existing(Path::new("/photos"));

    volume
        .create_directory_all(Path::new("/photos/2026/trip"))
        .await
        .expect("recursive create over a partially-existing MTP tree");

    // `create_directory` ran for exactly the two missing levels, never for the
    // pre-existing `/photos` (which would have duplicated it on MTP).
    let calls = volume.create_calls();
    assert_eq!(
        calls,
        vec![PathBuf::from("/photos/2026"), PathBuf::from("/photos/2026/trip")],
        "must create only the missing levels, in shallowest-first order"
    );
}

/// Minimal `Volume` double for the recursive-create tests. Tracks a set of
/// existing directory paths and records every `create_directory` call, so a
/// test can assert WHICH levels the helper created. `errors_on_existing`
/// toggles between the LocalPosix/SMB/InMemory semantics (error on a re-create)
/// and the MTP semantics (silently duplicate). `fail_create_at` forces a typed
/// failure at one level.
struct MockDirVolume {
    errors_on_existing: bool,
    existing: StdRwLock<HashSet<PathBuf>>,
    create_calls: Mutex<Vec<PathBuf>>,
    fail_at: Mutex<Option<PathBuf>>,
}

impl MockDirVolume {
    fn new(errors_on_existing: bool) -> Self {
        Self {
            errors_on_existing,
            existing: StdRwLock::new(HashSet::new()),
            create_calls: Mutex::new(Vec::new()),
            fail_at: Mutex::new(None),
        }
    }

    fn seed_existing(&self, path: &Path) {
        self.existing.write_ignore_poison().insert(path.to_path_buf());
    }

    fn fail_create_at(&self, path: &Path) {
        *self.fail_at.lock_ignore_poison() = Some(path.to_path_buf());
    }

    fn create_calls(&self) -> Vec<PathBuf> {
        self.create_calls.lock_ignore_poison().clone()
    }
}

impl Volume for MockDirVolume {
    fn name(&self) -> &str {
        "mock-dir"
    }
    fn root(&self) -> &Path {
        Path::new("/")
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.existing.read_ignore_poison().contains(path) })
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if self.existing.read_ignore_poison().contains(path) {
                Ok(true)
            } else {
                Err(VolumeError::NotFound(path.display().to_string()))
            }
        })
    }
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.create_calls.lock_ignore_poison().push(path.to_path_buf());
            if self.fail_at.lock_ignore_poison().as_deref() == Some(path) {
                return Err(VolumeError::IoError {
                    message: "injected create failure".to_string(),
                    raw_os_error: None,
                });
            }
            let mut existing = self.existing.write_ignore_poison();
            if existing.contains(path) && self.errors_on_existing {
                return Err(VolumeError::AlreadyExists(path.display().to_string()));
            }
            // MTP-like backend (errors_on_existing == false) inserts idempotently;
            // a real one would make a DUPLICATE sibling, which the helper avoids
            // by pre-checking existence.
            existing.insert(path.to_path_buf());
            Ok(())
        })
    }
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        self.errors_on_existing
    }
}
