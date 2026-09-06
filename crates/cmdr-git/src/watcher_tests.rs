//! What the watcher registry promises, asserted without the operating system.
//!
//! The bookkeeping is this crate's: one watch per repository however many
//! subscribers it has, torn down with the last one, a change reaching the sink
//! with a freshly read snapshot, and one burst of writes costing one report
//! however the debouncer batched it. All of it runs against
//! [`GitPortal::with_scripted_watcher`], so a cell here costs a repository open
//! rather than a real FSEvents stream.
//!
//! The one cell that pays for a real watcher is app-side
//! (`file_system::git::wiring_tests::a_debounced_burst_reports_once_and_the_watch_survives_for_the_next_one`),
//! because the debounce it proves is `notify`'s and no fake can stand in for it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

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

/// One burst of `.git/*` writes costs ONE report, however the debouncer batched
/// it. `notify_debouncer_full` emits on a tick cadence, so a burst whose
/// per-path deadlines straddle a tick arrives as two calls carrying the same
/// post-burst state, and each report costs a window event plus a re-read of
/// every open portal pane.
#[test]
fn a_second_call_carrying_the_same_state_reports_nothing() {
    let (dir, root, mut fixture) = a_repo("coalesced_repeat");
    let sink = Arc::new(RecordingGitStateSink::new());
    let portal = scripted_portal(Arc::clone(&sink) as Arc<dyn GitStateSink>);

    portal.subscribe_state(&root).expect("subscribing answers the state");
    fixture.create_branch("feature");
    fixture.checkout("feature");

    assert!(portal.fire_watcher(&root), "the repo has a watch to fire");
    assert!(portal.fire_watcher(&root), "the second batch of the same burst");

    let changes = sink.changes();
    assert_eq!(changes.len(), 1, "the repeat has no news in it: {changes:?}");
    assert_eq!(changes[0].1.branch.as_deref(), Some("feature"));

    portal.unsubscribe_state(&root);
    cleanup(&dir);
}

/// Coalescing ❌ never swallows a real change: two bursts leaving DIFFERENT
/// state report twice, back to back, with no quiet between them.
#[test]
fn two_bursts_with_different_end_states_both_report() {
    let (dir, root, mut fixture) = a_repo("two_end_states");
    let sink = Arc::new(RecordingGitStateSink::new());
    let portal = scripted_portal(Arc::clone(&sink) as Arc<dyn GitStateSink>);

    portal.subscribe_state(&root).expect("subscribing answers the state");

    fixture.create_branch("first-stop");
    fixture.checkout("first-stop");
    assert!(portal.fire_watcher(&root), "the repo has a watch to fire");

    fixture.create_branch("second-stop");
    fixture.checkout("second-stop");
    assert!(portal.fire_watcher(&root), "and again");

    let branches: Vec<Option<String>> = sink.changes().into_iter().map(|(_, info)| info.branch).collect();
    assert_eq!(
        branches,
        vec![Some("first-stop".to_string()), Some("second-stop".to_string())],
        "each end state is its own report"
    );

    portal.unsubscribe_state(&root);
    cleanup(&dir);
}

/// The SAME snapshot reported again after the debounce window still goes out.
///
/// ❗ This is why the coalescing is windowed rather than "skip any repeat".
/// `RepoInfo` is what the breadcrumb chip shows, ❌ not a fingerprint of the
/// repository: a second `git branch`, a commit in a worktree that stays dirty,
/// and a `git add` in a dirty repo all leave it byte-identical while the
/// `.git/branches/` and `.git/commits/` panes and the status column each have
/// something new to show. Inside the window a repeat is provably the same burst
/// (an emission never comes sooner than the window after the write behind it);
/// outside it, it's news.
#[test]
fn the_same_state_after_the_window_is_news_again() {
    let (dir, root, fixture) = a_repo("window_expiry");
    let sink = Arc::new(RecordingGitStateSink::new());
    let portal = scripted_portal(Arc::clone(&sink) as Arc<dyn GitStateSink>);

    portal.subscribe_state(&root).expect("subscribing answers the state");
    assert!(portal.fire_watcher(&root), "the repo has a watch to fire");

    // A ref the chip's snapshot can't see: `branch` is still `main` either way.
    fixture.create_branch("invisible-to-the-chip");
    // allowed-test-sleep: the debounce window IS the subject; this cell exists to
    // assert what happens once it has passed, and there is no condition to wait on.
    std::thread::sleep(crate::watcher::DEBOUNCE + Duration::from_millis(50));
    assert!(portal.fire_watcher(&root), "and again, a window later");

    let changes = sink.changes();
    assert_eq!(
        changes.len(),
        2,
        "a later repeat is a change the chip can't see: {changes:?}"
    );
    for (_, info) in &changes {
        assert_eq!(info.branch.as_deref(), Some("main"), "the snapshot really is identical");
    }

    portal.unsubscribe_state(&root);
    cleanup(&dir);
}

