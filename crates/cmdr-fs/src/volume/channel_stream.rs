//! The consumer half every network backend's read path lands on: a bounded
//! channel a background producer fills.

use std::pin::Pin;

use super::{VolumeError, VolumeReadStream};

/// A [`VolumeReadStream`] a background producer feeds through a bounded channel.
///
/// The producer owns the connection and pushes chunks; this is the consumer, and
/// the channel's capacity is what bounds peak memory — a 10 GB file costs the
/// same as a 10 KB one. Both network backends build one: `SmbVolume` behind an
/// `smb2::FileDownload`, `SftpVolume` behind its read window.
///
/// ❗ **Dropping it cancels the producer**, which is how a cancelled copy stops
/// paying for reads nobody will use. Two signals do it, and either alone is
/// enough: `Drop` fires the cancel channel, and the dropped receiver makes the
/// producer's next send fail.
pub struct ChannelReadStream {
    rx: tokio::sync::mpsc::Receiver<Result<Vec<u8>, VolumeError>>,
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
    total_size: u64,
    bytes_read: u64,
}

impl ChannelReadStream {
    /// Wraps a producer's chunk channel, with the size it settled up front.
    ///
    /// ❗ `total_size` is known BEFORE the stream exists on purpose: the transfer
    /// layer draws a progress bar from it before the first chunk lands, so a
    /// producer reports the size through its own channel and the constructor
    /// waits for it.
    pub fn new(
        rx: tokio::sync::mpsc::Receiver<Result<Vec<u8>, VolumeError>>,
        cancel: tokio::sync::oneshot::Sender<()>,
        total_size: u64,
    ) -> Self {
        Self {
            rx,
            cancel: Some(cancel),
            total_size,
            bytes_read: 0,
        }
    }
}

impl Drop for ChannelReadStream {
    fn drop(&mut self) {
        if let Some(tx) = self.cancel.take() {
            // Best-effort: a producer that already finished has dropped the
            // receiving half, and the send is a no-op.
            let _ = tx.send(());
        }
    }
}

impl VolumeReadStream for ChannelReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let chunk = self.rx.recv().await?;
            if let Ok(ref bytes) = chunk {
                self.bytes_read += bytes.len() as u64;
            }
            Some(chunk)
        })
    }

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}
