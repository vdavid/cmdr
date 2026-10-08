//! Pause against a fake S3: no upload request moves bytes while the operation
//! is paused, whether the pause lands before a request goes out or in the
//! middle of its body, for one PUT and for parts alike, and Resume finishes the
//! object whole. The pause reaches the volume through the source stream's
//! `stop_signal`, the way the transfer engine hands it down.
//!
//! The case that was broken: a local source drains into part buffers in
//! milliseconds, so the engine's pause, which parks the SOURCE, never reached
//! the bytes already read, and the upload ran to its end while "paused".

use std::ops::ControlFlow;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::scan_stop::TestScanStop;
use cmdr_fs::volume::{ScanStop, ScanStopSignal, StreamLength, Volume, VolumeError, VolumeReadStream, WriteMode};
use tokio::sync::Notify;

use super::S3Volume;
use super::fake_s3::FakeS3;
use super::testing::BytesSource;

const MIB: usize = 1024 * 1024;

/// How long a cell watches a paused upload for bytes that shouldn't move.
const WATCH: Duration = Duration::from_millis(800);

/// `bytes` as a source the operation `signal` can pause, which raises that
/// pause itself as it hands out its last piece when `pause_at_end` is set: the
/// moment a fast local source has just drained into the upload's buffers.
struct PausableSource {
    inner: BytesSource,
    signal: Arc<TestScanStop>,
    pause_at_end: bool,
    left: usize,
}

impl PausableSource {
    fn new(bytes: Vec<u8>, signal: &Arc<TestScanStop>, pause_at_end: bool) -> Self {
        Self {
            left: bytes.len(),
            inner: BytesSource::new(bytes),
            signal: Arc::clone(signal),
            pause_at_end,
        }
    }
}

impl VolumeReadStream for PausableSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let piece = self.inner.next_chunk().await;
            if let Some(Ok(bytes)) = &piece {
                self.left -= bytes.len();
                if self.left == 0 && self.pause_at_end {
                    self.signal.pause();
                }
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

    fn stop_signal(&self) -> ScanStop {
        ScanStop::new(Arc::clone(&self.signal) as Arc<dyn ScanStopSignal>)
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
    }
}

fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

/// Pauses the operation once progress passes `past` bytes, and says so on
/// `raised`.
struct PauseAt {
    past: u64,
    raised: Arc<Notify>,
}

impl PauseAt {
    fn past(past: usize) -> Self {
        Self {
            past: past as u64,
            raised: Arc::default(),
        }
    }
}

