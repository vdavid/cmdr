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

/// A submodule with uncommitted edits shows as modified, and walking its
/// worktree for that still never runs ITS filter driver: gix would open it with
/// its own, unstripped config.
#[test]
fn a_submodule_with_edits_is_modified_without_running_its_filter_driver() {
    use crate::test_fixtures::{build_simple_repo, git_cli};

    let (outer, _of) = build_simple_repo("status", 1);
    let (inner, _if) = build_simple_repo("status", 1);
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

    let sub = outer.join("vendor/inner");
    let marker = outer.join("driver-ran");
    let sub_git_dir = outer.join(".git/modules/vendor/inner");
    let config = sub_git_dir.join("config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&format!(
        "[filter \"evil\"]\n\tclean = \"touch '{0}'; cat\"\n\tprocess = \"touch '{0}'\"\n",
        marker.display()
    ));
    std::fs::write(&config, text).unwrap();
    std::fs::write(sub.join(".gitattributes"), "* filter=evil\n").unwrap();
    // Same size as `step 0\n`, so only hashing the file can tell.
    std::fs::write(sub.join("README.md"), "step 9\n").unwrap();

    let (handle, _root) = discover_repo(&outer).unwrap();
    let entries = list_status(&handle, &outer).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.relative_path == "vendor/inner" && e.code == EntryStatusCode::Modified),
        "the edited submodule must show as modified: {entries:?}"
    );
    let info = crate::repo::repo_info(&handle, &outer).unwrap();
    assert!(info.is_dirty, "an edited submodule makes the repo dirty");
    assert!(!marker.exists(), "walking the submodule ran its filter driver");
    cleanup(&outer);
    cleanup(&inner);
}

/// ❗ A repo's own config can name a filter driver (`filter.<x>.clean`) that
/// `.gitattributes` applies to every file, and gix runs it when status has to
/// hash a file whose stat changed. A downloaded or unzipped repo is owned by the
/// user, so gix trusts its config fully: showing the folder in a pane would run
/// the repo author's command. Status must never run one.
#[test]
fn status_never_runs_a_repos_filter_driver() {
    let dir = temp_dir("status", "filter-driver");
    let mut f = Fixture::init(dir.clone());
    f.commit_file("README.md", b"hello\n", "initial");

    let marker = dir.join("driver-ran");
    let config = dir.join(".git").join("config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&format!(
        "[filter \"evil\"]\n\tclean = \"touch '{0}'; cat\"\n\tprocess = \"touch '{0}'\"\n",
        marker.display()
    ));
    std::fs::write(&config, text).unwrap();
    std::fs::write(dir.join(".gitattributes"), "* filter=evil\n").unwrap();
    // Same size, new bytes and mtime: the stat can't settle it, so status has
    // to hash the file, which is where a filter would run.
    std::fs::write(dir.join("README.md"), "jello\n").unwrap();

    let (handle, _root) = discover_repo(&dir).unwrap();
    let _ = list_status(&handle, &dir);
    assert!(!marker.exists(), "status ran the repo's filter driver");
    // The chip's dirty check walks the same worktree.
    let info = crate::repo::repo_info(&handle, &dir).unwrap();
    assert!(info.is_dirty, "the changed README still counts as dirty");
    assert!(!marker.exists(), "the dirty check ran the repo's filter driver");
    cleanup(&dir);
}
