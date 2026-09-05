//! The app's half of a repo change: what the watcher reports, and what the app
//! turns it into.
//!
//! The watcher's own contract (one report per debounced burst) is asserted
//! against a [`RecordingGitStateSink`] here, because the recorder is the
//! instrument that makes it observable. What the APP does with a report is the
//! payload's shape and the listing refresh, which is everything below.

#![cfg(test)]

use std::sync::Arc;
use std::time::Duration;

use crate::test_support::wait_until;

use super::wiring::GitStateChangedPayload;
use cmdr_git::test_fixtures::{Fixture, cleanup, discover_repo, temp_dir};
use cmdr_git::{GitPortal, GitStateSink, RecordingGitStateSink, RepoInfo, no_git_state_sink};

/// A portal over the real host, reporting into `sink`, whose watcher is
/// scripted rather than real: [`GitPortal::fire_watcher`] stands in for the
/// operating system. What every cell here but one wants, because arming a real
/// FSEvents stream over a repository's ~10 `.git/*` paths is most of what a
/// subscribe costs and none of them assert on it.
fn portal_reporting_into(sink: Arc<dyn GitStateSink>) -> GitPortal {
    GitPortal::with_scripted_watcher(crate::volume_host::host(), sink)
}

/// A portal over the real host with a REAL `.git/*` watcher. ❗ The one cell
/// below that takes this pays for FSEvents; everything else takes
/// [`portal_reporting_into`].
fn portal_with_a_real_watcher(sink: Arc<dyn GitStateSink>) -> GitPortal {
    GitPortal::new(crate::volume_host::host(), sink)
}

/// The one thing about the wire event that can't be checked by the compiler:
/// the field names the frontend reads. `repoRoot` and `info` are what
/// `git-state-changed` has always carried, and `event_name` pins the event
/// string itself.
#[test]
fn the_payload_serializes_to_the_shape_the_frontend_subscribes_to() {
    let payload = GitStateChangedPayload {
        repo_root: "/repo".to_string(),
        info: RepoInfo {
            repo_root: "/repo".to_string(),
            branch: Some("main".to_string()),
            detached_sha: None,
            unborn: false,
            upstream: Some("origin/main".to_string()),
            ahead: Some(2),
            behind: Some(0),
            is_dirty: true,
        },
    };

    let json = serde_json::to_value(&payload).expect("the payload serializes");
    assert_eq!(json["repoRoot"], "/repo");
    assert_eq!(json["info"]["branch"], "main");
    assert_eq!(json["info"]["upstream"], "origin/main");
    assert_eq!(json["info"]["ahead"], 2);
    assert_eq!(json["info"]["isDirty"], true);
}

/// A burst of `.git/*` writes reaches the sink ONCE, carrying the repo root and
/// the state as it is after the burst. That debounce is what keeps a `git
/// checkout` (which rewrites `HEAD`, `index`, and a pile of refs) from driving
/// one event per file.
///
/// ❗ **The one cell in the app that arms a REAL `.git/*` watcher.** The debounce
/// it proves is `notify`'s own, so a scripted backend can't stand in: it would
/// assert the fake's arithmetic. Every other subscription cell here and in
/// `cmdr_git::watcher_tests` takes the scripted one and runs in milliseconds.
#[test]
fn a_debounced_burst_reports_one_change_with_the_new_state() {
    let dir = temp_dir("wiring", "one_report_per_burst");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    let (_, root) = discover_repo(&dir).expect("the fixture is a repo");

    let sink = Arc::new(RecordingGitStateSink::new());
    let portal = portal_with_a_real_watcher(Arc::clone(&sink) as Arc<dyn GitStateSink>);
    let first = portal
        .subscribe_state(&root)
        .expect("subscribing answers with the current state");
    assert_eq!(first.branch.as_deref(), Some("main"));
    assert_eq!(sink.count(), 0, "subscribing itself reports nothing");

    // One burst: several ref writes inside the 200 ms debounce window.
    for index in 0..5 {
        fixture.commit_file(&format!("f{index}.txt"), b"x\n", "more");
    }

    wait_until(Duration::from_secs(5), "the watcher reports the burst", || {
        sink.count() >= 1
    });
    let changes = sink.changes();
    assert_eq!(changes.len(), 1, "one report per burst, not one per write: {changes:?}");
    let (reported_root, info) = &changes[0];
    assert_eq!(reported_root, &root);
    assert_eq!(info.branch.as_deref(), Some("main"));

    portal.unsubscribe_state(&root);
    cleanup(&dir);
}

/// The app's sink refreshes every open listing the repo change can have moved:
/// each of the six virtual trees, and the repo's `.git/` itself, whose category
/// rows carry live counts. Asserted through the selection the refresh makes
/// rather than the `FullRefresh` itself, which needs a registered `AppHandle` to
/// land.
#[test]
fn a_report_selects_every_open_virtual_listing_for_the_repo() {
    use crate::file_system::listing::caching_test_support::TestListing;
    use crate::file_system::volume::DEFAULT_VOLUME_ID;

    let dir = temp_dir("wiring", "refresh_selection");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    let (_, root) = discover_repo(&dir).expect("the fixture is a repo");

    let dot_git = root.join(".git");
    let branches = dot_git.join("branches");
    let commits = dot_git.join("commits");
    let listings = [
        TestListing::new()
            .volume(DEFAULT_VOLUME_ID)
            .path(branches.clone())
            .insert("wiring-refresh-branches"),
        TestListing::new()
            .volume(DEFAULT_VOLUME_ID)
            .path(commits.clone())
            .insert("wiring-refresh-commits"),
        TestListing::new()
            .volume(DEFAULT_VOLUME_ID)
            .path(dot_git.clone())
            .insert("wiring-refresh-dot-git"),
    ];

    let selected = super::wiring::listings_a_repo_change_re_reads(&root);
    for path in [&branches, &commits, &dot_git] {
        assert!(
            selected.iter().any(|(_, listed)| listed == path),
            "{} is what a ref change re-reads: {selected:?}",
            path.display()
        );
    }

    // A full refresh, ❌ never an evict: the pane keeps its rows until the
    // re-read lands, so the user never sees an empty list flash by.
    super::wiring::refresh_virtual_listings(&root);
    for listing in &listings {
        assert!(listing.is_cached(), "the listing survives the refresh");
    }

    cleanup(&dir);
}

/// A repo nobody is watching still answers: the detached sink is what a test
/// binary and a headless bench report into.
#[test]
fn a_sink_that_reports_nowhere_is_a_valid_subscriber() {
    let dir = temp_dir("wiring", "detached_sink");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    let (_, root) = discover_repo(&dir).expect("the fixture is a repo");

    let portal = portal_reporting_into(no_git_state_sink());
    let info = portal
        .subscribe_state(&root)
        .expect("subscribing works with nowhere to report");
    assert_eq!(info.repo_root, root.display().to_string());
    assert_eq!(portal.watched_repo_count(), 1);

    portal.unsubscribe_state(&root);
    assert_eq!(portal.watched_repo_count(), 0);
    cleanup(&dir);
}
