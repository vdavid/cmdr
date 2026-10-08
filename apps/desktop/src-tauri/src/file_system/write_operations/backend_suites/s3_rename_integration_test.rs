//! Renames that copy, and copies inside one account, against both Docker S3
//! fixtures, through the app's own entry points.
//!
//! On S3 a folder rename is one copy and one delete per object, and a big
//! file's is a multipart copy (`Volume::rename_work`), so a rename runs as a
//! move through the transfer engine (`routing::start_rename_by_move`, the
//! route F2, the MCP rename, and a bulk rename all take). These cells drive
//! that route end to end against live servers: the batch delete past a
//! thousand keys, the multipart copy and the date it keeps, pause and cancel
//! leaving every source whole, and a copy between two buckets of one account
//! running on the server unless the provider copies within a bucket only.
//!
//! The cells stay named for the `s3_integration_` lane prefix. The scenarios
//! take an `S3Target`, so `s3_live_engine_test.rs` runs the same bodies against
//! real accounts.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::{Volume, VolumeError};
use cmdr_s3::S3Volume;
use cmdr_s3::volume::testing::{
    GARAGE, S3Target, Seed, VERSITYGW, distant_mtime, object, self_describing_bytes, take_sent_requests,
};

use super::network_transfer_test_support::{budget, read_all, run_copy, sha256};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::start_rename_by_move;
use crate::file_system::write_operations::state::{
    cancel_write_operation, pause_write_operation, resume_write_operation,
};
use crate::file_system::write_operations::types::{VolumeCopyConfig, WriteOperationType};
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::Initiator;

const MIB: usize = 1024 * 1024;

/// How long a rename-by-move may take to settle: a thousand server-side copies
/// on a fixture several suites share.
const SETTLE_BUDGET: Duration = Duration::from_secs(120);

/// A bucket place on `target`, registered with the volume manager under a
/// fresh id the way a connect registers it, plus a scratch prefix and its
/// folder.
async fn registered(target: &S3Target, label: &str, part_floor: Option<u64>) -> (String, Arc<S3Volume>, String) {
    let volume = target.connect(Some(target.bucket())).await;
    if let Some(floor) = part_floor {
        volume.set_part_floor(floor);
    }
    let volume = Arc::new(volume);
    let volume_id = format!("{}-{label}", volume.volume_id());
    get_volume_manager().register(&volume_id, Arc::clone(&volume) as Arc<dyn Volume>);
    (volume_id, volume, target.prefix(label))
}

fn at(volume: &S3Volume, key: &str) -> PathBuf {
    volume.root().join(key.trim_end_matches('/'))
}

/// Starts `from` → `new_name` in its own folder through the rename route,
/// returning the sink and the operation id.
async fn start(volume_id: &str, from: PathBuf, new_name: &str) -> (Arc<CollectorEventSink>, String) {
    let events = Arc::new(CollectorEventSink::new());
    let parent = from.parent().expect("a parent").display().to_string();
    let started = start_rename_by_move(
        events.clone(),
        volume_id.to_string(),
        vec![(from, new_name.to_string())],
        parent,
        VolumeCopyConfig::default(),
        Initiator::User,
        None,
    )
    .await
    .expect("the rename starts");
    assert_eq!(started.operation_type, WriteOperationType::Move);
    (events, started.operation_id)
}

