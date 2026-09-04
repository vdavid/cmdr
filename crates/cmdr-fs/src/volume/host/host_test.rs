//! What the seams promise a backend, independent of any backend.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::VolumeHost;
use super::activity::{self, BusyVolumes};
use super::analytics::RecordingAnalytics;
use super::credentials::{InMemoryCredentials, StoredCredentials};
use super::events::{RecordingVolumeEvents, VolumeConnection};
use super::indexing::{RecordingIndexNotifier, WatchGap};
use super::listings::{ListingHost, RecordingListings};
use crate::volume::DirectoryChange;

/// The promise that lets a backend hold a plain `VolumeHost` instead of an
/// `Option<VolumeHost>`: with nothing installed, every seam still answers. A
/// bench, a CLI tool, and any test that only exercises protocol code run this
/// way, so a seam that panicked here would take all three down at once.
#[test]
fn a_detached_host_answers_every_seam() {
    let host = VolumeHost::detached();
    let path = Path::new("/somewhere/else");

    host.listings()
        .directory_changed("vol", path, DirectoryChange::Removed("gone.txt".to_string()));
    assert!(
        host.listings().authoritative_listing("vol", path).is_none(),
        "nothing is showing anything, so the oracle must miss rather than claim an empty directory"
    );
    host.events().connection_changed("vol", VolumeConnection::Disconnected);
    assert!(host.credentials().credentials("server", None).is_none());
    assert!(
        host.credentials()
            .save_credentials(
                "server",
                None,
                &StoredCredentials {
                    username: "u".to_string(),
                    secret: "s".to_string(),
                },
            )
            .is_err(),
        "there's no store, so saving has to report that it didn't happen"
    );
    host.indexing().watch_gap("vol", WatchGap::WatcherStopped);
    host.indexing().resume_after_reconnect("vol");
    assert!(
        activity::volume_idle_for(host.activity(), "vol", Duration::from_millis(500)),
        "with no user around, bulk work must never stand aside"
    );
    host.analytics().record("something_happened", &[("outcome", "fine")]);
    assert!(host.settings().max_concurrent_operations("smb") >= 1);
}

/// A backend spawns from places with no ambient runtime (a watcher's OS thread,
/// a synchronous startup hook), so the seam has to RESOLVE a runtime rather than
/// inherit one. With nothing injected that means the shared fallback; if this
/// regresses, every backend's test binary fails at once with a runtime panic.
#[test]
fn background_work_spawns_with_no_runtime_injected() {
    let host = VolumeHost::detached();
    let spawned = host
        .runtime()
        .block_on(async { host.runtime().spawn(async { 7_u8 }).await });
    assert_eq!(spawned.expect("the spawned task joins cleanly"), 7);
}

/// An injected runtime is the one that runs the work. The app injects its own so
/// there's exactly one thread pool in the process, which is what keeps a bulk
/// transfer from outranking the window the user is looking at.
#[test]
fn an_injected_runtime_is_the_one_that_runs_the_work() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_name("the-injected-runtime")
        .enable_all()
        .build()
        .expect("building a runtime for the test");
    let host = VolumeHost::builder().runtime(runtime.handle().clone()).build();

    let ran_on = host
        .runtime()
        .block_on(async { host.runtime().spawn(async { thread_name() }).await })
        .expect("the spawned task joins cleanly");
    assert_eq!(ran_on.as_deref(), Some("the-injected-runtime"));
}

fn thread_name() -> Option<String> {
    std::thread::current().name().map(str::to_string)
}

/// Everything a backend reports reaches the seam it was installed for, and
/// nothing crosses wires on the way.
#[test]
fn what_a_backend_reports_reaches_the_installed_seam() {
    let listings = Arc::new(RecordingListings::new().with_authoritative_listing("share", "/share/open", Vec::new()));
    let events = Arc::new(RecordingVolumeEvents::new());
    let indexing = Arc::new(RecordingIndexNotifier::new());
    let analytics = Arc::new(RecordingAnalytics::new());
    let credentials = Arc::new(InMemoryCredentials::new().with_entry("nas.local", Some("share"), "dori", "hunter2"));

    let host = VolumeHost::builder()
        .listings(listings.clone())
        .events(events.clone())
        .indexing(indexing.clone())
        .analytics(analytics.clone())
        .credentials(credentials.clone())
        .activity(Arc::new(BusyVolumes::new().is_busy("share")))
        .build();

    host.listings()
        .directory_changed("share", Path::new("/share/open"), DirectoryChange::FullRefresh);
    host.events()
        .connection_changed("share", VolumeConnection::NeedsCredentials);
    host.indexing().watch_gap("share", WatchGap::EventsOverflowed);
    host.indexing().resume_after_reconnect("share");
    host.analytics().record("smb_connected", &[]);

    assert_eq!(listings.change_count(), 1);
    assert_eq!(listings.changes()[0].0, "share");
    assert!(
        host.listings()
            .authoritative_listing("share", Path::new("/share/open"))
            .is_some_and(|entries| entries.is_empty()),
        "a pane IS showing this directory, so the oracle hits"
    );
    assert!(
        host.listings()
            .authoritative_listing("share", Path::new("/share/elsewhere"))
            .is_none(),
        "a directory no pane is showing has to miss, so the caller asks the protocol"
    );
    assert_eq!(listings.authoritative_lookup_count(), 2);
    assert_eq!(
        events.transitions(),
        vec![("share".to_string(), VolumeConnection::NeedsCredentials)]
    );
    assert_eq!(indexing.gaps(), vec![("share".to_string(), WatchGap::EventsOverflowed)]);
    assert_eq!(indexing.resumes(), vec!["share".to_string()]);
    assert_eq!(analytics.events(), vec![("smb_connected".to_string(), Vec::new())]);

    // Narrow-then-wide is the conventional credential lookup, and the store has
    // to answer the narrow entry without also inventing a wide one.
    let found = host
        .credentials()
        .credentials("nas.local", Some("share"))
        .expect("the pre-seeded entry");
    assert_eq!(found.username, "dori");
    assert!(host.credentials().credentials("nas.local", None).is_none());
}

/// The dispatch rule, and the instrument that enforces it: a seam call belongs
/// on a per-mutation path, never inside a loop over entries. A compliant backend
/// folding a 250-entry directory into one change reports ONCE; the counter is
/// what turns a regression into a failing assertion instead of a slow app. Copy
/// this shape into a backend crate's own tests, driving its real walk.
#[test]
fn the_change_counter_separates_per_mutation_from_per_entry() {
    let listings = RecordingListings::new();
    let dir = Path::new("/share/big");

    let entries: Vec<String> = (0..250).map(|i| format!("file-{i}.txt")).collect();

    // Compliant: the whole directory is one piece of news.
    listings.directory_changed("share", dir, DirectoryChange::FullRefresh);
    assert_eq!(listings.change_count(), 1, "250 entries, one change");

    // What a per-entry regression would look like, so the guard is known to bite.
    for name in &entries {
        listings.directory_changed("share", dir, DirectoryChange::Removed(name.clone()));
    }
    assert_eq!(
        listings.change_count(),
        251,
        "the counter has to see every call, or it can't catch a seam that slipped into a loop"
    );
}
