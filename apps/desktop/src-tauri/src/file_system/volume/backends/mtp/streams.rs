//! Moving bytes over MTP in both directions: the bounded-window read stream the
//! backend hands out, and the adapter that feeds a `VolumeReadStream` into
//! mtp-rs on the way up.

use super::mapping::map_mtp_error;
use super::{VolumeError, VolumeReadStream};
use crate::mtp::connection::{MtpConnectionManager, MtpReadSession};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Adapts a `VolumeReadStream` into a `futures::Stream` that mtp-rs can
/// consume lazily, calling `on_progress` after each chunk and surfacing
/// `ControlFlow::Break` as an `io::Error` so the upload unwinds promptly.
///
/// The laziness is load-bearing: collecting every chunk into a `Vec<Bytes>`
/// before the first USB write risks OOM on a large file, and skips the transfer
/// progress / cancel callback entirely.
pub(in crate::file_system::volume::backends) fn volume_read_stream_to_chunk_stream<'a>(
    stream: Box<dyn VolumeReadStream>,
    total: u64,
    on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
) -> impl futures_util::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + 'a {
    futures_util::stream::unfold(
        (stream, 0u64, on_progress, total),
        |(mut stream, bytes_written, on_progress, total)| async move {
            match stream.next_chunk().await {
                Some(Ok(chunk)) => {
                    let new_total = bytes_written + chunk.len() as u64;
                    if on_progress(new_total, total) == std::ops::ControlFlow::Break(()) {
                        let err = std::io::Error::new(std::io::ErrorKind::Interrupted, "Operation cancelled");
                        return Some((Err(err), (stream, new_total, on_progress, total)));
                    }
                    Some((Ok(bytes::Bytes::from(chunk)), (stream, new_total, on_progress, total)))
                }
                Some(Err(e)) => {
                    let err = std::io::Error::other(e.to_string());
                    Some((Err(err), (stream, bytes_written, on_progress, total)))
                }
                None => None,
            }
        },
    )
}

/// Bytes-per-window for a [`MtpReadStream`]. Production uses
/// [`crate::mtp::connection::MTP_READ_WINDOW`]; tests shrink it via
/// `test_window` so a small fixture spans multiple windows.
pub(super) fn mtp_read_window() -> u32 {
    #[cfg(test)]
    {
        let o = test_window::get();
        if o != 0 {
            return o;
        }
    }
    crate::mtp::connection::MTP_READ_WINDOW
}

/// Bounded-window MTP read stream.
///
/// Reads a file as a sequence of bounded `GetPartialObject64` windows instead of
/// one held-open `GetObject`. Between windows nothing is in flight and the
/// one-per-device PTP session is free, so a foreground listing slips in at window
/// granularity (the whole point — navigate the phone during a copy).
///
/// `next_chunk` delegates to the connection layer's `read_next_window`, which
/// takes the per-device lock for each `GetPartialObject64`. The window
/// bookkeeping (total size, offset, clamp-to-remaining, EOF, advance-by-returned-
/// length, the 0-byte-before-EOF stall guard) lives in mtp-rs's `WindowedDownload`
/// inside the cached [`MtpReadSession`]; this struct just relays windows and
/// reports progress.
pub(super) struct MtpReadStream {
    pub(super) manager: Arc<MtpConnectionManager>,
    pub(super) session: MtpReadSession,
    pub(super) device_id: String,
}

impl VolumeReadStream for MtpReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            match self.manager.read_next_window(&mut self.session, &self.device_id).await {
                Ok(Some(bytes)) => Some(Ok(bytes)),
                Ok(None) => None,
                Err(e) => Some(Err(map_mtp_error(e))),
            }
        })
    }

    fn total_size(&self) -> u64 {
        self.session.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.session.bytes_read()
    }

    // `cancel_and_release` uses the trait default (no-op): bounded windows hold
    // nothing between reads, so there's no in-flight transaction to abort. A
    // window read in flight when the stream is dropped self-heals via mtp-rs's
    // `TransactionScope` (see the connection layer's `read_next_window`).
}

/// Test-only override for the read window size (see [`mtp_read_window`]). A
/// global is harmless here: every read test wants a small window, and the
/// production default is never asserted, so a value set by one test never
/// breaks another. Unit tests construct [`MtpReadStream`] with an explicit
/// `window` instead and don't touch this.
// `set` is widened to `backends` so the sibling `mtp_test` module (a child of
// `backends`) can reach it; `get` stays module-local because only the in-file
// `mtp_read_window` reads it.
#[cfg(test)]
pub(in crate::file_system::volume::backends) mod test_window {
    use std::sync::atomic::{AtomicU32, Ordering};

    static OVERRIDE: AtomicU32 = AtomicU32::new(0);

    pub(super) fn get() -> u32 {
        OVERRIDE.load(Ordering::Relaxed)
    }

    #[cfg(feature = "virtual-mtp")]
    pub(in crate::file_system::volume::backends) fn set(window: u32) {
        OVERRIDE.store(window, Ordering::Relaxed);
    }
}
