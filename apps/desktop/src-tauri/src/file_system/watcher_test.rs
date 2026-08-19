//! Tests for file system watcher

// Note: The watcher tests require async handling and app context
// which makes them difficult to unit test. Key functionality is tested via:
// 1. `compute_diff` tests in `listing/diff_test.rs` (unit tests for diff logic)
// 2. Manual testing of file watching in the actual app
// 3. Integration tests with the full Tauri app

// The start_watching/stop_watching functions require a running app context
// to emit events, so proper testing requires integration tests.

use super::listing::FileEntry;
use super::volume::Volume;
use super::watcher::{event_targets_watch_root, rebase_event_path};
use std::path::{Path, PathBuf};

fn make_entry_in(name: &str, dir: &str, size: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(name.to_string(), format!("{}/{}", dir, name), false, false)
    }
}

#[test]
fn test_rebase_event_path_exact_parent_match() {
    let dir = Path::new("/Users/jane/docs");
    assert_eq!(
        rebase_event_path(Path::new("/Users/jane/docs/file.txt"), dir, dir),
        Some(PathBuf::from("/Users/jane/docs/file.txt"))
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_rebase_event_path_resolves_private_symlink() {
    // Pre-fix, FSEvents' canonical /private/tmp/... paths never matched a listing
    // watched as /tmp/..., so the pane silently never updated.
    let dir = Path::new("/tmp/work");
    assert_eq!(
        rebase_event_path(Path::new("/private/tmp/work/new-dir"), dir, dir),
        Some(PathBuf::from("/tmp/work/new-dir")),
        "canonical event path must rebase into the listing's /tmp path space"
    );
    // And the inverse orientation (listing opened via the canonical path)
    let dir = Path::new("/private/tmp/work");
    assert_eq!(
        rebase_event_path(Path::new("/tmp/work/new-dir"), dir, dir),
        Some(PathBuf::from("/private/tmp/work/new-dir"))
    );
}

#[test]
fn test_rebase_event_path_resolves_symlinked_watch_root() {
    // Google Drive exposes "My Drive" as a symlink (…/CloudStorage/GoogleDrive-…/My Drive
    // → ~/My Drive). FSEvents resolves the watched symlink and reports events under the
    // real target, which never matched the listing's symlink-form path, so the pane
    // silently never updated on rename/create/delete. `canonical_dir` (the symlink-
    // resolved watch root) closes the gap; the rebase still lands in the listing's own
    // path space.
    let listing_dir = Path::new("/Users/jane/Library/CloudStorage/GoogleDrive-jane/My Drive");
    let canonical_dir = Path::new("/Users/jane/My Drive");
    assert_eq!(
        rebase_event_path(Path::new("/Users/jane/My Drive/photo.jpg"), listing_dir, canonical_dir),
        Some(PathBuf::from(
            "/Users/jane/Library/CloudStorage/GoogleDrive-jane/My Drive/photo.jpg"
        )),
        "event under the symlink-resolved target must rebase into the listing's own path space"
    );
    // A sibling of the real target, outside the watched dir, stays rejected.
    assert_eq!(
        rebase_event_path(Path::new("/Users/jane/Other/photo.jpg"), listing_dir, canonical_dir),
        None
    );
}

#[test]
fn test_rebase_event_path_rejects_non_children() {
    // Different directory
    assert_eq!(
        rebase_event_path(
            Path::new("/private/tmp/other/file"),
            Path::new("/tmp/work"),
            Path::new("/tmp/work")
        ),
        None
    );
    // Deeper descendant (watcher is non-recursive; only direct children count)
    assert_eq!(
        rebase_event_path(
            Path::new("/private/tmp/work/sub/file"),
            Path::new("/tmp/work"),
            Path::new("/tmp/work")
        ),
        None
    );
    // Prefix-similar but distinct dir name must not match (/tmpdir is not /tmp)
    assert_eq!(
        rebase_event_path(Path::new("/tmpdir/file"), Path::new("/tmp"), Path::new("/tmp")),
        None
    );
}

#[test]
fn event_targets_watch_root_matches_the_dir_itself() {
    // The signal that a directory was replaced wholesale is an event naming the WATCH
    // ROOT, which `rebase_event_path` rejects (its parent isn't the watched dir). This
    // predicate is what catches it.
    let dir = Path::new("/tmp/work");
    assert!(event_targets_watch_root(Path::new("/tmp/work"), dir, dir));

    // A symlinked watch root: the notifier reports the resolved target.
    let listing_dir = Path::new("/Users/jane/Library/CloudStorage/GoogleDrive-jane/My Drive");
    let canonical_dir = Path::new("/Users/jane/My Drive");
    assert!(event_targets_watch_root(canonical_dir, listing_dir, canonical_dir));
}

#[cfg(target_os = "macos")]
#[test]
fn event_targets_watch_root_matches_the_private_symlink_form() {
    // The predicate needs BOTH path forms, same as `rebase_event_path` above.
    // `firmlinks::normalize_path` maps `/tmp` → `/private/tmp` only on macOS (the
    // `PRIVATE_SYMLINK_DIRS` block is `cfg`-gated), so this equivalence is a macOS
    // fact, not a portable one: on Linux the two paths normalize to themselves and
    // the predicate correctly says no.
    let dir = Path::new("/tmp/work");
    assert!(
        event_targets_watch_root(Path::new("/private/tmp/work"), dir, dir),
        "FSEvents' canonical form of the watch root is still the watch root"
    );
}

#[test]
fn event_targets_watch_root_rejects_children_and_neighbours() {
    let dir = Path::new("/tmp/work");
    assert!(
        !event_targets_watch_root(Path::new("/tmp/work/file.txt"), dir, dir),
        "a child is an ordinary incremental event, not a replacement of the root"
    );
    assert!(!event_targets_watch_root(Path::new("/tmp"), dir, dir));
    // Prefix-similar but distinct (/tmp/workshop is not /tmp/work).
    assert!(!event_targets_watch_root(Path::new("/tmp/workshop"), dir, dir));
}

// ============================================================================
// handle_directory_change integration tests
// ============================================================================

/// Tests that `handle_directory_change` re-reads a directory via the Volume trait
/// and updates the LISTING_CACHE when the volume's contents have changed.
///
/// This also covers the `notify_full_refresh` / `FullRefresh` code path in caching.rs,
/// since both use the same mechanism: re-read via Volume, compute diff, update cache.
/// (`notify_directory_changed(FullRefresh)` requires a `tauri::AppHandle` and returns
/// early without one, so it can't be tested directly in unit tests.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_handle_directory_change_refreshes_from_volume() {
    use crate::file_system::listing::caching_test_support::TestListing;
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::watcher::handle_directory_change;
    use std::path::PathBuf;
    use std::sync::Arc;

    let volume_id = format!("test-vol-hdc-{}", uuid::Uuid::new_v4());
    let dir_path = PathBuf::from("/testdir");

    // Create volume with files X and Y (paths must match dir_path)
    let volume = Arc::new(InMemoryVolume::with_entries(
        "TestHDC",
        vec![
            make_entry_in("x.txt", "/testdir", Some(100)),
            make_entry_in("y.txt", "/testdir", Some(200)),
        ],
    ));

    // Register in VolumeManager
    get_volume_manager().register(&volume_id, volume);

    // Insert stale cache with only X
    let listing = TestListing::new()
        .volume(&volume_id)
        .path(&dir_path)
        .entries(vec![make_entry_in("x.txt", "/testdir", Some(100))])
        .insert("watcher-hdc");

    handle_directory_change(listing.id()).await;

    // Assert: cache now has both X and Y
    let names = listing.entry_names();
    assert_eq!(names.len(), 2, "Expected 2 entries, got: {:?}", names);
    assert!(names.iter().any(|n| n == "x.txt"), "Missing x.txt in {:?}", names);
    assert!(names.iter().any(|n| n == "y.txt"), "Missing y.txt in {:?}", names);

    get_volume_manager().unregister(&volume_id);
}

/// Tests that `handle_directory_change` correctly handles an InMemoryVolume where
/// entries were added after the initial cache was populated (simulating a file creation
/// on a remote volume that the watcher detected).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_handle_directory_change_detects_new_entries() {
    use crate::file_system::listing::caching_test_support::TestListing;
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::watcher::handle_directory_change;
    use std::path::PathBuf;
    use std::sync::Arc;

    let volume_id = format!("test-vol-new-{}", uuid::Uuid::new_v4());
    let dir_path = PathBuf::from("/testdir");

    // Create volume with file A initially (paths must match dir_path)
    let volume = Arc::new(InMemoryVolume::with_entries(
        "TestNew",
        vec![make_entry_in("a.txt", "/testdir", Some(100))],
    ));

    get_volume_manager().register(&volume_id, volume.clone());

    // Cache reflects current state (A only)
    let listing = TestListing::new()
        .volume(&volume_id)
        .path(&dir_path)
        .entries(vec![make_entry_in("a.txt", "/testdir", Some(100))])
        .insert("watcher-new-entries");

    // Add a new file to the volume (simulating external change).
    volume
        .create_file(Path::new("/testdir/b.txt"), b"new content")
        .await
        .unwrap();

    // Trigger re-read
    handle_directory_change(listing.id()).await;

    // Assert: cache now has A and B
    let names = listing.entry_names();
    assert_eq!(names.len(), 2, "Expected 2 entries, got: {:?}", names);
    assert!(names.iter().any(|n| n == "a.txt"));
    assert!(names.iter().any(|n| n == "b.txt"));

    get_volume_manager().unregister(&volume_id);
}