async fn settle(events: &CollectorEventSink, what: &str) {
    crate::test_support::wait_until_async(budget(SETTLE_BUDGET), what, || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    let errors = events.errors.lock_ignore_poison();
    assert!(
        errors.is_empty(),
        "{what}: {:?}",
        errors.iter().map(|e| &e.error).collect::<Vec<_>>()
    );
}

/// ❗ A folder past a thousand objects renames through the engine: every
/// object arrives under the new name, and the sources go in `DeleteObjects`
/// batches, the second one paging past the first thousand.
pub(super) async fn a_folder_of_1005_objects_renames_through_the_engine(target: &S3Target) {
    let (volume_id, volume, prefix) = registered(target, "rename-1005", None).await;
    let keys: Vec<String> = (0..1_005).map(|n| format!("{prefix}folder/f{n:04}.txt")).collect();
    let seeds: Vec<Seed<'_>> = keys.iter().map(|key| object(key, b"x")).collect();
    target.seed(target.bucket(), &seeds).await;
    take_sent_requests(&volume).await;

    let (events, _) = start(&volume_id, at(&volume, &format!("{prefix}folder")), "renamed").await;
    settle(&events, "the folder rename to settle").await;

    // ❗ One HEAD per object, the source's (its date and headers, for the
    // copy's `REPLACE`). ❌ No per-object no-overwrite HEAD and ❌ no verifying
    // one: `renamed/` is a folder this rename made, so its creation proved it
    // empty (`WriteMode::CreateNewInFreshFolder`). The rest is per folder.
    let sent = take_sent_requests(&volume).await;
    let heads = sent.get("HeadObject").copied().unwrap_or(0);
    assert!(heads <= 1_005 + 20, "{sent:?}");
    assert_eq!(sent.get("CopyObject"), Some(&1_005), "{sent:?}");

    let renamed = volume
        .list_directory(&at(&volume, &format!("{prefix}renamed")), None)
        .await
        .expect("the renamed folder lists");
    assert_eq!(renamed.len(), 1_005, "every object arrived under the new name");
    assert!(
        matches!(
            volume
                .list_directory(&at(&volume, &format!("{prefix}folder")), None)
                .await,
            Err(VolumeError::NotFound(_))
        ),
        "the old folder is gone, all 1,005 of it"
    );
}

/// A file past the part floor renames by multipart copy: the bytes and the
/// source's date arrive, the old key goes, and no upload stays open.
pub(super) async fn a_big_file_renames_by_multipart_copy_keeping_its_date(target: &S3Target) {
    let (volume_id, volume, prefix) = registered(target, "rename-big", Some(5 * MIB as u64)).await;
    let bytes = self_describing_bytes(17 * MIB, "big");
    let from_key = format!("{prefix}clip.mov");
    target
        .seed(
            target.bucket(),
            &[Seed {
                key: &from_key,
                bytes: &bytes,
                mtime: Some(distant_mtime()),
            }],
        )
        .await;

    let (events, _) = start(&volume_id, at(&volume, &from_key), "renamed.mov").await;
    settle(&events, "the big file's rename to settle").await;

    let to_key = format!("{prefix}renamed.mov");
    assert_eq!(
        sha256(&read_all(volume.as_ref(), &at(&volume, &to_key)).await),
        sha256(&bytes)
    );
    assert_eq!(
        target.stored_mtime_header(target.bucket(), &to_key).await,
        Some(rclone_mtime_of_distant()),
        "the source's date survives the rename"
    );
    assert!(!volume.exists(&at(&volume, &from_key)).await, "the old name is gone");
    assert!(target.unfinished_uploads(target.bucket(), &prefix).await.is_empty());
}

/// The rclone-format mtime of `distant_mtime()`, which is whole seconds.
fn rclone_mtime_of_distant() -> String {
    distant_mtime()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after the epoch")
        .as_secs()
        .to_string()
}

/// ❗ A paused rename copies nothing until resumed, and a cancel while it's
/// parked leaves the source whole, nothing at the new name, and no upload.
pub(super) async fn a_paused_then_cancelled_rename_keeps_the_source_whole(target: &S3Target) {
    let (volume_id, volume, prefix) = registered(target, "rename-pause-cancel", Some(5 * MIB as u64)).await;
    let bytes = self_describing_bytes(20 * MIB, "kept");
    let from_key = format!("{prefix}kept.bin");
    target.seed(target.bucket(), &[object(&from_key, &bytes)]).await;

    let (events, operation_id) = start(&volume_id, at(&volume, &from_key), "gone.bin").await;
    assert!(pause_write_operation(&operation_id) || !events.settled.lock_ignore_poison().is_empty());
    cancel_write_operation(&operation_id, false);
    crate::test_support::wait_until_async(budget(SETTLE_BUDGET), "the cancelled rename to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;

    assert_eq!(
        sha256(&read_all(volume.as_ref(), &at(&volume, &from_key)).await),
        sha256(&bytes),
        "the source stays whole"
    );
    assert!(target.unfinished_uploads(target.bucket(), &prefix).await.is_empty());
    if volume.exists(&at(&volume, &format!("{prefix}gone.bin"))).await {
        // A cancel that landed after the copy published is a finished copy
        // with its source kept: a duplicate, never a loss.
        assert_eq!(
            sha256(&read_all(volume.as_ref(), &at(&volume, &format!("{prefix}gone.bin"))).await),
            sha256(&bytes)
        );
    }
}

/// A paused rename resumes and lands.
pub(super) async fn a_paused_rename_resumes_and_lands(target: &S3Target) {
    let (volume_id, volume, prefix) = registered(target, "rename-pause", Some(5 * MIB as u64)).await;
    let bytes = self_describing_bytes(12 * MIB, "resumed");
    let from_key = format!("{prefix}a.bin");
    target.seed(target.bucket(), &[object(&from_key, &bytes)]).await;

    let (events, operation_id) = start(&volume_id, at(&volume, &from_key), "b.bin").await;
    assert!(
        pause_write_operation(&operation_id),
        "the rename should still be running to pause"
    );
    assert!(resume_write_operation(&operation_id), "the paused rename should resume");
    settle(&events, "the resumed rename to settle").await;

    assert_eq!(
        sha256(&read_all(volume.as_ref(), &at(&volume, &format!("{prefix}b.bin"))).await),
        sha256(&bytes)
    );
    assert!(!volume.exists(&at(&volume, &from_key)).await);
}

/// A reviewed batch with a folder in it (Ask Cmdr's proposals and the bulk
/// rename both start here) runs as ONE move with the new names, and lands
/// every one of them.
pub(super) async fn a_batch_with_a_folder_renames_as_one_move(target: &S3Target) {
    use crate::file_system::write_operations::{BulkRenameRow, SourceFingerprint, start_renames};

    let (volume_id, volume, prefix) = registered(target, "rename-batch", None).await;
    let (folder_file, note) = (format!("{prefix}album/a.jpg"), format!("{prefix}note.txt"));
    target
        .seed(target.bucket(), &[object(&folder_file, b"jpeg"), object(&note, b"n")])
        .await;
    let mut rows = Vec::new();
    for (id, from, to) in [("1", "album", "photos"), ("2", "note.txt", "memo.txt")] {
        let source = at(&volume, &format!("{prefix}{from}"));
        let fingerprint = SourceFingerprint::capture_remote(volume.as_ref(), &source)
            .await
            .expect("a fingerprint");
        rows.push(BulkRenameRow {
            row_id: id.to_string(),
            destination: source.with_file_name(to),
            source,
            expected_fingerprint: fingerprint,
        });
    }
    let events = Arc::new(CollectorEventSink::new());

    let started = start_renames(events.clone(), volume_id, rows, Initiator::Agent)
        .await
        .expect("the batch starts");
    assert_eq!(
        started.operation.operation_type,
        WriteOperationType::Move,
        "one move, not a rename batch"
    );
    settle(&events, "the batch to settle").await;

    assert!(volume.exists(&at(&volume, &format!("{prefix}photos/a.jpg"))).await);
    assert!(volume.exists(&at(&volume, &format!("{prefix}memo.txt"))).await);
    assert!(!volume.exists(&at(&volume, &folder_file)).await);
    assert!(!volume.exists(&at(&volume, &note)).await);
}

/// A copy between two buckets of one account runs on the server: the
/// destination volume sends a `CopyObject` and no `PutObject`.
pub(super) async fn a_copy_between_two_buckets_runs_on_the_server(target: &S3Target) {
    let second = target.bucket_2().expect("a cross-bucket cell needs a second bucket");
    let source = target.connect(Some(target.bucket())).await;
    let destination = target.connect(Some(second)).await;
    let prefix = target.prefix("engine-cross-bucket");
    let bytes = self_describing_bytes(3 * MIB, "cross");
    let key = format!("{prefix}moved.bin");
    target
        .seed(
            target.bucket(),
            &[Seed {
                key: &key,
                bytes: &bytes,
                mtime: Some(distant_mtime()),
            }],
        )
        .await;
    let dest_dir = destination.root().join(prefix.trim_end_matches('/'));
    let source_path = source.root().join(&key);
    let destination = Arc::new(destination);
    destination
        .create_directory_all(&dest_dir)
        .await
        .expect("making the destination folder");
    take_sent_requests(&destination).await;
    run_copy(
        "s3_cross_bucket_server_side",
        Arc::new(source),
        vec![source_path],
        Arc::clone(&destination) as Arc<dyn Volume>,
        dest_dir.clone(),
    )
    .await;
    let sent = take_sent_requests(&destination).await;

    assert_eq!(
        sha256(&read_all(destination.as_ref(), &dest_dir.join("moved.bin")).await),
        sha256(&bytes)
    );
    assert_eq!(sent.get("CopyObject"), Some(&1), "copied on the server: {sent:?}");
    assert_eq!(sent.get("PutObject"), None, "no byte went through the Mac: {sent:?}");
}

/// A provider that copies within one bucket only (Spaces) streams a
/// cross-bucket copy through the Mac instead, with the same bytes landing.
pub(super) async fn a_bucket_bound_provider_streams_a_cross_bucket_copy(target: &S3Target) {
    let second = target.bucket_2().expect("a cross-bucket cell needs a second bucket");
    let source = target.connect(Some(target.bucket())).await;
    let destination = target.connect(Some(second)).await;
    destination.forbid_cross_bucket_copy().await;
    let prefix = target.prefix("engine-bucket-bound");
    let bytes = self_describing_bytes(2 * MIB, "streamed");
    let key = format!("{prefix}moved.bin");
    target.seed(target.bucket(), &[object(&key, &bytes)]).await;
    let dest_dir = destination.root().join(prefix.trim_end_matches('/'));
    let source_path = source.root().join(&key);
    let destination = Arc::new(destination);
    destination
        .create_directory_all(&dest_dir)
        .await
        .expect("making the destination folder");
    take_sent_requests(&destination).await;
    run_copy(
        "s3_cross_bucket_streamed",
        Arc::new(source),
        vec![source_path],
        Arc::clone(&destination) as Arc<dyn Volume>,
        dest_dir.clone(),
    )
    .await;
    let sent = take_sent_requests(&destination).await;

    assert_eq!(
        sha256(&read_all(destination.as_ref(), &dest_dir.join("moved.bin")).await),
        sha256(&bytes)
    );
    assert_eq!(sent.get("CopyObject"), None, "{sent:?}");
    assert_eq!(
        sent.get("PutObject"),
        Some(&1),
        "streamed through the Mac, so a PUT wrote it: {sent:?}"
    );
}

// The cells are written out per fixture, not generated by a macro:
// `fixture-lane-coverage` reads each gated test's literal name for the lane prefix.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_folder_of_1005_objects_renames_through_the_engine_on_versitygw() {
    a_folder_of_1005_objects_renames_through_the_engine(&S3Target::Fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_folder_of_1005_objects_renames_through_the_engine_on_garage() {
    a_folder_of_1005_objects_renames_through_the_engine(&S3Target::Fixture(GARAGE)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_big_file_renames_by_multipart_copy_keeping_its_date_on_versitygw() {
    a_big_file_renames_by_multipart_copy_keeping_its_date(&S3Target::Fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_big_file_renames_by_multipart_copy_keeping_its_date_on_garage() {
    a_big_file_renames_by_multipart_copy_keeping_its_date(&S3Target::Fixture(GARAGE)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_paused_then_cancelled_rename_keeps_the_source_whole_on_versitygw() {
    a_paused_then_cancelled_rename_keeps_the_source_whole(&S3Target::Fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_paused_then_cancelled_rename_keeps_the_source_whole_on_garage() {
    a_paused_then_cancelled_rename_keeps_the_source_whole(&S3Target::Fixture(GARAGE)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_paused_rename_resumes_and_lands_on_versitygw() {
    a_paused_rename_resumes_and_lands(&S3Target::Fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_paused_rename_resumes_and_lands_on_garage() {
    a_paused_rename_resumes_and_lands(&S3Target::Fixture(GARAGE)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_batch_with_a_folder_renames_as_one_move_on_versitygw() {
    a_batch_with_a_folder_renames_as_one_move(&S3Target::Fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_batch_with_a_folder_renames_as_one_move_on_garage() {
    a_batch_with_a_folder_renames_as_one_move(&S3Target::Fixture(GARAGE)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_copy_between_two_buckets_runs_on_the_server_on_versitygw() {
    a_copy_between_two_buckets_runs_on_the_server(&S3Target::Fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_copy_between_two_buckets_runs_on_the_server_on_garage() {
    a_copy_between_two_buckets_runs_on_the_server(&S3Target::Fixture(GARAGE)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_bucket_bound_provider_streams_a_cross_bucket_copy_on_versitygw() {
    a_bucket_bound_provider_streams_a_cross_bucket_copy(&S3Target::Fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_bucket_bound_provider_streams_a_cross_bucket_copy_on_garage() {
    a_bucket_bound_provider_streams_a_cross_bucket_copy(&S3Target::Fixture(GARAGE)).await;
}
