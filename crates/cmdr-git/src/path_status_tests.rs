//! What [`path_status`] says about one path, against a real repository built with the git
//! CLI (so the index is git's own), one item per state.

use std::path::{Path, PathBuf};

use super::{PathStatus, path_status};
use crate::test_fixtures::{cleanup, discover_repo, git_cli, temp_dir};

/// A repo with a committed `README.md`, `src/lib.rs`, a `*.log` and `target/` ignore, and
/// a file whose name holds glob characters.
fn repo(name: &str) -> PathBuf {
    let dir = temp_dir("path_status", name);
    git_cli(&dir, &["init", "-q", "-b", "main"]);
    write(&dir, "README.md", "hi\n");
    write(&dir, "src/lib.rs", "fn a() {}\n");
    write(&dir, "src/main.rs", "fn main() {}\n");
    write(&dir, ".gitignore", "*.log\ntarget/\n");
    write(&dir, "star[1]*.txt", "glob\n");
    git_cli(&dir, &["add", "."]);
    git_cli(&dir, &["commit", "-q", "-m", "init"]);
    dir
}

fn write(dir: &Path, relative: &str, content: &str) {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(path, content).expect("write");
}

fn status(dir: &Path, relative: &str, is_folder: bool) -> PathStatus {
    let (handle, _root) = discover_repo(dir).expect("repo");
    path_status(&handle, relative, is_folder).expect("status")
}

#[test]
fn a_committed_unchanged_file_is_clean_and_an_edited_one_changed() {
    let dir = repo("file");
    assert_eq!(status(&dir, "README.md", false), PathStatus::Clean);
    assert_eq!(status(&dir, "src/lib.rs", false), PathStatus::Clean);
    write(&dir, "src/lib.rs", "fn b() {}\n");
    assert_eq!(status(&dir, "src/lib.rs", false), PathStatus::Changed);
    // Its unchanged sibling stays clean: the walk is limited to the one path.
    assert_eq!(status(&dir, "src/main.rs", false), PathStatus::Clean);
    cleanup(&dir);
}

#[test]
fn a_staged_new_file_has_changes_no_commit_holds() {
    let dir = repo("staged");
    write(&dir, "new.txt", "new\n");
    git_cli(&dir, &["add", "new.txt"]);
    assert_eq!(status(&dir, "new.txt", false), PathStatus::Changed);
    cleanup(&dir);
}

#[test]
fn untracked_and_ignored_items_are_told_apart() {
    let dir = repo("untracked");
    write(&dir, "notes.txt", "x\n");
    write(&dir, "debug.log", "x\n");
    write(&dir, "target/debug/app", "x\n");
    write(&dir, "scratch/a.txt", "x\n");
    assert_eq!(status(&dir, "notes.txt", false), PathStatus::Untracked);
    assert_eq!(status(&dir, "debug.log", false), PathStatus::Ignored);
    // Under an ignored folder, whatever its own name says.
    assert_eq!(status(&dir, "target/debug/app", false), PathStatus::Ignored);
    assert_eq!(status(&dir, "target", true), PathStatus::Ignored);
    assert_eq!(status(&dir, "scratch", true), PathStatus::Untracked);
    cleanup(&dir);
}

#[test]
fn a_folder_is_clean_only_when_git_holds_all_of_it() {
    let dir = repo("folder");
    assert_eq!(status(&dir, "src", true), PathStatus::Clean);
    // Ignored files beside the tracked ones: git can't bring those back.
    write(&dir, "src/trace.log", "x\n");
    assert_eq!(status(&dir, "src", true), PathStatus::TrackedInPart);
    // An untracked file in it is work no commit holds.
    write(&dir, "src/extra.rs", "x\n");
    assert_eq!(status(&dir, "src", true), PathStatus::Changed);
    // The root holds `target/`-style leftovers too, so it's tracked in part at best.
    std::fs::remove_file(dir.join("src/extra.rs")).expect("remove");
    assert_eq!(status(&dir, "", true), PathStatus::TrackedInPart);
    std::fs::remove_file(dir.join("src/trace.log")).expect("remove");
    assert_eq!(status(&dir, "", true), PathStatus::Clean);
    cleanup(&dir);
}

#[test]
fn a_folder_named_like_a_tracked_ones_prefix_is_its_own() {
    let dir = repo("prefix");
    write(&dir, "src-old/a.rs", "x\n");
    assert_eq!(status(&dir, "src-old", true), PathStatus::Untracked);
    // And the tracked `src` isn't changed by its neighbour's untracked file.
    assert_eq!(status(&dir, "src", true), PathStatus::Clean);
    cleanup(&dir);
}

#[test]
fn glob_characters_in_a_name_are_literal() {
    let dir = repo("glob");
    // `star[1]*.txt` as a glob would also match this tracked file, edited since.
    write(&dir, "star1-other.txt", "x\n");
    git_cli(&dir, &["add", "star1-other.txt"]);
    git_cli(&dir, &["commit", "-q", "-m", "other"]);
    write(&dir, "star1-other.txt", "edited\n");
    assert_eq!(status(&dir, "star[1]*.txt", false), PathStatus::Clean);
    write(&dir, "star[1]*.txt", "changed\n");
    assert_eq!(status(&dir, "star[1]*.txt", false), PathStatus::Changed);
    cleanup(&dir);
}
