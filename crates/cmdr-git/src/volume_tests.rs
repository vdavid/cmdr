//! What [`GitPortalVolume`](crate::volume::GitPortalVolume) promises as a
//! `Volume`: the shared read-only conformance assertions every backend runs,
//! plus the portal-specific half of its namespace.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::conformance;

use crate::portal::GitPortal;
use crate::test_fixtures::{EntryKind, Fixture, cleanup, git_cli_capture, temp_dir};
use crate::volume::GitPortalVolume;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{DirectoryCreation, InMemoryVolume, Volume, VolumeError};

/// A repo with one commit on `main` (a plain file and an executable one), a
/// second branch, and a tag, plus the portal volume serving it.
fn portal_over_a_repo(name: &str) -> (PathBuf, GitPortalVolume) {
    let dir = temp_dir("portal_volume", name);
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_files_with_modes(
        &[
            ("README.md", b"the bytes a copy would move", EntryKind::Blob),
            ("scripts/run.sh", b"#!/bin/sh\n", EntryKind::BlobExecutable),
        ],
        "initial",
        1_700_000_000,
    );
    fixture.create_branch("feature/foo");

    let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Parent"));
    let portal = Arc::new(GitPortal::new(
        VolumeHost::detached(),
        crate::state_sink::no_git_state_sink(),
    ));
    let volume = GitPortalVolume::new(portal, dir.clone(), parent);
    (dir, volume)
}

/// `<repo>/.git/<rest>`, the shape every path a portal volume is asked about
/// takes (the resolve hands the input path through verbatim).
fn virtual_path(repo: &Path, rest: &str) -> PathBuf {
    repo.join(".git").join(rest)
}

// ── The shared conformance assertions, from the read-only side ─────────────

/// The shared writability declaration: the portal offers no mutations, so it
/// must not claim any.
#[tokio::test]
async fn is_writable_honors_the_shared_declaration_contract() {
    let (dir, volume) = portal_over_a_repo("writability");

    conformance::assert_writability_matches_the_mutations_offered(&volume, &virtual_path(&dir, "branches/scratch"))
        .await;

    cleanup(&dir);
}

/// The shared export handshake: the portal exists to be copied OUT of, so the
/// bytes it streams and the capability it declares have to agree.
#[tokio::test]
async fn export_honors_the_shared_handshake_contract() {
    let (dir, volume) = portal_over_a_repo("export");

    conformance::assert_export_matches_the_bytes_offered(
        &volume,
        &virtual_path(&dir, "branches/main/README.md"),
        b"the bytes a copy would move",
    )
    .await;

    cleanup(&dir);
}

/// The shared `NotFound` payload: a name that isn't in the snapshot reports the
/// path the caller asked for, which is what the transfer layer renders as the
/// user's own file name.
#[tokio::test]
async fn not_found_honors_the_shared_path_payload_contract() {
    let (dir, volume) = portal_over_a_repo("not_found");

    conformance::assert_not_found_carries_the_path(&volume, &virtual_path(&dir, "branches/main/no-such-file.txt"))
        .await;

    cleanup(&dir);
}

/// The shared stop assertion: a copy scan of a snapshot honors Cancel, so a
/// user who starts one over a huge branch tree can get out of it.
#[tokio::test]
async fn a_batch_scan_stops_when_it_is_told_to() {
    let (dir, volume) = portal_over_a_repo("scan_stop");

    conformance::assert_batch_scan_stops_when_told(&volume, &virtual_path(&dir, "branches/main")).await;

    cleanup(&dir);
}

/// The shared "ask inside the walk" assertion: the boundary is consulted per
/// entry, ❗ not per source path, so Cancel reaches a walk already in progress.
#[tokio::test]
async fn a_batch_scan_asks_the_boundary_inside_the_walk() {
    let (dir, volume) = portal_over_a_repo("scan_asks");

    conformance::assert_batch_scan_asks_inside_the_walk(&volume, &virtual_path(&dir, "branches/main"), 2).await;

    cleanup(&dir);
}

// ── The portal's own namespace ─────────────────────────────────────────────