/// A pane must never keep showing entries that went away when its own directory was
/// replaced wholesale: a `git checkout` across branches, `rsync --delete`, unzipping
/// over a folder, a build regenerating an output dir.
///
/// macOS reports that replacement as `Remove(Folder)` + `Create(Folder)` on the WATCH
/// ROOT plus one `Create` per NEW child, and never a remove for the old children
/// (verified on macOS 26.5.2 / `notify-debouncer-full` 0.7.0, by logging the raw debounced
/// batch against a live pane, 2026-08-08). So a classifier that only rebases
/// direct-child events applies the adds, learns nothing about the removals, and the
/// vanished entries sit in the pane until the user navigates away and back.
///
/// The listing cache IS what the pane serves, so asserting on it is the user-visible
/// symptom, not an internal call count.
#[cfg(target_os = "macos")]
#[test]
fn entries_that_went_with_a_replaced_watch_root_leave_the_listing() {
    use crate::file_system::listing::caching_test_support::{TestListing, unique_test_id};
    use crate::file_system::listing::list_directory_core;
    use crate::file_system::watcher::start_watching;
    use crate::test_support::{TestDir, wait_until};
    use std::time::Duration;

    let scratch = TestDir::new("watch-root-replaced");
    let watched = scratch.join("target");
    std::fs::create_dir(&watched).expect("scratch dir is writable");
    std::fs::write(watched.join("alpha.txt"), b"a").expect("scratch dir is writable");
    std::fs::write(watched.join("beta.txt"), b"b").expect("scratch dir is writable");

    // An unregistered volume id on purpose: the re-read then goes through
    // `handle_directory_change`'s `list_directory_core` fallback, which is the same
    // real readdir a `LocalPosixVolume` would do, without this test depending on
    // whatever else the shared volume registry holds.
    let listing = TestListing::new()
        .volume(&unique_test_id("watch-root-replaced-vol"))
        .path(&watched)
        .entries(list_directory_core(&watched).expect("the fresh dir lists"))
        .insert("watch-root-replaced");
    start_watching(listing.id(), &watched).expect("start_watching should succeed on a real dir");

    // Arming an FSEvents stream is asynchronous, so prove the watch is delivering
    // before the replacement. Without this, a miss could just mean "too early".
    std::fs::write(watched.join("probe.txt"), b"p").expect("scratch dir is writable");
    // Generous deadlines: FSEvents delivery and the re-read both hop threads, and a
    // saturated `rust-tests` run starves them. A satisfied wait returns immediately, so
    // the headroom is free except when something is genuinely broken.
    wait_until(Duration::from_secs(30), "the watch to deliver its first event", || {
        listing.entry_names().iter().any(|name| name == "probe.txt")
    });

    // The replacement: same path, new inode, entirely different contents.
    std::fs::remove_dir_all(&watched).expect("scratch dir is writable");
    std::fs::create_dir(&watched).expect("scratch dir is writable");
    std::fs::write(watched.join("gamma.txt"), b"g").expect("scratch dir is writable");

    wait_until(
        Duration::from_secs(45),
        "the listing to show what the replaced directory actually holds",
        || listing.entry_names() == vec!["gamma.txt".to_string()],
    );
}

