//! What discovery and [`repo_info`] answer for a repository on disk: whether
//! there's a repo at all, which branch HEAD is on, and whether the worktree is
//! dirty.
//!
//! Standard init+commit fixtures go through [`Fixture`] (in-process gix); the
//! handful of cells that need a bare repo, a detached HEAD, or a linked
//! worktree still shell out via [`git_cli`], because gix exposes no public API
//! for those (verified on gix 0.87, 2026-09-05).

#![cfg(test)]

use std::path::{Path, PathBuf};

use crate::repo::repo_info;
use crate::test_fixtures::{Fixture, cleanup, discover_repo, git_cli, temp_dir};
use cmdr_fs::volume::friendly_error::git::FriendlyGitErrorKind;

fn temp(name: &str) -> PathBuf {
    temp_dir("repo", name)
}

/// Initialize a repo at `dir` and land an `initial` commit on `main`. Drops the
/// fixture; subsequent operations open the repo fresh as needed.
fn init_repo_with_commit(dir: &Path) {
    let mut f = Fixture::init(dir.to_path_buf());
    f.commit_file("README.md", b"hello\n", "initial");
}

#[test]
fn discover_real_dot_git() {
    let dir = temp("discover_real");
    init_repo_with_commit(&dir);
    let (handle, root) = discover_repo(&dir).expect("discover");
    assert_eq!(root.canonicalize().unwrap(), dir.canonicalize().unwrap());
    let info = repo_info(&handle, &root).unwrap();
    assert_eq!(info.branch.as_deref(), Some("main"));
    assert!(!info.unborn);
    cleanup(&dir);
}

#[test]
fn discover_no_repo() {
    let dir = temp("no_repo");
    let err = discover_repo(&dir).unwrap_err();
    assert_eq!(err.kind, FriendlyGitErrorKind::NotARepo);
    cleanup(&dir);
}

#[test]
fn discover_empty_mkdir_only() {
    // A literal `mkdir .git` is a malformed repo. gix surfaces it as an
    // open error; we map it to NotARepo / Corrupt either way (no panic).
    let dir = temp("mkdir_only");
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    let result = discover_repo(&dir);
    assert!(result.is_err(), "expected error for empty mkdir .git");
    cleanup(&dir);
}

#[test]
fn discover_bare_repo_rejected() {
    let dir = temp("bare");
    // `gix::init_bare` is the obvious choice but the rest of this test
    // is unaffected by where the bare init comes from; keep the
    // shell-out for parity with how a user would create one.
    git_cli(&dir, &["init", "-q", "--bare"]);
    let err = discover_repo(&dir).unwrap_err();
    assert_eq!(err.kind, FriendlyGitErrorKind::BareRepo);
    cleanup(&dir);
}

#[test]
fn discover_unborn_head() {
    // Fresh `git init` – HEAD points at refs/heads/main but no commit yet.
    let dir = temp("unborn");
    // gix::init sets up the same `HEAD -> refs/heads/main` symbolic
    // reference; no commit needed for the unborn case.
    gix::init(&dir).expect("gix::init");
    let (handle, root) = discover_repo(&dir).expect("discover");
    let info = repo_info(&handle, &root).unwrap();
    assert!(info.unborn);
    assert_eq!(info.branch.as_deref(), Some("main"));
    assert!(!info.is_dirty);
    cleanup(&dir);
}

#[test]
fn repo_info_dirty_with_modified_file() {
    let dir = temp("dirty");
    init_repo_with_commit(&dir);
    std::fs::write(dir.join("README.md"), "changed\n").unwrap();
    let (handle, root) = discover_repo(&dir).unwrap();
    let info = repo_info(&handle, &root).unwrap();
    assert!(info.is_dirty);
    cleanup(&dir);
}

#[test]
fn repo_info_detached_head() {
    let dir = temp("detached");
    init_repo_with_commit(&dir);
    // gix has no public "detach HEAD" API; one CLI call is fine on top
    // of an otherwise gix-built fixture.
    git_cli(&dir, &["checkout", "-q", "--detach"]);
    let (handle, root) = discover_repo(&dir).unwrap();
    let info = repo_info(&handle, &root).unwrap();
    assert!(info.branch.is_none());
    assert!(info.detached_sha.is_some());
    assert_eq!(info.detached_sha.as_ref().unwrap().len(), 7);
    cleanup(&dir);
}

#[test]
fn repo_info_no_upstream() {
    let dir = temp("no_upstream");
    init_repo_with_commit(&dir);
    let (handle, root) = discover_repo(&dir).unwrap();
    let info = repo_info(&handle, &root).unwrap();
    assert!(info.upstream.is_none());
    assert!(info.ahead.is_none());
    assert!(info.behind.is_none());
    cleanup(&dir);
}

#[test]
fn discover_gitlink_for_linked_worktree() {
    let main = temp("worktree_main");
    init_repo_with_commit(&main);
    let linked = main
        .parent()
        .unwrap()
        .join(format!("cmdr_git_test_worktree_linked_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&linked);
    // `git worktree add` has no gix-side public API; keep CLI.
    git_cli(
        &main,
        &["worktree", "add", "-q", linked.to_str().unwrap(), "-b", "feature"],
    );
    // The linked worktree's `.git` is a file (gitlink), not a dir.
    let dot_git = linked.join(".git");
    assert!(dot_git.is_file(), "linked worktree .git should be a file");
    let (handle, root) = discover_repo(&linked).expect("discover gitlink");
    let info = repo_info(&handle, &root).unwrap();
    assert_eq!(info.branch.as_deref(), Some("feature"));
    cleanup(&main);
    cleanup(&linked);
}

/// `repo_info` reads the worktree fresh on every call, which is what makes a
/// watcher report worth acting on: the same handle answers "clean" before an
/// edit, "dirty" after it, and "clean" again once the edit is committed.
#[test]
fn repo_info_recomputes_after_commit() {
    let dir = temp("recompute");
    let mut f = Fixture::init(dir.clone());
    f.commit_file("README.md", b"hello\n", "initial");

    let (handle, root) = discover_repo(&dir).unwrap();
    let before = repo_info(&handle, &root).unwrap();
    assert!(!before.is_dirty);

    // Make a change and commit. Both branch state and dirtiness should update.
    std::fs::write(dir.join("README.md"), "second\n").unwrap();
    let dirty = repo_info(&handle, &root).unwrap();
    assert!(dirty.is_dirty);

    f.commit_file("README.md", b"second\n", "second");
    let after = repo_info(&handle, &root).unwrap();
    assert!(!after.is_dirty);
    cleanup(&dir);
}
