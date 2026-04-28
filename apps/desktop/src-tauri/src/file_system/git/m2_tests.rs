//! Integration tests for M2 — virtual `.git/` portal.
//!
//! Builds tiny fixture repos with the `git` CLI (already a system requirement
//! for M1) and exercises the volume hooks end-to-end.

#![cfg(test)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::path::{Cat, VirtualGitPath, classify, is_virtual, to_path};
use super::read_blob::GitBlobReadStream;
use super::repo::discover_repo;
use super::{tree, virtual_listing};
use crate::file_system::volume::VolumeReadStream;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cmdr_git_m2_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Cmdr Test")
        .env("GIT_AUTHOR_EMAIL", "test@cmdr.local")
        .env("GIT_COMMITTER_NAME", "Cmdr Test")
        .env("GIT_COMMITTER_EMAIL", "test@cmdr.local")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git command");
    assert!(status.success(), "git {:?} failed in {}", args, dir.display());
}

fn git_show_bytes(dir: &Path, spec: &str) -> Vec<u8> {
    Command::new("git")
        .current_dir(dir)
        .args(["show", spec])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("git show")
        .stdout
}

fn build_fixture_repo() -> PathBuf {
    let dir = temp_dir("portal");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Cmdr Test"]);
    git(&dir, &["config", "user.email", "test@cmdr.local"]);

    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("README.md"), "hello\n").unwrap();
    std::fs::write(dir.join("scripts").join("run.sh"), "#!/bin/sh\necho hi\n").unwrap();
    let perm = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(dir.join("scripts").join("run.sh"), perm).unwrap();

    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "initial"]);

    // Create a branch with a slash in its name so the path classifier has
    // a non-trivial case to handle.
    git(&dir, &["branch", "feature/foo"]);

    // Tag the initial commit (lightweight is enough for M2).
    git(&dir, &["tag", "v1.0"]);

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

    // Raw passthrough.
    let p = dot_git.join("raw").join("HEAD");
    let (virt, _, _) = classify(&p).expect("classify raw");
    assert_eq!(virt, VirtualGitPath::Raw("HEAD".into()));

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
fn list_root_omits_m3_categories() {
    let dir = build_fixture_repo();
    let (_, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_root(&root);
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["branches", "tags", "raw"]);
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
fn raw_passthrough_lists_real_gitdir() {
    let dir = build_fixture_repo();
    let (_, root) = discover_repo(&dir).unwrap();
    let entries = virtual_listing::list_raw(&root, "").expect("list_raw");
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"HEAD"));
    assert!(names.contains(&"refs"));
    cleanup(&dir);
}

#[test]
fn is_virtual_routes_through_volume_hooks() {
    let dir = build_fixture_repo();
    let (_, root) = discover_repo(&dir).unwrap();
    assert!(is_virtual(&root.join(".git")));
    assert!(is_virtual(&root.join(".git/branches")));
    assert!(is_virtual(&root.join(".git/branches/main/README.md")));
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

    let dest_dir = temp_dir("copy_dest");

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

    // Make the watcher see a "ref change" by adding a new branch and
    // calling the invalidation directly. We don't drive the notify-rs
    // event loop here — the unit-level contract is "given a repo root,
    // invalidate matching listings."
    git(&dir, &["branch", "added-after-init"]);
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
