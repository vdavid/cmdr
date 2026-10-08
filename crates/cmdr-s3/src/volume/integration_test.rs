//! Connect and browse against both Docker fixtures, VersityGW and Garage.
//!
//! Every cell runs against BOTH servers, because they disagree where it
//! matters (a wrong secret is `SignatureDoesNotMatch` on one and
//! `AccessDenied` on the other) and a backend that only works on one isn't
//! S3-compatible. Each cell works under a key prefix of its own: the stack is
//! machine-wide and its objects persist across runs.
//!
//! The stack: `apps/desktop/test/s3-servers/start.sh`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{ListingProgress, Volume, VolumeError};
use tokio_util::sync::CancellationToken;

use super::testing::*;
use super::{S3Volume, connect_s3_volume};
use crate::refusal::S3ConnectError;

const FIXTURE: &str = "s3-servers/start.sh (s3-fixture)";

/// The app path of `key` on a bucket place.
fn at(volume: &S3Volume, key: &str) -> PathBuf {
    volume.root().join(key.trim_end_matches('/'))
}

/// A listing's names and whether each is a folder, sorted by name.
async fn listed(volume: &S3Volume, path: &Path) -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> = volume
        .list_directory(path, None)
        .await
        .unwrap_or_else(|e| panic!("listing {}: {e:?} ({FIXTURE})", path.display()))
        .into_iter()
        .map(|entry| (entry.name, entry.is_directory))
        .collect();
    entries.sort();
    entries
}

async fn connect_with(service: FixtureService, bucket: Option<&str>, secret: &str) -> Result<S3Volume, S3ConnectError> {
    let params = fixture_params(service, bucket);
    let volume_id = cmdr_fs::volume::s3_volume_id(params.host(), params.port(), FIXTURE_ACCESS_KEY, bucket);
    connect_s3_volume(
        "fixture",
        &volume_id,
        params,
        fixture_host_with_secret(secret),
        CancellationToken::new(),
    )
    .await
}

// ── Connect ──────────────────────────────────────────────────────────

