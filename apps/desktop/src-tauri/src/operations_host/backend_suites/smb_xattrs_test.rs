//! Finder tags, comments, FinderInfo, and resource forks through the app's whole
//! copy pipeline on a real share (require Docker SMB containers): local → share
//! → local on `smb-consumer-fruit` (named streams, `vfs_fruit`), files (one
//! frame and streamed) and a folder, byte for byte; and onto the plain guest
//! share, which has no streams, where the copy lands and the loss note names the
//! tags; and a rollback of tagged copies on the share.
//!
//! The backend's own half (the wire, the caps, a refusal mid-copy) is
//! `cmdr-smb`'s `copy_xattrs_integration_test.rs`. Every test is `#[ignore]`d;
//! start the containers with `./apps/desktop/test/smb-servers/start.sh`, then
//! run `cargo nextest run smb_integration --run-ignored all`. macOS only: the
//! local half writes `com.apple.*` xattrs.

use cmdr_fs::ignore_poison::IgnorePoison as _;
use cmdr_fs::volume::XattrReadMode;
use cmdr_operations::testing::{LostMetadata, LostMetadataClass, MetadataLeftBehind};

use super::network_transfer_test_support::{RunningCopy, clean_deep, run_copy, run_copy_on, self_describing_bytes};
use super::smb_test_support::*;
use crate::file_system::VolumeManager;
use crate::test_support::TestDir;
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::rollback::{execute_rollback, rollback_operation};
use cmdr_operation_log::types::Initiator;
use cmdr_operations::testing::{Reversal, TestOperationLog};

/// Big enough for the streaming writer and the streamed read, not one frame.
const STREAMED_BYTES: usize = 4 * 1024 * 1024;

/// What a tagged, commented item with a custom icon carries, the way Finder
/// leaves them, and for a file a resource fork too (APFS refuses a folder one).
/// Small: the fixture's file system keeps all of a file's xattrs in one 4 KiB
/// block.
fn finder_xattrs(path: &Path) -> Vec<(&'static str, Vec<u8>)> {
    let mut finder_info = vec![0u8; 32];
    finder_info[..8].copy_from_slice(b"TEXTttxt");
    finder_info[8] = 0x04; // kHasCustomIcon
    let mut xattrs = vec![
        ("com.apple.metadata:_kMDItemUserTags", b"bplist00 red tag".to_vec()),
        (
            "com.apple.metadata:kMDItemFinderComment",
            b"bplist00 a comment".to_vec(),
        ),
        ("com.apple.FinderInfo", finder_info),
    ];
    if path.extension().is_some() {
        xattrs.push(("com.apple.ResourceFork", vec![0x5A; 1_000]));
    }
    xattrs
}

fn decorate(path: &Path) {
    for (name, value) in finder_xattrs(path) {
        xattr::set(path, name, &value).unwrap_or_else(|e| panic!("set {name} on {}: {e}", path.display()));
    }
}

/// A local tree of a small file, a streamed one, and a folder, each carrying
/// [`finder_xattrs`].
fn decorated_tree(label: &str) -> TestDir {
    let dir = TestDir::new(label);
    std::fs::write(dir.join("small.txt"), b"small").unwrap();
    std::fs::write(dir.join("big.bin"), self_describing_bytes(STREAMED_BYTES, "big.bin")).unwrap();
    std::fs::create_dir(dir.join("album")).unwrap();
    for name in ["small.txt", "big.bin", "album"] {
        decorate(&dir.join(name));
    }
    dir
}

const COPIED: [&str; 3] = ["small.txt", "big.bin", "album"];

fn local_volume(dir: &TestDir) -> Arc<dyn Volume> {
    Arc::new(LocalPosixVolume::local_folder(
        "Local",
        &**dir,
        crate::volume_host::host(),
    ))
}

fn left_behind(copy: &RunningCopy) -> Option<MetadataLeftBehind> {
    copy.events.complete.lock_ignore_poison()[0]
        .metadata_left_behind
        .clone()
}

async fn fruit_fixture() -> (Arc<dyn Volume>, PathBuf) {
    let volume = make_docker_volume_through(smb2::testing::fruit_port(), crate::volume_host::host()).await;
    let base = test_dir_name();
    ensure_clean(&volume, &base).await;
    volume.create_directory(Path::new(&base)).await.unwrap();
    (Arc::new(volume), PathBuf::from(base))
}

