//! The count queue: requests join the running job, waiting rows light up after
//! the delay, and a cancel puts them back.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use super::jobs::{self, Joined};
use super::{HOURGLASS_DELAY, count_with, plan};
use crate::file_system::listing::caching_test_support::TestListing;
use crate::file_system::listing::metadata::FileEntry;
use crate::listing_index_sizes::ListingIndexSizesChanged;
use crate::listing_index_sizes::refresh::RowSizes;

fn folder(path: &str) -> FileEntry {
    let name = path.rsplit('/').next().expect("a path has a name").to_string();
    FileEntry::new(name, path.to_string(), true, false)
}

fn queued(paths: &[&str]) -> Vec<(String, RowSizes)> {
    paths.iter().map(|p| (p.to_string(), RowSizes::default())).collect()
}

#[tokio::test]
async fn esc_during_resolution_finds_the_job_and_prevents_a_walk() {
    let listing = TestListing::new()
        .path("/data")
        .entries(vec![folder("/data/a")])
        .insert("jobs-startup");
    let id = listing.id().to_string();
    let requested = plan(listing.id(), false, None, |_| false).expect("cached");
    let resolved = async move {
        assert!(jobs::cancel(&id), "Esc must find the job before resolving the volume");
        Some(Arc::new(cmdr_fs::volume::InMemoryVolume::new("startup")) as Arc<dyn crate::file_system::volume::Volume>)
    };
    let outcome = super::count_resolving(listing.id(), requested, resolved, &|_| {
        panic!("no row should be published")
    })
    .await
    .expect("cancelled, not disconnected");
    assert!(outcome.cancelled);
    assert_eq!(outcome.counted, 0);
}

#[test]
fn cancellation_reserves_publication_until_the_owner_finishes() {
    let id = "jobs-cancel-reservation";
    let Joined::Owner(old) = jobs::enqueue(id, queued(&["/a"]), Instant::now()) else {
        panic!("owner");
    };
    assert!(jobs::cancel(id));
    assert!(
        matches!(jobs::enqueue(id, queued(&["/a"]), Instant::now()), Joined::Retiring(_)),
        "a replacement cannot publish before the old owner restores its rows"
    );
    jobs::end(id, &old);
    assert!(matches!(
        jobs::enqueue(id, queued(&["/a"]), Instant::now()),
        Joined::Owner(_)
    ));
    jobs::cancel(id);
}

#[test]
fn restoring_a_manual_partial_preserves_its_ownership() {
    let listing = TestListing::new()
        .path("/data")
        .entries(vec![folder("/data/a")])
        .insert("jobs-manual-restore");
    jobs::mark_manual(listing.id(), "/data/a");
    let Joined::Owner(job) = jobs::enqueue(listing.id(), queued(&["/data/a"]), Instant::now()) else {
        panic!("owner");
    };
    let row = jobs::next(listing.id(), &job).expect("queued");
    super::publish::restore(listing.id(), &row.path, row.before, row.before_manual, &|_| {});
    assert!(jobs::manual_paths(listing.id(), super::publish::listing_is_open).contains("/data/a"));
    jobs::end(listing.id(), &job);
}

#[tokio::test(start_paused = true)]
async fn ui_cancellation_does_not_drop_an_in_flight_scan() {
    use crate::test_support::WedgedVolume;
    let listing = TestListing::new()
        .path("/data")
        .entries(vec![folder("/data/a")])
        .insert("jobs-scan-lifetime");
    let volume: Arc<dyn crate::file_system::volume::Volume> = Arc::new(WedgedVolume::new("lifetime"));
    let weak = Arc::downgrade(&volume);
    let id = listing.id().to_string();
    let sink = move |_| {
        jobs::cancel(&id);
    };
    let plan = plan(listing.id(), false, None, |_| false).expect("cached");
    let outcome = count_with(listing.id(), plan, volume, &sink).await;
    assert!(outcome.cancelled, "UI completion is responsive");
    assert!(
        weak.upgrade().is_some(),
        "the backend operation still owns its volume until it returns"
    );
}

#[test]
fn a_request_while_a_count_runs_joins_its_queue_without_repeats() {
    let id = "jobs-join";
    let Joined::Owner(job) = jobs::enqueue(id, queued(&["/a", "/b"]), Instant::now()) else {
        panic!("the first request owns the job");
    };
    // Space on /b (queued already) and /c: only /c is new.
    let Joined::Waiter { appended, .. } = jobs::enqueue(id, queued(&["/b", "/c"]), Instant::now()) else {
        panic!("a second request joins the running job");
    };
    assert_eq!(appended, 1);

    let order: Vec<String> = std::iter::from_fn(|| jobs::next(id, &job).map(|q| q.path)).collect();
    assert_eq!(order, vec!["/a", "/b", "/c"], "in request order");
    assert!(!jobs::cancel(id), "a drained job left the map");
}

#[test]
fn waiting_rows_light_up_only_after_the_delay() {
    let id = "jobs-light";
    let start = Instant::now();
    let Joined::Owner(job) = jobs::enqueue(id, queued(&["/a", "/b"]), start) else {
        panic!("owner");
    };
    assert!(
        job.due_to_light(start, HOURGLASS_DELAY).is_empty(),
        "nothing before the delay"
    );
    let lit = job.due_to_light(start + HOURGLASS_DELAY, HOURGLASS_DELAY);
    assert_eq!(lit.len(), 2);
    assert!(
        job.due_to_light(start + Duration::from_secs(5), HOURGLASS_DELAY)
            .is_empty(),
        "once each"
    );
    jobs::cancel(id);
}

#[tokio::test]
async fn a_cancel_puts_lit_waiting_rows_back_and_stops() {
    let listing = TestListing::new()
        .path("/data")
        .include_hidden(false)
        .entries(vec![folder("/data/a"), folder("/data/b")])
        .insert("jobs-cancel");
    let events: Arc<Mutex<Vec<ListingIndexSizesChanged>>> = Arc::default();
    let sink_events = Arc::clone(&events);
    let sink = move |e: ListingIndexSizesChanged| sink_events.lock().expect("test lock").push(e);
    let plan = plan(listing.id(), false, None, |_| false).expect("cached");
    // Esc before the walk starts: the job is already stopped when it runs.
    let id = listing.id().to_string();
    let volume: Arc<dyn crate::file_system::volume::Volume> = Arc::new(cmdr_fs::volume::InMemoryVolume::with_entries(
        "test",
        vec![folder("/data"), folder("/data/a"), folder("/data/b")],
    ));
    let canceller = {
        let id = id.clone();
        tokio::spawn(async move {
            while !jobs::cancel(&id) {
                tokio::task::yield_now().await;
            }
        })
    };
    let outcome = count_with(&id, plan, volume, &sink).await;
    canceller.abort();

    assert!(outcome.counted <= 2);
    let rows = listing.entries();
    assert!(
        rows.iter()
            .all(|e| e.recursive_size.is_none() || e.recursive_size_complete.is_some()),
        "every row ends either untouched or with a reading"
    );
}
