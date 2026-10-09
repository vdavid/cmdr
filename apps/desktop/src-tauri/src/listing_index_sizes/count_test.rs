//! Calculating folder sizes on demand: which folders a count walks, what the
//! pane hears while it walks, and how it ends.

use std::sync::{Arc, Mutex};

use cmdr_fs::volume::InMemoryVolume;

use super::ListingIndexSizesChanged;
use super::count::{CountFolderSizesError, CountPlan, cancel, count_with, plan};
use crate::file_system::listing::cached_listing::LISTING_CACHE;
use crate::file_system::listing::caching_test_support::{TestListing, TestListingGuard};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::volume::{LocalPosixVolume, Volume};
use crate::ignore_poison::RwLockIgnorePoison;
use crate::test_support::TestDir;

const DIR: &str = "/data";

fn file(path: &str, size: u64) -> FileEntry {
    let name = path.rsplit('/').next().expect("a path has a name").to_string();
    FileEntry {
        size: Some(size),
        ..FileEntry::new(name, path.to_string(), false, false)
    }
}

fn folder(path: &str) -> FileEntry {
    let name = path.rsplit('/').next().expect("a path has a name").to_string();
    FileEntry::new(name, path.to_string(), true, false)
}

fn sized(path: &str, size: u64, complete: bool) -> FileEntry {
    FileEntry {
        recursive_size: Some(size),
        recursive_size_complete: Some(complete),
        ..folder(path)
    }
}

fn pane(tag: &str, rows: Vec<FileEntry>) -> TestListingGuard {
    TestListing::new()
        .path(DIR)
        .include_hidden(false)
        .entries(rows)
        .insert(tag)
}

fn volume() -> Arc<dyn Volume> {
    Arc::new(InMemoryVolume::with_entries(
        "test",
        vec![
            folder("/data"),
            folder("/data/photos"),
            file("/data/photos/a.jpg", 100),
            file("/data/photos/b.jpg", 200),
            folder("/data/photos/raw"),
            file("/data/photos/raw/c.cr3", 1_000),
            folder("/data/empty"),
            file("/data/notes.txt", 5),
        ],
    ))
}

fn rows() -> Vec<FileEntry> {
    vec![
        folder("/data/empty"),
        file("/data/notes.txt", 5),
        folder("/data/photos"),
    ]
}

fn paths(plan: &CountPlan) -> Vec<&str> {
    plan.folders.iter().map(|f| f.path.as_str()).collect()
}

fn collect() -> (
    Arc<Mutex<Vec<ListingIndexSizesChanged>>>,
    impl Fn(ListingIndexSizesChanged) + Sync,
) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink_events = Arc::clone(&events);
    (events, move |event| sink_events.lock().expect("test lock").push(event))
}

#[test]
fn a_count_walks_every_folder_row_without_an_exact_size() {
    let mut link = folder("/data/link");
    link.is_symlink = true;
    let listing = pane(
        "count-plan",
        vec![
            sized("/data/known", 9, true),
            sized("/data/partial", 9, false),
            link,
            folder("/data/new"),
            file("/data/f.txt", 1),
        ],
    );

    let plan = plan(listing.id(), false, None, |_| false).expect("listing is cached");

    assert_eq!(
        paths(&plan),
        vec!["/data/partial", "/data/new"],
        "in the pane's row order"
    );
    assert_eq!(plan.dir.to_string_lossy(), DIR);
}

#[test]
fn space_on_a_folder_counts_just_that_folder_and_never_one_already_exact() {
    let listing = pane(
        "count-plan-only",
        vec![sized("/data/known", 9, true), folder("/data/other")],
    );

    let exact = plan(listing.id(), false, Some(&["/data/known".to_string()]), |_| false).expect("cached");
    let other = plan(listing.id(), false, Some(&["/data/other".to_string()]), |_| false).expect("cached");

    assert!(
        exact.folders.is_empty(),
        "the frontend skips an exact size, and so does the backend"
    );
    assert_eq!(paths(&other), vec!["/data/other"]);
}

#[test]
fn a_gone_listing_is_refused() {
    assert_eq!(
        plan("no-such-listing", false, None, |_| false),
        Err(CountFolderSizesError::Gone {
            listing_id: "no-such-listing".to_string()
        })
    );
}