/// The volume's root is `<worktree>/.git`, and listing it answers the six
/// virtual categories and NOTHING else. The real `.git/*` entries belong to the
/// parent volume, which is what keeps them writable and keeps a walker that
/// lists through this volume from meeting a name with no inode behind it.
#[tokio::test]
async fn the_root_listing_is_the_six_categories_and_no_real_entries() {
    let (dir, volume) = portal_over_a_repo("root_listing");

    assert_eq!(volume.root(), dir.join(".git"));
    let names: Vec<String> = volume
        .list_directory(&dir.join(".git"), None)
        .await
        .expect("the portal lists its own root")
        .into_iter()
        .map(|entry| entry.name)
        .collect();

    assert_eq!(
        names,
        ["branches", "tags", "commits", "stash", "worktrees", "submodules"]
    );

    cleanup(&dir);
}

/// A real `.git/*` entry is not in this volume's namespace, even though it's
/// right there on disk: the parent volume serves it, and this one says so
/// rather than inventing a row.
#[tokio::test]
async fn a_real_dot_git_entry_is_not_the_portals_to_serve() {
    let (dir, volume) = portal_over_a_repo("real_entries");

    for real in ["config", "HEAD"] {
        assert!(dir.join(".git").join(real).exists(), "{real} is on disk");
        let path = virtual_path(&dir, real);
        assert!(!volume.exists(&path).await, "{real} is not the portal's");
        assert!(
            matches!(volume.get_metadata(&path).await, Err(VolumeError::NotFound(_))),
            "{real} must read as not found here"
        );
    }

    cleanup(&dir);
}

/// A `.git` that isn't a repository at all answers `NotFound` instead of
/// failing to construct: routing is lexical and does no I/O, so this volume is
/// where "is there actually a repo here?" gets answered, on first use.
#[tokio::test]
async fn a_dot_git_that_is_not_a_repository_answers_not_found() {
    let dir = temp_dir("portal_volume", "not_a_repo");
    std::fs::create_dir_all(dir.join(".git")).expect("make a bare .git directory");
    std::fs::write(dir.join(".git").join("HEAD"), b"not a repo\n").expect("seed junk");

    let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Parent"));
    let portal = Arc::new(GitPortal::new(
        VolumeHost::detached(),
        crate::state_sink::no_git_state_sink(),
    ));
    let volume = GitPortalVolume::new(portal, dir.clone(), parent);

    let branches = virtual_path(&dir, "branches");
    assert!(matches!(
        volume.list_directory(&branches, None).await,
        Err(VolumeError::NotFound(_))
    ));
    assert!(!volume.exists(&branches).await);

    cleanup(&dir);
}

/// Browsing down a branch: the category lists both branches, and the branch
/// lists the snapshot's own tree with the executable bit intact, which is what
/// a cross-volume copy reads to set the mode on the destination.
#[tokio::test]
async fn a_branch_lists_its_snapshot_with_modes_intact() {
    let (dir, volume) = portal_over_a_repo("branch_tree");

    let branches: Vec<String> = volume
        .list_directory(&virtual_path(&dir, "branches"), None)
        .await
        .expect("list branches")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert!(branches.contains(&"main".to_string()), "{branches:?}");
    assert!(branches.contains(&"feature/foo".to_string()), "{branches:?}");

    let run_sh = volume
        .get_metadata(&virtual_path(&dir, "branches/main/scripts/run.sh"))
        .await
        .expect("stat a blob inside the snapshot");
    assert_eq!(run_sh.permissions & 0o111, 0o111, "the executable bit survives");
    assert!(!run_sh.is_directory);
    assert!(
        volume
            .is_directory(&virtual_path(&dir, "branches/main/scripts"))
            .await
            .expect("stat the tree"),
    );

    cleanup(&dir);
}

/// A whole branch tree can be scanned for a copy, so pasting one into another
/// volume has real totals to plan with rather than a refusal.
#[tokio::test]
async fn a_snapshot_subtree_scans_for_a_copy() {
    let (dir, volume) = portal_over_a_repo("copy_scan");

    let scanned = volume
        .scan_for_copy(&virtual_path(&dir, "branches/main"))
        .await
        .expect("scan a branch tree");

    // `README.md` and `scripts/run.sh`, under the snapshot root and `scripts/`
    // (the shared walk counts the source directory itself).
    assert_eq!(scanned.file_count, 2, "{scanned:?}");
    assert_eq!(scanned.dir_count, 2, "{scanned:?}");
    assert!(scanned.total_bytes > 0, "{scanned:?}");

    cleanup(&dir);
}

