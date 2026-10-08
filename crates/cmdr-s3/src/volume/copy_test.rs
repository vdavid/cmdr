//! Server-side copy against both fixtures: whole and in parts, keeping the
//! bytes and the date, across buckets and refused where the profile forbids
//! it, cancelled with nothing left behind, paused between parts, and refusing
//! an occupied key under `CreateNew`.
//!
//! Multipart cells cut 5 MiB parts (`S3Volume::set_part_floor`), so a copy of
//! a few tens of megabytes runs in several parts.

use std::future::Future;
use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{InMemoryVolume, ServerCopyProgress, Volume, VolumeError, WriteMode};

use super::S3Volume;
use super::testing::*;
use crate::metadata::format_mtime;

const MIB: usize = 1024 * 1024;

/// What a copy reported, with an optional Cancel past some byte count and an
/// optional pause at one checkpoint.
#[derive(Default)]
struct Watch {
    reported: Mutex<Vec<u64>>,
    highest: AtomicU64,
    cancel_past: Option<u64>,
    checkpoints: AtomicUsize,
    /// Parks the checkpoint with this number (1-based) until `resumed`.
    pause_at: Option<usize>,
    parked: AtomicBool,
    resumed: tokio::sync::Notify,
}

impl ServerCopyProgress for Watch {
    fn advanced(&self, done: u64, _total: u64) -> ControlFlow<()> {
        self.reported.lock_ignore_poison().push(done);
        self.highest.fetch_max(done, Ordering::SeqCst);
        match self.cancel_past {
            Some(limit) if done > limit => ControlFlow::Break(()),
            _ => ControlFlow::Continue(()),
        }
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        let number = self.checkpoints.fetch_add(1, Ordering::SeqCst) + 1;
        Box::pin(async move {
            if self.pause_at == Some(number) {
                self.parked.store(true, Ordering::SeqCst);
                self.resumed.notified().await;
                self.parked.store(false, Ordering::SeqCst);
            }
            ControlFlow::Continue(())
        })
    }
}

fn at(volume: &S3Volume, key: &str) -> std::path::PathBuf {
    volume.root().join(key)
}

async fn copy(
    volume: &S3Volume,
    source: &S3Volume,
    from: &str,
    to: &str,
    mode: WriteMode,
    watch: &Watch,
) -> Result<u64, VolumeError> {
    volume
        .copy_on_server(source, &at(source, from), &at(volume, to), mode, watch)
        .await
}

async fn a_small_object_copies_in_one_request_keeping_its_date(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("copy-small");
    let bytes = self_describing_bytes(2 * MIB + 5, "small");
    let from = format!("{prefix}from.bin");
    let to = format!("{prefix}to.bin");
    seed(
        service,
        FIXTURE_BUCKET,
        &[Seed {
            key: &from,
            bytes: &bytes,
            mtime: Some(distant_mtime()),
        }],
    )
    .await;

    let watch = Watch::default();
    let copied = copy(&volume, &volume, &from, &to, WriteMode::CreateNew, &watch)
        .await
        .expect("a small copy lands");
    assert_eq!(copied, bytes.len() as u64);
    assert!(read_back(&volume, &at(&volume, &to)).await == bytes);
    assert_eq!(
        stored_mtime_header(service, FIXTURE_BUCKET, &to).await,
        Some(format_mtime(distant_mtime()))
    );
    assert_eq!(watch.highest.load(Ordering::SeqCst), bytes.len() as u64);
}

/// ❗ Past the part floor a copy runs in parts: progress moves per part, the
/// bytes and the date arrive intact, and no upload stays open.
async fn a_big_object_copies_in_parts_keeping_its_bytes_and_date(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("copy-parts");
    let bytes = self_describing_bytes(23 * MIB, "parts");
    let from = format!("{prefix}from.bin");
    let to = format!("{prefix}to.bin");
    seed(
        service,
        FIXTURE_BUCKET,
        &[Seed {
            key: &from,
            bytes: &bytes,
            mtime: Some(distant_mtime()),
        }],
    )
    .await;

    let watch = Watch::default();
    copy(&volume, &volume, &from, &to, WriteMode::CreateNew, &watch)
        .await
        .expect("a multipart copy lands");
    assert!(read_back(&volume, &at(&volume, &to)).await == bytes);
    assert_eq!(
        stored_mtime_header(service, FIXTURE_BUCKET, &to).await,
        Some(format_mtime(distant_mtime())),
        "the mtime rides on the multipart copy's creation"
    );
    let reported = watch.reported.lock_ignore_poison().clone();
    let distinct: std::collections::BTreeSet<u64> = reported.iter().copied().filter(|done| *done > 0).collect();
    assert!(
        distinct.len() >= 4,
        "progress per part (4 parts plus a folded tail): {reported:?}"
    );
    assert!(unfinished_uploads(service, FIXTURE_BUCKET, &prefix).await.is_empty());
}