/// Starts a write of `source` to `key`, pausing the operation at `pause_at`
/// (`None`: the source pauses it, or nobody does), and Cancelling once
/// `cancel` is set.
fn start_write(
    volume: &Arc<S3Volume>,
    key: &str,
    source: PausableSource,
    pause_at: Option<&PauseAt>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> tokio::task::JoinHandle<Result<u64, VolumeError>> {
    let volume = Arc::clone(volume);
    let path = volume.root().join(key);
    let signal = Arc::clone(&source.signal);
    let pause_at = pause_at.map(|at| (at.past, Arc::clone(&at.raised)));
    let paused_once = std::sync::atomic::AtomicBool::new(false);
    tokio::spawn(async move {
        let length = source.total_size();
        volume
            .write_from_stream(&path, WriteMode::CreateNew, length, Box::new(source), &|progress| {
                if let Some((past, raised)) = &pause_at
                    && progress.bytes_written > *past
                    && !paused_once.swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    signal.pause();
                    raised.notify_one();
                }
                if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .await
    })
}

/// Waits until the fake's body bytes stop moving for a while, answering the
/// count they settled at; panics if they never do.
async fn bytes_settle(s3: &FakeS3) -> usize {
    let mut last = s3.body_bytes();
    for _ in 0..40 {
        // allowed-test-sleep: "no bytes arrive for a stretch" is the subject;
        // there is no event to wait on for something that must NOT happen.
        tokio::time::sleep(Duration::from_millis(250)).await;
        let now = s3.body_bytes();
        if now == last {
            return now;
        }
        last = now;
    }
    panic!("the upload's bytes kept arriving while paused");
}

/// Asserts that a paused write is still running and moves no bytes for
/// [`WATCH`].
async fn assert_stands_still(s3: &FakeS3, write: &tokio::task::JoinHandle<Result<u64, VolumeError>>, what: &str) {
    let settled = bytes_settle(s3).await;
    // allowed-test-sleep: the stretch of nothing IS the assertion.
    tokio::time::sleep(WATCH).await;
    assert!(!write.is_finished(), "{what}: the upload ran to its end while paused");
    assert_eq!(s3.body_bytes(), settled, "{what}: bytes moved while paused");
}

#[tokio::test(flavor = "multi_thread")]
async fn parts_wait_while_paused_and_finish_on_resume() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let volume = Arc::new(s3.volume());
    volume.set_part_floor(5 * MIB as u64);
    let bytes = pattern(20 * MIB);
    let signal = TestScanStop::new();
    let source = PausableSource::new(bytes.clone(), &signal, true);
    let write = start_write(&volume, "parts.bin", source, None, Arc::default());

    assert_stands_still(&s3, &write, "four parts read and paused").await;
    assert!(s3.object("parts.bin").is_none(), "nothing is published while paused");

    signal.resume();
    let written = write.await.expect("the write task ends");
    assert!(matches!(written, Ok(n) if n == bytes.len() as u64), "{written:?}");
    assert_eq!(s3.object("parts.bin").map(|s| s.len), Some(bytes.len()));
    assert_eq!(s3.open_uploads(), 0, "the upload completed");
}

#[tokio::test(flavor = "multi_thread")]
async fn parts_paused_mid_body_are_set_aside_and_sent_again_on_resume() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.read_at(2 * MIB);
    let volume = Arc::new(s3.volume());
    // Parts well past what the loopback socket buffers take in at once, so a
    // pause can land while their bodies are still going out.
    volume.set_part_floor(16 * MIB as u64);
    volume.set_pause_hold(Duration::from_millis(200));
    let bytes = pattern(64 * MIB);
    let signal = TestScanStop::new();
    let source = PausableSource::new(bytes.clone(), &signal, false);
    let pause_at = PauseAt::past(2 * MIB);
    let write = start_write(&volume, "parts.bin", source, Some(&pause_at), Arc::default());

    // Once the pause is up, drain what already sits in the socket buffers at
    // once (Linux's hold many MiB): it's the client that must stop sending,
    // and a fast reader shows any byte it still sends sooner.
    pause_at.raised.notified().await;
    s3.read_freely();
    assert_stands_still(&s3, &write, "parts paused mid-body").await;
    assert_eq!(
        s3.puts(),
        0,
        "no part the pause caught mid-body went on to arrive whole"
    );
    assert!(s3.object("parts.bin").is_none(), "nothing is published while paused");

    signal.resume();
    let written = write.await.expect("the write task ends");
    assert!(matches!(written, Ok(n) if n == bytes.len() as u64), "{written:?}");
    assert_eq!(s3.object("parts.bin").map(|s| s.len), Some(bytes.len()));
    assert_eq!(s3.open_uploads(), 0, "the upload completed");
    assert!(
        s3.body_bytes() > bytes.len(),
        "the parts the pause outlasted went again ({} bytes arrived)",
        s3.body_bytes()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_put_waits_while_paused_and_lands_on_resume() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let volume = Arc::new(s3.volume());
    let bytes = pattern(3 * MIB);
    let signal = TestScanStop::new();
    let source = PausableSource::new(bytes.clone(), &signal, true);
    let write = start_write(&volume, "one.bin", source, None, Arc::default());

    assert_stands_still(&s3, &write, "one PUT read and paused").await;
    assert!(s3.object("one.bin").is_none(), "nothing is published while paused");

    signal.resume();
    let written = write.await.expect("the write task ends");
    assert!(matches!(written, Ok(n) if n == bytes.len() as u64), "{written:?}");
    assert_eq!(s3.object("one.bin").map(|s| s.len), Some(bytes.len()));
}

/// The PUT a pause outlasted is set aside, and Resume sends the whole body
/// again. ❗ What a VersityGW-like server kept of the cut-off body carries our
/// token and is removed before the resend: left there, it would make the
/// resend's own `If-None-Match: *` refuse the name as taken.
#[tokio::test(flavor = "multi_thread")]
async fn a_put_paused_mid_body_is_set_aside_and_sent_again_whole() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.read_at(2 * MIB);
    s3.keep_cut_off_bodies();
    let volume = Arc::new(s3.volume());
    volume.set_pause_hold(Duration::from_millis(200));
    // One PUT, well past what the loopback socket buffers take in at once.
    let bytes = pattern(40 * MIB);
    let signal = TestScanStop::new();
    let source = PausableSource::new(bytes.clone(), &signal, false);
    let pause_at = PauseAt::past(2 * MIB);
    let write = start_write(&volume, "one.bin", source, Some(&pause_at), Arc::default());

    // Fast from the pause on, as in the parts cell.
    pause_at.raised.notified().await;
    s3.read_freely();
    assert_stands_still(&s3, &write, "one PUT paused mid-body").await;
    assert!(
        s3.object("one.bin").is_none_or(|stored| stored.len < bytes.len()),
        "nothing whole is published while paused"
    );

    signal.resume();
    let written = write.await.expect("the write task ends");
    assert!(matches!(written, Ok(n) if n == bytes.len() as u64), "{written:?}");
    assert_eq!(s3.object("one.bin").map(|s| s.len), Some(bytes.len()));
    assert_eq!(s3.puts(), 1, "only the whole resend arrived whole");
    assert!(s3.body_bytes() > bytes.len(), "the PUT the pause outlasted went again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_while_paused_ends_the_upload_and_leaves_nothing() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let volume = Arc::new(s3.volume());
    volume.set_part_floor(5 * MIB as u64);
    let signal = TestScanStop::new();
    let source = PausableSource::new(pattern(20 * MIB), &signal, true);
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let write = start_write(&volume, "parts.bin", source, None, Arc::clone(&cancel));

    assert_stands_still(&s3, &write, "paused before the cancel").await;
    cancel.store(true, std::sync::atomic::Ordering::SeqCst);
    signal.stop();
    let written = tokio::time::timeout(Duration::from_secs(5), write)
        .await
        .expect("a cancel ends a paused upload promptly")
        .expect("the write task ends");
    assert!(matches!(written, Err(VolumeError::Cancelled(_))), "{written:?}");
    assert!(s3.object("parts.bin").is_none());
    assert_eq!(s3.open_uploads(), 0, "the cancelled upload is aborted");
}