async fn the_account_root_lists_the_buckets(service: FixtureService) {
    let volume = connect_fixture(service, None).await;
    let buckets = listed(&volume, volume.root()).await;
    assert!(
        buckets.contains(&(FIXTURE_BUCKET.to_string(), true))
            && buckets.contains(&(FIXTURE_BUCKET_2.to_string(), true)),
        "{}: both fixture buckets list as folders, got {buckets:?}",
        service.key
    );
    let bucket = volume
        .get_metadata(&volume.root().join(FIXTURE_BUCKET))
        .await
        .expect(FIXTURE);
    assert!(bucket.is_directory, "{}: a bucket is a folder", service.key);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn the_account_root_lists_the_buckets_on_versitygw() {
    the_account_root_lists_the_buckets(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn the_account_root_lists_the_buckets_on_garage() {
    the_account_root_lists_the_buckets(GARAGE).await;
}

async fn a_wrong_secret_is_refused_with_a_typed_reason(service: FixtureService) {
    let wrong = "f".repeat(64);
    let root = connect_with(service, None, &wrong).await.err();
    let bucket = connect_with(service, Some(FIXTURE_BUCKET), &wrong).await.err();
    // ❗ The two servers word a wrong secret differently (fixture README), and
    // the table reads each the only honest way it can.
    if service == VERSITYGW {
        assert_eq!(
            root,
            Some(S3ConnectError::KeysRejected),
            "VersityGW says SignatureDoesNotMatch"
        );
        assert_eq!(bucket, Some(S3ConnectError::KeysRejected));
    } else {
        assert_eq!(
            root,
            Some(S3ConnectError::BucketListRefused),
            "Garage says AccessDenied, which a key without rights says too"
        );
        assert_eq!(bucket, Some(S3ConnectError::AccessDenied));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_wrong_secret_is_refused_with_a_typed_reason_on_versitygw() {
    a_wrong_secret_is_refused_with_a_typed_reason(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_wrong_secret_is_refused_with_a_typed_reason_on_garage() {
    a_wrong_secret_is_refused_with_a_typed_reason(GARAGE).await;
}

async fn a_missing_bucket_and_a_missing_secret_are_named(service: FixtureService) {
    let missing = connect_with(service, Some("cmdr-no-such-bucket"), &fixture_secret())
        .await
        .err();
    assert_eq!(missing, Some(S3ConnectError::NoSuchBucket), "{}", service.key);

    let params = fixture_params(service, None);
    let volume_id = cmdr_fs::volume::s3_volume_id(params.host(), params.port(), FIXTURE_ACCESS_KEY, None);
    let nothing_stored = connect_s3_volume(
        "fixture",
        &volume_id,
        params,
        cmdr_fs::volume::host::VolumeHost::detached(),
        CancellationToken::new(),
    )
    .await
    .err();
    assert_eq!(
        nothing_stored,
        Some(S3ConnectError::NeedsCredentials),
        "{}",
        service.key
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_missing_bucket_and_a_missing_secret_are_named_on_versitygw() {
    a_missing_bucket_and_a_missing_secret_are_named(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_missing_bucket_and_a_missing_secret_are_named_on_garage() {
    a_missing_bucket_and_a_missing_secret_are_named(GARAGE).await;
}

// ── Browse ───────────────────────────────────────────────────────────

/// The prefix the paging cells share, seeded once per fixture and kept: 1,005
/// PUTs per run would cost seconds for nothing.
const PAGING_PREFIX: &str = "cmdr-seed-paging-1005/";
const PAGING_COUNT: usize = 1_005;

async fn a_folder_past_one_page_lists_whole_and_reports_each_page(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let folder = at(&volume, PAGING_PREFIX);
    let already = volume.list_directory(&folder, None).await.map(|e| e.len()).unwrap_or(0);
    if already != PAGING_COUNT {
        let keys: Vec<String> = (0..PAGING_COUNT)
            .map(|n| format!("{PAGING_PREFIX}file-{n:04}.txt"))
            .collect();
        let seeds: Vec<Seed<'_>> = keys.iter().map(|key| object(key, b"x")).collect();
        seed(service, FIXTURE_BUCKET, &seeds).await;
    }

    let reports: Mutex<Vec<ListingProgress>> = Mutex::new(Vec::new());
    let on_progress = |progress: ListingProgress| reports.lock().expect("no poison in a test").push(progress);
    let entries = volume.list_directory(&folder, Some(&on_progress)).await.expect(FIXTURE);

    assert_eq!(entries.len(), PAGING_COUNT, "{}: every page lands", service.key);
    let reports = reports.into_inner().expect("no poison in a test");
    assert!(
        reports.len() >= 2,
        "{}: a 1,005-key folder is two pages, each reported; got {reports:?}",
        service.key
    );
    assert_eq!(reports.last().map(|r| r.files), Some(PAGING_COUNT), "{}", service.key);
    assert!(
        reports.windows(2).all(|pair| pair[0].files < pair[1].files),
        "{}: the tally only grows",
        service.key
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_folder_past_one_page_lists_whole_and_reports_each_page_on_versitygw() {
    a_folder_past_one_page_lists_whole_and_reports_each_page(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_folder_past_one_page_lists_whole_and_reports_each_page_on_garage() {
    a_folder_past_one_page_lists_whole_and_reports_each_page(GARAGE).await;
}

async fn nested_folders_and_awkward_names_list_as_stored(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("names");
    let nfc = "caf\u{e9}-nfc.txt";
    let nfd = "cafe\u{301}-nfd.txt";
    let keys = [
        format!("{prefix}a b+c.txt"),
        format!("{prefix}x + y/inner.txt"),
        format!("{prefix}deep/er/est.txt"),
        format!("{prefix}{nfc}"),
        format!("{prefix}{nfd}"),
        format!("{prefix} padded .txt"),
    ];
    let seeds: Vec<Seed<'_>> = keys.iter().map(|key| object(key, b"hello")).collect();
    seed(service, FIXTURE_BUCKET, &seeds).await;

    let top = listed(&volume, &at(&volume, &prefix)).await;
    let mut expected = vec![
        (" padded .txt".to_string(), false),
        ("a b+c.txt".to_string(), false),
        (nfd.to_string(), false),
        (nfc.to_string(), false),
        ("deep".to_string(), true),
        ("x + y".to_string(), true),
    ];
    expected.sort();
    assert_eq!(
        top, expected,
        "{}: a space, a `+`, and both Unicode forms come back byte for byte",
        service.key
    );
    assert_eq!(
        listed(&volume, &at(&volume, &format!("{prefix}deep/er"))).await,
        vec![("est.txt".to_string(), false)],
        "{}: two levels down",
        service.key
    );

    let plus = volume
        .get_metadata(&at(&volume, &format!("{prefix}a b+c.txt")))
        .await
        .expect(FIXTURE);
    assert_eq!(plus.size, Some(5), "{}: the `+` key is the one we wrote", service.key);
    let folder = volume
        .get_metadata(&at(&volume, &format!("{prefix}x + y")))
        .await
        .expect(FIXTURE);
    assert!(
        folder.is_directory,
        "{}: a prefix with no marker is still a folder",
        service.key
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn nested_folders_and_awkward_names_list_as_stored_on_versitygw() {
    nested_folders_and_awkward_names_list_as_stored(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn nested_folders_and_awkward_names_list_as_stored_on_garage() {
    nested_folders_and_awkward_names_list_as_stored(GARAGE).await;
}

async fn a_folder_marker_is_a_folder_and_never_a_file(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("markers");
    let marker = format!("{prefix}empty/");
    let file = format!("{prefix}file.txt");
    seed(
        service,
        FIXTURE_BUCKET,
        &[object(&prefix, b""), object(&marker, b""), object(&file, b"1")],
    )
    .await;

    assert_eq!(
        listed(&volume, &at(&volume, &prefix)).await,
        vec![("empty".to_string(), true), ("file.txt".to_string(), false)],
        "{}: the folder's own marker isn't listed, and a child's marker is a folder",
        service.key
    );
    let empty = at(&volume, &marker);
    assert_eq!(
        listed(&volume, &empty).await,
        Vec::<(String, bool)>::new(),
        "{}: an empty folder lists empty",
        service.key
    );
    let stat = volume.get_metadata(&empty).await.expect(FIXTURE);
    assert!(stat.is_directory, "{}: a marker stats as a folder", service.key);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_folder_marker_is_a_folder_and_never_a_file_on_versitygw() {
    a_folder_marker_is_a_folder_and_never_a_file(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_folder_marker_is_a_folder_and_never_a_file_on_garage() {
    a_folder_marker_is_a_folder_and_never_a_file(GARAGE).await;
}

async fn an_objects_own_mtime_shows_over_its_upload_time(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("mtime");
    let dated = format!("{prefix}dated.txt");
    let plain = format!("{prefix}plain.txt");
    seed(
        service,
        FIXTURE_BUCKET,
        &[
            Seed {
                key: &dated,
                bytes: b"dated",
                mtime: Some(distant_mtime()),
            },
            object(&plain, b"plain"),
        ],
    )
    .await;
    let distant = distant_mtime()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after the epoch")
        .as_secs();

    let stat = volume.get_metadata(&at(&volume, &dated)).await.expect(FIXTURE);
    assert_eq!(
        stat.modified_at,
        Some(distant),
        "{}: x-amz-meta-mtime is the date shown",
        service.key
    );
    let uploaded = volume.get_metadata(&at(&volume, &plain)).await.expect(FIXTURE);
    let upload_time = uploaded.modified_at.expect("Last-Modified is always there");
    assert!(
        upload_time > distant,
        "{}: without one, the upload time shows",
        service.key
    );

    // A listing carries no user metadata, so both show their upload time there.
    let entries: Vec<FileEntry> = volume.list_directory(&at(&volume, &prefix), None).await.expect(FIXTURE);
    assert!(
        entries
            .iter()
            .all(|entry| entry.modified_at.is_some_and(|at| at > distant)),
        "{}: a listing's dates are LastModified",
        service.key
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn an_objects_own_mtime_shows_over_its_upload_time_on_versitygw() {
    an_objects_own_mtime_shows_over_its_upload_time(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn an_objects_own_mtime_shows_over_its_upload_time_on_garage() {
    an_objects_own_mtime_shows_over_its_upload_time(GARAGE).await;
}

async fn what_isnt_there_is_not_found(service: FixtureService) {
    let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("missing");
    let missing = at(&volume, &format!("{prefix}nothing-here"));
    assert!(
        matches!(
            volume.list_directory(&missing, None).await,
            Err(VolumeError::NotFound(_))
        ),
        "{}: a prefix with no keys is no folder",
        service.key
    );
    assert!(
        matches!(volume.get_metadata(&missing).await, Err(VolumeError::NotFound(_))),
        "{}",
        service.key
    );
    assert!(!volume.exists(&missing).await, "{}", service.key);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn what_isnt_there_is_not_found_on_versitygw() {
    what_isnt_there_is_not_found(VERSITYGW).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn what_isnt_there_is_not_found_on_garage() {
    what_isnt_there_is_not_found(GARAGE).await;
}