/// ❗ A source that never carried an mtime keeps its date through a copy, in
/// one request or in parts: its upload time is written as the copy's mtime.
async fn a_source_without_an_mtime_keeps_its_date(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("copy-no-mtime");
    let small = self_describing_bytes(MIB, "small");
    let big = self_describing_bytes(11 * MIB, "big");
    let small_key = format!("{prefix}small.bin");
    let big_key = format!("{prefix}big.bin");
    seed(
        service,
        FIXTURE_BUCKET,
        &[object(&small_key, &small), object(&big_key, &big)],
    )
    .await;
    let small_date = volume
        .get_metadata(&at(&volume, &small_key))
        .await
        .expect("a stat")
        .modified_at;
    let big_date = volume
        .get_metadata(&at(&volume, &big_key))
        .await
        .expect("a stat")
        .modified_at;

    for (from, date) in [(&small_key, small_date), (&big_key, big_date)] {
        let to = format!("{from}.copy");
        copy(&volume, &volume, from, &to, WriteMode::CreateNew, &Watch::default())
            .await
            .expect("the copy lands");
        // Written as an mtime, so it can't be the copy's own upload time
        // however fast the copy ran.
        let date = date.expect("a seeded object has an upload time");
        assert_eq!(
            stored_mtime_header(service, FIXTURE_BUCKET, &to).await,
            Some(date.to_string()),
            "{from}: the source's date survives the copy"
        );
        let copied = volume.get_metadata(&at(&volume, &to)).await.expect("a stat");
        assert_eq!(copied.modified_at, Some(date));
    }
}

/// Two places of one account copy between their buckets on the server.
async fn a_copy_between_two_buckets_of_one_account_runs_on_the_server(service: FixtureService) {
    let source = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let destination = connect_fixture(service, Some(FIXTURE_BUCKET_2)).await;
    let prefix = scratch_prefix("copy-cross-bucket");
    let bytes = self_describing_bytes(MIB + 3, "cross");
    let key = format!("{prefix}moved.bin");
    seed(service, FIXTURE_BUCKET, &[object(&key, &bytes)]).await;

    copy(
        &destination,
        &source,
        &key,
        &key,
        WriteMode::CreateNew,
        &Watch::default(),
    )
    .await
    .expect("a cross-bucket copy lands");
    assert!(read_back(&destination, &at(&destination, &key)).await == bytes);
    assert!(
        read_back(&source, &at(&source, &key)).await == bytes,
        "a copy keeps its source"
    );
}

/// A provider that copies within one bucket only (Hetzner) refuses a
/// cross-bucket copy with `NotSupported`, which sends the engine to streaming;
/// within the bucket it still copies.
async fn a_bucket_bound_provider_refuses_a_cross_bucket_copy(service: FixtureService) {
    let source = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let destination = connect_fixture(service, Some(FIXTURE_BUCKET_2)).await;
    destination.forbid_cross_bucket_copy().await;
    let prefix = scratch_prefix("copy-bucket-bound");
    let key = format!("{prefix}a.bin");
    seed(service, FIXTURE_BUCKET, &[object(&key, b"hello")]).await;

    let refused = copy(
        &destination,
        &source,
        &key,
        &key,
        WriteMode::CreateNew,
        &Watch::default(),
    )
    .await;
    assert!(matches!(refused, Err(VolumeError::NotSupported)), "got {refused:?}");
    assert!(!destination.exists(&at(&destination, &key)).await, "nothing was copied");

    source.forbid_cross_bucket_copy().await;
    let within = format!("{prefix}b.bin");
    copy(&source, &source, &key, &within, WriteMode::CreateNew, &Watch::default())
        .await
        .expect("a copy within the bucket still runs on the server");
}

