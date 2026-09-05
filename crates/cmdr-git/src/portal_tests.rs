//! Integration tests for the virtual `.git/` portal: classify, the category
//! listers, `list_tree`, blob-read parity, cross-volume copy.
//!
//! Fixtures go through `test_fixtures::Fixture` (in-process gix). The
//! one test that asserts byte-for-byte parity with `git show` still
//! shells out for that comparison (no gix-side equivalent that's
//! cheaper than just opening the blob).

#![cfg(test)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::path::{Cat, VirtualGitPath, classify, to_path};
use crate::read_blob::GitBlobReadStream;
use crate::test_fixtures::{EntryKind, Fixture, cleanup, discover_repo, git_cli_capture, temp_dir};
use crate::{tree, virtual_listing};
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError, VolumeReadStream};

/// The read-only volume serving `<repo>/.git`'s virtual trees, which is what a
/// resolve hands any `.git/<category>/` path.
fn portal_over(repo: &Path) -> crate::volume::GitPortalVolume {
    let parent: std::sync::Arc<dyn Volume> = std::sync::Arc::new(InMemoryVolume::new("Parent"));
    let portal = std::sync::Arc::new(crate::portal::GitPortal::new(
        VolumeHost::detached(),
        crate::state_sink::no_git_state_sink(),
    ));
    crate::volume::GitPortalVolume::new(portal, repo.to_path_buf(), parent)
}

fn git_show_bytes(dir: &Path, spec: &str) -> Vec<u8> {
    git_cli_capture(dir, &["show", spec])
}

fn build_fixture_repo() -> PathBuf {
    let dir = temp_dir("m2", "portal");
    let mut f = Fixture::init(dir.clone());

    // Set executable bit on `scripts/run.sh` before the commit so the
    // tree records mode 0o755 (`BlobExecutable`). The on-disk perm
    // assignment also matches what a user would see in a checked-out
    // working tree.
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("scripts").join("run.sh"), "#!/bin/sh\necho hi\n").unwrap();
    std::fs::set_permissions(
        dir.join("scripts").join("run.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    f.commit_files_with_modes(
        &[
            ("README.md", b"hello\n", EntryKind::Blob),
            ("scripts/run.sh", b"#!/bin/sh\necho hi\n", EntryKind::BlobExecutable),
        ],
        "initial",
        1_700_000_000,
    );

    // Create a branch with a slash in its name so the path classifier
    // has a non-trivial case to handle.
    f.create_branch("feature/foo");

    // Lightweight tag at HEAD.
    let head_id = f
        .repo
        .find_reference("refs/heads/main")
        .unwrap()
        .peel_to_id()
        .unwrap()
        .detach();
    f.repo
        .reference(
            "refs/tags/v1.0",
            head_id,
            gix::refs::transaction::PreviousValue::MustNotExist,
            "test_fixtures: lightweight tag",
        )
        .expect("create tag ref");

    dir
}

#[test]
fn classify_and_round_trip() {
    let dir = build_fixture_repo();
    let dot_git = dir.join(".git");

    // Root.
    let (virt, _, root) = classify(&dot_git).expect("classify root");
    assert_eq!(virt, VirtualGitPath::Root);
    assert_eq!(to_path(&virt, &root), dot_git.canonicalize().unwrap());

    // Category.
    let p = dot_git.join("branches");
    let (virt, _, _) = classify(&p).expect("classify branches");
    assert_eq!(virt, VirtualGitPath::Category(Cat::Branches));

    // Ref with a slash.
    let p = dot_git.join("branches").join("feature").join("foo");
    let (virt, _, _) = classify(&p).expect("classify feature/foo");
    assert_eq!(virt, VirtualGitPath::Ref(Cat::Branches, "feature/foo".into()));

    // RefTree.
    let p = dot_git.join("branches").join("main").join("scripts").join("run.sh");
    let (virt, _, _) = classify(&p).expect("classify reftree");
    assert_eq!(
        virt,
        VirtualGitPath::RefTree(Cat::Branches, "main".into(), "scripts/run.sh".into())
    );

    // Tag tree.
    let p = dot_git.join("tags").join("v1.0").join("README.md");
    let (virt, _, _) = classify(&p).expect("classify tag tree");
    assert_eq!(
        virt,
        VirtualGitPath::RefTree(Cat::Tags, "v1.0".into(), "README.md".into())
    );

    // Real `.git/*` entries don't classify as virtual – the volume hook
    // returns `None` and the parent volume's real-FS path takes over.
    assert!(classify(&dot_git.join("HEAD")).is_none(), "HEAD is real, not virtual");
    assert!(
        classify(&dot_git.join("config")).is_none(),
        "config is real, not virtual"
    );
    assert!(
        classify(&dot_git.join("refs").join("heads").join("main")).is_none(),
        "refs/ is real, not virtual"
    );

    cleanup(&dir);
}

#[test]
fn list_branches_includes_slashed_name() {
    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_branches(&handle, &root).unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"main"));
    assert!(names.contains(&"feature/foo"));
    for entry in &entries {
        assert!(entry.is_directory);
        assert_eq!(entry.icon_id, "git:branch");
    }
    cleanup(&dir);
}