/// ❗ **Every watch target is a DIRECTORY**, so none of them can die the way a
/// watch on `HEAD` or `index` does.
///
/// git never writes those in place: it writes `HEAD.lock` and renames it over the
/// top. inotify watches an inode, so a watch on the file itself goes dead at the
/// first rename and every later write in the burst is lost with no error. macOS
/// FSEvents is path-based and tolerated it, so this only ever showed up on Linux
/// CI, where the debounce cell timed out with nothing after the first commit
/// (2026-09-06). A directory's inode survives the rename dance.
#[test]
fn every_watch_target_is_a_directory_no_rename_can_kill() {
    let (dir, root, _fixture) = a_repo("watch_targets");
    let git_dir = root.join(".git");

    let targets = crate::watcher::watch_targets(&git_dir);
    for (path, _) in &targets {
        let name = path.file_name().expect("a target always names something");
        assert!(
            !crate::watcher::STATE_FILES.iter().any(|state| name == *state),
            "{} is a file git renames over, ❌ never a watch target: {targets:?}",
            path.display()
        );
        assert!(
            path.is_dir() || !path.exists(),
            "a target is a directory or absent, ❌ never a file: {}",
            path.display()
        );
    }

    let watched: Vec<PathBuf> = targets.iter().map(|(path, _)| path.clone()).collect();
    for expected in [
        &git_dir,
        &git_dir.join("refs"),
        &git_dir.join("logs"),
        &git_dir.join("worktrees"),
    ] {
        assert!(
            watched.contains(expected),
            "{} is watched: {watched:?}",
            expected.display()
        );
    }

    cleanup(&dir);
}

/// The allowlist that pays for those directory watches: everything a `RepoInfo`,
/// a category listing, or the status column reads counts, and the churn a commit
/// makes beside it does not.
#[test]
fn only_the_paths_a_snapshot_reads_are_worth_a_recompute() {
    let git_dir = PathBuf::from("/repo/.git");
    let matters = |relative: &str| crate::watcher::is_repo_state_path(&git_dir, &git_dir.join(relative));

    for path in ["HEAD", "index", "packed-refs", "MERGE_HEAD", "ORIG_HEAD", "FETCH_HEAD"] {
        assert!(matters(path), "{path} should decide a RepoInfo");
    }
    for path in [
        "refs",
        "refs/heads/main",
        "refs/remotes/origin/main",
        "logs/HEAD",
        "worktrees/wt/HEAD",
    ] {
        assert!(matters(path), "{path} is state a listing reads");
    }
    // A commit writes all of these, and not one of them moves a pane.
    for path in [
        "COMMIT_EDITMSG",
        "MERGE_MSG",
        "objects/ab/cdef",
        "hooks/pre-commit",
        "config",
    ] {
        assert!(!matters(path), "{path} is noise the directory watch delivers");
    }
    // The lock half of git's write dance. The rename's TARGET rides in the same
    // event and answers `true`, so dropping these costs no report.
    for path in ["HEAD.lock", "index.lock", "refs/heads/main.lock"] {
        assert!(!matters(path), "{path} is the lock, not the write");
    }
    assert!(
        !crate::watcher::is_repo_state_path(&git_dir, Path::new("/somewhere/else/HEAD")),
        "a path outside the gitdir is never ours"
    );
}

