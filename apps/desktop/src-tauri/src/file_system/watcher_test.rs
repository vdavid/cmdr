//! Tests for the listing watcher: event-path rebasing, the batch handler
//! (`handle_directory_change*` with hand-built events), and arming
//! (`start_watching`, the detached arm, the reconcile on arm).
//!
//! A test build arms no OS stream (`file_system/DETAILS.md` § "Tests arm no OS
//! stream"), so arming cells run in milliseconds. The one cell that needs real
//! FSEvents delivery opts in with `watch_for_real`. The diff logic itself is
//! `listing/diff_test.rs`.

use super::volume::Volume;
use super::watcher::{event_targets_watch_root, rebase_event_path};
use cmdr_fs::entry::FileEntry;
use std::path::{Path, PathBuf};

fn make_entry_in(name: &str, dir: &str, size: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        ..FileEntry::new(name.to_string(), format!("{}/{}", dir, name), false, false)
    }
}

#[test]
fn test_rebase_event_path_exact_parent_match() {
    let dir = Path::new("/Users/test_user_12/docs");
    assert_eq!(
        rebase_event_path(Path::new("/Users/test_user_12/docs/file.txt"), dir, dir),
        Some(PathBuf::from("/Users/test_user_12/docs/file.txt"))
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
    let listing_dir = Path::new("/Users/test_user_12/Library/CloudStorage/GoogleDrive-test_user_12/My Drive");
    let canonical_dir = Path::new("/Users/test_user_12/My Drive");
    assert_eq!(
        rebase_event_path(
            Path::new("/Users/test_user_12/My Drive/photo.jpg"),
            listing_dir,
            canonical_dir
        ),
        Some(PathBuf::from(
            "/Users/test_user_12/Library/CloudStorage/GoogleDrive-test_user_12/My Drive/photo.jpg"
        )),
        "event under the symlink-resolved target must rebase into the listing's own path space"
    );
    // A sibling of the real target, outside the watched dir, stays rejected.
    assert_eq!(
        rebase_event_path(
            Path::new("/Users/test_user_12/Other/photo.jpg"),
            listing_dir,
            canonical_dir
        ),
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
    let listing_dir = Path::new("/Users/test_user_12/Library/CloudStorage/GoogleDrive-test_user_12/My Drive");
    let canonical_dir = Path::new("/Users/test_user_12/My Drive");
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
/// and updates the listing cache when the volume's contents have changed.
///
/// This also covers the `notify_full_refresh` / `FullRefresh` code path in `cmdr-listing`'s `refresh.rs`,
/// since both use the same mechanism: re-read via Volume, compute diff, update cache.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_handle_directory_change_refreshes_from_volume() {
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::watcher::handle_directory_change;
    use cmdr_listing::testing::TestListing;
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
        .insert(crate::listing_host::listings(), "watcher-hdc");

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
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::watcher::handle_directory_change;
    use cmdr_listing::testing::TestListing;
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
        .insert(crate::listing_host::listings(), "watcher-new-entries");

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
///
/// The listing watcher's one real-stream cell (`watch_for_real`): what FSEvents reports for a
/// replaced root is the operating system's to answer, so a scripted batch could only
/// assert our guess at it.
#[cfg(target_os = "macos")]
#[test]
fn entries_that_went_with_a_replaced_watch_root_leave_the_listing() {
    use crate::file_system::watcher::{has_os_stream, start_watching, watch_for_real};
    use crate::test_support::{TestDir, wait_until};
    use cmdr_fs::testing::unique_test_id;
    use cmdr_listing::testing::TestListing;
    use cmdr_local::reading::list_directory_core;
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
        .entries(
            list_directory_core(&watched, &crate::listing_host::backends::AppListings).expect("the fresh dir lists"),
        )
        .insert(crate::listing_host::listings(), "watch-root-replaced");
    watch_for_real(listing.id());
    start_watching(listing.id(), &watched).expect("start_watching should succeed on a real dir");
    assert!(has_os_stream(listing.id()), "this cell needs a real FSEvents stream");

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
    use crate::file_system::watcher::{start_watching_detached, stop_watching};
    use crate::test_support::{TestDir, wait_until};
    use cmdr_listing::testing::TestListing;
    use std::time::Duration;

    let scratch = TestDir::new("detached-arm-attaches");
    let watched = scratch.join("dir");
    std::fs::create_dir(&watched).expect("scratch dir is writable");

    let listing = TestListing::new()
        .path(&watched)
        .insert(crate::listing_host::listings(), "detached-arm-attaches");
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
    use crate::file_system::watcher::{arm_and_reconcile, start_watching};
    use crate::test_support::TestDir;
    use cmdr_fs::testing::unique_test_id;
    use std::time::SystemTime;

    let scratch = TestDir::new("detached-arm-orphan");
    let watched = scratch.join("dir");
    std::fs::create_dir(&watched).expect("scratch dir is writable");

    // No listing in the cache: exactly the state `list_directory_end` leaves behind when
    // it beats the arm.
    let listing_id = unique_test_id("detached-arm-orphan");

    arm_and_reconcile(&listing_id, &watched, SystemTime::now(), start_watching);

    assert!(
        !is_watching(&listing_id),
        "an arm for an already-ended listing must not leave its watch behind"
    );
}

/// A file created while the detached arm is in flight: after the listing read, before
/// the FSEvents stream exists. The stream starts "since now", so it never reports that
/// create, and under fseventsd load the arm lands seconds late (4–10 s measured in E2E,
/// 2026-10-13). Without the reconcile re-read the file stays missing from the pane until
/// the user navigates away and back.
///
/// The directory's mtime is pushed into the past first, so the re-read here is earned by
/// the mtime MOVING, not by the "modified too recently to trust" rule.
#[cfg(target_os = "macos")]
#[test]
fn a_detached_arm_rereads_a_change_that_landed_while_it_was_in_flight() {
    use crate::file_system::watcher::{arm_and_reconcile, start_watching, stop_watching};
    use crate::test_support::{TestDir, wait_until};
    use cmdr_fs::testing::unique_test_id;
    use cmdr_listing::testing::TestListing;
    use cmdr_local::reading::list_directory_core;
    use std::time::{Duration, SystemTime};

    let scratch = TestDir::new("arm-in-flight-change");
    let watched = scratch.join("dir");
    std::fs::create_dir(&watched).expect("scratch dir is writable");
    std::fs::write(watched.join("alpha.txt"), b"a").expect("scratch dir is writable");
    filetime::set_file_mtime(&watched, filetime::FileTime::from_unix_time(1_600_000_000, 0))
        .expect("the scratch dir's mtime is settable");

    // An unregistered volume id, as in the replaced-root test: the re-read takes the
    // `list_directory_core` fallback, a real readdir.
    let listing = TestListing::new()
        .volume(&unique_test_id("arm-in-flight-vol"))
        .path(&watched)
        .entries(
            list_directory_core(&watched, &crate::listing_host::backends::AppListings).expect("the fresh dir lists"),
        )
        .insert(crate::listing_host::listings(), "arm-in-flight-change");

    arm_and_reconcile(listing.id(), &watched, SystemTime::now(), |id, path| {
        std::fs::write(path.join("late.txt"), b"l").expect("scratch dir is writable");
        start_watching(id, path)
    });

    // Generous deadline: the re-read hops onto the async runtime, which a saturated
    // `rust-tests` run starves. A satisfied wait returns immediately.
    wait_until(
        Duration::from_secs(30),
        "the file created mid-arm to reach the listing",
        || listing.entry_names().iter().any(|name| name == "late.txt"),
    );

    stop_watching(listing.id());
}

/// A test build arms the bookkeeping and nothing else: a test that asserts a watch is
/// armed, disarmed, or reconciled must not wait on the host-wide `fseventsd`, whose
/// stream start took 4–10 s under a busy `rust-tests` run. Only a listing that opted in
/// with `watch_for_real` gets an OS stream.
#[test]
fn an_arm_in_a_test_build_opens_no_os_stream() {
    use crate::file_system::watcher::{has_os_stream, start_watching, stop_watching};
    use crate::test_support::TestDir;
    use cmdr_fs::testing::unique_test_id;
    use cmdr_fs::volume::WatchCoverage;

    let scratch = TestDir::new("scripted-arm");
    let listing_id = unique_test_id("scripted-arm");

    start_watching(&listing_id, &scratch).expect("arming a real dir succeeds");

    assert_eq!(
        crate::file_system::watcher::coverage_for_listings(std::slice::from_ref(&listing_id)),
        WatchCoverage::EveryWriter,
        "the scripted watch still records its coverage, decided by the same statfs"
    );
    assert!(
        !has_os_stream(&listing_id),
        "a test-build arm must not open an OS stream"
    );
    stop_watching(&listing_id);
}

/// The reconcile's decision, one rule per case. `opened_at` is a fixed instant, so the
/// "recent" boundary is exact.
#[test]
fn an_arm_rereads_only_when_the_directory_mtime_cannot_rule_out_a_change() {
    use crate::file_system::watcher::arm_missed_a_change;
    use std::time::{Duration, SystemTime};

    let opened_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let old = opened_at - Duration::from_secs(3600);
    let secs = Duration::from_secs;

    assert!(
        !arm_missed_a_change(Some(old), Some(old), opened_at),
        "an old, unmoved mtime is trusted"
    );
    assert!(
        arm_missed_a_change(Some(old), Some(old + secs(1)), opened_at),
        "a moved mtime means a child changed mid-arm"
    );
    assert!(
        arm_missed_a_change(
            Some(opened_at - Duration::from_millis(1500)),
            Some(opened_at - Duration::from_millis(1500)),
            opened_at
        ),
        "an mtime within a 2 s tick of the open could hide a same-tick change"
    );
    assert!(
        !arm_missed_a_change(Some(opened_at - secs(3)), Some(opened_at - secs(3)), opened_at),
        "an mtime a full tick before the open is trusted"
    );
    assert!(
        arm_missed_a_change(Some(opened_at + secs(1)), Some(opened_at + secs(1)), opened_at),
        "an mtime after the open (a change in the blocking-pool wait) is re-read"
    );
    assert!(
        arm_missed_a_change(None, Some(old), opened_at),
        "a failed stat before is re-read"
    );
    assert!(
        arm_missed_a_change(Some(old), None, opened_at),
        "a failed stat after is re-read"
    );
}

/// A real dotfile rewritten in a real folder, fed through the watcher's own
/// incremental path: the idle cost on a pane on `~`. `hidden` is the pane's setting.
#[cfg(test)]
fn rewrite_a_dotfile_beside_a_visible_file(
    tag: &str,
    hidden: bool,
) -> (crate::test_support::TestDir, cmdr_listing::testing::TestListingGuard) {
    use crate::file_system::watcher::{ListingChange, handle_directory_change_incremental};
    use cmdr_fs::testing::unique_test_id;
    use cmdr_listing::testing::TestListing;
    use cmdr_listing::testing::hold_diffs;
    use cmdr_local::reading::list_directory_core;

    let scratch = crate::test_support::TestDir::new(tag);
    let dotfile = scratch.join(".zsh_history");
    std::fs::write(&dotfile, b"ls\n").expect("scratch dir is writable");
    std::fs::write(scratch.join("notes.txt"), b"n").expect("scratch dir is writable");

    // An unregistered volume id, so the index enrich and the volume registry stay out of it.
    let listing = TestListing::new()
        .volume(&unique_test_id(&format!("{tag}-vol")))
        .path(scratch.to_path_buf())
        .include_hidden(hidden)
        .entries(
            list_directory_core(&scratch, &crate::listing_host::backends::AppListings).expect("the fresh dir lists"),
        )
        .insert(crate::listing_host::listings(), tag);
    hold_diffs(listing.engine(), listing.id());

    std::fs::write(&dotfile, b"ls\ncd ~\n").expect("scratch dir is writable");
    handle_directory_change_incremental(listing.id(), vec![ListingChange::Content(dotfile)]);
    (scratch, listing)
}

#[test]
fn a_dotfile_write_reaches_the_cache_but_not_a_pane_hiding_dotfiles() {
    use cmdr_listing::testing::pending_changes;

    let (_scratch, listing) = rewrite_a_dotfile_beside_a_visible_file("watch-dotfile-hidden-off", false);

    assert_eq!(
        pending_changes(listing.engine(), listing.id()).len(),
        0,
        "nothing the pane shows changed"
    );
    let history = listing.entries().into_iter().find(|e| e.name == ".zsh_history");
    assert_eq!(
        history.and_then(|e| e.size),
        Some(8),
        "the cache holds the new size anyway, so showing hidden files later is instant and right"
    );
}

#[test]
fn a_dotfile_write_reaches_a_pane_showing_dotfiles() {
    use cmdr_listing::DiffChangeType;
    use cmdr_listing::testing::pending_changes;

    let (_scratch, listing) = rewrite_a_dotfile_beside_a_visible_file("watch-dotfile-hidden-on", true);

    let queued: Vec<_> = pending_changes(listing.engine(), listing.id())
        .into_iter()
        .map(|c| (c.change_type, c.entry.name))
        .collect();
    assert_eq!(queued, vec![(DiffChangeType::Modify, ".zsh_history".to_string())]);
}
