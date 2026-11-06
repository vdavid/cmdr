//! The read window: positioned READs, several in flight on the one connection,
//! reassembled in file order. `crates/cmdr-sftp/DETAILS.md` § "The read window"
//! is the shape and the measurement behind the depth; the transport pipelines
//! calls by XID, so N READs in flight cost one connection and no locking.
//!
//! Two rules hold the module up:
//!
//! - **Every READ names its own offset.** v3 has no file position at all, so
//!   this is the protocol's own shape rather than a discipline.
//! - **A short answer is not the end of the file.** A chunk is re-asked at the
//!   offset the server stopped at until it's full; only `eof` (or an empty
//!   answer, which would otherwise ask forever) ends it.
//!
//! ❗ The length is what the attributes at open said. A file that grew under a
//! running read isn't chased; one that shrank reports what exists, since `eof`
//! ends the window wherever it lands.

use std::path::Path;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::CarriedMetadata;
use cmdr_fs::volume::read_window::ReadWindow;
use cmdr_fs::volume::{ChannelReadStream, SourceVersion, StreamLength, VolumeError};
use log::debug;

use super::NfsVolume;
use super::handles::Follow;
use super::mapping::system_time;
use crate::errors::map_nfs_error;
use crate::proto::nfs3::{Fattr3, FileHandle, FileType, FsInfo, Nfs3Client, NfsError, NfsTime};
use crate::transport::MAX_PAYLOAD;

/// How many READs one stream keeps in flight. SFTP's measured optimum, taken
/// as the starting point; to be re-measured against a NAS.
pub(super) const READ_WINDOW_DEPTH: usize = 8;

/// Chunks buffered between the producer and the consumer. Peak memory per
/// stream is `(READ_WINDOW_DEPTH + this) × chunk`, whatever the file's size.
const STREAM_CHANNEL_CAPACITY: usize = 2;

/// The bytes one READ asks for: the server's preferred size, within what it
/// allows and what our record cap admits. A server answering 0 for either
/// gets a sane 64 KiB.
pub(super) fn chunk_bytes(fsinfo: &FsInfo) -> usize {
    let preferred = if fsinfo.rtpref == 0 { 64 * 1024 } else { fsinfo.rtpref };
    let allowed = if fsinfo.rtmax == 0 { MAX_PAYLOAD } else { fsinfo.rtmax };
    preferred.min(allowed).clamp(4096, MAX_PAYLOAD) as usize
}

/// One positioned read: the seam the window is written against, so its
/// reassembly is testable against a double that short-reads and answers out
/// of order on purpose.
pub(super) trait PositionedRead: Clone + Send + 'static {
    /// Up to `len` bytes at `offset`, and whether the file ends there.
    fn read_at(&self, offset: u64, len: u32) -> impl Future<Output = Result<(bytes::Bytes, bool), VolumeError>> + Send;
}

/// One open file on the server: its handle and the connection to read it on.
#[derive(Clone)]
pub(super) struct RemoteFile {
    nfs: Nfs3Client,
    handle: FileHandle,
    /// The export-relative path, so a failure carries the path its variant is
    /// defined to carry.
    remote: std::sync::Arc<str>,
}

impl PositionedRead for RemoteFile {
    async fn read_at(&self, offset: u64, len: u32) -> Result<(bytes::Bytes, bool), VolumeError> {
        match self.nfs.read(&self.handle, offset, len).await {
            Ok(read) => Ok((read.data, read.eof)),
            Err(err) => Err(map_nfs_error(&err, &self.remote)),
        }
    }
}

/// One chunk, and whether the file ended inside it.
pub(super) struct ChunkRead {
    pub(super) bytes: Vec<u8>,
    pub(super) at_eof: bool,
}

/// Fills one chunk, as many READs as the server makes it take.
async fn read_chunk<S: PositionedRead>(reader: S, offset: u64, len: usize) -> Result<ChunkRead, VolumeError> {
    let mut bytes = Vec::with_capacity(len);
    loop {
        let asked = u32::try_from(len - bytes.len()).unwrap_or(u32::MAX);
        let (data, eof) = reader.read_at(offset + bytes.len() as u64, asked).await?;
        bytes.extend_from_slice(&data[..data.len().min(len - bytes.len())]);
        // An empty answer that doesn't say `eof` would be asked again forever.
        if eof || data.is_empty() {
            return Ok(ChunkRead { bytes, at_eof: true });
        }
        if bytes.len() >= len {
            return Ok(ChunkRead { bytes, at_eof: false });
        }
    }
}

/// The window every read here runs: `cmdr_fs`'s in-order fan-out, each chunk
/// filled by [`read_chunk`].
pub(super) type ChunkWindow<S> = ReadWindow<S, Result<ChunkRead, VolumeError>>;

/// `depth` chunk reads in flight over `[start, end)`, handed back in FILE order
/// however they complete.
pub(super) fn chunk_window<S: PositionedRead>(
    reader: S,
    start: u64,
    end: u64,
    depth: usize,
    chunk: usize,
) -> ChunkWindow<S> {
    ReadWindow::new(
        reader,
        |reader, offset, len| Box::pin(read_chunk(reader, offset, len)),
        start,
        end,
        depth,
        chunk,
    )
}

/// What the open learned before the first byte.
struct Opened {
    file: RemoteFile,
    attrs: Fattr3,
    chunk: usize,
}

