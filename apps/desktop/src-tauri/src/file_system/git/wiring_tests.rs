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
/// FSEvents stream on a repository's gitdir is most of what a
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

/// A burst of `.git/*` writes ends in a report carrying the state as it is AFTER
/// the burst, the reports then go quiet, and the watch is still alive to do the
/// same for the NEXT burst.
///
/// ❗ **The second burst is not a repetition.** git writes `HEAD` and `index` by
/// renaming a lockfile over them, so a watch registered on those FILES dies at
/// the first rename: burst one reports, and everything after it is lost with no
/// error. That is exactly how this cell failed on Linux, whose inotify is
/// inode-based, while passing on macOS, whose FSEvents is path-based (CI,
/// 2026-09-06). The watcher watches the gitdir DIRECTORY, and this second act is
/// what would catch a return to file watches on either platform.
///
/// ❗ **Quiet is the ceiling.** The settle wait catches the opposite Linux
/// failure: the watcher recomputing by reading the repository, inotify reporting
/// those reads back as events, and every report triggering the next one forever
/// (48 identical reports in 10 s, CI, 2026-09-06). A loop like that reports once
/// per debounce window and never goes quiet. The kind gate that closes it is
/// `cmdr_git`'s: `crates/cmdr-git/DETAILS.md` § "Watcher path set".
///
/// ❗ **The last report and quiet, ❌ never a count.** How many batches a real
/// burst arrives in depends on how long its writes take against the 200 ms
/// window, and a loaded machine stretches them: five commits spanning two windows
/// legitimately report an intermediate state first. Asserting "exactly one" made
/// this the most-retried cell in the Rust lane (78 runs, 2026-09-06 to
/// 2026-09-11). One report for a burst the debouncer SPLIT is
/// `cmdr_git::watcher::recompute_and_report`'s coalescing, asserted without any
/// timing against the scripted backend in `cmdr_git::watcher_tests`.
///
/// ❗ **The one cell in the app that arms a REAL `.git/*` watcher.** Delivery and
/// the watch surviving git's renames are the operating system's, so a scripted
/// backend can't stand in. Every other subscription cell here and in
/// `cmdr_git::watcher_tests` takes the scripted one and runs in milliseconds.
#[test]
fn a_burst_reports_its_end_state_and_the_watch_survives_for_the_next_one() {
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

    // The branch switch at the end is what makes the REPORTED state
    // distinguishable from the one `subscribe_state` answered with, which is
    // otherwise `main` either way.
    const COMMITS_IN_THE_BURST: usize = 5;
    for index in 0..COMMITS_IN_THE_BURST {
        fixture.commit_file(&format!("f{index}.txt"), b"x\n", "more");
    }
    fixture.create_branch("after-the-burst");
    fixture.checkout("after-the-burst");

    let changes = reports_settled_on(&sink, "after-the-burst");
    assert!(
        changes.iter().all(|(reported_root, _)| reported_root == &root),
        "every report names the canonical root: {changes:?}"
    );
    let after_first_burst = changes.len();

    // A SECOND burst, after the first has already renamed a lockfile over `HEAD`
    // and `index`. A dead watch delivers nothing here and the settle wait times
    // out naming the reports it kept.
    for index in 0..COMMITS_IN_THE_BURST {
        fixture.commit_file(&format!("g{index}.txt"), b"y\n", "more still");
    }
    fixture.create_branch("after-the-second-burst");
    fixture.checkout("after-the-second-burst");

    let changes = reports_settled_on(&sink, "after-the-second-burst");
    assert!(
        changes.len() > after_first_burst,
        "the watch was alive to report the second burst: {changes:?}"
    );

    portal.unsubscribe_state(&root);
    cleanup(&dir);
}

/// Everything the sink holds once its LAST report carries `branch` and it has
/// then gone quiet for longer than the watcher's debounce window.
///
/// ❗ Waits on the end state, ❌ never on a count: the count depends on how the
/// burst's writes fell against the window, the end state doesn't. A report taken
/// at the FIRST write of a burst would still say the previous branch, so a
/// watcher that stops reporting mid-burst never satisfies this.
///
/// ❗ **The timeout names the reports it saw**, because "timed out" is the same
/// message whether the watcher delivered nothing at all or delivered a burst that
/// never went quiet, and those are opposite bugs. A dead watch shows up here as
/// a list that stops short of `branch`; a read loop as one that keeps growing.
fn reports_settled_on(sink: &RecordingGitStateSink, branch: &str) -> Vec<(PathBuf, RepoInfo)> {
    // Twice the window, so a report arriving one poll interval late still counts
    // as part of the burst rather than as quiet.
    let quiet = WATCH_DEBOUNCE * 2;
    const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);
    let deadline = Instant::now() + SETTLE_TIMEOUT;
    let mut seen = 0usize;
    let mut unchanged_since = Instant::now();
    // The outer cap is deliberately unreachable: the deadline inside fires first
    // and carries the diagnostic, and this only backstops it.
    wait_until(SETTLE_TIMEOUT * 2, "the watcher's reports to settle", || {
        let changes = sink.changes();
        if changes.len() != seen {
            seen = changes.len();
            unchanged_since = Instant::now();
            return false;
        }
        let landed = changes
            .last()
            .is_some_and(|(_, info)| info.branch.as_deref() == Some(branch));
        if landed && unchanged_since.elapsed() >= quiet {
            return true;
        }
        assert!(
            Instant::now() < deadline,
            "the watcher's reports never settled on `{branch}` in {SETTLE_TIMEOUT:?}; reported so far: {changes:?}"
        );
        false
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
