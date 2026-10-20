//! Hidden and Locked through the app's whole copy pipeline on a real share
//! (require Docker SMB containers): onto the share and off it, files (one frame
//! and streamed) and folders, and a rollback that takes back a Locked copy
//! though the share refuses a plain delete of it.
//!
//! The backend's own half (what `HIDDEN` and `READONLY` do on the wire, and
//! Samba's dotfile rule) is `cmdr-smb`'s `attributes_integration_test.rs`.
//! Every test is `#[ignore]`d; start the containers with
//! `./apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`. macOS only: the
//! local half needs `UF_HIDDEN` and `uchg`.

use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses, UserFlags};
use cmdr_fs::testing::metadata_fixture::{HIDDEN_FLAG, LOCKED_FLAG, MetadataFixture, flags, set_flags};

use super::network_transfer_test_support::{clean_deep, run_copy, run_copy_on, self_describing_bytes};
use super::smb_test_support::*;
use crate::file_system::VolumeManager;
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::rollback::{execute_rollback, rollback_operation};
use cmdr_operation_log::types::Initiator;
use cmdr_operations::Operations;
use cmdr_operations::testing::{Reversal, TestOperationLog};

/// Big enough for the streaming writer, not the one-frame write.
const STREAMED_BYTES: usize = 10 * 1024 * 1024;

/// What the share says `path` wears: a file's flags off its read stream (the
/// copy's own read), a folder's off `folder_metadata`.
async fn flags_on_share(remote: &dyn Volume, path: &Path) -> UserFlags {
    if remote.is_directory(path).await.unwrap_or(false) {
        return remote.folder_metadata(path, MetadataClasses::FLAGS).await.flags;
    }
    remote
        .open_read_stream(path)
        .await
        .unwrap_or_else(|e| panic!("open {}: {e:?}", path.display()))
        .metadata(MetadataClasses::FLAGS)
        .flags
}

/// Takes `dir` off the share, unlocking every item first: a Read-only one
/// refuses a plain delete.
async fn unlock_and_clean(remote: &dyn Volume, dir: &Path) {
    let mut pending = vec![dir.to_path_buf()];
    while let Some(path) = pending.pop() {
        let _ = remote.clear_blockers(&path).await;
        if let Ok(entries) = remote.list_directory(&path, None).await {
            pending.extend(entries.into_iter().map(|entry| path.join(entry.name)));
        }
    }
    clean_deep(remote, dir).await;
}

/// A local tree carrying Hidden and Locked: a Hidden file, a Hidden and
/// Locked one, a big Locked one (the streaming writer), and a Locked, Hidden
/// folder with a plain file inside.
fn flagged_local_tree(label: &str) -> MetadataFixture {
    let fixture = MetadataFixture::new(label);
    std::fs::write(fixture.join("hidden.txt"), b"hidden").unwrap();
    std::fs::write(fixture.join("locked.txt"), b"locked").unwrap();
    std::fs::write(
        fixture.join("big.bin"),
        self_describing_bytes(STREAMED_BYTES, "big.bin"),
    )
    .unwrap();
    std::fs::create_dir(fixture.join("album")).unwrap();
    std::fs::write(fixture.join("album/inside.txt"), b"inside").unwrap();
    set_flags(&fixture.join("hidden.txt"), HIDDEN_FLAG);
    set_flags(&fixture.join("locked.txt"), HIDDEN_FLAG | LOCKED_FLAG);
    set_flags(&fixture.join("big.bin"), LOCKED_FLAG);
    set_flags(&fixture.join("album"), HIDDEN_FLAG | LOCKED_FLAG);
    fixture
}

const COPIED: [&str; 4] = ["hidden.txt", "locked.txt", "big.bin", "album"];

/// Files and a folder copied onto the share keep Hidden and Locked, as
/// `HIDDEN` and `READONLY`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_copy_onto_a_share_keeps_hidden_and_locked() {
    let (remote, dir) = fixture().await;
    let tree = flagged_local_tree("smb_flags_onto_share");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "Local",
        &*tree,
        crate::volume_host::host(),
    ));

    run_copy(
        "flags-onto-share",
        local,
        COPIED.iter().map(PathBuf::from).collect(),
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;

    let both = UserFlags::HIDDEN | UserFlags::LOCKED;
    for (name, expected) in [
        ("hidden.txt", UserFlags::HIDDEN),
        ("locked.txt", both),
        ("big.bin", UserFlags::LOCKED),
        ("album", both),
        ("album/inside.txt", UserFlags::NONE),
    ] {
        assert_eq!(
            flags_on_share(remote.as_ref(), &dir.join(name)).await,
            expected,
            "{name} on the share"
        );
    }

    unlock_and_clean(remote.as_ref(), &dir).await;
}