#[test]
fn on_an_indexed_volume_only_folders_the_index_says_nothing_about_are_walked() {
    let listing = pane(
        "count-plan-indexed",
        vec![sized("/data/scanning", 9, false), folder("/data/excluded")],
    );

    let plan = plan(listing.id(), false, None, |_| true).expect("listing is cached");

    assert_eq!(
        paths(&plan),
        vec!["/data/excluded"],
        "the index finishes its own lower bound"
    );
}

#[tokio::test]
async fn each_folder_lands_with_its_exact_size_in_the_cache_and_the_pane() {
    let listing = pane("count-run", rows());
    let (events, sink) = collect();
    let plan = plan(listing.id(), false, None, |_| false).expect("cached");

    let outcome = count_with(listing.id(), plan, volume(), &sink).await;

    assert_eq!(outcome.counted, 2);
    assert_eq!(outcome.unreadable, 0);
    assert!(!outcome.cancelled);
    let photos = listing
        .entries()
        .into_iter()
        .find(|e| e.name == "photos")
        .expect("row is there");
    assert_eq!(photos.recursive_size, Some(1_300));
    assert_eq!(photos.recursive_file_count, Some(3));
    assert_eq!(photos.recursive_size_complete, Some(true));

    // A walk shorter than the hourglass delay sends its exact size and nothing
    // before it: no flash of "<dir>" with an hourglass.
    let events = events.lock().expect("test lock");
    let photos_events: Vec<_> = events.iter().filter(|e| e.folders[0].path == "/data/photos").collect();
    assert_eq!(photos_events.len(), 1);
    let stats = photos_events[0].folders[0].stats.as_ref().expect("a reading");
    assert!(!stats.recursive_size_pending && stats.recursive_size_complete);
    assert_eq!(stats.recursive_size, 1_300);
}

#[tokio::test]
async fn a_folder_the_disk_cant_read_goes_back_to_what_it_showed_and_is_reported() {
    // A real local volume, so the count takes its own walk.
    let dir = TestDir::new("count-unreadable");
    std::fs::create_dir_all(dir.join("photos")).expect("mkdir");
    std::fs::write(dir.join("photos/a.jpg"), vec![0u8; 40]).expect("write");
    let root = dir.to_path_buf();
    let ghost = root.join("ghost").to_string_lossy().to_string();
    let photos = root.join("photos").to_string_lossy().to_string();
    let listing = TestListing::new()
        .path(root.clone())
        .include_hidden(false)
        .entries(vec![
            FileEntry {
                recursive_size: Some(7),
                recursive_size_complete: Some(false),
                ..FileEntry::new("ghost".to_string(), ghost.clone(), true, false)
            },
            FileEntry::new("photos".to_string(), photos.clone(), true, false),
        ])
        .insert("count-unreadable");
    let (_events, sink) = collect();
    let plan = plan(listing.id(), false, None, |_| false).expect("cached");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Disk", "/"));

    let outcome = count_with(listing.id(), plan, local, &sink).await;

    assert_eq!(
        (outcome.counted, outcome.unreadable),
        (1, 1),
        "photos counted, ghost isn't on disk"
    );
    let rows = listing.entries();
    let ghost_row = rows.iter().find(|e| e.path == ghost).expect("row is there");
    assert_eq!(
        (ghost_row.recursive_size, ghost_row.recursive_size_complete),
        (Some(7), Some(false)),
        "as it was"
    );
    let photos_row = rows.iter().find(|e| e.path == photos).expect("row is there");
    assert_eq!(
        (photos_row.recursive_size, photos_row.recursive_size_complete),
        (Some(40), Some(true))
    );
}

#[tokio::test]
async fn a_count_stops_when_its_pane_moves_on() {
    let listing = pane("count-gone", rows());
    let id = listing.id().to_string();
    let plan = plan(&id, false, None, |_| false).expect("cached");
    drop(listing);
    let (events, sink) = collect();

    let outcome = count_with(&id, plan, volume(), &sink).await;

    assert!(outcome.cancelled);
    assert_eq!(outcome.counted, 0);
    assert!(events.lock().expect("test lock").is_empty());
    assert!(!cancel(&id), "the job left with its listing");
}