/// Whether a watch is currently registered for `listing_id`.
///
/// Reads the manager directly rather than going through
/// `Volume::listing_watch_coverage`, which answers `None` for a listing that left the
/// cache and so can't tell "torn down" from "leaked behind a dead listing".
///
/// Gated to match its callers: both tests below are macOS-only (FSEvents), so on Linux the
/// helper is dead code and `-D unused` fails the build.
#[cfg(target_os = "macos")]
fn is_watching(listing_id: &str) -> bool {
    use crate::file_system::watcher::WATCHER_MANAGER;
    use crate::ignore_poison::RwLockIgnorePoison;

    WATCHER_MANAGER.read_ignore_poison().watches.contains_key(listing_id)
}

/// Arming is detached, so the listing pipeline no longer waits on it. It still has to
/// actually attach: a watch that never lands leaves the pane blind to changes on disk,
/// which is the same bug as never arming at all.
#[cfg(target_os = "macos")]
#[test]
fn a_detached_arm_attaches_the_watch() {
    use crate::file_system::listing::caching_test_support::TestListing;
    use crate::file_system::watcher::{start_watching_detached, stop_watching};
    use crate::test_support::{TestDir, wait_until};
    use std::time::Duration;

    let scratch = TestDir::new("detached-arm-attaches");
    let watched = scratch.join("dir");
    std::fs::create_dir(&watched).expect("scratch dir is writable");

    let listing = TestListing::new().path(&watched).insert("detached-arm-attaches");
    assert!(!is_watching(listing.id()), "nothing should be watching before the arm");

    start_watching_detached(listing.id(), &watched);

    // Generous deadline: the arm runs on the blocking pool and a saturated `rust-tests`
    // run starves it. A satisfied wait returns immediately, so the headroom is free.
    wait_until(Duration::from_secs(30), "the detached arm to attach the watch", || {
        is_watching(listing.id())
    });

    stop_watching(listing.id());
}

