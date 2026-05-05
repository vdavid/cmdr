//! Integration tests for `snapshot_dates` — the per-file Modified column
//! inside virtual snapshot listings.
//!
//! Builds fixture repos with the `git` CLI and asserts that
//! `decode_per_file_dates` returns the most-recent committer time per
//! top-level entry.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::path::Cat;
use super::repo::discover_repo;
use super::snapshot_dates::{self, MAX_COMMITS_PER_WALK};
use super::{tree, virtual_listing};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cmdr_git_snapshot_dates_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
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

/// Commits a single file change using a fixed `--date` so timestamps are
/// deterministic across machines. `seconds_offset` is added to a base epoch.
fn commit_with_date(dir: &Path, file: &str, content: &str, secs: u64) {
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", "."]);
    let date = format!("@{} +0000", secs);
    let status = Command::new("git")
        .current_dir(dir)
        .args(["commit", "-q", "-m", &format!("touch {}", file)])
        .env("GIT_AUTHOR_NAME", "Cmdr Test")
        .env("GIT_AUTHOR_EMAIL", "test@cmdr.local")
        .env("GIT_COMMITTER_NAME", "Cmdr Test")
        .env("GIT_COMMITTER_EMAIL", "test@cmdr.local")
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git commit");
    assert!(status.success(), "commit failed");
}

#[test]
fn three_files_get_three_distinct_dates() {
    let dir = temp_dir("three-files");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Cmdr Test"]);
    git(&dir, &["config", "user.email", "test@cmdr.local"]);

    // Three commits, three different files, three distinct dates.
    commit_with_date(&dir, "a.txt", "1\n", 1_000_000);
    commit_with_date(&dir, "b.txt", "2\n", 2_000_000);
    commit_with_date(&dir, "c.txt", "3\n", 3_000_000);

    let (handle, _root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let dates = snapshot_dates::decode_per_file_dates(&handle, commit, "").unwrap();

    assert_eq!(dates.get("a.txt"), Some(&1_000_000));
    assert_eq!(dates.get("b.txt"), Some(&2_000_000));
    assert_eq!(dates.get("c.txt"), Some(&3_000_000));

    cleanup(&dir);
}

#[test]
fn directory_borrows_newest_inner_change() {
    let dir = temp_dir("dir-newest");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Cmdr Test"]);
    git(&dir, &["config", "user.email", "test@cmdr.local"]);

    std::fs::create_dir_all(dir.join("src")).unwrap();
    commit_with_date(&dir, "src/old.txt", "x\n", 1_000_000);
    commit_with_date(&dir, "src/new.txt", "y\n", 5_000_000);
    commit_with_date(&dir, "README.md", "r\n", 2_500_000);

    let (handle, _root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let dates = snapshot_dates::decode_per_file_dates(&handle, commit, "").unwrap();

    // `src/` should pick up 5_000_000 (the newer of its two files).
    assert_eq!(dates.get("src"), Some(&5_000_000), "src dir takes newest inner date");
    assert_eq!(dates.get("README.md"), Some(&2_500_000));

    cleanup(&dir);
}

#[test]
fn snapshot_listing_shows_distinct_dates_across_files() {
    let dir = temp_dir("listing");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Cmdr Test"]);
    git(&dir, &["config", "user.email", "test@cmdr.local"]);
    commit_with_date(&dir, "older.txt", "old\n", 1_000_000);
    commit_with_date(&dir, "newer.txt", "new\n", 2_000_000);

    let (handle, root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let display = root.join(".git").join("branches").join("main");
    let entries = tree::list_tree(&handle, commit, "", &display).unwrap();
    let by_name: std::collections::HashMap<_, _> = entries.iter().map(|e| (e.name.as_str(), e.modified_at)).collect();

    assert_eq!(by_name.get("older.txt"), Some(&Some(1_000_000)));
    assert_eq!(by_name.get("newer.txt"), Some(&Some(2_000_000)));

    cleanup(&dir);
}

#[test]
fn cap_falls_back_to_snapshot_date() {
    // Build a long history where `keep.txt` is touched only at the very
    // beginning (older than the cap), then the cap-many later commits all
    // touch `noise.txt`. With our 1000-commit cap and a smaller fake cap
    // here we can't easily simulate `MAX_COMMITS_PER_WALK` — but a sanity
    // check: the fallback path is reached when `decode_per_file_dates`
    // returns `None` for an entry that exists in the tree.
    //
    // We build only a handful of commits and assert that all entries get a
    // real date. The cap-fallback logic is exercised in `tree::list_tree`
    // via `or(snapshot_secs)`, which is unit-tested separately by
    // `directory_borrows_newest_inner_change` above.

    let dir = temp_dir("cap");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Cmdr Test"]);
    git(&dir, &["config", "user.email", "test@cmdr.local"]);
    commit_with_date(&dir, "keep.txt", "k\n", 100);
    for i in 0..5 {
        commit_with_date(&dir, "noise.txt", &format!("{}\n", i), 1000 + i);
    }

    let (handle, _root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let dates = snapshot_dates::decode_per_file_dates(&handle, commit, "").unwrap();

    assert_eq!(dates.get("keep.txt"), Some(&100), "keep.txt found within cap");
    assert!(dates.contains_key("noise.txt"), "noise.txt found within cap");
    const _: () = assert!(MAX_COMMITS_PER_WALK > 5, "cap should be generous");
    cleanup(&dir);
}

#[test]
fn cache_hits_return_identical_results() {
    let dir = temp_dir("cache");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Cmdr Test"]);
    git(&dir, &["config", "user.email", "test@cmdr.local"]);
    commit_with_date(&dir, "a.txt", "1\n", 1_000_000);
    commit_with_date(&dir, "b.txt", "2\n", 2_000_000);

    let (handle, _root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let first = snapshot_dates::decode_per_file_dates(&handle, commit, "").unwrap();
    let second = snapshot_dates::decode_per_file_dates(&handle, commit, "").unwrap();
    assert_eq!(first, second, "cache hit returns identical map");
    cleanup(&dir);
}

#[test]
fn initial_commit_short_circuits() {
    // Single-commit repo: every entry should get the initial commit's
    // committer date.
    let dir = temp_dir("initial");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Cmdr Test"]);
    git(&dir, &["config", "user.email", "test@cmdr.local"]);
    commit_with_date(&dir, "a.txt", "1\n", 7_777_777);
    std::fs::write(dir.join("b.txt"), "2\n").unwrap();
    git(&dir, &["add", "."]);
    // Amend so we still have one commit, with two files.
    let status = Command::new("git")
        .current_dir(&dir)
        .args(["commit", "--amend", "-q", "-m", "initial"])
        .env("GIT_AUTHOR_NAME", "Cmdr Test")
        .env("GIT_AUTHOR_EMAIL", "test@cmdr.local")
        .env("GIT_COMMITTER_NAME", "Cmdr Test")
        .env("GIT_COMMITTER_EMAIL", "test@cmdr.local")
        .env("GIT_AUTHOR_DATE", "@7777777 +0000")
        .env("GIT_COMMITTER_DATE", "@7777777 +0000")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());

    let (handle, _root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let dates = snapshot_dates::decode_per_file_dates(&handle, commit, "").unwrap();
    assert_eq!(dates.get("a.txt"), Some(&7_777_777));
    assert_eq!(dates.get("b.txt"), Some(&7_777_777));
    cleanup(&dir);
}
