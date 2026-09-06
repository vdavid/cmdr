//! The `VolumeReadStream` → chunk-stream adapter the upload path feeds `mtp-rs`.
//!
//! Both cells are regression anchors on the same audit finding: the adapter is
//! what makes an upload STREAM (rather than drain the whole source into memory
//! first) and what makes a cancel reach the USB write (rather than only the loop
//! above it). Neither needs a device — the source is a double and the sink is the
//! stream itself.

use std::future::Future;
use std::pin::Pin;

use cmdr_fs::volume::{VolumeError, VolumeReadStream};

use crate::volume::volume_read_stream_to_chunk_stream;

/// Regression for the high-severity audit finding: pre-fix, MtpVolume's
/// `write_from_stream` was named `_on_progress` (signaling unused) and
/// drained the entire source into a `Vec<Bytes>` before any USB write.
/// Both behaviors are tested via the extracted stream adapter helper,
/// which is what `write_from_stream` now drives.
#[tokio::test]
async fn volume_read_stream_to_chunk_stream_calls_on_progress_per_chunk() {
    use futures_util::StreamExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct MockStream {
        chunks: std::vec::IntoIter<Vec<u8>>,
        total: u64,
        read: u64,
    }
    impl VolumeReadStream for MockStream {
        fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
            Box::pin(async move {
                self.chunks.next().map(|c| {
                    self.read += c.len() as u64;
                    Ok(c)
                })
            })
        }
        fn total_size(&self) -> u64 {
            self.total
        }
        fn bytes_read(&self) -> u64 {
            self.read
        }
    }

    let chunks = vec![vec![0u8; 64], vec![0u8; 64], vec![0u8; 64], vec![0u8; 64]];
    let total: u64 = chunks.iter().map(|c| c.len() as u64).sum();
    let stream = Box::new(MockStream {
        chunks: chunks.into_iter(),
        total,
        read: 0,
    });

    let calls = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&calls);
    let on_progress = move |_bytes, _total| {
        counter.fetch_add(1, Ordering::SeqCst);
        std::ops::ControlFlow::Continue(())
    };

    let mut adapter = Box::pin(volume_read_stream_to_chunk_stream(stream, total, &on_progress));
    let mut emitted = 0u64;
    while let Some(chunk) = adapter.next().await {
        emitted += chunk.expect("chunk should be Ok").len() as u64;
    }

    assert_eq!(emitted, total, "all bytes should be forwarded to the upload");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        4,
        "on_progress must fire once per chunk (4 chunks emitted)"
    );
}

/// Companion regression: `ControlFlow::Break(())` must unwind the upload
/// promptly. Pre-fix, the callback was never invoked, so a Cancel could
/// only stop the loop *above* the upload — not the upload itself.
#[tokio::test]
async fn volume_read_stream_to_chunk_stream_surfaces_cancellation() {
    use futures_util::StreamExt;

    struct InfiniteStream {
        total: u64,
        read: u64,
    }
    impl VolumeReadStream for InfiniteStream {
        fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
            Box::pin(async move {
                self.read += 64;
                Some(Ok(vec![0u8; 64]))
            })
        }
        fn total_size(&self) -> u64 {
            self.total
        }
        fn bytes_read(&self) -> u64 {
            self.read
        }
    }

    let stream = Box::new(InfiniteStream {
        total: u64::MAX,
        read: 0,
    });
    let on_progress = |_bytes, _total| std::ops::ControlFlow::Break(());

    let mut adapter = Box::pin(volume_read_stream_to_chunk_stream(stream, u64::MAX, &on_progress));
    let first = adapter.next().await.expect("adapter should yield once");
    assert!(first.is_err(), "Break(()) must produce an io::Error item");
    assert_eq!(
        first.unwrap_err().kind(),
        std::io::ErrorKind::Interrupted,
        "cancellation must surface as Interrupted"
    );
}