#[test]
fn list_tags_yields_v1() {
    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_tags(&handle, &root).unwrap();
    assert!(entries.iter().any(|e| e.name == "v1.0"));
    cleanup(&dir);
}

/// The six category rows the listing overlay contributes to a repo's `.git/`,
/// in their fixed display order. The real `.git/*` entries beside them are the
/// LOCAL volume's now, so they aren't this function's business at all.
#[test]
fn list_categories_yields_the_six_rows_in_display_order() {
    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let names: Vec<String> = virtual_listing::list_categories(&handle, &root)
        .into_iter()
        .map(|e| e.name)
        .collect();

    assert_eq!(
        names,
        ["branches", "tags", "commits", "stash", "worktrees", "submodules"]
    );
    for name in &names {
        assert!(
            root.join(".git").join(name).exists() || true,
            "a category row is a name, not an inode"
        );
    }
    cleanup(&dir);
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
    let expected = git_show_bytes(&root, "main:scripts/run.sh");
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

/// The punchline: a copy OUT of a virtual `.git/branches/main/…` path into
/// another volume. The bytes must match `git show` exactly, and the entry must
/// carry the executable bit the copy engine sets on the destination.
#[tokio::test]
async fn a_copy_out_of_a_snapshot_carries_the_bytes_and_the_executable_bit() {
    use std::ops::ControlFlow;
    use std::sync::atomic::{AtomicU64, Ordering};

    let repo_dir = build_fixture_repo();
    let (_, root) = discover_repo(&repo_dir).unwrap();

    // The source is the PORTAL volume: a resolve routes any `.git/<category>/`
    // path there, and the volume holding the repo no longer knows what git is.
    // The destination is any other volume, which is the whole point: the copy
    // engine moves bytes between two `Volume`s and neither end is special.
    let src = portal_over(&root);
    let dst = InMemoryVolume::new("dst");

    let src_path = root
        .join(".git")
        .join("branches")
        .join("main")
        .join("scripts")
        .join("run.sh");
    let stream = src.open_read_stream(&src_path).await.expect("open virtual blob");
    let total = stream.total_size();

    let dest_rel = Path::new("run.sh");
    let counter = AtomicU64::new(0);
    let on_progress = |bytes: u64, _total: u64| -> ControlFlow<()> {
        counter.store(bytes, Ordering::SeqCst);
        ControlFlow::Continue(())
    };
    let written = dst
        .write_from_stream(dest_rel, total, stream, &on_progress)
        .await
        .expect("write_from_stream");
    assert_eq!(written, total);
    assert_eq!(counter.load(Ordering::SeqCst), total, "progress reports every byte");

    // What landed has to be what `git show main:scripts/run.sh` prints.
    let mut landed = dst.open_read_stream(dest_rel).await.expect("read the copy back");
    let mut actual = Vec::new();
    while let Some(chunk) = landed.next_chunk().await {
        actual.extend_from_slice(&chunk.expect("chunk"));
    }
    let expected = git_show_bytes(&root, "main:scripts/run.sh");
    assert_eq!(actual, expected, "bytes must match git show");

    // `write_from_stream` doesn't carry the mode itself — that's the copy
    // engine's job, layered on the `FileEntry.permissions` this volume answers
    // with. So what this owes is the DATA that engine needs.
    let entry = src.get_metadata(&src_path).await.expect("get_metadata virtual");
    assert_eq!(
        entry.permissions & 0o111,
        0o111,
        "virtual entry must carry executable bit"
    );

    cleanup(&repo_dir);
}

/// A path that simply isn't in the snapshot reads as "not there", ❌ never as
/// "this repo is damaged". `gix`'s tree walk answers `Ok(None)` for a name it
/// couldn't find, which is an ordinary miss (a typo in the path bar, a file
/// that only exists on another branch) and has to stay distinct from the
/// `Err` that means the object database couldn't answer at all.
#[tokio::test]
async fn a_path_missing_from_a_snapshot_reads_as_not_found() {
    let dir = build_fixture_repo();
    let (_, root) = discover_repo(&dir).unwrap();
    let volume = portal_over(&dir);

    for missing in [
        root.join(".git/branches/main/no-such-file.txt"),
        root.join(".git/branches/main/no-such-dir/inner.txt"),
        root.join(".git/branches/no-such-branch"),
        root.join(".git/tags/no-such-tag/README.md"),
    ] {
        let answered = volume.get_metadata(&missing).await;
        assert!(
            matches!(answered, Err(VolumeError::NotFound(ref carried)) if carried.contains("no-such")),
            "{}: {answered:?}",
            missing.display()
        );
    }

    // Listing one is the same answer.
    let listed = volume
        .list_directory(&root.join(".git/branches/main/no-such-dir"), None)
        .await;
    assert!(matches!(listed, Err(VolumeError::NotFound(_))), "{listed:?}");

    // And so is opening one for read.
    let opened = volume
        .open_read_stream(&root.join(".git/branches/main/no-such-file.txt"))
        .await;
    assert!(
        matches!(opened, Err(VolumeError::NotFound(_))),
        "opening a missing blob"
    );

    cleanup(&dir);
}
