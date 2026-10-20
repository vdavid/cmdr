//! Renaming one of the six category rows a repo's `.git/` listing shows (#529).
//!
//! `worktrees` and `branches` stand in for git's REAL `.git/worktrees/` and
//! `.git/branches/` folders at the same path, and the `.git/` pane itself is
//! writable, so a rename of the row would rename git's own folder and break
//! every linked worktree. The portal serves the category path, so every rename
//! entry point refuses it as read-only, either way round, and leaves the folder.
//!
//! Each cell runs on an engine of its own over the APP's answers
//! (`operations_host::test_engine`): the portal's routing lives in the app's
//! volume registry, which a test's fake registry doesn't route through.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::file_system::git;
use crate::file_system::volume::manager::get_volume_manager;
use cmdr_fs::volume::VolumeError;
use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::types::Initiator;
use cmdr_operations::testing::CollectorEventSink;
use cmdr_operations::{BulkRenameRow, MutationError, RenameStartError, SourceFingerprint, WriteOperationError};

/// A repo with a real `.git/worktrees/feature/` admin folder, on the local
/// drive a pane browses (`"root"`), with the portal on.
fn repo_with_a_linked_worktree(name: &str) -> PathBuf {
    let dir = temp_dir("portal_rename", name);
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    std::fs::create_dir_all(dir.join(".git/worktrees/feature")).expect("the worktree admin folder");
    std::fs::write(dir.join(".git/worktrees/feature/gitdir"), b"/elsewhere/.git\n").expect("gitdir");
    // (nextest isolates the process-global manager per test.)
    get_volume_manager().register(
        "root",
        Arc::new(LocalPosixVolume::local_folder(
            "Root",
            dir.to_str().unwrap(),
            crate::volume_host::host(),
        )),
    );
    git::wiring::set_virtual_portal_enabled(true);
    dir
}

fn is_read_only(result: &Result<(), MutationError>) -> bool {
    matches!(
        result,
        Err(MutationError::Volume {
            error: VolumeError::ReadOnly(_)
        })
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn renaming_the_worktrees_row_refuses_and_leaves_gits_folder() {
    let dir = repo_with_a_linked_worktree("single_from");
    let worktrees = dir.join(".git/worktrees");

    let result = super::test_engine()
        .rename_managed(
            worktrees.clone(),
            dir.join(".git/worktrees-old"),
            false,
            "root".to_string(),
            Initiator::User,
        )
        .await;

    assert!(is_read_only(&result), "{result:?}");
    assert!(worktrees.join("feature/gitdir").exists(), "git's folder stays put");
    assert!(!dir.join(".git/worktrees-old").exists());
    cleanup(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn renaming_a_real_entry_onto_a_category_path_refuses() {
    let dir = repo_with_a_linked_worktree("single_to");
    std::fs::remove_dir_all(dir.join(".git/worktrees")).expect("no worktrees yet");
    let staging = dir.join(".git/staging");
    std::fs::create_dir(&staging).expect("a real folder under .git");

    let result = super::test_engine()
        .rename_managed(
            staging.clone(),
            dir.join(".git/worktrees"),
            false,
            "root".to_string(),
            Initiator::User,
        )
        .await;

    assert!(is_read_only(&result), "{result:?}");
    assert!(staging.exists());
    assert!(!dir.join(".git/worktrees").exists());
    cleanup(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bulk_rename_of_the_worktrees_row_refuses() {
    let dir = repo_with_a_linked_worktree("bulk");
    let worktrees = dir.join(".git/worktrees");
    let rows = vec![BulkRenameRow {
        row_id: "1".to_string(),
        source: worktrees.clone(),
        destination: dir.join(".git/worktrees-old"),
        expected_fingerprint: SourceFingerprint::capture_local(&worktrees).expect("a fingerprint"),
    }];

    let result = super::test_engine()
        .start_renames(
            Arc::new(CollectorEventSink::new()),
            "root".to_string(),
            rows,
            Initiator::User,
        )
        .await;

    assert!(
        matches!(
            &result,
            Err(RenameStartError::Engine {
                error: WriteOperationError::ReadOnlyDevice { .. }
            })
        ),
        "{result:?}"
    );
    assert!(worktrees.join("feature/gitdir").exists());
    cleanup(&dir);
}

/// The real entries beside the six rows stay the drive's: a rename of
/// `.git/description` still lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_entry_under_dot_git_still_renames() {
    let dir = repo_with_a_linked_worktree("real_entry");
    let from = dir.join(".git/description");
    std::fs::write(&from, b"repo").expect("a real file");

    let result = super::test_engine()
        .rename_managed(
            from.clone(),
            dir.join(".git/description.old"),
            false,
            "root".to_string(),
            Initiator::User,
        )
        .await;

    assert!(result.is_ok(), "{result:?}");
    assert!(Path::new(&dir.join(".git/description.old")).exists());
    cleanup(&dir);
}
