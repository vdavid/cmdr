//! Reading a commit's tree and the blobs in it: what a snapshot listing holds,
//! the modes it carries, and whether the bytes match what git itself prints.
//!
//! The one cell that asserts byte-for-byte parity shells out to `git show`,
//! since there's no gix-side equivalent cheaper than opening the blob.

#![cfg(test)]

use std::path::PathBuf;

use crate::path::Cat;
use crate::read_blob::GitBlobReadStream;
use crate::test_fixtures::{build_repo_with_a_tag_and_a_slashed_branch, cleanup, discover_repo, git_cli_capture};
use crate::{tree, virtual_listing};
use cmdr_fs::volume::VolumeReadStream;

fn build_fixture_repo() -> PathBuf {
    build_repo_with_a_tag_and_a_slashed_branch("tree")
}

#[test]
fn list_tree_at_main_includes_dirs_and_files() {
    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main")
        .unwrap()
        .expect("main exists");
    let display = root.join(".git").join("branches").join("main");
    let entries = tree::list_tree(&handle, commit, "", &display)
        .unwrap()
        .expect("the snapshot root is there");
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"scripts"));
    assert!(names.contains(&"README.md"));
    let scripts = entries.iter().find(|e| e.name == "scripts").unwrap();
    assert!(scripts.is_directory);
    let readme = entries.iter().find(|e| e.name == "README.md").unwrap();
    assert!(!readme.is_directory);
    cleanup(&dir);
}

#[test]
fn list_tree_preserves_executable_bit() {
    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main")
        .unwrap()
        .expect("main exists");
    let display = root.join(".git").join("branches").join("main").join("scripts");
    let entries = tree::list_tree(&handle, commit, "scripts", &display)
        .unwrap()
        .expect("scripts/ is there");
    let run = entries.iter().find(|e| e.name == "run.sh").expect("run.sh");
    assert_eq!(run.permissions, 0o755, "executable file should keep 0o755 mode");
    cleanup(&dir);
}

#[test]
fn read_blob_matches_git_show_bytes() {
    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main")
        .unwrap()
        .expect("main exists");
    let blob_id = tree::lookup_blob_id(&handle, commit, "scripts/run.sh")
        .unwrap()
        .expect("the blob is there");
    let bytes = tree::read_blob(&handle, blob_id).unwrap();
    let expected = git_cli_capture(&root, &["show", "main:scripts/run.sh"]);
    assert_eq!(bytes, expected);
    cleanup(&dir);
}

#[tokio::test]
async fn blob_stream_drains_to_full_blob() {
    let dir = build_fixture_repo();
    let (handle, _root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main")
        .unwrap()
        .expect("main exists");
    let blob_id = tree::lookup_blob_id(&handle, commit, "README.md")
        .unwrap()
        .expect("the blob is there");
    let bytes = tree::read_blob(&handle, blob_id).unwrap();

    let mut stream = GitBlobReadStream::new(bytes.clone());
    let mut drained = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        drained.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(drained, bytes);
    cleanup(&dir);
}
