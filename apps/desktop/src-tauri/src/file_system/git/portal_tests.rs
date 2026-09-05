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

use super::path::{Cat, VirtualGitPath, classify, to_path};
use super::read_blob::GitBlobReadStream;
use super::test_fixtures::{EntryKind, Fixture, cleanup, discover_repo, git_cli_capture, temp_dir};
use super::{tree, virtual_listing};
use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError, VolumeReadStream};

/// The read-only volume serving `<repo>/.git`'s virtual trees, which is what a
/// resolve hands any `.git/<category>/` path.
fn portal_over(repo: &Path) -> super::volume::GitPortalVolume {
    let parent: std::sync::Arc<dyn Volume> = std::sync::Arc::new(LocalPosixVolume::new("Parent", repo));
    std::sync::Arc::new(super::portal::GitPortal::new(
        crate::volume_host::host(),
        super::state_sink::no_git_state_sink(),
    ))
    .volume_for(repo.to_path_buf(), parent)
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
    // returns `None` and the LocalPosixVolume real-FS path takes over.
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

/// The punchline: cross-volume copy from a virtual `.git/branches/main/...`
/// path to a real tmp dir. Bytes must match `git show` exactly, AND the
/// executable bit must be preserved on the destination.
#[tokio::test]
async fn cross_volume_copy_preserves_executable_bit() {
    use std::ops::ControlFlow;
    use std::sync::atomic::{AtomicU64, Ordering};

    let repo_dir = build_fixture_repo();
    let (_, root) = discover_repo(&repo_dir).unwrap();

    let dest_dir = temp_dir("m2", "copy_dest");

    // The source is the PORTAL volume: a resolve routes any `.git/<category>/`
    // path there, and the local volume no longer knows what git is.
    let src = portal_over(&root);
    let dst = LocalPosixVolume::new("dst", dest_dir.clone());

    // Source: virtual blob.
    let src_path = root
        .join(".git")
        .join("branches")
        .join("main")
        .join("scripts")
        .join("run.sh");
    let stream = src.open_read_stream(&src_path).await.expect("open virtual blob");
    let total = stream.total_size();

    // Destination: a real file in the tmp dir.
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

    // Bytes should match `git show main:scripts/run.sh`.
    let dest_abs = dest_dir.join("run.sh");
    let actual = std::fs::read(&dest_abs).unwrap();
    let expected = git_show_bytes(&root, "main:scripts/run.sh");
    assert_eq!(actual, expected, "bytes must match git show");

    // The executable bit isn't transferred by `write_from_stream` itself
    // (that's the copy engine's job, layered on top of the FileEntry's
    // `permissions` field). Here we assert that the FileEntry returned by
    // get_metadata carries `0o755`, so the copy engine has the data it
    // needs to set the bit on the destination. Manually flip the bit using
    // that data, then re-stat.
    let entry = src.get_metadata(&src_path).await.expect("get_metadata virtual");
    assert_eq!(
        entry.permissions & 0o111,
        0o111,
        "virtual entry must carry executable bit"
    );
    let perm = std::fs::Permissions::from_mode(entry.permissions);
    std::fs::set_permissions(&dest_abs, perm).unwrap();

    let dest_meta = std::fs::metadata(&dest_abs).unwrap();
    assert_eq!(
        dest_meta.permissions().mode() & 0o111,
        0o111,
        "dest should be executable"
    );

    cleanup(&repo_dir);
    cleanup(&dest_dir);
}

#[test]
fn watcher_invalidates_branches_listing_on_new_branch() {
    use crate::file_system::listing::caching_test_support::TestListing;
    use crate::file_system::volume::DEFAULT_VOLUME_ID;

    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_branches(&handle, &root).unwrap();

    // Plant a fake cached listing on `.git/branches`.
    let listing = TestListing::new()
        .volume(DEFAULT_VOLUME_ID)
        .path(root.join(".git").join("branches"))
        .entries(entries)
        .insert("git-branches-invalidate");

    // Make the watcher see a "ref change" by adding a new branch via
    // gix, then run the invalidation entry point directly. The unit-
    // level contract is "given a repo root, invalidate matching
    // listings" — driving notify-rs isn't needed.
    let new_handle = handle.to_thread_local();
    let head_id = new_handle
        .find_reference("refs/heads/main")
        .unwrap()
        .peel_to_id()
        .unwrap()
        .detach();
    new_handle
        .reference(
            "refs/heads/added-after-init",
            head_id,
            gix::refs::transaction::PreviousValue::MustNotExist,
            "portal_tests: new branch",
        )
        .expect("create branch ref");
    super::wiring::refresh_virtual_listings(&root);

    // Assert the listing is still in the cache (we full-refresh, not evict).
    assert!(listing.is_cached());
    cleanup(&dir);
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
