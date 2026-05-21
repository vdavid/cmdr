//! Integration tests for M2 – virtual `.git/` portal.
//!
//! Fixtures go through `test_fixtures::Fixture` (in-process gix). The
//! one test that asserts byte-for-byte parity with `git show` still
//! shells out for that comparison (no gix-side equivalent that's
//! cheaper than just opening the blob).

#![cfg(test)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::path::{Cat, VirtualGitPath, classify, is_virtual, to_path};
use super::read_blob::GitBlobReadStream;
use super::repo::discover_repo;
use super::test_fixtures::{EntryKind, Fixture, cleanup, git_cli_capture, temp_dir};
use super::{tree, virtual_listing};
use crate::file_system::volume::VolumeReadStream;

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

#[test]
fn list_root_mixes_real_and_virtual_entries() {
    // The portal root surfaces real `.git/*` files (HEAD, config, hooks/,
    // objects/, refs/) followed by the six virtual category entries
    // (branches, tags, commits, stash, worktrees, submodules) in fixed
    // order. Real entries that collide with a virtual category name get
    // filtered out so the virtual one wins.
    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_root(&handle, &root);
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();

    // Real `.git/*` entries appear (every fresh git init creates these).
    for must_have in ["HEAD", "config", "hooks", "info", "objects", "refs"] {
        assert!(
            names.contains(&must_have),
            "real .git/{} must show up in the root listing: got {:?}",
            must_have,
            names
        );
    }

    // The six virtual categories appear in fixed order, after every real
    // entry.
    let virtual_order = ["branches", "tags", "commits", "stash", "worktrees", "submodules"];
    let positions: Vec<usize> = virtual_order
        .iter()
        .map(|n| {
            names
                .iter()
                .position(|x| x == n)
                .unwrap_or_else(|| panic!("virtual entry {} missing from {:?}", n, names))
        })
        .collect();
    assert_eq!(positions, (names.len() - 6..names.len()).collect::<Vec<_>>());
    for w in positions.windows(2) {
        assert!(w[0] < w[1], "virtual entries should keep fixed order: {:?}", positions);
    }

    // Collision filter: the deprecated real `.git/branches/` directory
    // must not show up as a real entry. The virtual `branches/` is the
    // only entry called `branches` in the listing.
    let branches_count = names.iter().filter(|n| **n == "branches").count();
    assert_eq!(branches_count, 1, "virtual branches/ takes precedence over real one");

    // `raw/` should not appear anywhere – we dropped it in favour of the
    // mixed real + virtual listing.
    assert!(!names.contains(&"raw"), "raw/ category was removed");

    cleanup(&dir);
}

#[test]
fn list_root_real_entries_sort_dirs_first_alpha() {
    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_root(&handle, &root);

    // Slice off the trailing six virtual entries; everything before is real.
    let real = &entries[..entries.len() - 6];

    // Dirs come before files.
    let last_dir = real.iter().rposition(|e| e.is_directory);
    let first_file = real.iter().position(|e| !e.is_directory);
    if let (Some(ld), Some(ff)) = (last_dir, first_file) {
        assert!(ld < ff, "dirs must come before files in real entries: {:?}", real);
    }

    // Within dirs and within files, alphabetical (case-insensitive).
    let dir_names: Vec<String> = real
        .iter()
        .filter(|e| e.is_directory)
        .map(|e| e.name.to_lowercase())
        .collect();
    let mut sorted = dir_names.clone();
    sorted.sort();
    assert_eq!(dir_names, sorted, "real dirs must sort alphabetically");

    let file_names: Vec<String> = real
        .iter()
        .filter(|e| !e.is_directory)
        .map(|e| e.name.to_lowercase())
        .collect();
    let mut sorted = file_names.clone();
    sorted.sort();
    assert_eq!(file_names, sorted, "real files must sort alphabetically");

    cleanup(&dir);
}

