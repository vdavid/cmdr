//! The write path against both fixtures: uploads byte for byte (one PUT, in
//! parts, of unknown length), the source's mtime, cancel, the no-overwrite
//! refusals on both the header path and the check-then-write one, the
//! unfinished-upload sweep, folders, delete, and rename.
//!
//! Most multipart cells cut 5 MiB parts (`S3Volume::set_part_floor`) so they
//! see several parts without uploading hundreds of megabytes; one cell per
//! fixture uploads at the production 64 MiB.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::{DirectoryCreation, StreamLength, Volume, VolumeError, VolumeReadStream, WriteMode};

use super::S3Volume;
use super::testing::*;
use super::upload_ledger::{UnfinishedUpload, UploadLedger};
use crate::metadata::format_mtime;

const MIB: usize = 1024 * 1024;

fn key_of(prefix: &str, name: &str) -> String {
    format!("{prefix}{name}")
}

async fn write(volume: &S3Volume, path: &Path, mode: WriteMode, source: BytesSource) -> Result<u64, VolumeError> {
    let length = source.total_size();
    volume
        .write_from_stream(path, mode, length, Box::new(source), &|_| ControlFlow::Continue(()))
        .await
}

async fn small_and_large_uploads_round_trip_with_their_mtime(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("write-round-trip");
    let mtime = SystemTime::UNIX_EPOCH + Duration::new(1_354_040_105, 123_456_789);

    let small = self_describing_bytes(3 * MIB + 17, "small");
    let small_path = volume.root().join(key_of(&prefix, "small.bin"));
    let written = write(
        &volume,
        &small_path,
        WriteMode::CreateNew,
        BytesSource::new(small.clone()).modified_at(mtime),
    )
    .await
    .expect("a small upload lands");
    assert_eq!(written, small.len() as u64);
    assert!(
        read_back(&volume, &small_path).await == small,
        "the small upload must read back byte for byte"
    );
    // rclone's format, to the nanosecond the source had.
    assert_eq!(
        stored_mtime_header(service, FIXTURE_BUCKET, &key_of(&prefix, "small.bin")).await,
        Some(format_mtime(mtime))
    );
    let entry = volume.get_metadata(&small_path).await.expect("a stat");
    assert_eq!(
        entry.modified_at,
        Some(1_354_040_105),
        "a stat shows the source's own date"
    );

    // Production parts: 64 + 64 + 12 MiB.
    let large = self_describing_bytes(140 * MIB, "large");
    let large_path = volume.root().join(key_of(&prefix, "large.bin"));
    let written = write(
        &volume,
        &large_path,
        WriteMode::CreateNew,
        BytesSource::new(large.clone()).modified_at(mtime),
    )
    .await
    .expect("a multipart upload lands");
    assert_eq!(written, large.len() as u64);
    assert!(
        read_back(&volume, &large_path).await == large,
        "the multipart upload must read back byte for byte"
    );
    assert_eq!(
        stored_mtime_header(service, FIXTURE_BUCKET, &key_of(&prefix, "large.bin")).await,
        Some(format_mtime(mtime)),
        "the metadata rides on the multipart upload's creation"
    );
    assert!(unfinished_uploads(service, FIXTURE_BUCKET, &prefix).await.is_empty());
}

async fn uploads_of_unknown_length_round_trip(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("write-unknown");

    // Ends inside its first part: one PUT after all.
    let short = self_describing_bytes(300 * 1024, "short");
    let short_path = volume.root().join(key_of(&prefix, "short.bin"));
    let written = write(
        &volume,
        &short_path,
        WriteMode::CreateNew,
        BytesSource::new(short.clone()).of_unknown_length(),
    )
    .await
    .expect("a short stream of unknown length lands");
    assert_eq!(written, short.len() as u64);
    assert!(read_back(&volume, &short_path).await == short);

    // Three parts: 5 + 5 + 2 MiB.
    let long = self_describing_bytes(12 * MIB, "long");
    let long_path = volume.root().join(key_of(&prefix, "long.bin"));
    let written = write(
        &volume,
        &long_path,
        WriteMode::CreateNew,
        BytesSource::new(long.clone()).of_unknown_length(),
    )
    .await
    .expect("a long stream of unknown length lands in parts");
    assert_eq!(written, long.len() as u64);
    assert!(read_back(&volume, &long_path).await == long);
    assert!(unfinished_uploads(service, FIXTURE_BUCKET, &prefix).await.is_empty());
}

