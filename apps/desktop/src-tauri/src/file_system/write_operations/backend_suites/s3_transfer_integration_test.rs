//! Real copies onto, off, and between live S3 buckets, driven through the
//! app's own `copy_between_volumes`, against both Docker fixtures.
//!
//! ❗ **The point is the entry point**, the same as
//! `webdav_transfer_integration_test.rs`: `cmdr-s3`'s own suite exercises every
//! read and write method, but a copy also needs the capability predicates, the
//! scan, the pre-flight, the conflict layer, and the staging decision to agree,
//! and none of those live in the crate. S3 is the first destination that
//! publishes every write whole, so these cells are also where the engine's
//! final-name writes and in-place overwrites meet a real server
//! (`write_operations/transfer/volume/DETAILS.md` § "Whole-publish
//! destinations").
//!
//! Every cell checksums the bytes at BOTH ends. The cells stay named for the
//! `s3_integration_` lane prefix.
//!
//! The scenarios take an `S3Target`, so `s3_live_engine_test.rs` runs the same
//! bodies against real accounts.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_s3::volume::testing::{FixtureService, GARAGE, S3Target, Seed, VERSITYGW, object, seed_once};

use super::network_transfer_test_support::{
    a_cancelled_upload_leaves_nothing_behind, a_directory_tree_lands_intact_off_the_server,
    a_directory_tree_lands_intact_on_the_server, a_pre_existing_destination_still_probes_each_name,
    a_seeded_tree_lands_intact_off_the_server, an_overwrite_answer_replaces_the_destination_bytes,
    assert_no_staging_litter, awkward_names_survive_a_round_trip, read_all, run_copy, self_describing_bytes, sha256,
    tree_files,
};
use crate::file_system::volume::LocalPosixVolume;
use crate::test_support::TestDir;

/// Big enough to cross the read windows rather than ride in one chunk.
const PAYLOAD_BYTES: usize = 700_000;

/// Past 64 MiB, the size where holding a file whole would show. Shared with the
/// crate's own large-object cell, and seeded once per fixture (`seed_once`).
const LARGE_LEN: usize = 65 * 1024 * 1024 + 123;
const LARGE_KEY: &str = "cmdr-seed-large-65mib/blob.bin";

/// A bucket place on `target`, as the transfer engine sees it, plus the key
/// prefix and app path of a scratch folder nothing else in the run uses.
pub(super) async fn place(target: &S3Target, label: &str) -> (Arc<dyn Volume>, String, PathBuf) {
    let volume = target.connect(Some(target.bucket())).await;
    let prefix = target.prefix(label);
    let dir = volume.root().join(prefix.trim_end_matches('/'));
    (Arc::new(volume), prefix, dir)
}

fn fixture(service: FixtureService) -> S3Target {
    S3Target::Fixture(service)
}

/// Copies `source` off the bucket into a fresh local directory and insists the
/// bytes that landed checksum to what the bucket holds.
async fn copy_off_and_compare(label: &str, remote: Arc<dyn Volume>, source: PathBuf, expected_len: usize) {
    let source_digest = sha256(&read_all(remote.as_ref(), &source).await);
    let name = source.file_name().expect("a file name").to_string_lossy().into_owned();

    let local_dir = TestDir::new(label);
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Local", &*local_dir));
    run_copy(
        label,
        Arc::clone(&remote),
        vec![source],
        Arc::clone(&local),
        PathBuf::from(""),
    )
    .await;

    let landed = read_all(local.as_ref(), Path::new(&name)).await;
    assert_eq!(
        landed.len(),
        expected_len,
        "{label}: the copy landed the wrong number of bytes"
    );
    assert_eq!(
        sha256(&landed),
        source_digest,
        "{label}: the bytes on local disk must checksum to what the bucket holds"
    );
}

