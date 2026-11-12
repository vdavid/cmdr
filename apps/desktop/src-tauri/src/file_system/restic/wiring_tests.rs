//! The confirmed local check the write guards ask: a real repo's view is
//! refused, a real folder that merely shares the name isn't.

use super::wiring::local_path_is_in_a_restic_view;
use std::path::Path;

fn repo_shaped(root: &Path) {
    for folder in ["data", "index", "keys", "snapshots"] {
        std::fs::create_dir_all(root.join(folder)).expect("repo folder");
    }
    std::fs::write(root.join("config"), b"encrypted").expect("config");
}

#[test]
fn the_view_of_a_real_repo_is_in_a_restic_view() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("test-restic");
    repo_shaped(&repo);

    assert!(local_path_is_in_a_restic_view(&repo.join("Browse snapshots")));
    assert!(local_path_is_in_a_restic_view(
        &repo.join("Browse snapshots/latest/a.txt")
    ));
    assert!(
        !local_path_is_in_a_restic_view(&repo),
        "the repo folder is a real folder"
    );
    assert!(!local_path_is_in_a_restic_view(&repo.join("config")));
}

/// A real folder named `Browse snapshots` outside a repo takes ordinary writes.
#[test]
fn a_real_folder_with_the_name_is_not() {
    let dir = tempfile::tempdir().expect("tempdir");
    let real = dir.path().join("photos/Browse snapshots");
    std::fs::create_dir_all(&real).expect("a real folder of that name");

    assert!(!local_path_is_in_a_restic_view(&real));
    assert!(!local_path_is_in_a_restic_view(&real.join("x.jpg")));
}

/// The agent's rename plan asks the memo alone: a view the route confirmed is
/// refused, a folder the hub never met isn't, and nothing is read to find out.
#[test]
fn the_memo_check_answers_only_what_the_hub_confirmed() {
    use super::wiring::{memo_says_in_a_restic_view, repos};

    let repo = Path::new("/share/backups/test-restic");
    let inside = repo.join("Browse snapshots/latest/a.txt");
    assert!(!memo_says_in_a_restic_view("memo-check", &inside), "never met");

    repos().overlay_row("memo-check", repo);
    assert!(memo_says_in_a_restic_view("memo-check", &inside));
    assert!(!memo_says_in_a_restic_view("memo-check", &repo.join("config")));
    assert!(
        !memo_says_in_a_restic_view("another-volume", &inside),
        "keyed by volume"
    );
}

/// A repo whose folder holds more than a repo root can isn't read to the end.
#[test]
fn a_crowded_folder_is_not_a_repo() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("crowded");
    repo_shaped(&repo);
    for index in 0..super::wiring::MAX_REPO_ROOT_ENTRIES {
        std::fs::write(repo.join(format!("extra{index}")), b"").expect("extra file");
    }

    assert!(!local_path_is_in_a_restic_view(&repo.join("Browse snapshots")));
}