/// A source that hands out its first pieces and then never another: the
/// upload is parked mid-body, with the server already holding all but the
/// read-ahead piece, for as long as the cell likes.
struct StallingSource {
    pieces: Vec<Vec<u8>>,
    total: u64,
}

impl VolumeReadStream for StallingSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.pieces.is_empty() {
                std::future::pending::<()>().await;
            }
            Some(Ok(self.pieces.remove(0)))
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.total)
    }

    fn bytes_read(&self) -> u64 {
        0
    }

    fn modified_at(&self) -> Option<SystemTime> {
        None
    }
}

/// ❗ A single PUT cancelled while its source stalls leaves nothing under the
/// name, and never hangs on the stalled source. A PUT's body is read whole
/// before it goes (`writes.rs::read_whole`), so the Cancel lands while it
/// fills, answered every progress tick, and not a byte reaches the server
/// (VersityGW would otherwise store a cut-off body under the user's name). A
/// PUT cut off mid-body is settled by its token: `pause_test.rs` pins that
/// against a fake that keeps cut-off bodies.
async fn a_cancelled_single_put_publishes_nothing(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("write-cancel-put");
    let path = volume.root().join(key_of(&prefix, "cancelled.bin"));
    // Two pieces of three, then the source stalls for good.
    let source = StallingSource {
        pieces: vec![
            self_describing_bytes(MIB, "first"),
            self_describing_bytes(MIB, "second"),
        ],
        total: 3 * MIB as u64,
    };
    let asked = std::sync::atomic::AtomicUsize::new(0);

    let outcome = volume
        .write_from_stream(
            &path,
            WriteMode::CreateNew,
            source.total_size(),
            Box::new(source),
            // The user's Cancel, a few ticks into the stall.
            &|_| {
                if asked.fetch_add(1, Ordering::Relaxed) >= 4 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        )
        .await;

    assert!(matches!(outcome, Err(VolumeError::Cancelled(_))), "got {outcome:?}");
    let left = volume.get_metadata(&path).await.ok().map(|entry| entry.size);
    assert!(
        left.is_none(),
        "a cancelled PUT left an object of {left:?} bytes under the name"
    );
}

/// ❗ An overwrite of an existing object that's cancelled mid-body keeps the
/// ORIGINAL. VersityGW publishes a PUT cut off mid-body (fixture README), so
/// on a provider not trusted to refuse a short body the write goes as a
/// multipart upload, which publishes nothing until its completion. Nothing of
/// it stays behind: no upload on the server, no record in the ledger.
async fn a_cancelled_overwrite_keeps_the_original(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("write-cancel-overwrite");
    let key = key_of(&prefix, "kept.txt");
    let original = b"the original, which must survive".to_vec();
    seed(service, FIXTURE_BUCKET, &[object(&key, &original)]).await;
    let path = volume.root().join(&key);
    // Two MiB of three, then the source stalls: the cancel lands while the
    // one part is still filling, and must not wait for the source.
    let source = StallingSource {
        pieces: vec![
            self_describing_bytes(MIB, "first"),
            self_describing_bytes(MIB, "second"),
        ],
        total: 3 * MIB as u64,
    };
    let asked = std::sync::atomic::AtomicUsize::new(0);

    let outcome = volume
        .write_from_stream(
            &path,
            WriteMode::CreateOrReplace,
            source.total_size(),
            Box::new(source),
            &|_| {
                if asked.fetch_add(1, Ordering::Relaxed) >= 4 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        )
        .await;

    assert!(matches!(outcome, Err(VolumeError::Cancelled(_))), "got {outcome:?}");
    assert!(
        read_back(&volume, &path).await == original,
        "a cancelled overwrite must leave the original byte for byte"
    );
    let folder = volume.root().join(prefix.trim_end_matches('/'));
    let names: Vec<String> = volume
        .list_directory(&folder, None)
        .await
        .expect("the folder lists")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, vec!["kept.txt".to_string()], "nothing stays beside it");
    assert!(
        unfinished_uploads(service, FIXTURE_BUCKET, &prefix).await.is_empty(),
        "the cancelled upload must be aborted on the server"
    );
    assert!(
        volume
            .inner
            .ledger
            .open_under(&volume.inner.account(), &prefix)
            .is_empty()
    );
}

/// An overwrite that finishes lands the new bytes and the source's date at
/// the name as ONE multipart upload, even for a small file, with nothing left
/// beside it or unfinished, on both fixtures. A stream of unknown length that
/// ends inside its first part goes the same way.
async fn a_finished_overwrite_lands_whole_as_one_upload(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("write-overwrite-upload");
    let key = key_of(&prefix, "replaced.bin");
    let open_key = key_of(&prefix, "open.bin");
    seed(
        service,
        FIXTURE_BUCKET,
        &[object(&key, b"old bytes"), object(&open_key, b"old, too")],
    )
    .await;
    let path = volume.root().join(&key);
    let open_path = volume.root().join(&open_key);
    let fresh = self_describing_bytes(2 * MIB + 9, "fresh");
    let mtime = SystemTime::UNIX_EPOCH + Duration::new(1_354_040_105, 5);

    write(
        &volume,
        &path,
        WriteMode::CreateOrReplace,
        BytesSource::new(fresh.clone()).modified_at(mtime),
    )
    .await
    .expect("an overwrite lands");
    write(
        &volume,
        &open_path,
        WriteMode::CreateOrReplace,
        BytesSource::new(b"short, of unknown length".to_vec()).of_unknown_length(),
    )
    .await
    .expect("an overwrite of unknown length lands");

    assert!(read_back(&volume, &path).await == fresh);
    assert_eq!(read_back(&volume, &open_path).await, b"short, of unknown length");
    assert_eq!(
        stored_mtime_header(service, FIXTURE_BUCKET, &key).await,
        Some(format_mtime(mtime))
    );
    for key in [&key, &open_key] {
        let etag = stored_etag(service, FIXTURE_BUCKET, key).await.unwrap_or_default();
        assert!(
            etag.trim_matches('"').ends_with("-1"),
            "{key} must land as a one-part multipart upload, got the ETag {etag}"
        );
    }
    let folder = volume.root().join(prefix.trim_end_matches('/'));
    let listed = volume.list_directory(&folder, None).await.expect("the folder lists");
    assert_eq!(listed.len(), 2, "nothing stays beside them: {listed:?}");
    assert!(unfinished_uploads(service, FIXTURE_BUCKET, &prefix).await.is_empty());
    assert!(
        volume
            .inner
            .ledger
            .open_under(&volume.inner.account(), &prefix)
            .is_empty()
    );
}

/// ❗ A cancel aborts the multipart upload: no parts stay behind on the server
/// (billed, invisible), no record stays in the ledger, and no object appears.
async fn a_cancel_mid_multipart_leaves_no_upload_behind(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("write-cancel");
    let path = volume.root().join(key_of(&prefix, "cancelled.bin"));
    let bytes = self_describing_bytes(30 * MIB, "cancel");
    let length = StreamLength::Known(bytes.len() as u64);

    let outcome = volume
        .write_from_stream(
            &path,
            WriteMode::CreateNew,
            length,
            Box::new(BytesSource::new(bytes)),
            &|progress| {
                if progress.bytes_written > 6 * MIB as u64 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        )
        .await;

    assert!(matches!(outcome, Err(VolumeError::Cancelled(_))), "got {outcome:?}");
    assert!(
        unfinished_uploads(service, FIXTURE_BUCKET, &prefix).await.is_empty(),
        "a cancelled upload must be aborted on the server"
    );
    assert!(
        volume
            .inner
            .ledger
            .open_under(&volume.inner.account(), &prefix)
            .is_empty()
    );
    assert!(!volume.exists(&path).await, "nothing was published");
}

/// The header path, against VersityGW, which honours `If-None-Match: *` on
/// PUT and on `CompleteMultipartUpload`: the refusal is the server's 412, and
/// the refused multipart upload is aborted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn create_new_is_refused_by_the_conditional_header_on_versitygw() {
    let volume = connect_fixture(VERSITYGW, Some(FIXTURE_BUCKET)).await;
    volume.trust_conditional_writes().await;
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("write-conditional");
    let theirs: &[u8] = b"another writer's file";
    let key = key_of(&prefix, "taken.txt");
    seed(VERSITYGW, FIXTURE_BUCKET, &[object(&key, theirs)]).await;
    let path = volume.root().join(&key);

    let single = write(&volume, &path, WriteMode::CreateNew, BytesSource::new(vec![1; 1000])).await;
    assert!(matches!(single, Err(VolumeError::AlreadyExists(_))), "got {single:?}");
    let parts = write(
        &volume,
        &path,
        WriteMode::CreateNew,
        BytesSource::new(self_describing_bytes(12 * MIB, "parts")),
    )
    .await;
    assert!(matches!(parts, Err(VolumeError::AlreadyExists(_))), "got {parts:?}");

    assert_eq!(
        read_back(&volume, &path).await,
        theirs,
        "their file must survive byte for byte"
    );
    assert!(
        unfinished_uploads(VERSITYGW, FIXTURE_BUCKET, &prefix).await.is_empty(),
        "the refused multipart upload must be aborted"
    );
}

/// A source that lets another writer take the name while the upload runs:
/// on its last piece, it puts an object at the key first.
struct RacedSource {
    inner: BytesSource,
    service: FixtureService,
    key: String,
    raced: AtomicBool,
    remaining: usize,
}

impl VolumeReadStream for RacedSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.remaining <= MIB && !self.raced.swap(true, Ordering::Relaxed) {
                seed(
                    self.service,
                    FIXTURE_BUCKET,
                    &[object(&self.key, b"the other writer won")],
                )
                .await;
            }
            let piece = self.inner.next_chunk().await;
            if let Some(Ok(bytes)) = &piece {
                self.remaining = self.remaining.saturating_sub(bytes.len());
            }
            piece
        })
    }

    fn total_size(&self) -> StreamLength {
        self.inner.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.inner.bytes_read()
    }

    fn modified_at(&self) -> Option<SystemTime> {
        None
    }
}

