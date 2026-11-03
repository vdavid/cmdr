//! A repo whose disk is force-detached while its pack is mapped: the read that follows faults,
//! the page comes back as zeros, and the read answers `VolumeGone` instead of the process dying.
//!
//! This is `CRASH-33865`'s shape: a status walk decoding a mapped pack half a second after a USB
//! volume unmounted. The handle is opened and its pack mapped before the detach, which is where
//! a walk in flight stands when the disk goes.
//!
//! `#[ignore]`d: it attaches an HFS+ image. Hand-run with
//! `cargo nextest run -p cmdr-git --run-ignored only -E 'test(fault_guard_real_image::)'`, or
//! through `pnpm check disk-images`.

use cmdr_fs::mapped_file_faults;
use cmdr_fs::testing::disk_images::{DiskImage, DiskImageSession, ImageSpec};
use cmdr_fs::volume::git_error::FriendlyGitErrorKind;

use std::time::{Duration, Instant};

use cmdr_fs::volume::host::VolumeHost;

use crate::fault_guard::FaultWatch;
use crate::repo::RepoCache;
use crate::test_fixtures::{Fixture, git_cli};
use crate::{GitPortal, no_git_state_sink};

/// Big enough that most of the pack isn't resident when the disk goes: 8 MiB of bytes zlib can't
/// shrink.
const BLOB_BYTES: usize = 8 * 1024 * 1024;

/// Bytes no compressor shrinks, without a `rand` dependency: xorshift64.
fn incompressible(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn reading_a_packed_blob_after_its_disk_was_force_detached_answers_volume_gone() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    let dir = image.volumes()[0].mount_point.join("repo");
    std::fs::create_dir(&dir).expect("make the repo dir");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    fixture.commit_file("big.bin", &incompressible(BLOB_BYTES), "the big one");
    git_cli(&dir, &["repack", "-a", "-d", "-q"]);
    drop(fixture);

    let cache = RepoCache::new();
    let (handle, root) = cache.discover(&dir).expect("open the repo on the image");
    let blob_id = {
        let repo = handle.to_thread_local();
        // Reading HEAD's commit and tree maps the pack, as a walk in flight would have.
        let tree = repo.head_commit().expect("HEAD").tree().expect("its tree");
        tree.find_entry("big.bin").expect("the big blob").object_id()
    };
    mapped_file_faults::install_test_handler();

    image.force_detach().expect("force-detach the image");

    let watch = FaultWatch::start();
    let read = crate::tree::read_blob(&handle, blob_id);
    assert!(
        watch.tripped(),
        "the pack's pages were a dead vnode's and came back as zeros (the read answered {:?} bytes)",
        read.as_ref().map(Vec::len)
    );
    let refused = watch
        .vouch(&root, read)
        .expect_err("a blob read across a recovery is never handed out");
    assert_eq!(refused.kind, FriendlyGitErrorKind::VolumeGone);

    drop(handle);
    assert!(
        cache.discover(&dir).is_err(),
        "the repo isn't there any more, and the dead handle isn't handed back in its place"
    );
    assert_eq!(cache.len(), 0, "the recovery emptied the cache");
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn an_open_repo_lets_its_volume_unmount_once_released() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    let mount_point = image.volumes()[0].mount_point.clone();
    let dir = mount_point.join("repo");
    std::fs::create_dir(&dir).expect("make the repo dir");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    fixture.commit_file("big.bin", &incompressible(BLOB_BYTES), "the big one");
    git_cli(&dir, &["repack", "-a", "-d", "-q"]);
    drop(fixture);
    let portal = GitPortal::with_scripted_watcher(VolumeHost::detached(), no_git_state_sink());
    {
        // Reading HEAD's tree maps the pack; the cached handle keeps it mapped.
        let (handle, _) = portal.discover(&dir).expect("open the repo on the image");
        let repo = handle.to_thread_local();
        let _tree = repo.head_commit().expect("HEAD").tree().expect("its tree");
    }

    assert!(
        image.unmount_volume(0).is_err(),
        "a plain unmount fails while the pack is mapped, which is the eject Cmdr used to block"
    );
    let mount_root = mount_point.canonicalize().expect("still mounted");
    assert!(portal.release_volume(&mount_root, Instant::now() + Duration::from_secs(5)));

    image
        .unmount_volume(0)
        .expect("once the repo let go, the same plain unmount goes through");
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_repo_off_the_startup_disk_reads_its_packed_refs_into_memory() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    let dir = image.volumes()[0].mount_point.join("repo");
    std::fs::create_dir(&dir).expect("make the repo dir");
    Fixture::init(dir.clone()).commit_file("README.md", b"hello\n", "initial");

    let (handle, _) = RepoCache::new().discover(&dir).expect("open the repo on the image");

    assert_eq!(
        crate::repo_tests::packed_refs_mmap_threshold(&handle),
        u64::MAX,
        "a packed-refs mapping on a disk that can go away is one more page to fault on"
    );
}
