//! A write whose server published while the answer was lost, against a fake S3
//! that commits the object and then answers slowly (R2 did, live, 2026-10-02:
//! `live_hostile_cancel_uploads`). The publish can't be taken back by then, so
//! the write must report the file it finished, ❌ never `Cancelled` over an
//! object that's already replaced, and ❌ never remove it as a "cut-off" PUT.

use std::future::Future;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::time::Duration;

use cmdr_fs::volume::{ServerCopyProgress, Volume, VolumeError, VolumeReadStream, WriteMode};

use super::S3Volume;
use super::fake_s3::FakeS3;
use super::testing::BytesSource;

const MIB: usize = 1024 * 1024;

/// Writes `len` bytes to `key`, asking Cancel once every byte was handed over.
async fn write_cancelled_at_the_end(
    volume: &S3Volume,
    key: &str,
    mode: WriteMode,
    len: usize,
) -> Result<u64, VolumeError> {
    let source = BytesSource::new(vec![7u8; len]);
    let length = source.total_size();
    let total = len as u64;
    volume
        .write_from_stream(&volume.root().join(key), mode, length, Box::new(source), &|progress| {
            if progress.bytes_written >= total {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_after_the_whole_body_went_out_keeps_the_overwrite_it_finished() {
    let s3 = FakeS3::start(Duration::from_millis(700)).await;
    s3.seed("kept.bin", 32);
    let volume = s3.volume();
    let outcome = write_cancelled_at_the_end(&volume, "kept.bin", WriteMode::CreateOrReplace, 2 * MIB).await;
    let stored = s3.object("kept.bin");
    // Pre-fix: `Cancelled`, and the "cut-off" cleanup found our token on the
    // finished object and deleted it, so neither the original nor the new
    // bytes survived.
    assert_eq!(
        stored.map(|s| s.len),
        Some(2 * MIB),
        "the finished overwrite stays ({outcome:?})"
    );
    assert!(
        matches!(outcome, Ok(n) if n == (2 * MIB) as u64),
        "a publish that can't be taken back reports the file: {outcome:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_after_the_whole_body_went_out_keeps_the_new_file_it_finished() {
    let s3 = FakeS3::start(Duration::from_millis(700)).await;
    let volume = s3.volume();
    let outcome = write_cancelled_at_the_end(&volume, "fresh.bin", WriteMode::CreateOrReplace, 2 * MIB).await;
    assert!(matches!(outcome, Ok(n) if n == (2 * MIB) as u64), "{outcome:?}");
    assert_eq!(s3.object("fresh.bin").map(|s| s.len), Some(2 * MIB));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_before_the_last_piece_still_publishes_nothing() {
    let s3 = FakeS3::start(Duration::from_millis(700)).await;
    s3.seed("kept.bin", 32);
    let volume = s3.volume();
    let source = BytesSource::new(vec![7u8; 2 * MIB]);
    let length = source.total_size();
    let outcome = volume
        .write_from_stream(
            &volume.root().join("kept.bin"),
            WriteMode::CreateOrReplace,
            length,
            Box::new(source),
            &|progress| {
                if progress.bytes_written >= MIB as u64 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        )
        .await;
    assert!(matches!(outcome, Err(VolumeError::Cancelled(_))), "{outcome:?}");
    let stored = s3.object("kept.bin").expect("the original stays");
    assert_eq!((stored.len, stored.etag.as_str()), (32, "\"original\""));
}

/// Writes `source` to `key` with nobody asking to stop.
async fn write_through(volume: &S3Volume, key: &str, mode: WriteMode, source: BytesSource) -> Result<u64, VolumeError> {
    let length = source.total_size();
    volume
        .write_from_stream(&volume.root().join(key), mode, length, Box::new(source), &|_| {
            ControlFlow::Continue(())
        })
        .await
}

/// ❗ The link dies right after the server published a PUT: what's at the key
/// is our whole new object, so the write reports it. Pre-fix the cut-off
/// cleanup found our token on it and deleted it, losing the original and the
/// new bytes alike.
#[tokio::test(flavor = "multi_thread")]
async fn a_put_whose_answer_never_comes_keeps_the_overwrite_it_finished() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed("kept.bin", 32);
    s3.hang_up_after_commit();
    let volume = s3.volume();
    let outcome = write_through(
        &volume,
        "kept.bin",
        WriteMode::CreateOrReplace,
        BytesSource::new(vec![7u8; 2 * MIB]),
    )
    .await;
    assert_eq!(
        s3.object("kept.bin").map(|s| s.len),
        Some(2 * MIB),
        "the published overwrite stays ({outcome:?})"
    );
    assert!(matches!(outcome, Ok(n) if n == (2 * MIB) as u64), "{outcome:?}");
}

/// The same for a short stream of unknown length, which goes as one buffered PUT.
#[tokio::test(flavor = "multi_thread")]
async fn a_buffered_put_whose_answer_never_comes_keeps_the_overwrite_it_finished() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed("kept.bin", 32);
    s3.hang_up_after_commit();
    let volume = s3.volume();
    let source = BytesSource::new(vec![7u8; MIB]).of_unknown_length();
    let outcome = write_through(&volume, "kept.bin", WriteMode::CreateOrReplace, source).await;
    assert_eq!(
        s3.object("kept.bin").map(|s| s.len),
        Some(MIB),
        "the published overwrite stays ({outcome:?})"
    );
    assert!(matches!(outcome, Ok(n) if n == MIB as u64), "{outcome:?}");
}

/// ❗ The link dies right after the server completed a multipart upload: the
/// object landed whole, so the write reports it, and nothing aborts or deletes
/// its way into losing it.
#[tokio::test(flavor = "multi_thread")]
async fn a_completion_whose_answer_never_comes_reports_the_file_it_published() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed("big.bin", 32);
    s3.hang_up_after_commit();
    let volume = s3.volume();
    volume.set_part_floor((5 * MIB) as u64);
    let outcome = write_through(
        &volume,
        "big.bin",
        WriteMode::CreateOrReplace,
        BytesSource::new(vec![7u8; 11 * MIB]),
    )
    .await;
    assert_eq!(
        s3.object("big.bin").map(|s| s.len),
        Some(11 * MIB),
        "the published object stays ({outcome:?})"
    );
    assert_eq!(s3.open_uploads(), 0);
    assert!(matches!(outcome, Ok(n) if n == (11 * MIB) as u64), "{outcome:?}");
}

/// A body genuinely cut short on a server that keeps what arrived (VersityGW
/// does): that object is ours and truncated, so it still goes.
#[tokio::test(flavor = "multi_thread")]
async fn a_truncated_body_a_server_kept_is_still_removed() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.keep_cut_off_bodies();
    let volume = s3.volume();
    let source = BytesSource::new(vec![7u8; 4 * MIB]);
    let length = source.total_size();
    let outcome = volume
        .write_from_stream(
            &volume.root().join("cut.bin"),
            WriteMode::CreateNew,
            length,
            Box::new(source),
            &|progress| {
                if progress.bytes_written >= MIB as u64 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        )
        .await;
    assert!(matches!(outcome, Err(VolumeError::Cancelled(_))), "{outcome:?}");
    assert!(s3.object("cut.bin").is_none(), "the truncated object is removed");
}

/// A server-side copy's hook that never pauses or cancels.
struct Straight;

impl ServerCopyProgress for Straight {
    fn advanced(&self, _done: u64, _total: u64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        Box::pin(async { ControlFlow::Continue(()) })
    }
}

async fn copy_in_parts(s3: &FakeS3) -> Result<u64, VolumeError> {
    let volume = s3.volume();
    volume.set_part_floor((5 * MIB) as u64);
    volume
        .copy_on_server(
            &volume,
            &volume.root().join("source.bin"),
            &volume.root().join("copy.bin"),
            WriteMode::CreateNew,
            &Straight,
        )
        .await
}

/// ❗ The link dies right after the server completed a server-side copy in
/// parts: the copy carries this write's token in its creation metadata, so a
/// HEAD proves the destination is ours and whole, and the copy reports it.
/// Pre-fix it reported a failure for a copy that landed (safe, since a move
/// keeps its source, but false).
#[tokio::test(flavor = "multi_thread")]
async fn a_copy_whose_completion_answer_never_comes_reports_the_copy_it_published() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed("source.bin", 11 * MIB);
    s3.hang_up_after_commit();
    let outcome = copy_in_parts(&s3).await;
    assert_eq!(
        s3.object("copy.bin").map(|s| s.len),
        Some(11 * MIB),
        "the copy landed ({outcome:?})"
    );
    assert_eq!(s3.open_uploads(), 0);
    assert!(matches!(outcome, Ok(n) if n == (11 * MIB) as u64), "{outcome:?}");
}

/// The other side of that rule: a completion that never ran publishes nothing,
/// so the copy reports the failure and leaves no upload behind.
#[tokio::test(flavor = "multi_thread")]
async fn a_copy_whose_completion_never_ran_reports_the_failure() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed("source.bin", 11 * MIB);
    s3.hang_up_before_completing();
    let outcome = copy_in_parts(&s3).await;
    assert!(outcome.is_err(), "{outcome:?}");
    assert!(s3.object("copy.bin").is_none());
    assert_eq!(s3.open_uploads(), 0, "the failed copy's upload is aborted");
}
