use super::*;
use crate::test_support::TestDir;
use cmdr_git::test_fixtures::{Fixture, git_cli};

// ── The card's word for each state ──────────────────────────────────────────

#[test]
fn only_a_clean_item_is_committed_with_no_changes() {
    assert_eq!(
        status_from(PathStatus::Clean),
        Fact::Known(GitStatus::Tracked(TrackedState::Clean))
    );
    assert_eq!(
        status_from(PathStatus::Changed),
        Fact::Known(GitStatus::Tracked(TrackedState::Modified))
    );
    assert_eq!(status_from(PathStatus::Untracked), Fact::Known(GitStatus::Untracked));
    assert_eq!(status_from(PathStatus::Ignored), Fact::Known(GitStatus::Ignored));
    // Git holds only part of it: the row never says clean, so removal never says restore.
    assert_eq!(
        status_from(PathStatus::TrackedInPart),
        Fact::unknown(UnknownWhy::NotInStatus)
    );
}

// ── Over a real repo ────────────────────────────────────────────────────────

/// A repo with `clean.txt`, `changed.txt`, `src/lib.rs`, and a `*.log` ignore rule
/// committed, `changed.txt` edited since, and `new.txt` never added.
fn repo() -> TestDir {
    let dir = TestDir::new("about-git");
    let _fixture = Fixture::init(dir.to_path_buf());
    std::fs::write(dir.join("clean.txt"), "same").expect("write a file");
    std::fs::write(dir.join("changed.txt"), "before").expect("write a file");
    std::fs::create_dir(dir.join("src")).expect("make a folder");
    std::fs::write(dir.join("src/lib.rs"), "fn a() {}").expect("write a file");
    std::fs::write(dir.join(".gitignore"), "*.log\n").expect("write a file");
    git_cli(&dir, &["add", "."]);
    git_cli(&dir, &["commit", "-q", "-m", "first"]);
    std::fs::write(dir.join("changed.txt"), "after").expect("edit a file");
    std::fs::write(dir.join("new.txt"), "new").expect("write a file");
    dir
}

fn fact_for(path: &Path, is_folder: bool) -> Fact<GitContext> {
    git_fact(&repo_around(path, is_folder), path, is_folder, &SubJobs::default())
}

fn status(fact: &Fact<GitContext>) -> &Fact<GitStatus> {
    &fact.known().expect("the repo answers").status
}

#[test]
fn a_modified_file_answers_its_branch_and_uncommitted_work() {
    let dir = repo();
    let fact = fact_for(&dir.join("changed.txt"), false);
    let git = fact.known().expect("the repo answers");
    assert_eq!(git.branch.as_ref().map(ItemText::as_str), Some("main"));
    let root = dir.canonicalize().expect("the fixture is there");
    assert_eq!(git.repo_root.as_str(), root.to_string_lossy());
    assert_eq!(git.status, Fact::Known(GitStatus::Tracked(TrackedState::Modified)));
}

#[test]
fn a_clean_untracked_and_ignored_file_each_say_so() {
    let dir = repo();
    assert_eq!(
        status(&fact_for(&dir.join("new.txt"), false)),
        &Fact::Known(GitStatus::Untracked)
    );
    assert_eq!(
        status(&fact_for(&dir.join("clean.txt"), false)),
        &Fact::Known(GitStatus::Tracked(TrackedState::Clean))
    );
    std::fs::write(dir.join("trace.log"), "x").expect("write a file");
    assert_eq!(
        status(&fact_for(&dir.join("trace.log"), false)),
        &Fact::Known(GitStatus::Ignored)
    );
}

#[test]
fn a_folder_is_clean_only_while_git_holds_all_of_it() {
    let dir = repo();
    assert_eq!(
        status(&fact_for(&dir.join("src"), true)),
        &Fact::Known(GitStatus::Tracked(TrackedState::Clean))
    );
    std::fs::write(dir.join("src/build.log"), "x").expect("write a file");
    assert_eq!(
        status(&fact_for(&dir.join("src"), true)),
        &Fact::unknown(UnknownWhy::NotInStatus),
        "an ignored file beside the tracked ones is nothing git can restore"
    );
    assert_eq!(
        status(&fact_for(&dir, true)),
        &Fact::Known(GitStatus::Tracked(TrackedState::Modified)),
        "the root holds every change"
    );
}

#[test]
fn a_file_outside_every_repo_has_none_around_it() {
    let dir = TestDir::new("about-no-git");
    let file = dir.join("loose.txt");
    std::fs::write(&file, "x").expect("write a file");
    assert!(!repo_around(&file, false).is_some());
}

#[test]
fn a_walk_past_its_budget_leaves_the_root_and_branch_standing() {
    let dir = repo();
    let RepoAround::Found { handle, .. } = repo_around(&dir, true) else {
        panic!("the fixture is a repo");
    };
    // No wait at all: the read was only just handed to the pool.
    let status = status_within(&handle, "changed.txt", false, Duration::ZERO, &SubJobs::default());
    assert_eq!(status, Fact::unknown(UnknownWhy::TimedOut));
}