pub(super) async fn copying_off_a_bucket_lands_every_byte(target: &S3Target) {
    let (remote, prefix, dir) = place(target, "copy-off").await;
    let content = self_describing_bytes(PAYLOAD_BYTES, "downloaded.bin");
    target
        .seed(target.bucket(), &[object(&format!("{prefix}downloaded.bin"), &content)])
        .await;
    assert_eq!(
        sha256(&read_all(remote.as_ref(), &dir.join("downloaded.bin")).await),
        sha256(&content),
        "the fixture seed must round-trip"
    );

    copy_off_and_compare("s3_copy_off_bucket", remote, dir.join("downloaded.bin"), PAYLOAD_BYTES).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_copying_off_a_bucket_lands_every_byte_on_versitygw() {
    copying_off_a_bucket_lands_every_byte(&fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_copying_off_a_bucket_lands_every_byte_on_garage() {
    copying_off_a_bucket_lands_every_byte(&fixture(GARAGE)).await;
}

/// A fixture keeps its large object across runs (`seed_once`); a live account
/// gets a fresh one under the run's prefix, which the cleanup takes away.
pub(super) async fn a_large_object_copies_off_a_bucket_byte_for_byte(target: &S3Target) {
    let key = match target {
        S3Target::Fixture(service) => {
            seed_once(*service, target.bucket(), LARGE_KEY, LARGE_LEN, || {
                cmdr_s3::volume::testing::self_describing_bytes(LARGE_LEN, "large")
            })
            .await;
            LARGE_KEY.to_string()
        }
        S3Target::Live(_) => {
            let key = format!("{}blob.bin", target.prefix("large-off"));
            let bytes = cmdr_s3::volume::testing::self_describing_bytes(LARGE_LEN, "large");
            target.seed(target.bucket(), &[object(&key, &bytes)]).await;
            key
        }
    };
    let remote: Arc<dyn Volume> = Arc::new(target.connect(Some(target.bucket())).await);
    let source = remote.root().join(&key);
    copy_off_and_compare("s3_copy_large_off_bucket", remote, source, LARGE_LEN).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_large_object_copies_off_a_bucket_byte_for_byte_on_versitygw() {
    a_large_object_copies_off_a_bucket_byte_for_byte(&fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_large_object_copies_off_a_bucket_byte_for_byte_on_garage() {
    a_large_object_copies_off_a_bucket_byte_for_byte(&fixture(GARAGE)).await;
}

pub(super) async fn a_directory_tree_lands_intact_off_a_bucket(target: &S3Target) {
    let (remote, prefix, dir) = place(target, "tree-off").await;
    let files: Vec<(String, Vec<u8>)> = tree_files()
        .map(|(relative, bytes)| (format!("{prefix}tree/{relative}"), bytes))
        .collect();
    let seeds: Vec<_> = files.iter().map(|(key, bytes)| object(key, bytes)).collect();
    target.seed(target.bucket(), &seeds).await;

    a_seeded_tree_lands_intact_off_the_server(remote, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_directory_tree_lands_intact_off_a_bucket_on_versitygw() {
    a_directory_tree_lands_intact_off_a_bucket(&fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_directory_tree_lands_intact_off_a_bucket_on_garage() {
    a_directory_tree_lands_intact_off_a_bucket(&fixture(GARAGE)).await;
}

// ── Onto a bucket, and between buckets ───────────────────────────────

/// Past one 64 MiB part, so the engine's copy onto S3 goes out as a multipart
/// upload (three parts) through `CheckpointStream`.
const MULTIPART_BYTES: usize = 140 * 1024 * 1024;

/// A local file onto a bucket at the final key (no `.cmdr-tmp-*` staging: S3
/// publishes whole), as one PUT and as a multipart upload, each carrying the
/// local file's own mtime.
pub(super) async fn copying_onto_a_bucket_lands_every_byte_and_the_mtime(target: &S3Target) {
    let (remote, prefix, dir) = place(target, "copy-onto").await;
    let local_dir = TestDir::new("s3_copy_onto_bucket");
    let mtime = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1_354_040_105, 250_000_000);
    for (name, len) in [("small.bin", PAYLOAD_BYTES), ("large.bin", MULTIPART_BYTES)] {
        let path = local_dir.join(name);
        std::fs::write(&path, self_describing_bytes(len, name)).expect("seeding the local file");
        std::fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_modified(mtime))
            .expect("dating the local file");
    }
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Local", &*local_dir));

    run_copy(
        "s3_copy_onto_bucket",
        Arc::clone(&local),
        vec![PathBuf::from("small.bin"), PathBuf::from("large.bin")],
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;

    for (name, len) in [("small.bin", PAYLOAD_BYTES), ("large.bin", MULTIPART_BYTES)] {
        let landed = read_all(remote.as_ref(), &dir.join(name)).await;
        assert_eq!(landed.len(), len, "{name}: the copy landed the wrong number of bytes");
        assert_eq!(
            sha256(&landed),
            sha256(&self_describing_bytes(len, name)),
            "{name}: the object must checksum to the local file"
        );
        assert_eq!(
            target
                .stored_mtime_header(target.bucket(), &format!("{prefix}{name}"))
                .await,
            Some("1354040105.25".to_string()),
            "{name}: the local file's own date rides as x-amz-meta-mtime"
        );
    }
    assert_no_staging_litter(remote.as_ref(), &dir, "a copy onto a bucket").await;
    assert!(
        target.unfinished_uploads(target.bucket(), &prefix).await.is_empty(),
        "the multipart upload must be completed, not left behind"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_copying_onto_a_bucket_lands_every_byte_and_the_mtime_on_versitygw() {
    copying_onto_a_bucket_lands_every_byte_and_the_mtime(&fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_copying_onto_a_bucket_lands_every_byte_and_the_mtime_on_garage() {
    copying_onto_a_bucket_lands_every_byte_and_the_mtime(&fixture(GARAGE)).await;
}

/// S3 → S3, between two buckets of one account, and the source object's
/// stored mtime carries over. Needs a second bucket.
pub(super) async fn copying_between_buckets_lands_every_byte(target: &S3Target) {
    let second = target.bucket_2().expect("a cross-bucket cell needs a second bucket");
    let (source, prefix, source_dir) = place(target, "copy-between").await;
    let content = self_describing_bytes(PAYLOAD_BYTES, "between.bin");
    target
        .seed(
            target.bucket(),
            &[Seed {
                key: &format!("{prefix}between.bin"),
                bytes: &content,
                mtime: Some(cmdr_s3::volume::testing::distant_mtime()),
            }],
        )
        .await;
    let dest: Arc<dyn Volume> = Arc::new(target.connect(Some(second)).await);
    let dest_dir = dest.root().join(prefix.trim_end_matches('/'));

    run_copy(
        "s3_copy_between_buckets",
        Arc::clone(&source),
        vec![source_dir.join("between.bin")],
        Arc::clone(&dest),
        dest_dir.clone(),
    )
    .await;

    assert_eq!(
        sha256(&read_all(dest.as_ref(), &dest_dir.join("between.bin")).await),
        sha256(&content),
        "the copy in the second bucket must checksum to the first"
    );
    assert_eq!(
        target
            .stored_mtime_header(second, &format!("{prefix}between.bin"))
            .await,
        Some("1354040105".to_string()),
        "the source object's stored mtime carries over"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_copying_between_buckets_lands_every_byte_on_versitygw() {
    copying_between_buckets_lands_every_byte(&fixture(VERSITYGW)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_copying_between_buckets_lands_every_byte_on_garage() {
    copying_between_buckets_lands_every_byte(&fixture(GARAGE)).await;
}

// ── The shared network scenarios, each against both fixtures ─────────
//
// A whole tree each way, a cancel mid-upload, an answered Overwrite (in place,
// on S3), a pre-existing destination folder still probed per name, and awkward
// names. Spelled out per fixture: the lane selects cells by name, and a macro
// would hide the names from `fixture-lane-coverage`.

/// Runs one shared scenario on a fresh scratch folder of `target`'s bucket.
pub(super) async fn scenario<F, Fut>(target: &S3Target, label: &str, run: F)
where
    F: FnOnce(Arc<dyn Volume>, PathBuf) -> Fut,
    Fut: Future<Output = ()>,
{
    let (remote, _, dir) = place(target, label).await;
    run(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_directory_tree_lands_intact_onto_a_bucket_on_versitygw() {
    scenario(
        &fixture(VERSITYGW),
        "a_directory_tree_lands_intact_on_the_server",
        a_directory_tree_lands_intact_on_the_server,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_directory_tree_lands_intact_onto_a_bucket_on_garage() {
    scenario(
        &fixture(GARAGE),
        "a_directory_tree_lands_intact_on_the_server",
        a_directory_tree_lands_intact_on_the_server,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_tree_written_through_the_volume_lands_intact_off_a_bucket_on_versitygw() {
    scenario(
        &fixture(VERSITYGW),
        "a_directory_tree_lands_intact_off_the_server",
        a_directory_tree_lands_intact_off_the_server,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_tree_written_through_the_volume_lands_intact_off_a_bucket_on_garage() {
    scenario(
        &fixture(GARAGE),
        "a_directory_tree_lands_intact_off_the_server",
        a_directory_tree_lands_intact_off_the_server,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_cancelled_upload_leaves_nothing_behind_on_versitygw() {
    scenario(
        &fixture(VERSITYGW),
        "a_cancelled_upload_leaves_nothing_behind",
        a_cancelled_upload_leaves_nothing_behind,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_cancelled_upload_leaves_nothing_behind_on_garage() {
    scenario(
        &fixture(GARAGE),
        "a_cancelled_upload_leaves_nothing_behind",
        a_cancelled_upload_leaves_nothing_behind,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_an_overwrite_answer_replaces_the_object_in_place_on_versitygw() {
    scenario(
        &fixture(VERSITYGW),
        "an_overwrite_answer_replaces_the_destination_bytes",
        an_overwrite_answer_replaces_the_destination_bytes,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_an_overwrite_answer_replaces_the_object_in_place_on_garage() {
    scenario(
        &fixture(GARAGE),
        "an_overwrite_answer_replaces_the_destination_bytes",
        an_overwrite_answer_replaces_the_destination_bytes,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_pre_existing_destination_still_probes_each_name_on_versitygw() {
    scenario(
        &fixture(VERSITYGW),
        "a_pre_existing_destination_still_probes_each_name",
        a_pre_existing_destination_still_probes_each_name,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_pre_existing_destination_still_probes_each_name_on_garage() {
    scenario(
        &fixture(GARAGE),
        "a_pre_existing_destination_still_probes_each_name",
        a_pre_existing_destination_still_probes_each_name,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_awkward_names_survive_a_round_trip_on_versitygw() {
    scenario(
        &fixture(VERSITYGW),
        "awkward_names_survive_a_round_trip",
        awkward_names_survive_a_round_trip,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_awkward_names_survive_a_round_trip_on_garage() {
    scenario(
        &fixture(GARAGE),
        "awkward_names_survive_a_round_trip",
        awkward_names_survive_a_round_trip,
    )
    .await;
}
