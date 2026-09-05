//! What the watcher registry promises, asserted without the operating system.
//!
//! The bookkeeping is this crate's: one watch per repository however many
//! subscribers it has, torn down with the last one, and a change reaching the
//! sink with a freshly read snapshot. All of it runs against
//! [`GitPortal::with_scripted_watcher`], so a cell here costs a repository open
//! rather than a real FSEvents stream.
//!
//! The one cell that pays for a real watcher is app-side
//! (`file_system::git::wiring_tests::a_debounced_burst_reports_one_change_with_the_new_state`),
//! because the debounce it proves is `notify`'s and no fake can stand in for it.

use std::path::PathBuf;
use std::sync::Arc;

use crate::test_fixtures::{Fixture, cleanup, discover_repo, temp_dir};
use crate::{GitPortal, GitStateSink, RecordingGitStateSink, no_git_state_sink};
use cmdr_fs::volume::host::VolumeHost;

/// A portal over a detached host, reporting into `sink`, whose watcher is
/// scripted rather than real.
fn scripted_portal(sink: Arc<dyn GitStateSink>) -> GitPortal {
    GitPortal::with_scripted_watcher(VolumeHost::detached(), sink)
}

/// A repository with one commit: its directory, its canonical root, and the
/// fixture to keep committing through.
fn a_repo(name: &str) -> (PathBuf, PathBuf, Fixture) {
    let dir = temp_dir("watcher", name);
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    let (_, root) = discover_repo(&dir).expect("the fixture is a repo");
    (dir, root, fixture)
}

/// Two subscribers on one repository share ONE watch, and it survives until the
/// last of them leaves. Without the refcount, a `.git/` pane closing would stop
/// the watcher a working-tree pane is still reading from.
#[test]
fn two_subscribers_share_one_watch_and_the_last_one_out_stops_it() {
    let (dir, root, _fixture) = a_repo("shared_refcount");
    let portal = scripted_portal(no_git_state_sink());

    portal.subscribe_state(&root).expect("the first subscriber arms it");
    portal.subscribe_state(&root).expect("the second one joins it");
    assert_eq!(
        portal.watched_repo_count(),
        1,
        "one watch, whatever the subscriber count"
    );

    portal.unsubscribe_state(&root);
    assert_eq!(portal.watched_repo_count(), 1, "the second subscriber still holds it");

    portal.unsubscribe_state(&root);
    assert_eq!(portal.watched_repo_count(), 0);
    cleanup(&dir);
}

/// A change reaches the sink carrying the repository root and the state as it
/// reads NOW, ❌ never the snapshot the subscribe handshake answered with.
#[test]
fn a_change_reports_the_root_and_a_freshly_read_snapshot() {
    let (dir, root, mut fixture) = a_repo("fresh_snapshot");
    let sink = Arc::new(RecordingGitStateSink::new());
    let portal = scripted_portal(Arc::clone(&sink) as Arc<dyn GitStateSink>);

    let handshake = portal.subscribe_state(&root).expect("subscribing answers the state");
    assert_eq!(handshake.branch.as_deref(), Some("main"));
    assert_eq!(sink.count(), 0, "subscribing itself reports nothing");

    fixture.create_branch("feature");
    fixture.checkout("feature");

    assert!(portal.fire_watcher(&root), "the repo has a watch to fire");
    let changes = sink.changes();
    assert_eq!(changes.len(), 1, "one report per fired change: {changes:?}");
    assert_eq!(changes[0].0, root);
    assert_eq!(
        changes[0].1.branch.as_deref(),
        Some("feature"),
        "the report re-reads the repository rather than replaying the handshake"
    );

    portal.unsubscribe_state(&root);
    cleanup(&dir);
}

/// Firing a repository nobody subscribed reports nothing, which is what makes
/// `fire_watcher`'s answer readable as "was this repo armed?".
#[test]
fn an_unwatched_repo_has_nothing_to_fire() {
    let (dir, root, _fixture) = a_repo("unwatched");
    let sink = Arc::new(RecordingGitStateSink::new());
    let portal = scripted_portal(Arc::clone(&sink) as Arc<dyn GitStateSink>);

    assert!(!portal.fire_watcher(&root), "nothing armed it");
    assert_eq!(sink.count(), 0);
    cleanup(&dir);
}

/// The watch goes away with the last subscriber, so a change after it reports
/// nowhere. That teardown is what keeps a closed pane from holding a status
/// snapshot and an open `gix` handle for the rest of the session.
#[test]
fn the_last_unsubscribe_leaves_nothing_to_fire() {
    let (dir, root, _fixture) = a_repo("teardown");
    let sink = Arc::new(RecordingGitStateSink::new());
    let portal = scripted_portal(Arc::clone(&sink) as Arc<dyn GitStateSink>);

    portal.subscribe_state(&root).expect("subscribing works");
    portal.unsubscribe_state(&root);

    assert!(!portal.fire_watcher(&root), "the watch went with the last subscriber");
    assert_eq!(sink.count(), 0);
    cleanup(&dir);
}
