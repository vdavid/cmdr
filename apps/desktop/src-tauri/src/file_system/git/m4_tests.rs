//! Integration tests for Modified + Size column population on virtual
//! git entries (the M4-era follow-up to the M1+M2+M3 portal).
//!
//! Builds tiny fixture repos with the `git` CLI (already a system
//! requirement for M1), exercises each listing module, and asserts the
//! `display_size`, `display_size_tooltip`, and `modified_at` fields land
//! per the contract documented in `git/CLAUDE.md` § Columns.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::path::{Cat, VirtualGitPath, classify};
use super::repo::discover_repo;
use super::{log as git_log, stash, submodules, virtual_listing, worktrees};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cmdr_git_m4_{}_{}_{}",
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

fn build_repo_with_branches(branches: &[(&str, usize)]) -> PathBuf {
    // First (main_commits, then per-branch extra_commits). Each extra
    // commit on a branch makes it ahead-of-main by that many.
    let dir = temp_dir("branches");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Cmdr Test"]);
    git(&dir, &["config", "user.email", "test@cmdr.local"]);
    std::fs::write(dir.join("README.md"), "main\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "initial"]);

    for (name, extra) in branches {
        git(&dir, &["branch", name]);
        git(&dir, &["checkout", "-q", name]);
        for n in 0..*extra {
            std::fs::write(dir.join(format!("{}-{}.txt", name, n)), "x\n").unwrap();
            git(&dir, &["add", "."]);
            git(&dir, &["commit", "-q", "-m", &format!("on {} #{}", name, n)]);
        }
        git(&dir, &["checkout", "-q", "main"]);
    }
    dir
}

fn build_simple_repo(commits: usize) -> PathBuf {
    let dir = temp_dir("simple");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Cmdr Test"]);
    git(&dir, &["config", "user.email", "test@cmdr.local"]);
    for n in 0..commits {
        std::fs::write(dir.join("README.md"), format!("step {}\n", n)).unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", &format!("commit {}", n)]);
    }
    dir
}

// ── Root listing — counts and dates ─────────────────────────────────

#[test]
fn root_listing_populates_size_with_item_counts() {
    let dir = build_repo_with_branches(&[("feature-a", 1), ("feature-b", 2)]);
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_root(&handle, &root);

    let by_name: std::collections::HashMap<&str, &crate::file_system::listing::FileEntry> =
        entries.iter().map(|e| (e.name.as_str(), e)).collect();

    let branches = by_name["branches"];
    assert_eq!(branches.size, Some(3), "main + feature-a + feature-b = 3 branches");
    assert_eq!(branches.display_size.as_deref(), Some("3 branches"));

    let commits = by_name["commits"];
    assert!(commits.size.is_some(), "commits/ category gets a count");
    assert!(
        commits
            .display_size
            .as_deref()
            .map(|s| s.contains("commit"))
            .unwrap_or(false),
        "commits/ display says 'commits'"
    );

    // Real `.git/*` entries land in the mixed listing too. HEAD is the
    // canary: every fresh git init writes one.
    let head = by_name.get("HEAD").expect("real .git/HEAD shows up in root");
    assert!(!head.is_directory, "HEAD is a real file");
    assert!(head.modified_at.is_some(), "real entries carry stat mtime");

    cleanup(&dir);
}

#[test]
fn root_listing_pluralizes_singular_entries() {
    let dir = build_simple_repo(1);
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_root(&handle, &root);
    let branches = entries.iter().find(|e| e.name == "branches").unwrap();
    assert_eq!(branches.display_size.as_deref(), Some("1 branch"));
    cleanup(&dir);
}

// ── Branches — ahead/behind + branch tip date ───────────────────────

#[test]
fn branches_listing_populates_ahead_behind() {
    let dir = build_repo_with_branches(&[("feat", 3)]);
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_branches(&handle, &root).unwrap();

    let feat = entries.iter().find(|e| e.name == "feat").expect("feat branch");
    assert_eq!(feat.display_size.as_deref(), Some("+3 / -0"));
    assert_eq!(feat.size, Some(3), "ahead-count is the within-category sort key");
    let tip = feat.display_size_tooltip.as_deref().unwrap_or("");
    assert!(tip.contains("3 commits ahead"), "tooltip mentions ahead: {}", tip);
    assert!(tip.contains("`main`"), "tooltip mentions fallback branch: {}", tip);
    assert!(feat.modified_at.is_some(), "branch tip date populated");

    cleanup(&dir);
}

#[test]
fn branches_listing_sorts_by_ahead_count_within_category() {
    let dir = build_repo_with_branches(&[("a", 5), ("b", 1), ("c", 2)]);
    let (handle, root) = discover_repo(&dir).unwrap();
    let mut entries = virtual_listing::list_branches(&handle, &root).unwrap();
    // Drop main (size==0 against itself = blank). Sort by `size` descending
    // to mirror what the listing pipeline does for Sort/Size descending.
    entries.retain(|e| e.size.unwrap_or(0) > 0);
    entries.sort_by_key(|e| std::cmp::Reverse(e.size));
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["a", "c", "b"], "5 ahead, 2 ahead, 1 ahead");

    cleanup(&dir);
}

#[test]
fn branches_default_branch_alone_has_blank_size() {
    // Single branch (main), no upstream, no fallback different from itself.
    let dir = build_simple_repo(1);
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_branches(&handle, &root).unwrap();
    let main = entries.iter().find(|e| e.name == "main").unwrap();
    assert!(main.display_size.is_none(), "main with no upstream stays blank");
    cleanup(&dir);
}