/// The issue's whole round trip: local → share → local keeps every xattr byte
/// for byte, on a one-frame file, a streamed one, and a folder, and neither copy
/// says anything was left behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_finder_metadata_survives_local_to_share_to_local() {
    let (remote, dir) = fruit_fixture().await;
    let source = decorated_tree("smb_xattrs_source");
    let onto = run_copy(
        "xattrs-onto-share",
        local_volume(&source),
        COPIED.iter().map(PathBuf::from).collect(),
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;
    assert_eq!(left_behind(&onto), None, "a share with streams keeps them all");

    for name in COPIED {
        for (xattr, value) in finder_xattrs(Path::new(name)) {
            let read = remote
                .read_xattr_many(&[dir.join(name)], xattr, XattrReadMode::Verified)
                .await
                .pop()
                .expect("one answer")
                .unwrap_or_else(|e| panic!("{xattr} on {name}: {e:?}"));
            assert_eq!(read, Some(value), "{xattr} on {name} on the share");
        }
    }

    let back = TestDir::new("smb_xattrs_back");
    let off = run_copy(
        "xattrs-off-share",
        Arc::clone(&remote),
        COPIED.iter().map(|name| dir.join(name)).collect(),
        local_volume(&back),
        PathBuf::from(""),
    )
    .await;
    assert_eq!(left_behind(&off), None, "local disk keeps them all");
    for name in COPIED {
        for (xattr, value) in finder_xattrs(Path::new(name)) {
            assert_eq!(
                xattr::get(back.join(name), xattr).unwrap_or_else(|e| panic!("{xattr} on {name}: {e}")),
                Some(value),
                "{xattr} on {name} back on local disk"
            );
        }
    }
    assert_eq!(
        std::fs::read(back.join("big.bin")).unwrap(),
        self_describing_bytes(STREAMED_BYTES, "big.bin")
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A share with no streams: the copy lands, and the note names the xattrs it
/// left behind, ❌ never an error.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_share_without_streams_lands_the_copy_and_names_the_tags_left_behind() {
    let (remote, dir) = fixture().await;
    let source = TestDir::new("smb_xattrs_no_streams");
    std::fs::write(source.join("tagged.txt"), b"tagged").unwrap();
    decorate(&source.join("tagged.txt"));

    let copy = run_copy(
        "xattrs-no-streams",
        local_volume(&source),
        vec![PathBuf::from("tagged.txt")],
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;
    let left = left_behind(&copy).expect("the note says something stayed behind");
    assert!(
        left.classes.contains(&LostMetadata {
            class: LostMetadataClass::ExtendedAttributes,
            item_count: 1
        }),
        "{left:?}"
    );
    assert!(remote.exists(&dir.join("tagged.txt")).await);

    clean_deep(remote.as_ref(), &dir).await;
}

/// Invariant 9 with streams on: a finished copy's rollback takes back every
/// tagged file and folder it made on the share, and the folder empties, since a
/// delete takes a file's streams with it on `streams_xattr`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_rollback_takes_back_tagged_copies_on_the_share() {
    let log = TestOperationLog::new("smb-xattrs-rollback-journal");
    let engine = crate::operations_host::test_engine_journaling_into(&log);
    let (remote, dir) = fruit_fixture().await;
    let dir = remote.root().join(dir);
    let source = decorated_tree("smb_xattrs_rollback");
    let local = local_volume(&source);
    let vm = VolumeManager::new();
    vm.register("smb-xattrs-rollback-source", Arc::clone(&local));
    vm.register("smb-xattrs-rollback-dest", Arc::clone(&remote));

    let running = run_copy_on(
        &engine,
        "smb-xattrs-rollback",
        local,
        COPIED.iter().map(PathBuf::from).collect(),
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;

    let writer = log.writer();
    writer.flush_blocking().expect("the journal flushes");
    let plan = rollback_operation(&vm, writer, running.operation_id.as_str(), |_plan| Ok(()))
        .unwrap_or_else(|refusal| panic!("the finished copy must be reversible, got {refusal:?}"));
    let reversal = Reversal::on(&engine, "smb-xattrs-rollback");
    let report = execute_rollback(
        &vm,
        writer,
        &plan.original,
        &plan.inverse_op_id,
        Initiator::User,
        reversal.runner(),
    )
    .await;

    assert_eq!(report.skipped, 0, "every item it copied is its to remove: {report:?}");
    let left: Vec<String> = remote
        .list_directory(&dir, None)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert!(left.is_empty(), "the rollback takes back every tagged copy: {left:?}");

    clean_deep(remote.as_ref(), &dir).await;
}