#[tokio::test]
async fn a_cancelled_count_stops_and_says_so() {
    let listing = pane("count-cancel", rows());
    let listing_id = listing.id().to_string();
    let sink = move |_: ListingIndexSizesChanged| {
        cancel(&listing_id);
    };
    let plan = plan(listing.id(), false, None, |_| false).expect("cached");

    let outcome = count_with(listing.id(), plan, volume(), &sink).await;

    assert!(outcome.cancelled);
    assert!(outcome.counted < 2, "it stopped before the end");
    assert!(!cancel(listing.id()), "a stopped count is no longer running");
}

#[tokio::test]
async fn a_stopped_lower_bound_on_an_indexed_volume_stays_retryable() {
    // The index owns every size it has, but a lower bound a stopped count left is
    // ours: a plan must offer it again, or Space can never finish it.
    let listing = pane("count-retry", vec![folder("/data/photos")]);
    let (_events, sink) = collect();
    let only = ["/data/photos".to_string()];
    let first = plan(listing.id(), false, Some(&only), |_| true).expect("cached");
    assert_eq!(paths(&first), vec!["/data/photos"], "no size yet: ours to count");
    let ran = count_with(listing.id(), first, volume(), &sink).await;
    assert_eq!(ran.counted, 1);
    // Leave a lower bound on the row, as a stopped walk does.
    if let Some(l) = LISTING_CACHE.write_ignore_poison().get_mut(listing.id()) {
        l.update_index_sizes_by_path(&["/data/photos".to_string()], |_, e| {
            e.recursive_size_complete = Some(false);
        });
    }

    let again = plan(listing.id(), false, Some(&only), |_| true).expect("cached");

    assert_eq!(
        paths(&again),
        vec!["/data/photos"],
        "the count's own lower bound is retryable"
    );
}

#[tokio::test(start_paused = true)]
async fn a_long_walk_shows_the_hourglass_on_its_folder_and_on_the_ones_waiting_then_esc_keeps_or_restores() {
    use crate::listing_index_sizes::count::HOURGLASS_DELAY;
    use crate::test_support::{WedgedVolume, wait_until_async};
    use std::time::Duration;

    let listing = pane(
        "count-hourglass",
        vec![folder("/data/slow"), sized("/data/next", 3, false)],
    );
    let (events, sink) = collect();
    let plan = plan(listing.id(), false, None, |_| false).expect("cached");
    let id = listing.id().to_string();
    let wedged: Arc<dyn Volume> = Arc::new(WedgedVolume::new("wedged"));
    // Esc completes the UI wait; the owned backend worker stays parked until
    // this test's runtime shuts down, because this volume never returns from I/O.
    let count = tokio::spawn(async move { count_with(&id, plan, wedged, &sink).await });

    let lit = |events: &[ListingIndexSizesChanged], path: &str| {
        events.iter().any(|e| {
            e.folders
                .iter()
                .any(|f| f.path == path && f.stats.as_ref().is_some_and(|s| s.recursive_size_pending))
        })
    };
    // allowed-test-sleep: the subject is that nothing shows BEFORE the delay; the clock is paused, so this is instant.
    tokio::time::sleep(HOURGLASS_DELAY / 2).await;
    assert!(
        events.lock().expect("test lock").is_empty(),
        "nothing before the delay: no flicker"
    );
    wait_until_async(Duration::from_secs(2), "both rows to show the hourglass", || {
        let events = events.lock().expect("test lock");
        lit(&events, "/data/slow") && lit(&events, "/data/next")
    })
    .await;

    assert!(cancel(listing.id()), "Esc finds the running count");
    let outcome = count.await.expect("the count task ends");
    assert!(outcome.cancelled);
    let rows = listing.entries();
    let slow = rows.iter().find(|e| e.path == "/data/slow").expect("row");
    assert_eq!(
        slow.recursive_size_complete,
        Some(false),
        "the walked folder keeps its lower bound"
    );
    let next = rows.iter().find(|e| e.path == "/data/next").expect("row");
    assert_eq!(
        (next.recursive_size, next.recursive_size_complete),
        (Some(3), Some(false)),
        "the waiting one is back as it was"
    );
}
