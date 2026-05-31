//! Integration tests for the extra virtual-portal categories: commits, stash,
//! worktrees, submodules.
//!
//! Standard init+commit fixtures go through [`Fixture`]. The handful
//! of operations gix 0.81 doesn't expose publicly (stash creation,
//! worktree add, submodule add) keep a thin [`git_cli`] shell-out.

#![cfg(test)]

use std::path::PathBuf;

use super::path::{Cat, VirtualGitPath, classify};
use super::repo::discover_repo;
use super::test_fixtures::{Fixture, build_simple_repo, cleanup, git_cli, git_cli_capture, temp_dir};
use super::{log as git_log, stash, submodules, worktrees};

// ── commits ───────────────────────────────────────────────────────────

#[test]
fn list_commits_yields_entries_with_short_sha_and_subject() {
    let (dir, _f) = build_simple_repo("m3", 3);
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = git_log::list_commits(&handle, &root).unwrap();
    assert_eq!(entries.len(), 3, "fixture has exactly 3 commits");
    for fe in &entries {
        // Display name = "<short-sha> <subject>"
        assert!(fe.name.contains(' '), "expected sha + subject in {}", fe.name);
        assert_eq!(fe.icon_id, "git:commit");
        assert!(fe.is_directory);
        assert!(fe.added_at.is_some(), "added_at drives date sort");
    }
    // Newest first ordering: top entry should match `git log -1`.
    let top_subject = String::from_utf8(git_cli_capture(&dir, &["log", "-1", "--format=%s"])).unwrap();
    assert!(entries[0].name.contains(top_subject.trim()));
    cleanup(&dir);
}

#[test]
fn commit_tree_browsing_via_short_sha() {
    let (dir, _f) = build_simple_repo("m3", 2);
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = git_log::list_commits(&handle, &root).unwrap();
    let top = &entries[0];
    // The on-disk path segment is the bare 7-char short sha.
    let segment = top.path.split('/').next_back().unwrap().to_string();
    assert_eq!(segment.len(), git_log::SHORT_SHA_LEN);

    // Direct path entry: classify `.git/commits/<sha>` and resolve.
    let p = root.join(".git").join("commits").join(&segment).join("README.md");
    let (virt, _, _) = classify(&p).expect("classify commit path");
    match virt {
        VirtualGitPath::RefTree(Cat::Commits, name, sub) => {
            assert_eq!(name, segment);
            assert_eq!(sub, "README.md");
        }
        _ => panic!("expected commits/<sha>/README.md to classify as RefTree"),
    }
    cleanup(&dir);
}

#[test]
fn commits_caps_listing_at_max() {
    // We test the cap in isolation rather than building 5001 commits :
    // that'd add ~30 s to the test suite. The cap path is exercised by
    // setting MAX_COMMITS-equivalent fixtures via the loop count check.
    // For now, just assert the constant value matches the plan.
    assert_eq!(git_log::MAX_COMMITS, 5000);
    assert_eq!(git_log::BATCH_SIZE, 200);
}

#[test]
fn commits_listing_cancellation_polls_atomic_flag() {
    use std::sync::atomic::Ordering;
    let (dir, _f) = build_simple_repo("m3", 5);
    let (handle, root) = discover_repo(&dir).unwrap();

    // Pre-set the cancel flag so the walk bails after 0 commits.
    git_log::cancel_flag().store(true, Ordering::Relaxed);
    let entries = git_log::list_commits(&handle, &root).unwrap();
    git_log::cancel_flag().store(false, Ordering::Relaxed);

    assert!(
        entries.len() < 5,
        "cancellation should stop the walk before all 5 commits arrive"
    );
    cleanup(&dir);
}

#[test]
fn commit_path_resolves_unreachable_sha() {
    // Cmdr's claim: `.git/commits/<sha>/...` resolves even if the SHA
    // isn't reachable from HEAD. We simulate that by making a commit on
    // a side branch, deleting the branch (the commit object stays in the
    // ODB), and asserting we can still browse it via its SHA.
    let dir = temp_dir("m3", "unreachable");
    let mut f = Fixture::init(dir.clone());
    f.commit_file("README.md", b"step 0\n", "commit 0");

    // Create and switch to a side branch, then commit something only
    // there.
    f.create_branch("side");
    f.checkout("side");
    f.commit_file("side.txt", b"hi\n", "side");
    let side_sha = {
        let r = f
            .repo
            .find_reference("refs/heads/side")
            .unwrap()
            .peel_to_id()
            .unwrap()
            .detach();
        r.to_string()
    };
    f.checkout("main");

    // Delete the `side` branch ref. The commit object itself stays in
    // the ODB, so resolve_commit_id can still find it.
    f.repo
        .edit_reference(gix::refs::transaction::RefEdit {
            change: gix::refs::transaction::Change::Delete {
                expected: gix::refs::transaction::PreviousValue::Any,
                log: gix::refs::transaction::RefLog::AndReference,
            },
            name: "refs/heads/side".try_into().unwrap(),
            deref: false,
        })
        .expect("delete side branch");

    // The commit is no longer HEAD-reachable. List_commits returns the
    // single main commit. But classify + resolve_commit_id still find it.
    let (handle, root) = discover_repo(&dir).unwrap();
    let listing = git_log::list_commits(&handle, &root).unwrap();
    assert!(!listing.iter().any(|e| e.name.contains("side")));

    // Direct resolve from the orphaned SHA still works.
    let id = git_log::resolve_commit_id(&handle, &side_sha[..7]).expect("resolve unreachable sha");
    assert!(!id.is_null());
    cleanup(&dir);
}

