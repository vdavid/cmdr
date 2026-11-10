//! Moving bytes over MTP in both directions: the bounded-window read stream the
//! backend hands out, and the adapter that feeds a `VolumeReadStream` into
//! mtp-rs on the way up.

use super::mapping::map_mtp_error;
use crate::connection::{MtpConnectionManager, MtpReadSession};
use cmdr_fs::volume::{StreamLength, VolumeError, VolumeReadStream};
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
pub fn volume_read_stream_to_chunk_stream<'a>(
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
                    // Typed, so a reader that turns it back into a `VolumeError` (`io_detail`) still marks
                    // its path.
                    let err = std::io::Error::other(e);
                    Some((Err(err), (stream, bytes_written, on_progress, total)))
                }
                None => None,
            }
        },
    )
}

/// Bytes-per-window for a [`MtpReadStream`]. Production uses the session layer's
/// `MTP_READ_WINDOW`; a test shrinks it through `testing::set_read_window` so a
/// small fixture spans several windows.
pub(super) fn mtp_read_window() -> u32 {
    #[cfg(any(test, feature = "testing"))]
    {
        let override_window = super::testing::read_window_override();
        if override_window != 0 {
            return override_window;
        }
    }
    crate::connection::MTP_READ_WINDOW
}

/// Bounded-window MTP read stream: about one `MTP_READ_WINDOW` per `next_chunk`.
///
/// On a phone, each window is one `GetPartialObject64` and the one-per-device PTP
/// session is free between them, so a foreground listing slips in at window
/// granularity (the whole point: navigate the phone during a copy). A device
/// without partial reads gets one held-open `GetObject`, and the session keeps
/// the device until EOF or a cancel.
///
/// `next_chunk` delegates to the connection layer's `read_next_window`, which
/// owns the device lock and both paths; the window bookkeeping lives in mtp-rs's
/// `FileDownload` inside the cached [`MtpReadSession`]. This struct just relays
/// windows and reports progress.
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

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.session.total_size())
    }

    fn bytes_read(&self) -> u64 {
        self.session.bytes_read()
    }

    /// Between windows this costs nothing. Mid-transfer (a held-open read on a
    /// device without partial reads) it stops the transfer and gives the device
    /// back before returning; a plain drop does the same on the runtime
    /// (`MtpReadSession`'s `Drop`).
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn cancel_and_release(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(self.session.cancel())
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        self.session.modified_at()
    }
}
