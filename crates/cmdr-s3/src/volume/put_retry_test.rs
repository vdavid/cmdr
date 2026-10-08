//! A single PUT the server faults goes again, the way a part does, against a
//! fake that answers `500 InternalError`: B2 answered two of about 1,200 PUTs
//! that way in one live run (2026-10-03), and each failed a file of a copy
//! that a resend would have landed. The body is in memory, so the resend is
//! the same request; a fault that still published is the write, ❌ never a
//! name taken by someone else.

use std::ops::ControlFlow;
use std::time::Duration;

use cmdr_fs::volume::{Volume, VolumeError, VolumeReadStream, WriteMode};

use super::S3Volume;
use super::fake_s3::FakeS3;
use super::testing::BytesSource;

async fn write(volume: &S3Volume, key: &str, len: usize) -> Result<u64, VolumeError> {
    let source = BytesSource::new(vec![7u8; len]);
    let length = source.total_size();
    volume
        .write_from_stream(
            &volume.root().join(key),
            WriteMode::CreateNew,
            length,
            Box::new(source),
            &|_| ControlFlow::Continue(()),
        )
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_faulted_put_goes_again_and_lands() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.fault_puts(1);
    let volume = s3.volume();

    let outcome = write(&volume, "photo.jpg", 1_000).await;

    assert!(matches!(outcome, Ok(1_000)), "{outcome:?}");
    assert_eq!(s3.object("photo.jpg").map(|s| s.len), Some(1_000));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fault_after_the_publish_reports_the_write() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.fault_a_put_after_commit();
    let volume = s3.volume();

    // The resend's `If-None-Match: *` would meet our own object: the write
    // must find it ours and report it, not `AlreadyExists`.
    let outcome = write(&volume, "photo.jpg", 1_000).await;

    assert!(matches!(outcome, Ok(1_000)), "{outcome:?}");
    assert_eq!(s3.object("photo.jpg").map(|s| s.len), Some(1_000));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_put_still_faulted_after_its_retries_fails_and_stores_nothing() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.fault_puts(usize::MAX);
    let volume = s3.volume();

    let outcome = write(&volume, "photo.jpg", 1_000).await;

    assert!(outcome.is_err(), "{outcome:?}");
    assert!(s3.object("photo.jpg").is_none());
    assert_eq!(s3.puts(), 3, "the PUT went three times: once, then after 1 s and 2 s");
}
