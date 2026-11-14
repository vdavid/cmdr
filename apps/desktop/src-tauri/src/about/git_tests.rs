use super::*;
use crate::test_support::TestDir;
use cmdr_git::test_fixtures::{Fixture, git_cli};

fn entry(path: &str, code: EntryStatusCode) -> EntryStatus {
    EntryStatus {
        relative_path: path.to_string(),
        code,
    }
}

// ── What the changed-entries list proves ────────────────────────────────────

#[test]
fn a_file_absent_from_the_list_is_never_clean() {
    let entries = [entry("src/other.rs", EntryStatusCode::Modified)];
    assert_eq!(
        status_of(&entries, "src/lib.rs", false),
        Fact::unknown(UnknownWhy::NotInStatus)
    );
}

#[test]
fn a_files_own_entry_says_what_changed() {
    let modified = [entry("a.txt", EntryStatusCode::Modified)];
    assert_eq!(
        status_of(&modified, "a.txt", false),
        Fact::Known(GitStatus::Tracked(TrackedState::Modified))
    );
    let untracked = [entry("a.txt", EntryStatusCode::Untracked)];
    assert_eq!(status_of(&untracked, "a.txt", false), Fact::Known(GitStatus::Untracked));
    // A staged add, delete, or rename is changes no commit has: worded from the
    // side that never understates a loss.
    for code in [
        EntryStatusCode::Added,
        EntryStatusCode::Deleted,
        EntryStatusCode::Renamed,
        EntryStatusCode::Conflicted,
    ] {
        assert_eq!(
            status_of(&[entry("a.txt", code)], "a.txt", false),
            Fact::Known(GitStatus::Tracked(TrackedState::Modified)),
            "{code:?}"
        );
    }
}

#[test]
fn a_name_that_only_starts_like_the_file_isnt_the_file() {
    let entries = [entry("a.txt.bak", EntryStatusCode::Untracked)];
    assert_eq!(
        status_of(&entries, "a.txt", false),
        Fact::unknown(UnknownWhy::NotInStatus)
    );
}

#[test]
fn a_folder_with_a_tracked_change_holds_uncommitted_work() {
    let entries = [
        entry("src/new.rs", EntryStatusCode::Untracked),
        entry("src/lib.rs", EntryStatusCode::Modified),
        entry("srcs/x", EntryStatusCode::Modified),
    ];
    assert_eq!(
        status_of(&entries, "src", true),
        Fact::Known(GitStatus::Tracked(TrackedState::Modified))
    );
    // `srcs/x` sits beside `src`, not under it.
    assert_eq!(
        status_of(&entries[..1], "src", true),
        Fact::unknown(UnknownWhy::NotInStatus),
        "untracked entries alone don't say whether git tracks the rest"
    );
    assert_eq!(
        status_of(&entries[2..], "src", true),
        Fact::unknown(UnknownWhy::NotInStatus)
    );
}

#[test]
fn the_repo_root_sees_every_entry() {
    let entries = [entry("deep/down/file", EntryStatusCode::Modified)];
    assert_eq!(
        status_of(&entries, "", true),
        Fact::Known(GitStatus::Tracked(TrackedState::Modified))
    );
}

// ── Over a real repo ────────────────────────────────────────────────────────

/// A repo with `clean.txt` and `changed.txt` committed, `changed.txt` edited
/// since, and `new.txt` never added.
fn repo() -> TestDir {
    let dir = TestDir::new("about-git");
    let _fixture = Fixture::init(dir.to_path_buf());
    std::fs::write(dir.join("clean.txt"), "same").expect("write a file");
    std::fs::write(dir.join("changed.txt"), "before").expect("write a file");
    std::fs::create_dir(dir.join("src")).expect("make a folder");
    std::fs::write(dir.join("src/lib.rs"), "fn a() {}").expect("write a file");
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
fn an_untracked_file_says_so_and_a_clean_one_is_only_not_in_the_status() {
    let dir = repo();
    assert_eq!(
        status(&fact_for(&dir.join("new.txt"), false)),
        &Fact::Known(GitStatus::Untracked)
    );
    assert_eq!(
        status(&fact_for(&dir.join("clean.txt"), false)),
        &Fact::unknown(UnknownWhy::NotInStatus)
    );
}

#[test]
fn a_folder_with_no_changes_under_it_is_only_not_in_the_status() {
    let dir = repo();
    assert_eq!(
        status(&fact_for(&dir.join("src"), true)),
        &Fact::unknown(UnknownWhy::NotInStatus)
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
    let RepoAround::Found { handle, root } = repo_around(&dir, true) else {
        panic!("the fixture is a repo");
    };
    // No wait at all: the walk was only just handed to the pool.
    let status = status_within(
        &handle,
        &root,
        "changed.txt",
        false,
        Duration::ZERO,
        &SubJobs::default(),
    );
    assert_eq!(status, Fact::unknown(UnknownWhy::TimedOut));
}
