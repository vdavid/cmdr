//! The app's half of a repo change: what the watcher reports, and what the app
//! turns it into.
//!
//! The watcher's own contract (one report per debounced burst) is asserted
//! against a [`RecordingGitStateSink`] here, because the recorder is the
//! instrument that makes it observable. What the APP does with a report is the
//! payload's shape and the listing refresh, which is everything below.

#![cfg(test)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::test_support::wait_until;

use super::wiring::GitStateChangedPayload;
use cmdr_git::test_fixtures::{Fixture, WATCH_DEBOUNCE, cleanup, discover_repo, temp_dir};
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

/// A burst of `.git/*` writes collapses into a couple of reports rather than one
/// per file, and what lands carries the state as it is AFTER the burst. That
/// debounce is what keeps a `git checkout` (which rewrites `HEAD`, `index`, and
/// a pile of refs) from driving an event per file.
///
/// ❗ **The one cell in the app that arms a REAL `.git/*` watcher.** The debounce
/// it proves is `notify`'s own, so a scripted backend can't stand in: it would
/// assert the fake's arithmetic. Every other subscription cell here and in
/// `cmdr_git::watcher_tests` takes the scripted one and runs in milliseconds.
///
/// ❗ **Gotcha/Why the count is a ceiling, not `== 1`.** The backend calls
/// `on_change` once per batch `notify_debouncer_full` EMITS, and it emits on a
/// tick cadence: a burst whose events settle across two ticks produces two
/// reports, both carrying the same post-burst snapshot. Measured on an M1 Max
/// (2026-09-06, 20 runs of this cell): 7 writes gave 1 report about half the
/// time and 2 the rest, never more. So "exactly one per burst" is not a property
/// the watcher has, and a cell asserting it fails at random. The invariant that
/// IS real, and the one the debounce exists for, is that a burst costs a
/// bounded few reports instead of one per write.
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

    // One burst, and nothing waits inside it: the writes land back to back, in a
    // span measured in microseconds, so this is one burst by construction rather
    // than by timing luck. The branch switch at the end is what makes the
    // REPORTED state distinguishable from the one `subscribe_state` answered
    // with, which is otherwise `main` either way.
    const COMMITS_IN_THE_BURST: usize = 5;
    for index in 0..COMMITS_IN_THE_BURST {
        fixture.commit_file(&format!("f{index}.txt"), b"x\n", "more");
    }
    fixture.create_branch("after-the-burst");
    fixture.checkout("after-the-burst");

    let changes = changes_once_settled(&sink);
    assert!(
        changes.len() < COMMITS_IN_THE_BURST,
        "a burst costs a few reports, not one per write: {changes:?}"
    );
    let (reported_root, info) = changes.last().expect("the burst reported at least once");
    assert_eq!(reported_root, &root);
    // The state AFTER the whole burst, not the `main` the subscribe saw: a report
    // carrying a snapshot taken at the FIRST write would still say `main` here.
    assert_eq!(
        info.branch.as_deref(),
        Some("after-the-burst"),
        "the report carries the state the burst left behind: {info:?}"
    );

    portal.unsubscribe_state(&root);
    cleanup(&dir);
}

/// Everything the sink holds once it has gone QUIET for longer than the
/// watcher's debounce window.
///
/// ❗ Read the count through this, ❌ never by waiting for the FIRST report.
/// `wait_until(count >= 1)` returns while a second report may still be in
/// flight, so the same run asserted 1 when it read early and 2 when it read
/// late: the cell passed and failed at random without the behavior changing.
/// Waiting for quiet makes the number observed the number that actually
/// happened, so a real regression fails every time and a slow machine doesn't.
fn changes_once_settled(sink: &RecordingGitStateSink) -> Vec<(PathBuf, RepoInfo)> {
    // Twice the window, so a report arriving one poll interval late still counts
    // as part of the burst rather than as quiet.
    let quiet = WATCH_DEBOUNCE * 2;
    let mut seen = 0usize;
    let mut unchanged_since = Instant::now();
    wait_until(Duration::from_secs(10), "the watcher's reports settle", || {
        let now = sink.count();
        if now != seen {
            seen = now;
            unchanged_since = Instant::now();
            return false;
        }
        now > 0 && unchanged_since.elapsed() >= quiet
    });
    sink.changes()
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

/// A listing keeps whatever spelling the user navigated with, and a watcher
/// report carries the CANONICAL root, so the two are matched by resolving the
/// listing's worktree root rather than by comparing paths as strings.
///
/// The bug this pins: on macOS a repo under `/tmp` (a symlink to `/private/tmp`)
/// matched no listing at all, and a `git branch` never reached the open
/// `branches/` pane (caught by `git-portal.spec.ts`, 2026-09-05). A symlink to
/// the fixture reproduces it wherever the suite runs.
#[test]
fn a_listing_reached_through_a_symlink_is_still_the_repos() {
    use crate::file_system::listing::caching_test_support::TestListing;
    use crate::file_system::volume::DEFAULT_VOLUME_ID;

    let dir = temp_dir("wiring", "symlinked_spelling");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    let (_, root) = discover_repo(&dir).expect("the fixture is a repo");

    let elsewhere = temp_dir("wiring", "symlinked_spelling_link");
    let link = elsewhere.join("repo");
    std::os::unix::fs::symlink(&root, &link).expect("symlink the repo");
    assert_ne!(
        link, root,
        "the two spellings have to differ for this to assert anything"
    );

    let through_link = link.join(".git").join("branches");
    let _listing = TestListing::new()
        .volume(DEFAULT_VOLUME_ID)
        .path(through_link.clone())
        .insert("wiring-symlinked-branches");

    let selected = super::wiring::listings_a_repo_change_re_reads(&root);
    assert!(
        selected.iter().any(|(_, listed)| *listed == through_link),
        "the pane's own spelling is what gets re-read: {selected:?}"
    );

    cleanup(&elsewhere);
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