/// ❗ A cancel mid-copy aborts the multipart upload: no parts stay behind, no
/// object appears, the ledger forgets it, and the source is untouched.
async fn a_cancel_mid_copy_leaves_no_upload_and_the_source_intact(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("copy-cancel");
    let bytes = self_describing_bytes(30 * MIB, "cancel");
    let from = format!("{prefix}from.bin");
    let to = format!("{prefix}to.bin");
    seed(service, FIXTURE_BUCKET, &[object(&from, &bytes)]).await;

    let watch = Watch {
        cancel_past: Some(5 * MIB as u64),
        ..Watch::default()
    };
    let outcome = copy(&volume, &volume, &from, &to, WriteMode::CreateNew, &watch).await;
    assert!(matches!(outcome, Err(VolumeError::Cancelled(_))), "got {outcome:?}");
    assert!(
        unfinished_uploads(service, FIXTURE_BUCKET, &prefix).await.is_empty(),
        "a cancelled copy must be aborted on the server"
    );
    assert!(
        volume
            .inner
            .ledger
            .open_under(&volume.inner.account(), &prefix)
            .is_empty()
    );
    assert!(!volume.exists(&at(&volume, &to)).await, "nothing was published");
    assert!(
        read_back(&volume, &at(&volume, &from)).await == bytes,
        "the source stays whole"
    );
}

/// Replaces a copy's source with other bytes at one checkpoint (1-based; the
/// first comes before the upload is created, then one before each part).
struct ReplaceSourceAt {
    service: FixtureService,
    key: String,
    bytes: Vec<u8>,
    at: usize,
    checkpoints: AtomicUsize,
}

impl ServerCopyProgress for ReplaceSourceAt {
    fn advanced(&self, _done: u64, _total: u64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        let number = self.checkpoints.fetch_add(1, Ordering::SeqCst) + 1;
        Box::pin(async move {
            if number == self.at {
                seed(self.service, FIXTURE_BUCKET, &[object(&self.key, &self.bytes)]).await;
            }
            ControlFlow::Continue(())
        })
    }
}

/// ❗ A source replaced between parts fails the copy as `SourceChanged`:
/// nothing is published (it could be stitched from two versions), no upload
/// stays behind, and the new source is untouched, so a move never deletes it.
/// Both fixtures are "Other", whose profile doesn't trust the copy-source ETag
/// pin, so the copy HEADs the source before its completion.
async fn a_source_replaced_mid_copy_fails_as_changed_and_publishes_nothing(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("copy-source-changed");
    let from = format!("{prefix}from.bin");
    let to = format!("{prefix}to.bin");
    seed(
        service,
        FIXTURE_BUCKET,
        &[object(&from, &self_describing_bytes(30 * MIB, "v1"))],
    )
    .await;
    let replacement = self_describing_bytes(30 * MIB, "v2");
    let progress = ReplaceSourceAt {
        service,
        key: from.clone(),
        bytes: replacement.clone(),
        at: 3,
        checkpoints: AtomicUsize::new(0),
    };

    let outcome = volume
        .copy_on_server(
            &volume,
            &at(&volume, &from),
            &at(&volume, &to),
            WriteMode::CreateNew,
            &progress,
        )
        .await;

    assert!(matches!(outcome, Err(VolumeError::SourceChanged(_))), "got {outcome:?}");
    assert!(!volume.exists(&at(&volume, &to)).await, "nothing was published");
    assert!(unfinished_uploads(service, FIXTURE_BUCKET, &prefix).await.is_empty());
    assert!(
        volume
            .inner
            .ledger
            .open_under(&volume.inner.account(), &prefix)
            .is_empty()
    );
    assert!(
        read_back(&volume, &at(&volume, &from)).await == replacement,
        "the new source stays whole"
    );
}

/// Replaces a copy's source once every part has landed, right before the
/// completion: past every part's ETag pin, so only the HEAD before the
/// completion can see it.
struct ReplaceSourceAfterParts {
    service: FixtureService,
    key: String,
    bytes: Vec<u8>,
    done: AtomicBool,
}