/// ❗ **A READ of the gitdir is never a change**, and dropping one is what keeps
/// the watcher from feeding itself.
///
/// Linux inotify asks for `IN_OPEN` (`notify` 8.2 sets it in `add_single_watch`),
/// so every file a recompute OPENS — `HEAD`, `index`, `packed-refs`, a
/// `refs/heads/` directory — comes straight back as `Access(Open)` on a directory
/// we watch. With only the path allowlist in front of it, the recompute became its
/// own trigger: one report per debounce window, forever, each carrying the
/// identical snapshot. macOS FSEvents reports no reads at all, so it only ever ran
/// away on Linux (`a_debounced_burst_reports_once_and_the_watch_survives_for_the_next_one`
/// timed out there with 48 identical reports in 10 s, CI, 2026-09-06).
#[test]
fn the_watchers_own_reads_are_not_changes() {
    use notify::EventKind;
    use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, ModifyKind, RenameMode};

    let git_dir = PathBuf::from("/repo/.git");
    let about = |kind: EventKind, relative: &str| {
        crate::watcher::is_repo_state_change(&git_dir, &notify::Event::new(kind).add_path(git_dir.join(relative)))
    };

    // Everything the recompute itself provokes. Each of these names a path the
    // allowlist accepts, which is exactly why the kind has to be read too.
    for relative in ["HEAD", "index", "packed-refs", "refs", "refs/heads/main", "logs/HEAD"] {
        assert!(
            !about(EventKind::Access(AccessKind::Open(AccessMode::Any)), relative),
            "opening {relative} is the watcher reading, ❌ never a change to report"
        );
        assert!(
            !about(EventKind::Access(AccessKind::Read), relative),
            "reading {relative} is not a change"
        );
        assert!(
            !about(EventKind::Access(AccessKind::Close(AccessMode::Read)), relative),
            "closing {relative} after a read is not a change"
        );
    }

    // …and every shape an actual write arrives in still counts.
    for kind in [
        EventKind::Modify(ModifyKind::Data(DataChange::Any)),
        EventKind::Modify(ModifyKind::Name(RenameMode::To)),
        EventKind::Create(CreateKind::File),
        EventKind::Access(AccessKind::Close(AccessMode::Write)),
    ] {
        assert!(about(kind, "HEAD"), "a write to HEAD is news whichever kind it wears");
    }
}

/// ❗ **Reading a repository leaves its gitdir untouched.** The watcher recomputes
/// by reading, and a `.git` watch delivers events for whatever happens in there,
/// so a reader that wrote so much as an index stat-cache refresh would arm the
/// same feedback loop [`the_watchers_own_reads_are_not_changes`] closes — and that
/// one no filter could close, because a write really is a change.
#[test]
fn reading_a_repository_writes_nothing_into_the_gitdir() {
    let (dir, root, _fixture) = a_repo("read_only_reader");
    let git_dir = root.join(".git");
    let (handle, discovered) = discover_repo(&root).expect("the fixture is a repo");

    let before = gitdir_fingerprint(&git_dir);
    for _ in 0..5 {
        crate::repo::repo_info(&handle, &discovered).expect("the snapshot reads");
    }
    let after = gitdir_fingerprint(&git_dir);

    assert_eq!(
        before, after,
        "a snapshot read is a pure read; anything it wrote would retrigger the watch"
    );
    cleanup(&dir);
}

/// Every entry under `path`, as `(relative path, length, modified)`, sorted. The
/// instrument for [`reading_a_repository_writes_nothing_into_the_gitdir`].
fn gitdir_fingerprint(path: &Path) -> Vec<(PathBuf, u64, Option<std::time::SystemTime>)> {
    fn walk(root: &Path, at: &Path, into: &mut Vec<(PathBuf, u64, Option<std::time::SystemTime>)>) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if metadata.is_dir() {
                walk(root, &entry_path, into);
                continue;
            }
            let relative = entry_path.strip_prefix(root).unwrap_or(&entry_path).to_path_buf();
            into.push((relative, metadata.len(), metadata.modified().ok()));
        }
    }
    let mut out = Vec::new();
    walk(path, path, &mut out);
    out.sort();
    out
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
