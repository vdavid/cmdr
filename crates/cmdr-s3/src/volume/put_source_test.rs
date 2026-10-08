//! A single PUT's source that breaks its promise, against a fake S3: one that
//! ends short, runs long, or fails is caught while the body fills, so not a
//! byte goes out and nothing can be published under the user's name.

use std::ops::ControlFlow;
use std::pin::Pin;
use std::time::Duration;

use cmdr_fs::volume::{StreamLength, Volume, VolumeError, VolumeReadStream, WriteMode};

use super::fake_s3::FakeS3;

const MIB: usize = 1024 * 1024;

const SOURCE_ERRNO: i32 = 5;

/// Yields `actual` bytes in 1 MiB pieces while promising `promised`, then
/// fails instead of ending when `fail` is set.
struct LyingSource {
    promised: u64,
    left: usize,
    fail: bool,
}

impl VolumeReadStream for LyingSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.left == 0 {
                if self.fail {
                    self.fail = false;
                    return Some(Err(VolumeError::IoError {
                        message: "the disk went away".to_string(),
                        // An errno no S3 failure carries, so the error is
                        // recognizably the source's own.
                        raw_os_error: Some(SOURCE_ERRNO),
                    }));
                }
                return None;
            }
            let piece = self.left.min(MIB);
            self.left -= piece;
            Some(Ok(vec![5u8; piece]))
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.promised)
    }

    fn bytes_read(&self) -> u64 {
        0
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
    }
}

async fn put(promised: usize, actual: usize, fail: bool) -> (FakeS3, Result<u64, VolumeError>) {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let volume = s3.volume();
    let source = LyingSource {
        promised: promised as u64,
        left: actual,
        fail,
    };
    let written = volume
        .write_from_stream(
            &volume.root().join("one.bin"),
            WriteMode::CreateNew,
            StreamLength::Known(promised as u64),
            Box::new(source),
            &|_| ControlFlow::Continue(()),
        )
        .await;
    (s3, written)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_source_longer_than_promised_sends_nothing() {
    let (s3, written) = put(3 * MIB, 3 * MIB + 1, false).await;
    assert!(matches!(written, Err(VolumeError::IoError { .. })), "{written:?}");
    assert_eq!(s3.body_bytes(), 0, "not a byte went out");
    assert!(s3.object("one.bin").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_source_shorter_than_promised_sends_nothing() {
    let (s3, written) = put(3 * MIB, 2 * MIB, false).await;
    assert!(matches!(written, Err(VolumeError::IoError { .. })), "{written:?}");
    assert_eq!(s3.body_bytes(), 0, "not a byte went out");
    assert!(s3.object("one.bin").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_source_ends_the_write_with_its_own_error() {
    let (s3, written) = put(3 * MIB, MIB, true).await;
    assert!(
        matches!(
            &written,
            Err(VolumeError::IoError {
                raw_os_error: Some(SOURCE_ERRNO),
                ..
            })
        ),
        "{written:?}"
    );
    assert_eq!(s3.body_bytes(), 0, "not a byte went out");
    assert!(s3.object("one.bin").is_none());
}