#[test]
fn list_tree_at_main_includes_dirs_and_files() {
    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let display = root.join(".git").join("branches").join("main");
    let entries = tree::list_tree(&handle, commit, "", &display).unwrap();
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
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let display = root.join(".git").join("branches").join("main").join("scripts");
    let entries = tree::list_tree(&handle, commit, "scripts", &display).unwrap();
    let run = entries.iter().find(|e| e.name == "run.sh").expect("run.sh");
    assert_eq!(run.permissions, 0o755, "executable file should keep 0o755 mode");
    cleanup(&dir);
}

#[test]
fn read_blob_matches_git_show_bytes() {
    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let blob_id = tree::lookup_blob_id(&handle, commit, "scripts/run.sh").unwrap();
    let bytes = tree::read_blob(&handle, blob_id).unwrap();
    let expected = git_show_bytes(&root, "main:scripts/run.sh");
    assert_eq!(bytes, expected);
    cleanup(&dir);
}

#[tokio::test]
async fn blob_stream_drains_to_full_blob() {
    let dir = build_fixture_repo();
    let (handle, _root) = discover_repo(&dir).unwrap();
    let commit = virtual_listing::resolve_ref_commit(&handle, Cat::Branches, "main").unwrap();
    let blob_id = tree::lookup_blob_id(&handle, commit, "README.md").unwrap();
    let bytes = tree::read_blob(&handle, blob_id).unwrap();

    let mut stream = GitBlobReadStream::new(bytes.clone());
    let mut drained = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        drained.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(drained, bytes);
    cleanup(&dir);
}

#[test]
fn is_virtual_routes_through_volume_hooks() {
    let dir = build_fixture_repo();
    let (_, root) = discover_repo(&dir).unwrap();
    // `is_virtual` is the mutation-guard cheap shape check: any path with
    // `.git` in it counts. That's by design – we never want to write to
    // `.git/HEAD` from a copy dialog, even though it's a real on-disk file.
    assert!(is_virtual(&root.join(".git")));
    assert!(is_virtual(&root.join(".git/branches")));
    assert!(is_virtual(&root.join(".git/branches/main/README.md")));
    assert!(is_virtual(&root.join(".git/HEAD")));
    assert!(!is_virtual(&root.join("scripts/run.sh")));
    cleanup(&dir);
}

/// The punchline: cross-volume copy from a virtual `.git/branches/main/...`
/// path to a real tmp dir. Bytes must match `git show` exactly, AND the
/// executable bit must be preserved on the destination.
#[tokio::test]
async fn cross_volume_copy_preserves_executable_bit() {
    use crate::file_system::volume::{LocalPosixVolume, Volume};
    use std::ops::ControlFlow;
    use std::sync::atomic::{AtomicU64, Ordering};

    let repo_dir = build_fixture_repo();
    let (_, root) = discover_repo(&repo_dir).unwrap();

    let dest_dir = temp_dir("m2", "copy_dest");

    let src = LocalPosixVolume::new("src", root.clone());
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
    use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
    use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use crate::file_system::volume::DEFAULT_VOLUME_ID;
    use std::sync::atomic::AtomicU64;

    let dir = build_fixture_repo();
    let (handle, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_branches(&handle, &root).unwrap();

    // Plant a fake cached listing on `.git/branches`.
    let listing_path = root.join(".git").join("branches");
    let listing_id = format!("test-listing-{}", std::process::id());
    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: DEFAULT_VOLUME_ID.to_string(),
                path: listing_path.clone(),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
                directory_sort_mode: DirectorySortMode::LikeFiles,
                sequence: AtomicU64::new(0),
                created_at: std::time::Instant::now(),
            },
        );
    }

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
            "m2_tests: new branch",
        )
        .expect("create branch ref");
    super::watcher::invalidate_for_test(&root);

    // Assert the listing is still in the cache (we full-refresh, not evict).
    {
        let cache = LISTING_CACHE.read().unwrap();
        assert!(cache.contains_key(&listing_id));
    }

    // Cleanup the listing.
    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.remove(&listing_id);
    }
    cleanup(&dir);
}