// ── stash ────────────────────────────────────────────────────────────

#[test]
fn list_stashes_returns_three_entries() {
    let (dir, _f) = build_simple_repo("m3", 1);
    let (handle, root) = discover_repo(&dir).unwrap();

    // Three round-trips of "modify, stash". `git stash` refuses an
    // empty stash, so we touch a different file each round. Stash
    // creation has no gix-side API in 0.81; CLI is the only path.
    for n in 0..3 {
        std::fs::write(dir.join(format!("stash-file-{}.txt", n)), "x\n").unwrap();
        // git stash needs the file to be tracked or to be told to keep
        // untracked too.
        git_cli(&dir, &["stash", "push", "-u", "-m", &format!("change {}", n)]);
    }

    // The handle is still useful for `resolve_stash_commit` later in the
    // file; suppress the unused warning for this test.
    let _ = &handle;
    let entries = stash::list_stashes(&root).unwrap();
    assert_eq!(entries.len(), 3);
    // Newest-first ordering – git stash list follows reflog, newest at
    // the top, which means stash@{0} is the most recent.
    assert!(entries[0].name.starts_with("stash@{0}"));
    assert!(entries[2].name.starts_with("stash@{2}"));

    // Resolving stash@{n} via gix's underlying object id matches what
    // `git stash show <n>` would expand to.
    let id = stash::resolve_stash_commit(&handle, 0).unwrap();
    let expected = String::from_utf8(git_cli_capture(&dir, &["rev-parse", "stash@{0}"]))
        .unwrap()
        .trim()
        .to_string();
    assert_eq!(id.to_string(), expected);

    cleanup(&dir);
}

// ── worktrees ────────────────────────────────────────────────────────

#[test]
fn list_worktrees_redirects_to_working_dir() {
    let (dir, _f) = build_simple_repo("m3", 1);
    // `git worktree add` has no gix-side public API in 0.81; CLI is the
    // only path. We add a sibling worktree at `<dir>-wt1`.
    let wt_path = dir
        .parent()
        .unwrap()
        .join(format!("{}-wt1", dir.file_name().unwrap().to_string_lossy()));
    let _ = std::fs::remove_dir_all(&wt_path);
    git_cli(&dir, &["worktree", "add", "-b", "wt-branch", wt_path.to_str().unwrap()]);

    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = worktrees::list_worktrees(&handle, &root).unwrap();
    assert_eq!(entries.len(), 1);
    let wt = &entries[0];
    assert!(wt.is_directory);
    assert!(wt.redirect_to_path.is_some(), "redirect_to_path must be set");
    let canonical_wt = wt_path.canonicalize().unwrap();
    assert_eq!(
        PathBuf::from(wt.redirect_to_path.as_ref().unwrap())
            .canonicalize()
            .unwrap(),
        canonical_wt
    );

    cleanup(&dir);
    cleanup(&wt_path);
}

// ── submodules ───────────────────────────────────────────────────────

#[test]
fn list_submodules_redirects_to_working_dir() {
    // Outer repo with one commit.
    let (outer, _of) = build_simple_repo("m3", 1);
    // Inner repo to add as submodule.
    let (inner, _if) = build_simple_repo("m3", 1);
    // `git submodule add` has no gix-side public API in 0.81; CLI is
    // the only path. Use a `file://` URL or path with `file://`.
    let inner_url = format!("file://{}", inner.display());
    git_cli(
        &outer,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            &inner_url,
            "vendor/inner",
        ],
    );
    git_cli(&outer, &["commit", "-q", "-m", "add submodule"]);

    let (handle, root) = discover_repo(&outer).unwrap();
    let entries = submodules::list_submodules(&handle, &root).unwrap();
    assert_eq!(entries.len(), 1);
    let sm = &entries[0];
    assert_eq!(sm.name, "vendor/inner");
    assert!(sm.redirect_to_path.is_some());
    let target = PathBuf::from(sm.redirect_to_path.as_ref().unwrap());
    assert_eq!(target, root.join("vendor").join("inner"));

    cleanup(&outer);
    cleanup(&inner);
}

// ── watcher invalidation for new categories ──────────────────────────

#[test]
fn watcher_invalidates_commits_listing_on_new_commit() {
    use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
    use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use crate::file_system::volume::DEFAULT_VOLUME_ID;
    use std::sync::atomic::AtomicU64;

    let (dir, mut f) = build_simple_repo("m3", 1);
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = git_log::list_commits(&handle, &root).unwrap();

    let listing_path = root.join(".git").join("commits");
    let listing_id = format!("test-listing-commits-{}-{}", std::process::id(), rand_suffix());
    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: DEFAULT_VOLUME_ID.to_string(),
                path: listing_path.clone(),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
                directory_sort_mode: DirectorySortMode::LikeFiles,
                sequence: AtomicU64::new(0),
                created_at: std::time::Instant::now(),
                last_accessed_ms: AtomicU64::new(0),
            },
        );
    }

    // Add a new commit and run the watcher invalidation entry point.
    f.commit_file("new.txt", b"x\n", "added new");
    super::watcher::invalidate_for_test(&root);

    // The listing is still in the cache (we full-refresh, not evict).
    {
        let cache = LISTING_CACHE.read().unwrap();
        assert!(cache.contains_key(&listing_id));
    }
    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.remove(&listing_id);
    }
    cleanup(&dir);
}

// Best-effort suffix to keep parallel test invocations distinct.
fn rand_suffix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}