/// Every mutation refuses, including the recursive create whose trait default
/// would answer `Ok` for a directory that's already there.
#[tokio::test]
async fn every_mutation_is_unsupported() {
    let (dir, volume) = portal_over_a_repo("mutations");
    let existing = virtual_path(&dir, "branches");
    let blob = virtual_path(&dir, "branches/main/README.md");

    assert!(matches!(
        volume.create_file(&blob, b"x").await,
        Err(VolumeError::NotSupported)
    ));
    assert!(matches!(
        volume.create_directory(&existing).await,
        Err(VolumeError::NotSupported)
    ));
    assert!(
        matches!(
            volume.create_directory_all(&existing).await,
            Err(VolumeError::NotSupported)
        ),
        "the trait default would answer {:?} for a directory that exists",
        DirectoryCreation::AlreadyExisted
    );
    assert!(matches!(volume.delete(&blob).await, Err(VolumeError::NotSupported)));
    assert!(matches!(
        volume
            .rename(&blob, &virtual_path(&dir, "branches/main/other.md"), false)
            .await,
        Err(VolumeError::NotSupported)
    ));

    cleanup(&dir);
}

/// The punchline the whole routing design exists for: a copy OUT of a virtual
/// `.git/branches/main/…` path into another volume. The bytes must match
/// `git show` exactly, and the entry must carry the executable bit the copy
/// engine sets on the destination.
#[tokio::test]
async fn a_copy_out_of_a_snapshot_carries_the_bytes_and_the_executable_bit() {
    use std::ops::ControlFlow;
    use std::sync::atomic::{AtomicU64, Ordering};

    let (dir, src) = portal_over_a_repo("copy_out");
    // The destination is any other volume, which is the whole point: the copy
    // engine moves bytes between two `Volume`s and neither end is special.
    let dst = InMemoryVolume::new("dst");

    let src_path = virtual_path(&dir, "branches/main/scripts/run.sh");
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
    let expected = git_cli_capture(&dir, &["show", "main:scripts/run.sh"]);
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

    cleanup(&dir);
}

/// A path that simply isn't in the snapshot reads as "not there", ❌ never as
/// "this repo is damaged". `gix`'s tree walk answers `Ok(None)` for a name it
/// couldn't find, which is an ordinary miss (a typo in the path bar, a file
/// that only exists on another branch) and has to stay distinct from the `Err`
/// that means the object database couldn't answer at all. Every entry point
/// gives the same answer.
#[tokio::test]
async fn a_path_missing_from_a_snapshot_reads_as_not_found() {
    let (dir, volume) = portal_over_a_repo("missing_paths");

    for missing in [
        "branches/main/no-such-file.txt",
        "branches/main/no-such-dir/inner.txt",
        "branches/no-such-branch",
        "tags/no-such-tag/README.md",
    ] {
        let path = virtual_path(&dir, missing);
        let answered = volume.get_metadata(&path).await;
        assert!(
            matches!(answered, Err(VolumeError::NotFound(ref carried)) if carried.contains("no-such")),
            "{missing}: {answered:?}",
        );
    }

    // Listing one is the same answer.
    let listed = volume
        .list_directory(&virtual_path(&dir, "branches/main/no-such-dir"), None)
        .await;
    assert!(matches!(listed, Err(VolumeError::NotFound(_))), "{listed:?}");

    // And so is opening one for read.
    let opened = volume
        .open_read_stream(&virtual_path(&dir, "branches/main/no-such-file.txt"))
        .await;
    assert!(
        matches!(opened, Err(VolumeError::NotFound(_))),
        "opening a missing blob"
    );

    cleanup(&dir);
}

/// The capability answers a routed read-only volume owes, in one place: no
/// watch (nothing on disk to arm one on), no local-FS access (a blob lives in a
/// pack file), no space poll, and the parent's lane so portal reads can't run
/// beside other work on the same disk.
#[tokio::test]
async fn the_capability_answers_match_a_routed_read_only_volume() {
    let (dir, volume) = portal_over_a_repo("capabilities");
    let parent = InMemoryVolume::new("Parent");

    assert!(!volume.can_watch_listings());
    assert_eq!(
        volume.listing_watch_coverage(&virtual_path(&dir, "branches")),
        cmdr_fs::volume::WatchCoverage::None
    );
    assert!(!volume.supports_local_fs_access());
    assert_eq!(volume.local_path(), None);
    assert_eq!(volume.space_poll_interval(), None);
    assert_eq!(volume.lane_key(), parent.lane_key());
    assert!(volume.capabilities().can_export);
    assert!(!volume.capabilities().backend_can_write);

    cleanup(&dir);
}