impl ServerCopyProgress for ReplaceSourceAfterParts {
    fn advanced(&self, done: u64, total: u64) -> ControlFlow<()> {
        if done == total && !self.done.swap(true, Ordering::SeqCst) {
            // `advanced` is synchronous, so the seed runs on a runtime of its
            // own; the copy's runtime has other workers meanwhile.
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("a seeding runtime")
                        .block_on(seed(self.service, FIXTURE_BUCKET, &[object(&self.key, &self.bytes)]));
                });
            });
        }
        ControlFlow::Continue(())
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        Box::pin(async { ControlFlow::Continue(()) })
    }
}

/// ❗ The same where the server would have taken every part: a source
/// replaced after the last part copied is caught by the HEAD before the
/// completion, on a profile that doesn't trust the pin.
async fn a_source_replaced_after_its_last_part_fails_as_changed(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("copy-source-changed-late");
    let from = format!("{prefix}from.bin");
    let to = format!("{prefix}to.bin");
    seed(
        service,
        FIXTURE_BUCKET,
        &[object(&from, &self_describing_bytes(12 * MIB, "v1"))],
    )
    .await;
    let replacement = self_describing_bytes(12 * MIB, "v2");
    let progress = ReplaceSourceAfterParts {
        service,
        key: from.clone(),
        bytes: replacement.clone(),
        done: AtomicBool::new(false),
    };

    let outcome = volume
        .copy_on_server(
            &volume,
            &at(&volume, &from),
            &at(&volume, &to),
            WriteMode::CreateNew,
            &progress,
        )
        .await;

    assert!(matches!(outcome, Err(VolumeError::SourceChanged(_))), "got {outcome:?}");
    assert!(!volume.exists(&at(&volume, &to)).await, "nothing was published");
    assert!(unfinished_uploads(service, FIXTURE_BUCKET, &prefix).await.is_empty());
    assert!(
        read_back(&volume, &at(&volume, &from)).await == replacement,
        "the new source stays whole"
    );
}

/// ❗ A pause lands between parts: once the checkpoint parks, the parts
/// already in flight finish and no new one starts until it's resumed.
async fn a_paused_copy_starts_no_part_until_resumed(service: FixtureService) {
    let volume = Arc::new(connect_fixture(service, Some(FIXTURE_BUCKET)).await);
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("copy-pause");
    let bytes = self_describing_bytes(30 * MIB, "pause");
    let from = format!("{prefix}from.bin");
    let to = format!("{prefix}to.bin");
    seed(service, FIXTURE_BUCKET, &[object(&from, &bytes)]).await;

    // The first checkpoint is before the upload is created; the second and
    // third let parts 1 and 2 go; the fourth parks.
    let watch = Arc::new(Watch {
        pause_at: Some(4),
        ..Watch::default()
    });
    let task = {
        let volume = Arc::clone(&volume);
        let watch = Arc::clone(&watch);
        let (from, to) = (from.clone(), to.clone());
        tokio::spawn(async move { copy(&volume, &volume, &from, &to, WriteMode::CreateNew, &watch).await })
    };
    wait_until_async(
        Duration::from_secs(30),
        "the copy to park and its two parts to land",
        || watch.parked.load(Ordering::SeqCst) && watch.highest.load(Ordering::SeqCst) >= 10 * MIB as u64,
    )
    .await;
    assert_eq!(
        watch.highest.load(Ordering::SeqCst),
        10 * MIB as u64,
        "only the two parts started before the pause landed"
    );
    assert_eq!(
        watch.checkpoints.load(Ordering::SeqCst),
        4,
        "no part started past the pause"
    );

    watch.resumed.notify_one();
    task.await
        .expect("the copy task finished")
        .expect("a resumed copy lands");
    assert!(read_back(&volume, &at(&volume, &to)).await == bytes);
}

/// ❗ `CreateNew` never copies over an occupied key, whole or in parts: both
/// fixtures check first (neither honours a precondition on a copy).
async fn a_no_overwrite_copy_refuses_an_occupied_key_and_keeps_it(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("copy-no-overwrite");
    let small = self_describing_bytes(MIB, "small");
    let big = self_describing_bytes(12 * MIB, "big");
    let taken = b"the user's own".to_vec();
    let (small_key, big_key, taken_key) = (
        format!("{prefix}small.bin"),
        format!("{prefix}big.bin"),
        format!("{prefix}taken.bin"),
    );
    seed(
        service,
        FIXTURE_BUCKET,
        &[
            object(&small_key, &small),
            object(&big_key, &big),
            object(&taken_key, &taken),
        ],
    )
    .await;

    for from in [&small_key, &big_key] {
        let refused = copy(
            &volume,
            &volume,
            from,
            &taken_key,
            WriteMode::CreateNew,
            &Watch::default(),
        )
        .await;
        assert!(
            matches!(refused, Err(VolumeError::AlreadyExists(_))),
            "{from}: got {refused:?}"
        );
        assert!(
            read_back(&volume, &at(&volume, &taken_key)).await == taken,
            "{from}: theirs stays"
        );
    }
    copy(
        &volume,
        &volume,
        &big_key,
        &taken_key,
        WriteMode::CreateOrReplace,
        &Watch::default(),
    )
    .await
    .expect("a replace copies over it");
    assert!(read_back(&volume, &at(&volume, &taken_key)).await == big);
}