impl NfsVolume {
    /// Looks the file up FRESH (its attributes are the length) and refuses a
    /// folder.
    async fn open_file(&self, path: &Path) -> Result<Opened, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.client().await?;
        let found = self.resolve(&client, &remote, Follow::Always, true).await?;
        let attrs = match found.attributes {
            Some(attrs) => attrs,
            None => client
                .nfs
                .getattr(&found.handle)
                .await
                .map_err(|err: NfsError| map_nfs_error(&err, &remote))?,
        };
        if attrs.file_type == FileType::Directory {
            return Err(VolumeError::IsADirectory(remote));
        }
        Ok(Opened {
            file: RemoteFile {
                nfs: client.nfs.clone(),
                handle: found.handle,
                remote: remote.into(),
            },
            attrs,
            chunk: chunk_bytes(&client.fsinfo),
        })
    }

    /// Opens `path` for streaming from `offset`. The length is settled before
    /// this returns, so a caller's first progress tick is honest.
    pub(super) async fn open_read_stream_impl(
        &self,
        path: &Path,
        offset: u64,
    ) -> Result<ChannelReadStream, VolumeError> {
        let opened = self.open_file(path).await?;
        Ok(self.stream_opened(opened, offset))
    }

    /// [`Volume::resume_read_stream`](cmdr_fs::volume::Volume::resume_read_stream):
    /// the stream from `offset`, once a fresh lookup still names the file
    /// `version` saw (the same file system and file number, length, and both
    /// times); any other is `SourceChanged` before a byte is read.
    pub(super) async fn resume_read_stream_impl(
        &self,
        path: &Path,
        version: &SourceVersion,
        offset: u64,
    ) -> Result<ChannelReadStream, VolumeError> {
        let Some(expected) = version.downcast_ref::<NfsVersion>() else {
            return Err(VolumeError::NotSupported);
        };
        let opened = self.open_file(path).await?;
        if NfsVersion::of(&opened.attrs) != *expected {
            return Err(VolumeError::SourceChanged(opened.file.remote.to_string()));
        }
        Ok(self.stream_opened(opened, offset))
    }

    fn stream_opened(&self, opened: Opened, offset: u64) -> ChannelReadStream {
        let size = opened.attrs.size;
        let version = NfsVersion::of(&opened.attrs);
        let (chunk_tx, chunk_rx) = tokio::sync::mpsc::channel(STREAM_CHANNEL_CAPACITY);
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        // ❌ Never `tokio::spawn`: a backend inherits whatever runtime it's
        // called on, and some callers are OS threads with none.
        self.inner.host.runtime().spawn(produce_stream(
            chunk_window(opened.file, offset, size, READ_WINDOW_DEPTH, opened.chunk),
            chunk_tx,
            cancel_rx,
        ));
        // `fattr3` always carries a size, so the length is `Known`: the WHOLE
        // file's, from any offset, as `Volume::open_read_stream_at_offset`
        // promises, so a resumed transfer's progress stays anchored to it.
        ChannelReadStream::new(chunk_rx, cancel_tx, StreamLength::Known(size))
            .with_modified_at(Some(system_time(opened.attrs.mtime)))
            .with_metadata(CarriedMetadata {
                mode: Some(opened.attrs.mode & 0o7777),
                ..CarriedMetadata::default()
            })
            .with_source_version(SourceVersion::new(version))
    }

    /// Exactly `[offset, offset + len)`, short only at end of file, so a caller
    /// never loops for a network short read.
    pub(super) async fn read_range_impl(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, VolumeError> {
        let opened = self.open_file(path).await?;
        let end = offset.saturating_add(len as u64).min(opened.attrs.size.max(offset));
        let mut window = chunk_window(opened.file, offset, end, READ_WINDOW_DEPTH, opened.chunk);
        let mut out = Vec::with_capacity(len.min((end - offset) as usize));
        while let Some(chunk) = window.next_chunk().await {
            let chunk = chunk?;
            out.extend_from_slice(&chunk.bytes);
            if chunk.at_eof {
                break;
            }
        }
        Ok(out)
    }
}

/// Which version of a file an NFS read saw: the file system and file number
/// (`fsid`, `fileid`), its length, and its write and change times. The change
/// time moves on any change at all, so a rewrite in place is refused even at the
/// same length. What a copy carries on from after an outage.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct NfsVersion {
    fsid: u64,
    fileid: u64,
    size: u64,
    mtime: NfsTime,
    ctime: NfsTime,
}

impl NfsVersion {
    fn of(attrs: &Fattr3) -> Self {
        Self {
            fsid: attrs.fsid,
            fileid: attrs.fileid,
            size: attrs.size,
            mtime: attrs.mtime,
            ctime: attrs.ctime,
        }
    }
}

/// The producer half of a read stream: chunks until the file, the consumer, or
/// a cancel ends it. ❗ Dropping the stream cancels it; a closed channel stops
/// the next send too.
async fn produce_stream(
    mut window: ChunkWindow<RemoteFile>,
    chunk_tx: tokio::sync::mpsc::Sender<Result<Vec<u8>, VolumeError>>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let mut delivered = 0u64;
    loop {
        let next = tokio::select! {
            biased;
            _ = &mut cancel_rx => {
                debug!(
                    target: "nfs",
                    "the read of {} was cancelled after {delivered} bytes", // allowed-pluralize-noun: a log line's byte count, read by a person debugging.
                    LogPrivate::file_path(&*window.reader().remote)
                );
                return;
            }
            next = window.next_chunk() => next,
        };
        let Some(chunk) = next else { return };
        match chunk {
            Err(err) => {
                let _ = chunk_tx.send(Err(err)).await;
                return;
            }
            Ok(chunk) => {
                delivered += chunk.bytes.len() as u64;
                if !chunk.bytes.is_empty() && chunk_tx.send(Ok(chunk.bytes)).await.is_err() {
                    return;
                }
                if chunk.at_eof {
                    // The file ended before its own size said it would (it
                    // shrank under the copy): report what exists.
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "streams_test.rs"]
mod streams_test;
