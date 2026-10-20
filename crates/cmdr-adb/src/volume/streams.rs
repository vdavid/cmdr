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

use cmdr_fs::log_detail::LogDetail;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses, default_file_mode, routed_mode};
use cmdr_fs::staging::temp_sibling_name;
use cmdr_fs::volume::{
    ChannelReadStream, StreamLength, StreamWriteProgress, StreamWritten, VolumeError, VolumeReadStream, WriteMode,
};
use log::debug;

use super::AdbVolume;
use super::paths::join_device_path;
use super::writes::WhatIsThere;
use crate::errors::{ENOENT, volume_error_from_errno};
use crate::sync::{MAX_DATA_CHUNK, SyncEntryKind, SyncSession, done_mtime_word};

/// Chunks buffered between the producer and the consumer. Peak memory per
/// stream is `this * MAX_DATA_CHUNK`, regardless of file size.
const STREAM_CHANNEL_CAPACITY: usize = 4;

/// Device-side `dd` range reads align down to this block. Toybox seeks over
/// skipped regular-file blocks, then performs at most one extra block of I/O.
const RANGE_BLOCK_BYTES: u64 = 64 * 1024;

/// The mode a file from a modeless source lands with: `rw-rw----`, what `adb
/// push` uses.
const NEW_FILE_MODE: u32 = 0o100660;

/// `S_IFREG`, the file-type bits `SEND`'s mode word carries.
const S_IFREG: u32 = 0o100_000;