/// The check-then-write path (every provider but AWS and R2; Garage ignores
/// the header outright). A name taken BEFORE the write is refused by the HEAD
/// in front of it; one taken DURING a multipart upload is caught by the check
/// right before `CompleteMultipartUpload`. Either way the other writer's file
/// survives and nothing of ours is left on the server.
async fn create_new_checks_then_writes_and_catches_a_writer_mid_upload(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    volume.set_part_floor(5 * MIB as u64);
    let prefix = scratch_prefix("write-check-then-write");

    let taken = key_of(&prefix, "taken.txt");
    seed(service, FIXTURE_BUCKET, &[object(&taken, b"already here")]).await;
    let refused = write(
        &volume,
        &volume.root().join(&taken),
        WriteMode::CreateNew,
        BytesSource::new(vec![2; 4000]),
    )
    .await;
    assert!(matches!(refused, Err(VolumeError::AlreadyExists(_))), "got {refused:?}");
    assert_eq!(read_back(&volume, &volume.root().join(&taken)).await, b"already here");

    let raced = key_of(&prefix, "raced.bin");
    let bytes = self_describing_bytes(12 * MIB, "raced");
    let source = RacedSource {
        remaining: bytes.len(),
        inner: BytesSource::new(bytes.clone()),
        service,
        key: raced.clone(),
        raced: AtomicBool::new(false),
    };
    let outcome = volume
        .write_from_stream(
            &volume.root().join(&raced),
            WriteMode::CreateNew,
            StreamLength::Known(bytes.len() as u64),
            Box::new(source),
            &|_| ControlFlow::Continue(()),
        )
        .await;
    assert!(matches!(outcome, Err(VolumeError::AlreadyExists(_))), "got {outcome:?}");
    assert_eq!(
        read_back(&volume, &volume.root().join(&raced)).await,
        b"the other writer won",
        "the writer that took the name mid-upload keeps it"
    );
    assert!(unfinished_uploads(service, FIXTURE_BUCKET, &prefix).await.is_empty());
}

