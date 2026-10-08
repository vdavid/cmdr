//! A write into a folder the operation itself just created
//! (`WriteMode::CreateNewInFreshFolder`) skips the no-overwrite HEAD a
//! check-then-write provider otherwise sends before each object: the folder's
//! creation proved it empty. Counted over `fake_s3.rs` on Wasabi's preset,
//! which trusts no conditional header at all.

use std::future::Future;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::time::Duration;

use cmdr_fs::volume::{ServerCopyProgress, Volume, VolumeReadStream, WriteMode};

use super::fake_s3::FakeS3;
use super::testing::BytesSource;
use crate::params::S3Provider;

struct Straight;

impl ServerCopyProgress for Straight {
    fn advanced(&self, _done: u64, _total: u64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        Box::pin(async { ControlFlow::Continue(()) })
    }
}

fn wasabi() -> S3Provider {
    S3Provider::Wasabi {
        region: "eu-central-1".into(),
    }
}

async fn write(s3: &FakeS3, key: &str, mode: WriteMode) {
    let volume = s3.volume_for(wasabi());
    let source = BytesSource::new(vec![7; 100]);
    let length = source.total_size();
    let written = volume
        .write_from_stream(&volume.root().join(key), mode, length, Box::new(source), &|_| {
            ControlFlow::Continue(())
        })
        .await;
    assert!(matches!(written, Ok(100)), "{written:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upload_into_a_fresh_folder_sends_no_no_overwrite_head() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    write(&s3, "checked.bin", WriteMode::CreateNew).await;
    write(&s3, "fresh/unchecked.bin", WriteMode::CreateNewInFreshFolder).await;

    // HEAD before, PUT, verifying HEAD; then the same minus the first HEAD.
    assert_eq!(s3.requests_about("checked.bin"), 3);
    assert_eq!(s3.requests_about("fresh/unchecked.bin"), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_copy_into_a_fresh_folder_sends_no_head_at_its_destination() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed("src.bin", 100);
    let volume = s3.volume_for(wasabi());
    for (key, mode) in [
        ("checked.bin", WriteMode::CreateNew),
        ("fresh/unchecked.bin", WriteMode::CreateNewInFreshFolder),
    ] {
        let copied = volume
            .copy_on_server(
                &volume,
                &volume.root().join("src.bin"),
                &volume.root().join(key),
                mode,
                &Straight,
            )
            .await;
        assert!(matches!(copied, Ok(100)), "{copied:?}");
    }

    // HEAD before, the copy, verifying HEAD; then the copy alone: the
    // folder's creation proved what both HEADs would ask.
    assert_eq!(s3.requests_about("checked.bin"), 3);
    assert_eq!(s3.requests_about("fresh/unchecked.bin"), 1);
}