/// A detached arm can land AFTER its listing already ended, because
/// `list_directory_end` removes the cache entry and then removes a watch that hasn't
/// been inserted yet. The arm has to notice it lost that race and hand the watch back.
///
/// Otherwise every such navigation strands an FSEvents stream, its CFRunLoop thread, and
/// a manager entry for a listing nobody will ever close again: they accumulate for the
/// life of the process and each one keeps costing `fseventsd` fan-out.
///
/// Drives `arm_and_reconcile` directly instead of racing the real spawn, so the losing
/// interleaving is the only one under test.
#[cfg(target_os = "macos")]
#[test]
fn a_detached_arm_that_lost_the_race_leaves_no_watch_behind() {
    use crate::file_system::listing::caching_test_support::unique_test_id;
    use crate::file_system::watcher::arm_and_reconcile;
    use crate::test_support::TestDir;

    let scratch = TestDir::new("detached-arm-orphan");
    let watched = scratch.join("dir");
    std::fs::create_dir(&watched).expect("scratch dir is writable");

    // No listing in the cache: exactly the state `list_directory_end` leaves behind when
    // it beats the arm.
    let listing_id = unique_test_id("detached-arm-orphan");

    arm_and_reconcile(&listing_id, &watched);

    assert!(
        !is_watching(&listing_id),
        "an arm for an already-ended listing must not leave its watch behind"
    );
}
