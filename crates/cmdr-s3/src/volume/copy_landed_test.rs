//! A one-request server-side copy (`CopyObject`) whose server applied it while
//! the answer was lost, against a fake that commits the copy and then hangs up.
//!
//! Before the copy carried a token of its own, that answer was a transport
//! failure: the engine fell back to streaming the file, and the streamed
//! write's no-overwrite check then refused the name the copy itself had just
//! taken, so the operation stopped with `DestinationExists` on a fresh key
//! (live, Hetzner, one 1,005-object rename in six, 2026-10-02).

use std::future::Future;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::time::Duration;

use cmdr_fs::volume::{ServerCopyProgress, Volume, VolumeError, WriteMode};

use super::fake_s3::FakeS3;
use crate::metadata::{MTIME_HEADER, WRITE_TOKEN_HEADER};

struct Straight;

impl ServerCopyProgress for Straight {
    fn advanced(&self, _done: u64, _total: u64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        Box::pin(async { ControlFlow::Continue(()) })
    }
}

fn source_meta() -> String {
    format!("{MTIME_HEADER}: 1354040105")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_one_request_copy_whose_answer_never_comes_reports_the_copy_it_published() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed_with_meta("src.txt", 1_000, &[&source_meta()]);
    s3.hang_up_after_commit();
    let volume = s3.volume();

    let outcome = volume
        .copy_on_server(
            &volume,
            &volume.root().join("src.txt"),
            &volume.root().join("dst.txt"),
            WriteMode::CreateNew,
            &Straight,
        )
        .await;

    assert_eq!(s3.object("dst.txt").map(|s| s.len), Some(1_000), "the copy landed");
    assert!(matches!(outcome, Ok(1_000)), "{outcome:?}");
}

/// The token that proves the landing costs nothing on the happy path: the
/// copy restates the source's metadata (its date kept) beside its own token,
/// in the same one request.
#[tokio::test(flavor = "multi_thread")]
async fn a_one_request_copy_keeps_the_sources_date_beside_its_own_token() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed_with_meta("src.txt", 10, &[&source_meta()]);
    let volume = s3.volume();

    let outcome = volume
        .copy_on_server(
            &volume,
            &volume.root().join("src.txt"),
            &volume.root().join("dst.txt"),
            WriteMode::CreateNew,
            &Straight,
        )
        .await;

    assert!(matches!(outcome, Ok(10)), "{outcome:?}");
    let meta = s3.object("dst.txt").expect("copied").meta;
    let lowered: Vec<String> = meta.iter().map(|line| line.to_ascii_lowercase()).collect();
    assert!(
        lowered
            .iter()
            .any(|line| line.starts_with(MTIME_HEADER) && line.ends_with("1354040105")),
        "{meta:?}"
    );
    assert!(
        lowered.iter().any(|line| line.starts_with(WRITE_TOKEN_HEADER)),
        "{meta:?}"
    );
}

/// ❗ The copy is pinned to the ETag its source HEAD saw
/// (`x-amz-copy-source-if-match`): a source another writer replaced in between
/// is `SourceChanged` and nothing lands, where an unpinned copy would publish
/// the new version under facts (size, date) read from the old one.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_replaced_after_its_head_is_source_changed_and_copies_nothing() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed_with_meta("src.txt", 10, &[&source_meta()]);
    s3.replace_after_head("src.txt");
    let volume = s3.volume();

    let outcome = volume
        .copy_on_server(
            &volume,
            &volume.root().join("src.txt"),
            &volume.root().join("dst.txt"),
            WriteMode::CreateNew,
            &Straight,
        )
        .await;

    assert!(matches!(outcome, Err(VolumeError::SourceChanged(_))), "{outcome:?}");
    assert!(s3.object("dst.txt").is_none(), "nothing landed");
}

/// R2's `412` names either condition (the pin or its no-overwrite header); a
/// source that still matches its pin means the destination is taken.
#[tokio::test(flavor = "multi_thread")]
async fn an_r2_copy_onto_a_taken_name_is_already_exists() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed_with_meta("src.txt", 10, &[&source_meta()]);
    s3.seed("dst.txt", 5);
    let volume = s3.volume();

    let outcome = volume
        .copy_on_server(
            &volume,
            &volume.root().join("src.txt"),
            &volume.root().join("dst.txt"),
            WriteMode::CreateNew,
            &Straight,
        )
        .await;

    assert!(matches!(outcome, Err(VolumeError::AlreadyExists(_))), "{outcome:?}");
    assert_eq!(s3.object("dst.txt").map(|s| s.len), Some(5), "theirs stays");
}