/// The mode word `SEND` creates a file with: the source's mode folded under the
/// user's umask (`cmdr_fs::metadata::routed_mode`: never wider, no setuid), or
/// [`NEW_FILE_MODE`] for a source that reports none. `/sdcard` ignores it
/// (`DETAILS.md` § "The `Volume` answers, and why"); `/data/local/tmp` and a rooted device's
/// own filesystems keep it.
fn send_mode(source_mode: Option<u32>) -> u32 {
    source_mode
        .and_then(|mode| routed_mode(mode, default_file_mode()))
        .map_or(NEW_FILE_MODE, |mode| S_IFREG | mode)
}

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
        // ❗ Only an exact size is declared (#538): a v1 `STAT` size wraps past
        // 4 GiB and is a symlink's own, so the size check would fail a copy that
        // was fine. Unknown reads to the device's `DONE` instead.
        let length = if stat.size_is_exact {
            StreamLength::Known(stat.size)
        } else {
            StreamLength::Unknown
        };
        let modified_at = stat.modified_at();
        // The device's mode, off the same `stat`: what keeps a `0600` file
        // private and a script executable when it lands on local disk.
        let carried = CarriedMetadata {
            mode: Some(stat.mode & 0o7777),
            ..CarriedMetadata::default()
        };

        let (chunk_tx, chunk_rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, VolumeError>>(STREAM_CHANNEL_CAPACITY);
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        let inner = std::sync::Arc::clone(&self.inner);
        // ❌ Never `tokio::spawn` here: a backend inherits whatever runtime it
        // is called on, and some of those have none.
        self.inner
            .host
            .runtime()
            .spawn(async move { produce(inner, session, device, offset, chunk_tx, cancel_rx).await });

        Ok(ChannelReadStream::new(chunk_rx, cancel_tx, length)
            .with_modified_at(modified_at)
            .with_metadata(carried))
    }

    /// Reads one bounded range without `RECV` downloading the prefix.
    ///
    /// Android's Toybox `dd` supports block `skip`/`count` and seeks on regular
    /// files, but does not support GNU's byte flags. Aligning down keeps both
    /// device I/O and shell stdout bounded to the requested range plus one
    /// block; the host trims that leading block fragment.
    pub(super) async fn read_range_impl(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, VolumeError> {
        let device = self.to_device_path(path)?;
        let prefix = (offset % RANGE_BLOCK_BYTES) as usize;
        let requested = u64::try_from(len).map_err(|_| invalid_range(&device))?;
        let covered = u64::try_from(prefix)
            .ok()
            .and_then(|prefix| prefix.checked_add(requested))
            .ok_or_else(|| invalid_range(&device))?;
        let count = covered.div_ceil(RANGE_BLOCK_BYTES);
        let max_stdout = count
            .checked_mul(RANGE_BLOCK_BYTES)
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or_else(|| invalid_range(&device))?;
        let input = format!("if={device}");
        let block = format!("bs={RANGE_BLOCK_BYTES}");
        let skip = format!("skip={}", offset / RANGE_BLOCK_BYTES);
        let count = format!("count={count}");
        let outcome = crate::shell::run_bounded(
            &self.inner.endpoint,
            &self.inner.serial,
            &["dd", &input, &block, &skip, &count],
            max_stdout,
        )
        .await
        .map_err(|error| self.inner.map_adb_error(error, &device))?;
        if !outcome.succeeded() {
            let metadata = self.get_metadata_impl(path).await?;
            if metadata.is_directory {
                return Err(VolumeError::IsADirectory(device));
            }
            debug!(
                "AdbVolume::read_range: dd exited {} for {device}: stderr={:?}",
                outcome.exit_code,
                LogDetail(outcome.stderr_text().trim()).marking(&[LogPrivate::file_path(&device)]),
                device = LogPrivate::file_path(&device)
            );
            return Err(VolumeError::IoError {
                message: format!("device-side range read exited {}", outcome.exit_code).into(),
                raw_os_error: None,
            });
        }
        if outcome.stdout.len() <= prefix {
            return Ok(Vec::new());
        }
        let end = prefix.saturating_add(len).min(outcome.stdout.len());
        Ok(outcome.stdout[prefix..end].to_vec())
    }

    /// Streams `stream` onto `dest` through a staging sibling, then moves it
    /// into place.
    ///
    /// `SEND` writes the file at the name it is given, byte by byte, so the
    /// name holds a partial for the whole upload. It lands on
    /// `<name>.cmdr-tmp-<uuid>` beside the destination and takes the real name by
    /// one `mv -f`, so nothing half-written ever wears the user's filename; on
    /// any failure the staging name is removed. ❗ Cancellation arrives only
    /// through `on_progress` answering `Break`: there is no token on this path.
    ///
    /// `CreateNew` stats the destination right before the `mv -f` and refuses
    /// an occupied one, taking only the staging name away (`writes.rs` § "The
    /// two accepted TOCTOU windows": `mv -n` can't say whether it moved).
    pub(super) async fn write_from_stream_impl(
        &self,
        dest: &Path,
        mode: WriteMode,
        length: StreamLength,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Result<StreamWritten, VolumeError> {
        let device = self.to_device_path(dest)?;
        let staging = staging_name_for(&device);
        debug!(
            "AdbVolume::write_from_stream: {device} via {staging}, length={length:?}",
            device = LogPrivate::file_path(&device),
            staging = LogPrivate::file_path(&staging)
        );

        let mut session = self.open_sync(&device).await?;
        let pumped = self
            .pump(&mut session, &staging, &device, length, &mut stream, on_progress)
            .await;
        session.quit().await;

        match pumped {
            Ok(written) => {
                if mode.refuses_occupied() {
                    let refusal = match self.occupant(&device).await {
                        Ok(WhatIsThere::Nothing) => None,
                        Ok(_) => Some(VolumeError::AlreadyExists(device.clone())),
                        Err(cant_tell) => Some(cant_tell),
                    };
                    if let Some(refusal) = refusal {
                        self.remove_partial(&staging).await;
                        return Err(refusal);
                    }
                }
                if let Err(e) = self.shell_verb(&["mv", "-f", &staging, &device], &device).await {
                    self.remove_partial(&staging).await;
                    return Err(e);
                }
                self.notify_created(dest).await;
                // The mode rides `SEND` itself: a refusal fails the write.
                Ok(StreamWritten::nothing_refused(written))
            }
            Err(e) => {
                self.remove_partial(&staging).await;
                Err(e)
            }
        }
    }

    /// The upload itself: `SEND` with the source's folded mode, one `DATA` frame
    /// per chunk, `DONE` with the mtime, progress after every chunk the device
    /// took.
    ///
    /// The mtime is the source's date, so a copy keeps it; `DONE` must carry
    /// one, so a source without a date lands dated now, as `adb push` does.
    /// The staging `mv` keeps it.
    async fn pump(
        &self,
        session: &mut SyncSession,
        staging: &str,
        device: &str,
        length: StreamLength,
        stream: &mut Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Result<u64, VolumeError> {
        let map = |e| self.inner.map_adb_error(e, device);
        // Asked before the first chunk, while the stream still holds its open's answer.
        let mode = send_mode(stream.metadata(MetadataClasses::MODE).mode);
        session.send_start(staging, mode).await.map_err(map)?;
        let mut written = 0u64;
        while let Some(chunk) = stream.next_chunk().await {
            let chunk = chunk?;
            for piece in chunk.chunks(MAX_DATA_CHUNK) {
                session.send_chunk(piece).await.map_err(map)?;
                written += piece.len() as u64;
            }
            if on_progress(StreamWriteProgress {
                bytes_written: written,
                expected_length: length,
            })
            .is_break()
            {
                return Err(VolumeError::Cancelled(device.to_string().into()));
            }
        }
        let date = stream.modified_at().unwrap_or_else(std::time::SystemTime::now);
        session.send_finish(done_mtime_word(date)).await.map_err(map)?;
        Ok(written)
    }

    /// Best-effort removal of a partial upload. A failure here must never
    /// replace the error that caused it.
    async fn remove_partial(&self, staging: &str) {
        if let Err(e) = crate::shell::run(&self.inner.endpoint, &self.inner.serial, &["rm", "-f", staging]).await {
            debug!(
                "AdbVolume::write_from_stream: couldn't remove the partial {staging}: {e:?}",
                staging = LogPrivate::file_path(&staging)
            );
        }
    }
}

fn invalid_range(path: &str) -> VolumeError {
    VolumeError::IoError {
        message: format!("range is too large for {path}").into(),
        raw_os_error: None,
    }
}

/// `<dir>/<name>.cmdr-tmp-<uuid>`: the house marker, so a leftover from a
/// crash is recognized as Cmdr's, a UUID so two uploads of one name never share
/// a partial, and the name shortened when the whole wouldn't fit the phone's
/// name limit (`cmdr_fs::staging::names`).
fn staging_name_for(device: &str) -> String {
    let (parent, name) = match device.rfind('/') {
        Some(at) => (&device[..at.max(1)], &device[at + 1..]),
        None => ("/", device),
    };
    join_device_path(parent, &temp_sibling_name(name))
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
                debug!("AdbVolume::open_read_stream({device}): cancelled", device = LogPrivate::file_path(&device));
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

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.total)
    }

    fn bytes_read(&self) -> u64 {
        self.read
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        // `create_file`'s fresh bytes: the device stamps its own date.
        None
    }
}