// ── Tags — short SHA ────────────────────────────────────────────────

#[test]
fn tags_listing_populates_short_sha() {
    let dir = build_simple_repo(1);
    git(&dir, &["tag", "v1.0"]);
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_tags(&handle, &root).unwrap();
    let v1 = entries.iter().find(|e| e.name == "v1.0").unwrap();
    let sha = v1.display_size.as_deref().expect("display_size set");
    assert_eq!(sha.len(), 7, "short SHA is 7 chars");
    assert!(sha.chars().all(|c| c.is_ascii_hexdigit()), "all hex");
    assert!(v1.modified_at.is_some(), "tag carries a date");
    cleanup(&dir);
}

// ── Commits — files-changed count ───────────────────────────────────

#[test]
fn commits_listing_populates_files_changed() {
    let dir = build_simple_repo(2);
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = git_log::list_commits(&handle, &root).unwrap();
    let top = &entries[0];
    let n = top.size.expect("files-changed size set");
    assert!(n >= 1, "at least one file changed in the second commit");
    let label = top.display_size.as_deref().unwrap_or("");
    assert!(label.contains("file"), "display says 'file' or 'files': {}", label);
    cleanup(&dir);
}

// ── Stash — branch parsing ──────────────────────────────────────────

#[test]
fn stash_listing_extracts_branch_from_subject() {
    let dir = build_simple_repo(1);
    std::fs::write(dir.join("scratch.txt"), "x\n").unwrap();
    git(&dir, &["stash", "push", "-u", "-m", "scratch work"]);

    let (_, root) = discover_repo(&dir).unwrap();
    let entries = stash::list_stashes(&root).unwrap();
    let first = &entries[0];
    assert_eq!(
        first.display_size.as_deref(),
        Some("on main"),
        "stash subject parses to branch"
    );
    cleanup(&dir);
}

// ── Worktrees — branch / SHA ────────────────────────────────────────

#[test]
fn worktree_listing_shows_branch() {
    let dir = build_simple_repo(1);
    let wt = dir
        .parent()
        .unwrap()
        .join(format!("{}-wt", dir.file_name().unwrap().to_string_lossy()));
    let _ = std::fs::remove_dir_all(&wt);
    git(&dir, &["worktree", "add", "-b", "wt-branch", wt.to_str().unwrap()]);

    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = worktrees::list_worktrees(&handle, &root).unwrap();
    let wt_entry = &entries[0];
    assert_eq!(wt_entry.display_size.as_deref(), Some("on wt-branch"));
    assert!(wt_entry.modified_at.is_some(), "worktree HEAD date set");
    cleanup(&dir);
    cleanup(&wt);
}

// ── Submodules — pinned short SHA ───────────────────────────────────

#[test]
fn submodule_listing_shows_pinned_sha() {
    let outer = build_simple_repo(1);
    let inner = build_simple_repo(1);
    let inner_url = format!("file://{}", inner.display());
    git(
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
    git(&outer, &["commit", "-q", "-m", "add submodule"]);

    let (handle, root) = discover_repo(&outer).unwrap();
    let entries = submodules::list_submodules(&handle, &root).unwrap();
    let sm = &entries[0];
    let sha = sm.display_size.as_deref().expect("submodule short SHA");
    assert_eq!(sha.len(), 7);
    assert!(sm.modified_at.is_some(), "pinned commit date");
    cleanup(&outer);
    cleanup(&inner);
}

// ── Snapshot interior — files share commit date, dirs get bytes ────

#[test]
fn snapshot_files_borrow_commit_date() {
    let dir = build_simple_repo(1);
    let (handle, root) = discover_repo(&dir).unwrap();
    let p = root.join(".git").join("branches").join("main");
    let (virt, _, _) = classify(&p).expect("classify branch tip");
    assert!(matches!(virt, VirtualGitPath::Ref(Cat::Branches, _)));

    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let entries = super::tree::list_tree(&handle, commit, "", &p).unwrap();
    for fe in &entries {
        assert!(fe.modified_at.is_some(), "every snapshot row carries the commit date");
    }
    // All entries share the same date (frozen point in time).
    let first = entries[0].modified_at;
    assert!(entries.iter().all(|e| e.modified_at == first));

    cleanup(&dir);
}

#[test]
fn snapshot_dirs_carry_recursive_bytes() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("snapshot-dirs");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Cmdr Test"]);
    git(&dir, &["config", "user.email", "test@cmdr.local"]);
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("scripts").join("a.sh"), "#!/bin/sh\necho hi\n").unwrap();
    std::fs::set_permissions(dir.join("scripts").join("a.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.join("scripts").join("b.sh"), "echo bye\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "init"]);

    let (handle, root) = discover_repo(&dir).unwrap();
    let p = root.join(".git").join("branches").join("main");
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let entries = super::tree::list_tree(&handle, commit, "", &p).unwrap();
    let scripts = entries.iter().find(|e| e.name == "scripts").unwrap();
    assert!(
        scripts.size.unwrap_or(0) > 0,
        "directory size is the recursive byte total"
    );
    assert!(
        scripts.recursive_size.unwrap_or(0) > 0,
        "recursive_size mirrors size for dirs"
    );

    cleanup(&dir);
}
