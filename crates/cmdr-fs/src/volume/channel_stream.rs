//! The consumer half every network backend's read path lands on: a bounded
//! channel a background producer fills.

use std::pin::Pin;

use super::{StreamLength, VolumeError, VolumeReadStream};
use crate::metadata::{CarriedMetadata, MetadataClasses};

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
    total_size: StreamLength,
    bytes_read: u64,
    modified_at: Option<std::time::SystemTime>,
    metadata: CarriedMetadata,
    /// What `metadata_outline` answers when the open learned more classes than
    /// it read values for (an xattr's name without its value). `None`: the
    /// same as `metadata`.
    outline: Option<CarriedMetadata>,
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
        total_size: StreamLength,
    ) -> Self {
        Self {
            rx,
            cancel: Some(cancel),
            total_size,
            bytes_read: 0,
            modified_at: None,
            metadata: CarriedMetadata::default(),
            outline: None,
        }
    }

    /// The source file's date, for [`VolumeReadStream::modified_at`]. Pass what
    /// the open already learned (its stat or listing); `None` leaves the
    /// destination's own date.
    #[must_use]
    pub fn with_modified_at(mut self, modified_at: Option<std::time::SystemTime>) -> Self {
        self.modified_at = modified_at;
        self
    }

    /// The rest of what the source file carries, for
    /// [`VolumeReadStream::metadata`]. Pass what the open already learned;
    /// the stream hands out the classes a destination asks for.
    #[must_use]
    pub fn with_metadata(mut self, metadata: CarriedMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// What the source file carries, as far as the open learned it without
    /// reading values, for [`VolumeReadStream::metadata_outline`]: an xattr's
    /// name with an empty value, say. ❗ Never what `metadata` hands out, so an
    /// empty value can't be written as a real one.
    #[must_use]
    pub fn with_metadata_outline(mut self, outline: CarriedMetadata) -> Self {
        self.outline = Some(outline);
        self
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

    fn total_size(&self) -> StreamLength {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        self.modified_at
    }

    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        self.metadata.clone().restricted_to(wanted)
    }

    fn metadata_outline(&self, wanted: MetadataClasses) -> CarriedMetadata {
        self.outline
            .as_ref()
            .unwrap_or(&self.metadata)
            .clone()
            .restricted_to(wanted)
    }
}