/// A file→file Overwrite reaches S3 as `CreateOrReplace` at the final key,
/// and the new bytes replace the old whole.
async fn a_replace_writes_over_the_object_in_place(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("write-replace");
    let key = key_of(&prefix, "report.txt");
    seed(service, FIXTURE_BUCKET, &[object(&key, b"the old report")]).await;
    let path = volume.root().join(&key);

    write(
        &volume,
        &path,
        WriteMode::CreateOrReplace,
        BytesSource::new(b"the new, longer report".to_vec()),
    )
    .await
    .expect("a replace lands");
    assert_eq!(read_back(&volume, &path).await, b"the new, longer report");
}

/// ❗ The sweep aborts the uploads Cmdr recorded and ❌ never one it didn't:
/// an upload ID the ledger doesn't hold is another tool's, and may be live.
async fn the_sweep_aborts_only_recorded_uploads(service: FixtureService) {
    let state = TestDir::new("s3_ledger_sweep");
    let volume = connect_fixture_with_host(service, Some(FIXTURE_BUCKET), fixture_host_with_state(&state)).await;
    let prefix = scratch_prefix("write-sweep");
    let ours_key = key_of(&prefix, "ours.bin");
    let theirs_key = key_of(&prefix, "theirs.bin");
    let ours_id = start_foreign_upload(service, FIXTURE_BUCKET, &ours_key).await;
    let theirs_id = start_foreign_upload(service, FIXTURE_BUCKET, &theirs_key).await;
    // What a crash mid-upload leaves: a record, and nothing running it.
    let ledger = UploadLedger::at(Some(state.join("s3")));
    let record = UnfinishedUpload {
        account: volume.inner.account(),
        bucket: FIXTURE_BUCKET.to_string(),
        key: ours_key.clone(),
        upload_id: ours_id.clone(),
    };
    ledger.started(&record);
    ledger.abandoned(&record);

    volume.sweep_unfinished_uploads().await;

    let left = unfinished_uploads(service, FIXTURE_BUCKET, &prefix).await;
    assert!(
        !left.iter().any(|(_, id)| *id == ours_id),
        "the recorded upload must be aborted: {left:?}"
    );
    assert!(
        left.iter().any(|(_, id)| *id == theirs_id),
        "an upload Cmdr didn't record must be left alone: {left:?}"
    );
    assert!(
        ledger.open_under(&volume.inner.account(), &prefix).is_empty(),
        "the aborted record is forgotten"
    );
    abort_foreign_upload(service, FIXTURE_BUCKET, &theirs_key, &theirs_id).await;
}

