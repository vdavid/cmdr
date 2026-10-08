//! The read path against both Docker fixtures: whole objects, ranged windows,
//! a big object streamed without being held, and the refusals a read can meet.
//!
//! The stack: `apps/desktop/test/s3-servers/start.sh`.

use std::path::{Path, PathBuf};

use cmdr_fs::volume::{ShareLinkExpiry, StreamLength, Volume, VolumeError};

use super::S3Volume;
use super::testing::*;

const FIXTURE: &str = "s3-servers/start.sh (s3-fixture)";

/// Past `PartPlan`'s 64 MiB floor, so it's the size where a full buffer would
/// show and the size a multipart upload starts at.
const LARGE_LEN: usize = 65 * 1024 * 1024 + 123;

/// Where the large object lives: fixed and shared across runs (`seed_once`),
/// so ❌ never write to it.
const LARGE_KEY: &str = "cmdr-seed-large-65mib/blob.bin";

fn at(volume: &S3Volume, key: &str) -> PathBuf {
    volume.root().join(key)
}

async fn read_all(volume: &S3Volume, path: &Path) -> Vec<u8> {
    let mut stream = volume
        .open_read_stream(path)
        .await
        .unwrap_or_else(|e| panic!("opening {}: {e:?} ({FIXTURE})", path.display()));
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk.unwrap_or_else(|e| panic!("a chunk of {}: {e:?}", path.display())));
    }
    out
}

// ── Whole objects ────────────────────────────────────────────────────