/// Files and a folder copied off the share land Hidden and Locked on local
/// disk, and a dotfile, which the share marks Hidden for its name, lands
/// without the flag.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_copy_off_a_share_keeps_hidden_and_locked() {
    let (remote, dir) = fixture().await;
    remote.create_file(&dir.join("hidden.txt"), b"hidden").await.unwrap();
    remote.create_file(&dir.join("locked.txt"), b"locked").await.unwrap();
    remote
        .create_file(&dir.join("big.bin"), &self_describing_bytes(STREAMED_BYTES, "big.bin"))
        .await
        .unwrap();
    remote.create_file(&dir.join(".dotfile"), b"dot").await.unwrap();
    remote.create_directory(&dir.join("album")).await.unwrap();
    remote
        .create_file(&dir.join("album/inside.txt"), b"inside")
        .await
        .unwrap();
    let both = UserFlags::HIDDEN | UserFlags::LOCKED;
    for (name, set) in [
        ("hidden.txt", UserFlags::HIDDEN),
        ("locked.txt", both),
        ("big.bin", UserFlags::LOCKED),
        ("album", both),
    ] {
        let carried = CarriedMetadata {
            flags: set,
            ..CarriedMetadata::default()
        };
        let report = remote.apply_metadata(&dir.join(name), &carried, None).await;
        assert_eq!(
            report.flags,
            cmdr_fs::metadata::ClassOutcome::Kept,
            "fixture: flag {name} on the share"
        );
    }
    let landed = MetadataFixture::new("smb_flags_off_share");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "Local",
        &*landed,
        crate::volume_host::host(),
    ));

    run_copy(
        "flags-off-share",
        Arc::clone(&remote),
        ["hidden.txt", "locked.txt", "big.bin", ".dotfile", "album"]
            .iter()
            .map(|name| dir.join(name))
            .collect(),
        local,
        PathBuf::from(""),
    )
    .await;

    let wears = |name: &str| flags(&landed.join(name)) & (HIDDEN_FLAG | LOCKED_FLAG);
    assert_eq!(wears("hidden.txt"), HIDDEN_FLAG);
    assert_eq!(wears("locked.txt"), HIDDEN_FLAG | LOCKED_FLAG);
    assert_eq!(wears("big.bin"), LOCKED_FLAG);
    assert_eq!(wears("album"), HIDDEN_FLAG | LOCKED_FLAG, "a folder keeps its flags");
    assert_eq!(wears("album/inside.txt"), 0);
    assert_eq!(wears(".dotfile"), 0, "a dotfile is hidden by its name, not a flag");

    unlock_and_clean(remote.as_ref(), &dir).await;
}

/// The journal a rollback reads, and an engine of the cell's own (the app's
/// answers) journaling into it.
fn journaled() -> (Operations, TestOperationLog) {
    let log = TestOperationLog::new("smb-flags-rollback-journal");
    (crate::operations_host::test_engine_journaling_into(&log), log)
}

/// Invariant 9 on a share: a finished copy's rollback takes back its Locked copies
/// and its Locked folder, though the share refuses a plain delete of each
/// (`STATUS_CANNOT_DELETE`). The reversal verifies each as its own, clears
/// `READONLY`, and deletes it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_rollback_removes_a_locked_copy_on_the_share() {
    let (engine, journal) = journaled();
    let (remote, dir) = fixture().await;
    // The journal records what it's handed, and a rollback resolves it against
    // the volume's root: the path the transfer dialog would hand in.
    let dir = remote.root().join(dir);
    let tree = flagged_local_tree("smb_flags_rollback");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "Local",
        &*tree,
        crate::volume_host::host(),
    ));
    let vm = VolumeManager::new();
    vm.register("smb-flags-rollback-source", Arc::clone(&local));
    vm.register("smb-flags-rollback-dest", Arc::clone(&remote));

    let running = run_copy_on(
        &engine,
        "smb-flags-rollback",
        local,
        COPIED.iter().map(PathBuf::from).collect(),
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;
    assert!(
        matches!(
            remote.delete(&dir.join("locked.txt")).await,
            Err(VolumeError::PermissionDenied { .. })
        ),
        "fixture precondition: the share refuses a plain delete of the Locked copy"
    );

    let writer = journal.writer();
    writer.flush_blocking().expect("the journal flushes");
    let plan = rollback_operation(&vm, writer, running.operation_id.as_str(), |_plan| Ok(()))
        .unwrap_or_else(|refusal| panic!("the finished copy must be reversible, got {refusal:?}"));
    let reversal = Reversal::on(&engine, "smb-flags-rollback");
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
    assert!(
        left.is_empty(),
        "the rollback takes back every copy, Locked ones too: {left:?}"
    );

    unlock_and_clean(remote.as_ref(), &dir).await;
}