async fn folders_are_markers_and_prefixes(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("write-folders");
    seed(service, FIXTURE_BUCKET, &[object(&key_of(&prefix, "anchor.txt"), b"x")]).await;
    let at = |name: &str| -> PathBuf { volume.root().join(key_of(&prefix, name)) };

    volume.create_directory(&at("empty")).await.expect("a new folder");
    let names: Vec<(String, bool)> = volume
        .list_directory(&volume.root().join(prefix.trim_end_matches('/')), None)
        .await
        .expect("a listing")
        .into_iter()
        .map(|entry| (entry.name, entry.is_directory))
        .collect();
    assert!(
        names.contains(&("empty".to_string(), true)),
        "the empty folder shows: {names:?}"
    );
    assert!(matches!(
        volume.create_directory(&at("empty")).await,
        Err(VolumeError::AlreadyExists(_))
    ));
    assert!(
        matches!(
            volume.create_directory(&at("missing/child")).await,
            Err(VolumeError::NotFound(_))
        ),
        "a folder needs its parent, the way mkdir does"
    );

    assert_eq!(
        volume.create_directory_all(&at("deep/er/est")).await.ok(),
        Some(DirectoryCreation::Created)
    );
    for level in ["deep", "deep/er", "deep/er/est"] {
        assert!(
            volume.is_directory(&at(level)).await.unwrap_or(false),
            "{level} is a folder"
        );
    }

    // An empty folder deletes as its marker, and is gone.
    volume.delete(&at("empty")).await.expect("an empty folder deletes");
    assert!(!volume.exists(&at("empty")).await);
}

async fn delete_takes_one_node_and_refuses_a_folder_with_keys_under_it(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("write-delete");
    let at = |name: &str| -> PathBuf { volume.root().join(key_of(&prefix, name)) };
    // A folder with no marker: it exists only through the key under it.
    seed(
        service,
        FIXTURE_BUCKET,
        &[
            object(&key_of(&prefix, "unmarked/inside.txt"), b"keep me"),
            object(&key_of(&prefix, "file.txt"), b"delete me"),
        ],
    )
    .await;

    let refused = volume.delete(&at("unmarked")).await;
    assert!(
        matches!(refused, Err(VolumeError::IoError { raw_os_error: Some(code), .. }) if code == super::mutation::ENOTEMPTY),
        "got {refused:?}"
    );
    assert!(
        volume.exists(&at("unmarked/inside.txt")).await,
        "a refused delete destroys nothing"
    );

    volume.delete(&at("file.txt")).await.expect("a file deletes");
    assert!(!volume.exists(&at("file.txt")).await);
    assert!(matches!(
        volume.delete(&at("file.txt")).await,
        Err(VolumeError::NotFound(_))
    ));
}

