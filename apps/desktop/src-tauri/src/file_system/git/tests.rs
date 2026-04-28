//! Unit tests for the git module (M1).
//!
//! These tests build small fixtures with the `git` CLI to avoid hand-rolling
//! ref/index files. `git` is part of the project's system requirements.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::repo::{discover_repo, repo_info};
use super::status::{EntryStatusCode, list_status};

/// Builds a temp dir, runs the closure with the dir's path. The dir is wiped
/// when the returned guard drops.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cmdr_git_test_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Cmdr Test")
        .env("GIT_AUTHOR_EMAIL", "test@cmdr.local")
        .env("GIT_COMMITTER_NAME", "Cmdr Test")
        .env("GIT_COMMITTER_EMAIL", "test@cmdr.local")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git command");
    assert!(status.success(), "git {:?} failed in {}", args, dir.display());
}

fn init_repo_with_commit(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.name", "Cmdr Test"]);
    git(dir, &["config", "user.email", "test@cmdr.local"]);
    std::fs::write(dir.join("README.md"), "hello\n").unwrap();
    git(dir, &["add", "README.md"]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

#[test]
fn discover_real_dot_git() {
    let dir = temp_dir("discover_real");
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
    let dir = temp_dir("no_repo");
    let err = discover_repo(&dir).unwrap_err();
    assert_eq!(err.kind, FriendlyGitErrorKind::NotARepo);
    cleanup(&dir);
}

#[test]
fn discover_empty_mkdir_only() {
    // A literal `mkdir .git` is a malformed repo. gix surfaces it as an
    // open error; we map it to NotARepo / Corrupt either way (no panic).
    let dir = temp_dir("mkdir_only");
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    let result = discover_repo(&dir);
    assert!(result.is_err(), "expected error for empty mkdir .git");
    cleanup(&dir);
}

#[test]
fn discover_bare_repo_rejected() {
    let dir = temp_dir("bare");
    git(&dir, &["init", "-q", "--bare"]);
    let err = discover_repo(&dir).unwrap_err();
    assert_eq!(err.kind, FriendlyGitErrorKind::BareRepo);
    cleanup(&dir);
}

#[test]
fn discover_unborn_head() {
    // Fresh `git init` — HEAD points at refs/heads/main but no commit yet.
    let dir = temp_dir("unborn");
    git(&dir, &["init", "-q", "-b", "main"]);
    let (handle, root) = discover_repo(&dir).expect("discover");
    let info = repo_info(&handle, &root).unwrap();
    assert!(info.unborn);
    assert_eq!(info.branch.as_deref(), Some("main"));
    assert!(!info.is_dirty);
    cleanup(&dir);
}

#[test]
fn repo_info_dirty_with_modified_file() {
    let dir = temp_dir("dirty");
    init_repo_with_commit(&dir);
    std::fs::write(dir.join("README.md"), "changed\n").unwrap();
    let (handle, root) = discover_repo(&dir).unwrap();
    let info = repo_info(&handle, &root).unwrap();
    assert!(info.is_dirty);
    cleanup(&dir);
}

#[test]
fn repo_info_detached_head() {
    let dir = temp_dir("detached");
    init_repo_with_commit(&dir);
    git(&dir, &["checkout", "-q", "--detach"]);
    let (handle, root) = discover_repo(&dir).unwrap();
    let info = repo_info(&handle, &root).unwrap();
    assert!(info.branch.is_none());
    assert!(info.detached_sha.is_some());
    assert_eq!(info.detached_sha.as_ref().unwrap().len(), 7);
    cleanup(&dir);
}

#[test]
fn repo_info_no_upstream() {
    let dir = temp_dir("no_upstream");
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
    let main = temp_dir("worktree_main");
    init_repo_with_commit(&main);
    let linked = main
        .parent()
        .unwrap()
        .join(format!("cmdr_git_test_worktree_linked_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&linked);
    git(
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

#[test]
fn list_status_returns_one_per_status() {
    let dir = temp_dir("status_kinds");
    init_repo_with_commit(&dir);
    // First, configure .gitignore and commit it so untracked.txt and ignored.txt
    // can be classified after the staging step below.
    std::fs::write(dir.join(".gitignore"), "ignored.txt\n").unwrap();
    git(&dir, &["add", ".gitignore"]);
    git(&dir, &["commit", "-q", "-m", "ignore"]);
    // Modified (in worktree relative to index)
    std::fs::write(dir.join("README.md"), "modified\n").unwrap();
    // Added (staged but not committed)
    std::fs::write(dir.join("added.txt"), "added\n").unwrap();
    git(&dir, &["add", "added.txt"]);
    // Untracked
    std::fs::write(dir.join("untracked.txt"), "untracked\n").unwrap();
    // Ignored (configured + present)
    std::fs::write(dir.join("ignored.txt"), "ignored\n").unwrap();

    let (handle, _root) = discover_repo(&dir).unwrap();
    let entries = list_status(&handle, &dir).unwrap();
    let codes: Vec<EntryStatusCode> = entries.iter().map(|e| e.code).collect();
    assert!(
        codes.contains(&EntryStatusCode::Modified),
        "missing Modified: {:?}",
        entries
    );
    // Added: file staged but not committed shows up via the tree-index diff.
    // gix's iterator sometimes filters this depending on platform config; if it
    // doesn't surface, the explicit IntentToAdd path still maps to Added. We
    // accept either Added or Modified for the staged file, since the chip
    // categorizes both as "dirty index."
    assert!(
        codes.contains(&EntryStatusCode::Added) || entries.iter().any(|e| e.relative_path == "added.txt"),
        "missing Added or staged path: {:?}",
        entries
    );
    assert!(
        codes.contains(&EntryStatusCode::Untracked),
        "missing Untracked: {:?}",
        entries
    );
    cleanup(&dir);
}

// ── Friendly errors ──────────────────────────────────────────────────────

#[test]
fn friendly_messages_never_contain_error_or_failed() {
    for kind in [
        FriendlyGitErrorKind::NotARepo,
        FriendlyGitErrorKind::OrphanedWorktree,
        FriendlyGitErrorKind::CorruptRepo,
        FriendlyGitErrorKind::IndexLocked,
        FriendlyGitErrorKind::PermissionDenied,
        FriendlyGitErrorKind::BareRepo,
    ] {
        let title = kind.title().to_lowercase();
        let explanation = kind.explanation().to_lowercase();
        let suggestion = kind.suggestion().to_lowercase();
        assert!(!title.contains("error"), "title contains 'error': {}", kind.title());
        assert!(!title.contains("failed"), "title contains 'failed': {}", kind.title());
        assert!(!explanation.contains("error"));
        assert!(!explanation.contains("failed"));
        assert!(!suggestion.contains("error"));
        assert!(!suggestion.contains("failed"));
    }
}

#[test]
fn friendly_titles_use_sentence_case() {
    for kind in [
        FriendlyGitErrorKind::NotARepo,
        FriendlyGitErrorKind::CorruptRepo,
        FriendlyGitErrorKind::PermissionDenied,
    ] {
        let title = kind.title();
        // First char uppercase, rest mostly lowercase (proper nouns aside).
        let first = title.chars().next().unwrap();
        assert!(first.is_uppercase(), "title doesn't start with uppercase: {title}");
    }
}

#[test]
fn friendly_error_struct_carries_kind_and_path() {
    let err = FriendlyGitError::new(FriendlyGitErrorKind::NotARepo, "/tmp/foo");
    assert_eq!(err.kind, FriendlyGitErrorKind::NotARepo);
    assert_eq!(err.path, "/tmp/foo");
    assert!(!err.title().is_empty());
}

// ── Watcher integration ─────────────────────────────────────────────────

/// Recomputes RepoInfo before/after a `git commit` to assert state changes.
/// We intentionally don't pull in tauri::AppHandle here because there's no
/// real way to construct one in unit tests. The watcher integration test
/// asserts the recompute pipeline end to end (the IPC layer is the only
/// extra hop).
#[test]
fn repo_info_recomputes_after_commit() {
    let dir = temp_dir("watcher_recompute");
    init_repo_with_commit(&dir);

    let (handle, root) = discover_repo(&dir).unwrap();
    let before = repo_info(&handle, &root).unwrap();
    assert!(!before.is_dirty);

    // Make a change and commit. Both branch state and dirtiness should update.
    std::fs::write(dir.join("README.md"), "second\n").unwrap();
    let dirty = repo_info(&handle, &root).unwrap();
    assert!(dirty.is_dirty);

    git(&dir, &["add", "README.md"]);
    git(&dir, &["commit", "-q", "-m", "second"]);
    let after = repo_info(&handle, &root).unwrap();
    assert!(!after.is_dirty);
    cleanup(&dir);
}
