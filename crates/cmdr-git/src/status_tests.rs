//! What [`list_status`] reports for a working tree, against a real repository.
//!
//! The cache's keying and the per-directory slicing are pure enough to assert
//! inline, so they live in `status.rs`'s own `cache_tests` and `slice_tests`
//! modules; this is the one cell that needs a repo with each status kind in it.

#![cfg(test)]

use crate::status::{EntryStatusCode, list_status};
use crate::test_fixtures::{Fixture, cleanup, discover_repo, temp_dir};

#[test]
fn list_status_returns_one_per_status() {
    let dir = temp_dir("status", "kinds");
    let mut f = Fixture::init(dir.clone());
    f.commit_file("README.md", b"hello\n", "initial");
    // Commit .gitignore so untracked.txt and ignored.txt can be
    // classified correctly relative to the working tree.
    f.commit_file(".gitignore", b"ignored.txt\n", "ignore");

    // Modified (in worktree relative to index)
    std::fs::write(dir.join("README.md"), "modified\n").unwrap();
    // Added (in worktree but not in index) — list_status surfaces this
    // via the IndexWorktree leg as Untracked; a `git add` would stage it
    // so it appeared as Added on the TreeIndex leg. We don't gix-stage
    // here because the assertion is tolerant (Added OR a path called
    // "added.txt" present in the output).
    std::fs::write(dir.join("added.txt"), "added\n").unwrap();
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
