//! `inspect_file` over a repo's virtual `.git` trees: a file in a branch snapshot
//! reads through the same bounded temp the viewer uses, a directory in one is a
//! folder, and a path that isn't in the snapshot is missing.
//!
//! The archive twin is `archive_tests.rs`. That one carries the richer archive
//! rows (the entry listing, the encrypted and corrupt verdicts); this one is the
//! plain-file half every other route gets.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::*;
use crate::file_system::git;
use crate::file_system::volume::LocalPosixVolume;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_viewer::routed_extract::{EXTRACT_CAP_BYTES, extract_if_routed_with};
use crate::test_support::TestDir;
use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};

/// A repo with one commit, registered as the local drive holding it, with the
/// portal switched on. (nextest isolates the process-global manager per test.)
fn repo_registered_as_the_local_drive(name: &str) -> std::path::PathBuf {
    let dir = temp_dir("inspect_git", name);
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("notes.txt", b"line one\nline two\n", "initial");
    get_volume_manager().register("root", Arc::new(LocalPosixVolume::new("Root", dir.to_str().unwrap())));
    git::wiring::set_virtual_portal_enabled(true);
    dir
}

/// Inspect with the materialization pointed at `extract_dir`, so the test can see
/// whether the temp was cleaned up.
fn inspect_extracting_to(path: &Path, extract_dir: &Path) -> FileRow {
    let extract = |requested: &Path, volume_id: &str| {
        extract_if_routed_with(requested, volume_id, extract_dir, EXTRACT_CAP_BYTES)
    };
    inspect_path_with(
        path.to_str().unwrap(),
        &TextAsk::Window(WindowOpts::default()),
        &AtomicBool::new(false),
        &extract,
    )
}

#[test]
fn a_file_in_a_branch_snapshot_reads_through_a_temp_that_is_gone_afterwards() {
    let dir = repo_registered_as_the_local_drive("file");
    let extract_dir = TestDir::new("inspect_git_extract");

    let row = inspect_extracting_to(&dir.join(".git/branches/main/notes.txt"), &extract_dir);
    let FileRow::Ok(file) = &row else {
        panic!("expected an ok row, got {row:?}");
    };
    assert_eq!(file.name, "notes.txt");
    assert_eq!(file.extension.as_deref(), Some("txt"));
    let Content::Text(text) = &file.content else {
        panic!("expected text, got {:?}", file.content);
    };
    assert_eq!(text.window.as_ref().unwrap().content, "line one\nline two\n");
    // ❌ No `modified`: the temp was written a moment ago, and quoting its mtime
    // would report today for a file committed years back.
    assert_eq!(file.modified, None);

    let leftovers: Vec<_> = std::fs::read_dir(&*extract_dir)
        .expect("read the extract dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    assert!(leftovers.is_empty(), "the temp is removed, found {leftovers:?}");

    cleanup(&dir);
}

#[test]
fn a_directory_in_a_snapshot_is_a_folder_and_a_missing_one_is_missing() {
    let dir = repo_registered_as_the_local_drive("shapes");
    let extract_dir = TestDir::new("inspect_git_extract_shapes");

    assert!(matches!(
        inspect_extracting_to(&dir.join(".git/branches/main"), &extract_dir),
        FileRow::Folder { .. }
    ));
    assert!(matches!(
        inspect_extracting_to(&dir.join(".git/branches/main/nope.txt"), &extract_dir),
        FileRow::Missing { .. }
    ));

    cleanup(&dir);
}

/// The real files under `.git/` are the parent volume's, so they take the plain
/// `std::fs` pipeline and report their own mtime.
#[test]
fn a_real_file_under_dot_git_stays_on_the_plain_pipeline() {
    let dir = repo_registered_as_the_local_drive("real_file");
    let extract_dir = TestDir::new("inspect_git_extract_real");

    let row = inspect_extracting_to(&dir.join(".git/HEAD"), &extract_dir);
    let FileRow::Ok(file) = &row else {
        panic!("expected an ok row, got {row:?}");
    };
    assert_eq!(file.name, "HEAD");
    assert!(file.modified.is_some(), "a real file reports its own mtime");

    cleanup(&dir);
}
