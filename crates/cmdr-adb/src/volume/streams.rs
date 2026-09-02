//! The byte path: one `RECV` socket per file out, one `SEND` socket per file
//! in.
//!
//! The sync service streams a file as a sequence of `DATA` frames of at most
//! `MAX_DATA_CHUNK` bytes and ends it with `DONE`, so a read is exactly the
//! chunk shape [`ChannelReadStream`] wants and ❌ nothing here ever holds a
//! whole file. The producer runs on the host's runtime and stops on the first
//! of: the file ending, the consumer going away, or the stream's cancel.

use std::ops::ControlFlow;
use std::path::Path;

use cmdr_fs::staging::STAGING_TEMP_MARKER;
use cmdr_fs::volume::{ChannelReadStream, VolumeError, VolumeReadStream};
use log::debug;

use super::AdbVolume;
use super::paths::join_device_path;
use crate::errors::{ENOENT, volume_error_from_errno};
use crate::sync::{MAX_DATA_CHUNK, SyncEntryKind, SyncSession};

/// Chunks buffered between the producer and the consumer. Peak memory per
/// stream is `this * MAX_DATA_CHUNK`, regardless of file size.
const STREAM_CHANNEL_CAPACITY: usize = 4;

/// The mode a fresh file lands with: `rw-rw----`, what `adb push` uses.
const NEW_FILE_MODE: u32 = 0o100660;