async fn a_whole_object_streams_back_byte_for_byte(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let key = format!("{}whole.bin", scratch_prefix("read-whole"));
    let content = self_describing_bytes(700_000, "whole");
    seed(service, FIXTURE_BUCKET, &[object(&key, &content)]).await;

    let path = at(&volume, &key);
    let stream = volume.open_read_stream(&path).await.expect(FIXTURE);
    assert_eq!(
        stream.total_size(),
        StreamLength::Known(content.len() as u64),
        "{}: the stream names the object's length up front",
        service.key
    );
    drop(stream);
    assert!(
        read_all(&volume, &path).await == content,
        "{}: the bytes read back must be the bytes stored",
        service.key
    );
    assert!(
        volume
            .open_read_stream_with_hint(&path, Some(content.len() as u64))
            .await
            .is_ok(),
        "{}: a size hint changes nothing",
        service.key
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_whole_object_streams_back_byte_for_byte_on_versitygw() {
    a_whole_object_streams_back_byte_for_byte(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_whole_object_streams_back_byte_for_byte_on_garage() {
    a_whole_object_streams_back_byte_for_byte(GARAGE).await;
}

async fn an_empty_object_reads_as_no_bytes(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let key = format!("{}empty.txt", scratch_prefix("read-empty"));
    seed(service, FIXTURE_BUCKET, &[object(&key, b"")]).await;

    let path = at(&volume, &key);
    assert!(read_all(&volume, &path).await.is_empty(), "{}", service.key);
    assert!(
        volume.read_range(&path, 0, 10).await.expect(FIXTURE).is_empty(),
        "{}: a window on an empty object is empty",
        service.key
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn an_empty_object_reads_as_no_bytes_on_versitygw() {
    an_empty_object_reads_as_no_bytes(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn an_empty_object_reads_as_no_bytes_on_garage() {
    an_empty_object_reads_as_no_bytes(GARAGE).await;
}

// ── Ranges ───────────────────────────────────────────────────────────

async fn ranged_reads_answer_exactly_the_window_asked_for(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let key = format!("{}ranged.bin", scratch_prefix("read-range"));
    let content = self_describing_bytes(300_000, "ranged");
    seed(service, FIXTURE_BUCKET, &[object(&key, &content)]).await;
    let path = at(&volume, &key);
    let len = content.len() as u64;

    let window = |offset: u64, want: usize| {
        let volume = &volume;
        let path = &path;
        async move { volume.read_range(path, offset, want).await.expect(FIXTURE) }
    };
    assert_eq!(window(0, 32).await, content[..32], "{}: the head", service.key);
    assert_eq!(
        window(123_457, 4_096).await,
        content[123_457..123_457 + 4_096],
        "{}: a window from the middle",
        service.key
    );
    assert_eq!(
        window(len - 5, 100).await,
        content[content.len() - 5..],
        "{}: a window running past the end is cut at the end",
        service.key
    );
    assert!(window(len, 10).await.is_empty(), "{}: a window at the end", service.key);
    assert!(
        window(len + 10, 10).await.is_empty(),
        "{}: a window past the end",
        service.key
    );
    assert!(window(5, 0).await.is_empty(), "{}: a zero-length window", service.key);

    let mut tail = volume.open_read_stream_at_offset(&path, 200_000).await.expect(FIXTURE);
    assert_eq!(
        tail.total_size(),
        StreamLength::Known(len),
        "{}: a resumed stream names the FULL length, so progress stays anchored to the whole file",
        service.key
    );
    let mut read = Vec::new();
    while let Some(chunk) = tail.next_chunk().await {
        read.extend_from_slice(&chunk.expect(FIXTURE));
    }
    assert!(read == content[200_000..], "{}: the tail from the offset", service.key);
    assert_eq!(
        tail.bytes_read(),
        len - 200_000,
        "{}: counts only its segment",
        service.key
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn ranged_reads_answer_exactly_the_window_asked_for_on_versitygw() {
    ranged_reads_answer_exactly_the_window_asked_for(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn ranged_reads_answer_exactly_the_window_asked_for_on_garage() {
    ranged_reads_answer_exactly_the_window_asked_for(GARAGE).await;
}

// ── A big object ─────────────────────────────────────────────────────

async fn a_large_object_streams_in_small_pieces(service: FixtureService) {
    seed_once(service, FIXTURE_BUCKET, LARGE_KEY, LARGE_LEN, || {
        self_describing_bytes(LARGE_LEN, "large")
    })
    .await;
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let expected = self_describing_bytes(LARGE_LEN, "large");

    let mut stream = volume.open_read_stream(&at(&volume, LARGE_KEY)).await.expect(FIXTURE);
    let mut offset = 0usize;
    let mut chunks = 0usize;
    let mut largest = 0usize;
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk.expect(FIXTURE);
        assert!(
            chunk == expected[offset..offset + chunk.len()],
            "{}: the bytes at offset {offset} must be the ones stored there",
            service.key
        );
        offset += chunk.len();
        chunks += 1;
        largest = largest.max(chunk.len());
    }
    assert_eq!(offset, LARGE_LEN, "{}: every byte arrived", service.key);
    // A buffered read hands the whole object back as one piece (or a few
    // huge ones); a streamed one hands back what the socket delivered.
    assert!(
        largest <= 4 * 1024 * 1024 && chunks >= 16,
        "{}: chunk count {chunks}, largest chunk {largest} B: the body must stream, ❌ never be held whole",
        service.key
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_large_object_streams_in_small_pieces_on_versitygw() {
    a_large_object_streams_in_small_pieces(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_large_object_streams_in_small_pieces_on_garage() {
    a_large_object_streams_in_small_pieces(GARAGE).await;
}

// ── Share links ──────────────────────────────────────────────────────

async fn a_share_link_downloads_the_object_with_no_keys_at_all(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let key = format!("{}shared/a b+c.txt", scratch_prefix("share-link"));
    let content = self_describing_bytes(5_000, "shared");
    seed(service, FIXTURE_BUCKET, &[object(&key, &content)]).await;

    let link = volume
        .share_link(&at(&volume, &key), ShareLinkExpiry::OneHour)
        .await
        .expect(FIXTURE)
        .into_url();
    // A plain client with no signer and no credentials, the way the person the
    // link goes to would fetch it. ❗ This is the one place outside
    // `transport/` that speaks `reqwest`: it stands in for a browser.
    let fetched = cmdr_http::client_builder()
        .build()
        .expect("a plain client builds")
        .get(&link)
        .send()
        .await
        .expect(FIXTURE);
    assert_eq!(fetched.status(), 200, "{}: the link opens", service.key);
    assert!(
        fetched.bytes().await.expect(FIXTURE) == content,
        "{}: and hands back the object's bytes",
        service.key
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_share_link_downloads_the_object_with_no_keys_at_all_on_versitygw() {
    a_share_link_downloads_the_object_with_no_keys_at_all(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_share_link_downloads_the_object_with_no_keys_at_all_on_garage() {
    a_share_link_downloads_the_object_with_no_keys_at_all(GARAGE).await;
}

// ── Refusals ─────────────────────────────────────────────────────────

async fn reads_that_cant_happen_say_why(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("read-refused");
    seed(
        service,
        FIXTURE_BUCKET,
        &[object(&format!("{prefix}folder/inner.txt"), b"x")],
    )
    .await;

    let missing = at(&volume, &format!("{prefix}missing.txt"));
    match volume.open_read_stream(&missing).await {
        Err(VolumeError::NotFound(carried)) => assert!(
            carried.contains("missing.txt"),
            "{}: NotFound names the path, got {carried:?}",
            service.key
        ),
        other => panic!(
            "{}: a missing object reads as NotFound, got {:?}",
            service.key,
            other.err()
        ),
    }
    assert!(
        matches!(volume.read_range(&missing, 0, 10).await, Err(VolumeError::NotFound(_))),
        "{}: and so does a window on it",
        service.key
    );
    assert!(
        matches!(
            volume.open_read_stream(volume.root()).await,
            Err(VolumeError::IsADirectory(_))
        ),
        "{}: the bucket's top is a folder",
        service.key
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn reads_that_cant_happen_say_why_on_versitygw() {
    reads_that_cant_happen_say_why(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn reads_that_cant_happen_say_why_on_garage() {
    reads_that_cant_happen_say_why(GARAGE).await;
}

// ── The scan a cost estimate prices from ─────────────────────────────

async fn a_scan_keeps_every_objects_size_and_upload_date(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("scan-files");
    let (top, nested) = (format!("{prefix}a.bin"), format!("{prefix}sub/b.bin"));
    seed(
        service,
        FIXTURE_BUCKET,
        &[object(&top, &[1; 10]), object(&nested, &[2; 5])],
    )
    .await;
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs()
        - 3_600;

    let folder = at(&volume, prefix.trim_end_matches('/'));
    let batch = volume
        .scan_for_copy_batch(std::slice::from_ref(&folder))
        .await
        .unwrap_or_else(|e| panic!("{}: scanning: {e:?} ({FIXTURE})", service.key));

    let mut files = batch.files.expect("an S3 scan keeps its files");
    files.sort_by_key(|file| file.size);
    let sizes: Vec<u64> = files.iter().map(|file| file.size).collect();
    assert_eq!(sizes, vec![5, 10], "{}", service.key);
    assert!(
        files.iter().all(|file| file.modified_at.is_some_and(|at| at >= before)),
        "{}: every object carries its upload time, got {files:?}",
        service.key
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_scan_keeps_every_objects_size_and_upload_date_on_versitygw() {
    a_scan_keeps_every_objects_size_and_upload_date(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_scan_keeps_every_objects_size_and_upload_date_on_garage() {
    a_scan_keeps_every_objects_size_and_upload_date(GARAGE).await;
}