async fn rename_moves_one_small_file_and_refuses_a_folder(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("write-rename");
    let at = |name: &str| -> PathBuf { volume.root().join(key_of(&prefix, name)) };
    let mtime = distant_mtime();
    seed(
        service,
        FIXTURE_BUCKET,
        &[
            Seed {
                key: &key_of(&prefix, "before.txt"),
                bytes: b"renamed bytes",
                mtime: Some(mtime),
            },
            object(&key_of(&prefix, "folder/child.txt"), b"x"),
        ],
    )
    .await;

    volume
        .rename(&at("before.txt"), &at("after.txt"), false)
        .await
        .expect("a small file renames");
    assert!(
        !volume.exists(&at("before.txt")).await,
        "the source goes once the copy is verified"
    );
    assert_eq!(read_back(&volume, &at("after.txt")).await, b"renamed bytes");
    assert_eq!(
        stored_mtime_header(service, FIXTURE_BUCKET, &key_of(&prefix, "after.txt")).await,
        Some(format_mtime(mtime)),
        "a rename keeps the file's own date"
    );
    assert!(matches!(
        volume.rename(&at("folder"), &at("moved"), false).await,
        Err(VolumeError::NotSupported)
    ));
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
    small_and_large_uploads_round_trip_with_their_mtime
        => small_and_large_uploads_round_trip_with_their_mtime_on_versitygw,
           small_and_large_uploads_round_trip_with_their_mtime_on_garage;
    uploads_of_unknown_length_round_trip
        => uploads_of_unknown_length_round_trip_on_versitygw, uploads_of_unknown_length_round_trip_on_garage;
    a_cancel_mid_multipart_leaves_no_upload_behind
        => a_cancel_mid_multipart_leaves_no_upload_behind_on_versitygw,
           a_cancel_mid_multipart_leaves_no_upload_behind_on_garage;
    a_cancelled_single_put_publishes_nothing
        => a_cancelled_single_put_publishes_nothing_on_versitygw,
           a_cancelled_single_put_publishes_nothing_on_garage;
    a_cancelled_overwrite_keeps_the_original
        => a_cancelled_overwrite_keeps_the_original_on_versitygw, a_cancelled_overwrite_keeps_the_original_on_garage;
    a_finished_overwrite_lands_whole_as_one_upload
        => a_finished_overwrite_lands_whole_as_one_upload_on_versitygw,
           a_finished_overwrite_lands_whole_as_one_upload_on_garage;
    create_new_checks_then_writes_and_catches_a_writer_mid_upload
        => create_new_checks_then_writes_and_catches_a_writer_mid_upload_on_versitygw,
           create_new_checks_then_writes_and_catches_a_writer_mid_upload_on_garage;
    a_replace_writes_over_the_object_in_place
        => a_replace_writes_over_the_object_in_place_on_versitygw, a_replace_writes_over_the_object_in_place_on_garage;
    the_sweep_aborts_only_recorded_uploads
        => the_sweep_aborts_only_recorded_uploads_on_versitygw, the_sweep_aborts_only_recorded_uploads_on_garage;
    folders_are_markers_and_prefixes
        => folders_are_markers_and_prefixes_on_versitygw, folders_are_markers_and_prefixes_on_garage;
    delete_takes_one_node_and_refuses_a_folder_with_keys_under_it
        => delete_takes_one_node_and_refuses_a_folder_with_keys_under_it_on_versitygw,
           delete_takes_one_node_and_refuses_a_folder_with_keys_under_it_on_garage;
    rename_moves_one_small_file_and_refuses_a_folder
        => rename_moves_one_small_file_and_refuses_a_folder_on_versitygw,
           rename_moves_one_small_file_and_refuses_a_folder_on_garage;
}