/// A copy from anything but a place of this account answers `NotSupported`
/// before a request goes out: a bucket and a key mean nothing elsewhere.
#[tokio::test]
async fn a_copy_from_another_backend_is_not_this_servers_to_make() {
    let volume = super::test_support::make_test_volume(Some("photos"));
    let other = InMemoryVolume::new("elsewhere");
    let outcome = volume
        .copy_on_server(
            &other,
            Path::new("/a.bin"),
            Path::new("/photos/a.bin"),
            WriteMode::CreateNew,
            &Watch::default(),
        )
        .await;
    assert!(matches!(outcome, Err(VolumeError::NotSupported)), "got {outcome:?}");
}

/// One `#[tokio::test]` per fixture for each cell, `#[ignore]`d for the shared
/// fixture lane.
macro_rules! on_both_fixtures {
    ($($cell:ident => $versitygw:ident, $garage:ident;)*) => {$(
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        #[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
        async fn $versitygw() {
            $cell(VERSITYGW).await;
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        #[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
        async fn $garage() {
            $cell(GARAGE).await;
        }
    )*};
}

on_both_fixtures! {
    a_small_object_copies_in_one_request_keeping_its_date
        => a_small_object_copies_in_one_request_keeping_its_date_on_versitygw,
           a_small_object_copies_in_one_request_keeping_its_date_on_garage;
    a_big_object_copies_in_parts_keeping_its_bytes_and_date
        => a_big_object_copies_in_parts_keeping_its_bytes_and_date_on_versitygw,
           a_big_object_copies_in_parts_keeping_its_bytes_and_date_on_garage;
    a_source_without_an_mtime_keeps_its_date
        => a_source_without_an_mtime_keeps_its_date_on_versitygw, a_source_without_an_mtime_keeps_its_date_on_garage;
    a_copy_between_two_buckets_of_one_account_runs_on_the_server
        => a_copy_between_two_buckets_of_one_account_runs_on_the_server_on_versitygw,
           a_copy_between_two_buckets_of_one_account_runs_on_the_server_on_garage;
    a_bucket_bound_provider_refuses_a_cross_bucket_copy
        => a_bucket_bound_provider_refuses_a_cross_bucket_copy_on_versitygw,
           a_bucket_bound_provider_refuses_a_cross_bucket_copy_on_garage;
    a_cancel_mid_copy_leaves_no_upload_and_the_source_intact
        => a_cancel_mid_copy_leaves_no_upload_and_the_source_intact_on_versitygw,
           a_cancel_mid_copy_leaves_no_upload_and_the_source_intact_on_garage;
    a_source_replaced_mid_copy_fails_as_changed_and_publishes_nothing
        => a_source_replaced_mid_copy_fails_as_changed_and_publishes_nothing_on_versitygw,
           a_source_replaced_mid_copy_fails_as_changed_and_publishes_nothing_on_garage;
    a_source_replaced_after_its_last_part_fails_as_changed
        => a_source_replaced_after_its_last_part_fails_as_changed_on_versitygw,
           a_source_replaced_after_its_last_part_fails_as_changed_on_garage;
    a_paused_copy_starts_no_part_until_resumed
        => a_paused_copy_starts_no_part_until_resumed_on_versitygw, a_paused_copy_starts_no_part_until_resumed_on_garage;
    a_no_overwrite_copy_refuses_an_occupied_key_and_keeps_it
        => a_no_overwrite_copy_refuses_an_occupied_key_and_keeps_it_on_versitygw,
           a_no_overwrite_copy_refuses_an_occupied_key_and_keeps_it_on_garage;
}