impl AdbVolume {
    /// Opens `[offset, size)` of `path` for streaming.
    ///
    /// The size is settled by a `stat` before this returns, so the caller has
    /// an honest `total_size()` from its first progress tick. `RECV` has no
    /// offset of its own, so a non-zero `offset` is honored by discarding bytes
    /// as they arrive: correct, and the cost of a resume on this protocol.
    pub(super) async fn open_read_stream_impl(
        &self,
        path: &Path,
        offset: u64,
    ) -> Result<ChannelReadStream, VolumeError> {
        let device = self.to_device_path(path)?;
        let mut session = self.open_sync(&device).await?;
        let stat = session
            .stat(&device)
            .await
            .map_err(|e| self.inner.map_adb_error(e, &device))?;
        if !stat.exists() {
            session.quit().await;
            return Err(volume_error_from_errno(stat.errno.unwrap_or(ENOENT), &device));
        }
        if stat.kind() == SyncEntryKind::Directory {
            session.quit().await;
            return Err(VolumeError::IsADirectory(device));
        }
        let total_size = stat.size;

        let (chunk_tx, chunk_rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, VolumeError>>(STREAM_CHANNEL_CAPACITY);
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        let inner = std::sync::Arc::clone(&self.inner);
        // ❌ Never `tokio::spawn` here: a backend inherits whatever runtime it
        // is called on, and some of those have none.
        self.inner
            .host
            .runtime()
            .spawn(async move { produce(inner, session, device, offset, chunk_tx, cancel_rx).await });

        Ok(ChannelReadStream::new(chunk_rx, cancel_tx, total_size))
    }

    /// Streams `stream` onto `dest` through a staging sibling, then moves it
    /// into place.
    ///
    /// `SEND` writes the file at the name it is given, byte by byte, so the
    /// name holds a partial for the whole upload. It lands on
    /// `<name>.cmdr-tmp-<n>` beside the destination and takes the real name by
    /// one `mv -f`, so nothing half-written ever wears the user's filename; on
    /// any failure the staging name is removed. ❗ Cancellation arrives only
    /// through `on_progress` answering `Break`: there is no token on this path.
    pub(super) async fn write_from_stream_impl(
        &self,
        dest: &Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<u64, VolumeError> {
        let device = self.to_device_path(dest)?;
        let staging = staging_name_for(&device);
        debug!("AdbVolume::write_from_stream: {device} via {staging}, size={size}");

        let mut session = self.open_sync(&device).await?;
        let pumped = self
            .pump(&mut session, &staging, &device, size, &mut stream, on_progress)
            .await;
        session.quit().await;

        match pumped {
            Ok(written) => {
                if let Err(e) = self.shell_verb(&["mv", "-f", &staging, &device], &device).await {
                    self.remove_partial(&staging).await;
                    return Err(e);
                }
                self.notify_created(dest).await;
                Ok(written)
            }
            Err(e) => {
                self.remove_partial(&staging).await;
                Err(e)
            }
        }
    }

    /// The upload itself: `SEND`, one `DATA` frame per chunk, `DONE` with the
    /// mtime, progress after every chunk the device took.
    async fn pump(
        &self,
        session: &mut SyncSession,
        staging: &str,
        device: &str,
        size: u64,
        stream: &mut Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<u64, VolumeError> {
        let map = |e| self.inner.map_adb_error(e, device);
        session.send_start(staging, NEW_FILE_MODE).await.map_err(map)?;
        let mut written = 0u64;
        while let Some(chunk) = stream.next_chunk().await {
            let chunk = chunk?;
            for piece in chunk.chunks(MAX_DATA_CHUNK) {
                session.send_chunk(piece).await.map_err(map)?;
                written += piece.len() as u64;
            }
            if on_progress(written, size).is_break() {
                return Err(VolumeError::Cancelled(device.to_string()));
            }
        }
        let mtime = u32::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        )
        .unwrap_or(u32::MAX);
        session.send_finish(mtime).await.map_err(map)?;
        Ok(written)
    }

    /// Best-effort removal of a partial upload. A failure here must never
    /// replace the error that caused it.
    async fn remove_partial(&self, staging: &str) {
        if let Err(e) = crate::shell::run(&self.inner.endpoint, &self.inner.serial, &["rm", "-f", staging]).await {
            debug!("AdbVolume::write_from_stream: couldn't remove the partial {staging}: {e:?}");
        }
    }
}

/// `<dir>/<name>.cmdr-tmp-<pid>-<n>`: the house marker, so a leftover from a
/// crash is recognized as Cmdr's, and a counter so two uploads of one name
/// never share a partial.
fn staging_name_for(device: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let (parent, name) = match device.rfind('/') {
        Some(at) => (&device[..at.max(1)], &device[at + 1..]),
        None => ("/", device),
    };
    join_device_path(
        parent,
        &format!(
            "{name}{STAGING_TEMP_MARKER}{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ),
    )
}

/// The producer half of a read stream: `RECV`, then chunks until the file, the
/// consumer, or a cancellation ends it.
async fn produce(
    inner: std::sync::Arc<super::AdbVolumeInner>,
    mut session: SyncSession,
    device: String,
    offset: u64,
    chunk_tx: tokio::sync::mpsc::Sender<Result<Vec<u8>, VolumeError>>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    if let Err(e) = session.recv_start(&device).await {
        let _ = chunk_tx.send(Err(inner.map_adb_error(e, &device))).await;
        return;
    }
    let mut to_skip = offset;
    loop {
        let next = tokio::select! {
            biased;
            _ = &mut cancel_rx => {
                debug!("AdbVolume::open_read_stream({device}): cancelled");
                break;
            }
            next = session.recv_chunk() => next,
        };
        match next {
            Ok(Some(mut bytes)) => {
                if to_skip > 0 {
                    let drop_now = (to_skip.min(bytes.len() as u64)) as usize;
                    bytes.drain(..drop_now);
                    to_skip -= drop_now as u64;
                }
                if !bytes.is_empty() && chunk_tx.send(Ok(bytes)).await.is_err() {
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => {
                let _ = chunk_tx.send(Err(inner.map_adb_error(e, &device))).await;
                break;
            }
        }
    }
    // Closing the socket IS the release: the device stops sending the moment
    // the transport goes, so a cancelled 2 GB read costs nothing further.
    session.quit().await;
}

/// A stream over bytes already in hand, for `create_file`'s small payload.
///
/// ❌ Not a general-purpose reader: the only caller holds the bytes already,
/// by contract (`Volume::create_file` takes a slice), so nothing here
/// pre-buffers a file.
pub(super) struct BytesReadStream {
    bytes: Option<Vec<u8>>,
    total: u64,
    read: u64,
}

impl BytesReadStream {
    pub(super) fn new(bytes: Vec<u8>) -> Self {
        let total = bytes.len() as u64;
        Self {
            bytes: Some(bytes),
            total,
            read: 0,
        }
    }
}

impl VolumeReadStream for BytesReadStream {
    fn next_chunk(
        &mut self,
    ) -> std::pin::Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let bytes = self.bytes.take()?;
            if bytes.is_empty() {
                return None;
            }
            self.read += bytes.len() as u64;
            Some(Ok(bytes))
        })
    }

    fn total_size(&self) -> u64 {
        self.total
    }

    fn bytes_read(&self) -> u64 {
        self.read
    }
}
