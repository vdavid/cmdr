//! The read window: what makes a file over a 50 ms link arrive at more than one
//! chunk per round trip.
//!
//! A sequential SFTP read is one request, one round trip, one chunk — 255 KiB /
//! 50 ms ≈ 4 MB/s however fast the server and the disk are. The window keeps N
//! positioned reads in flight and reassembles them in file order, which is worth
//! roughly an order of magnitude on a link with any latency in it
//! (`docs/notes/sftp-crate-evaluation-2026-08-22.md`).
//!
//! Two rules hold the whole module up:
//!
//! - **Every read carries its own offset** ([`RemoteFile::read_at`]). ❌ Never
//!   the engine's own file offset, which advances by the REQUESTED length even
//!   when the server returns less.
//! - **A short answer is not the end of the file.** [`ChunkWindow`] fills each
//!   chunk with as many reads as the server makes it do, and only an empty
//!   answer means end of file.
//!
//! `DETAILS.md` § "The read window" carries the measurements and the depth they
//! set.

use std::io::SeekFrom;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::CarriedMetadata;
use cmdr_fs::volume::read_window::ReadWindow;
use cmdr_fs::volume::{ChannelReadStream, SourceVersion, StreamLength, VolumeError};
use log::debug;
use openssh_sftp_client::file::File;
use tokio::io::AsyncSeek;

use super::SftpVolume;
use crate::errors::map_sftp_error;
use crate::transport::SshConnection;

/// How many bytes one read request asks for.
///
/// ❗ Load-bearing at exactly this value: OpenSSH's `sftp-server` answers
/// `limits@openssh.com` with a 255 KiB read limit, so a chunk is one request. Ask
/// for more and the engine splits it; ask for less and every round trip carries
/// less. Every figure in the crate evaluation was measured here.
pub(super) const CHUNK_BYTES: usize = 255 * 1024;

/// How many chunk reads one foreground stream keeps in flight.
///
/// Set from the four-concurrent-streams curve in `DETAILS.md`, ❗ not from the
/// single-stream number: four streams at depth 32 would put ~32 MiB of
/// outstanding read data against a 16 MiB channel window, and they'd spend the
/// difference throttling each other.
pub(super) const READ_WINDOW_DEPTH: usize = 8;

/// The same for a background scan read.
///
/// Shallower on purpose: the index scan's prefetch shares one SSH channel with
/// whatever the user is doing, and a background read that fills the channel
/// window is a foreground read that waits for it.
pub(super) const SCAN_WINDOW_DEPTH: usize = 2;

/// Chunks buffered between the producer and the consumer.
///
/// Peak memory per stream is `(READ_WINDOW_DEPTH + this) * CHUNK_BYTES`,
/// regardless of file size.
const STREAM_CHANNEL_CAPACITY: usize = 2;

/// One positioned read against an open remote file.
///
/// The seam the window is written against, so its reassembly is testable
/// without a server: a double can short-read, reorder, and stall where a real
/// one only sometimes does.
pub(super) trait PositionedRead: Clone + Send + 'static {
    /// Up to `len` bytes starting at `offset`.
    ///
    /// Fewer is legal and ordinary — SFTP lets a server answer a read with less
    /// than it was asked for. An EMPTY answer, and only an empty answer, means
    /// end of file.
    fn read_at(&mut self, offset: u64, len: usize) -> impl Future<Output = Result<Vec<u8>, VolumeError>> + Send;
}

/// An open remote file, read one position at a time.
///
/// Cloning shares the remote handle through an `Arc`, so N clones seeked to
/// their own offsets give depth N and cost no extra `SSH_FXP_OPEN`.
#[derive(Clone)]
pub(super) struct RemoteFile {
    file: File,
    /// The remote path this handle was opened at, carried so a failure on it can
    /// answer with the path its `VolumeError` variant is defined to carry.
    /// `Arc<str>` because a clone rides along with every in-flight read.
    remote: Arc<str>,
}

impl RemoteFile {
    fn new(file: File, remote: Arc<str>) -> Self {
        Self { file, remote }
    }

    /// The file's size, modification date, and mode, from an `fstat` on the
    /// handle already open.
    ///
    /// The handle rather than the path, so the answer describes the bytes this
    /// stream is about to read even if the name is replaced under it. The date
    /// and mode ride the same answer, so reporting them costs no round trip.
    async fn stat(&mut self) -> Result<OpenedFile, VolumeError> {
        let meta = self
            .file
            .metadata()
            .await
            .map_err(|e| map_sftp_error(&e, &self.remote))?;
        Ok(OpenedFile {
            size: meta.len(),
            modified_at: meta.modified().map(|stamp| stamp.as_system_time()),
            mode: meta.permissions().map(super::mapping::permission_bits),
        })
    }
}

/// What the open learns about a file before its first byte is handed over.
struct OpenedFile {
    /// `None` when the server's `fstat` answer carries no size (the attribute
    /// is optional in the protocol).
    size: Option<u64>,
    /// `None` when the server's `fstat` answer carries no times.
    modified_at: Option<SystemTime>,
    /// The permission bits (`0o7777`) off the same answer: the mode a copy
    /// carries, at no round trip. `None` when the server sends none.
    mode: Option<u32>,
}

/// Which version of a file an SFTP read saw: its size and modified time, all an
/// `fstat` says about a file's content (protocol v3 has no inode and no change
/// time). ❗ Best-effort: a rewrite to the same size inside the same second
/// passes. What a copy carries on from after an outage
/// (`Volume::resume_read_stream`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct SftpVersion {
    size: u64,
    modified_at: SystemTime,
}

impl SftpVersion {
    /// `None` when the `fstat` left out either: nothing could prove the file
    /// unchanged later.
    fn of(opened: &OpenedFile) -> Option<Self> {
        Some(Self {
            size: opened.size?,
            modified_at: opened.modified_at?,
        })
    }
}

/// Where a carried-on read starts, and the version the file must still be.
#[derive(Clone, Copy)]
pub(super) struct ReadFrom {
    pub(super) version: SftpVersion,
    pub(super) offset: u64,
}

impl OpenedFile {
    /// The length the stream declares. ❗ A size the server didn't send is
    /// `Unknown`, ❌ never `Known(0)`: the copy engine fails a stream whose bytes
    /// miss its declared length, so a zero stand-in failed every such file
    /// (#538). The rule S3 and WebDAV keep for a missing `Content-Length`.
    fn length(&self) -> StreamLength {
        self.size.map_or(StreamLength::Unknown, StreamLength::Known)
    }

    /// Where the read window stops asking: the size, else wherever the server
    /// says the file ends (an empty answer).
    fn read_end(&self) -> u64 {
        self.size.unwrap_or(u64::MAX)
    }
}

impl PositionedRead for RemoteFile {
    async fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, VolumeError> {
        // ❗ Seek before EVERY read, and never trust the offset the engine keeps.
        // `File::read` advances that offset by the length it was ASKED for, not
        // by the length it returned, so a short read leaves it pointing past a
        // gap — a hole in the file plus a duplicate of the bytes after it. Naming
        // the offset on every request is what makes a short answer harmless.
        Pin::new(&mut self.file)
            .start_seek(SeekFrom::Start(offset))
            .map_err(|e| VolumeError::IoError {
                message: e.to_string().into(),
                raw_os_error: e.raw_os_error(),
            })?;
        let len = u32::try_from(len).unwrap_or(u32::MAX);
        match self.file.read(len, Default::default()).await {
            // The engine appends into the buffer we hand it, so an empty one in
            // means exactly the answer's bytes out.
            Ok(Some(buffer)) => Ok(buffer.into()),
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(map_sftp_error(&e, &self.remote)),
        }
    }
}

/// One chunk's worth of bytes, and whether the file ended inside it.
pub(super) struct ChunkRead {
    /// What the server actually returned, in order.
    pub(super) bytes: Vec<u8>,
    /// The server answered a read with nothing, so there is nothing past this.
    pub(super) at_eof: bool,
}

/// Fills one chunk, taking as many round trips as the server makes it take.
async fn read_chunk<S: PositionedRead>(reader: &mut S, offset: u64, len: usize) -> Result<ChunkRead, VolumeError> {
    let mut bytes = reader.read_at(offset, len).await?;
    if bytes.is_empty() {
        return Ok(ChunkRead { bytes, at_eof: true });
    }
    while bytes.len() < len {
        let more = reader.read_at(offset + bytes.len() as u64, len - bytes.len()).await?;
        if more.is_empty() {
            return Ok(ChunkRead { bytes, at_eof: true });
        }
        bytes.extend_from_slice(&more);
    }
    Ok(ChunkRead { bytes, at_eof: false })
}

/// The window every read here runs: `cmdr_fs`'s in-order fan-out, each chunk
/// filled by [`read_chunk`] (`cmdr_fs::volume::read_window` says why
/// `FuturesOrdered` needs no reassembly buffer).
pub(super) type ChunkWindow<S> = ReadWindow<S, Result<ChunkRead, VolumeError>>;

/// A window over `[start, end)`, reading `chunk` bytes at a time with `depth`
/// reads in flight, handed back in FILE order however they complete.
pub(super) fn chunk_window<S: PositionedRead>(
    reader: S,
    start: u64,
    end: u64,
    depth: usize,
    chunk: usize,
) -> ChunkWindow<S> {
    ReadWindow::new(
        reader,
        |mut reader, offset, len| Box::pin(async move { read_chunk(&mut reader, offset, len).await }),
        start,
        end,
        depth,
        chunk,
    )
}

impl SftpVolume {
    /// Opens `path` for streaming, with `depth` chunk reads in flight.
    ///
    /// The size is settled before this returns, so the caller has an honest
    /// `total_size()` from its first progress tick.
    pub(super) async fn open_read_stream_impl(
        &self,
        path: &Path,
        depth: usize,
    ) -> Result<ChannelReadStream, VolumeError> {
        self.read_stream_from(path, depth, None).await
    }

    /// [`Volume::resume_read_stream`](cmdr_fs::volume::Volume::resume_read_stream):
    /// the same stream from `offset`, once the file's `fstat` still says
    /// `version`; a file that changed is `SourceChanged` before a byte is
    /// handed out.
    pub(super) async fn resume_read_stream_impl(
        &self,
        path: &Path,
        version: &SourceVersion,
        offset: u64,
    ) -> Result<ChannelReadStream, VolumeError> {
        let Some(version) = version.downcast_ref::<SftpVersion>() else {
            return Err(VolumeError::NotSupported);
        };
        let from = ReadFrom {
            version: *version,
            offset,
        };
        self.read_stream_from(path, READ_WINDOW_DEPTH, Some(from)).await
    }

    async fn read_stream_from(
        &self,
        path: &Path,
        depth: usize,
        from: Option<ReadFrom>,
    ) -> Result<ChannelReadStream, VolumeError> {
        let remote = self.to_remote_path(path)?;
        // ❗ Cloned out from under a short read guard. Holding it across the read
        // would serialize every other operation on the one channel.
        let session = self.clone_session().await?;

        let (size_tx, size_rx) = tokio::sync::oneshot::channel::<Result<OpenedFile, VolumeError>>();
        let (chunk_tx, chunk_rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, VolumeError>>(STREAM_CHANNEL_CAPACITY);
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

        // ❌ Never `tokio::spawn` here: a backend inherits whatever runtime it is
        // called on, and some of those are watcher OS threads with none.
        self.inner.host.runtime().spawn(produce_stream(
            session, remote, depth, from, size_tx, chunk_tx, cancel_rx,
        ));

        let opened = match size_rx.await {
            Ok(Ok(opened)) => opened,
            Ok(Err(e)) => return Err(e),
            // The producer's task went away without answering, which only a
            // runtime shutdown does.
            Err(_) => return Err(VolumeError::DeviceDisconnected(self.inner.volume_id.clone().into())),
        };

        let stream = ChannelReadStream::new(chunk_rx, cancel_tx, opened.length())
            .with_modified_at(opened.modified_at)
            .with_metadata(CarriedMetadata {
                mode: opened.mode,
                ..CarriedMetadata::default()
            });
        Ok(match SftpVersion::of(&opened) {
            Some(version) => stream.with_source_version(SourceVersion::new(version)),
            None => stream,
        })
    }

    /// Exactly `[offset, offset + len)`, filled from as few round trips as the
    /// server allows.
    ///
    /// The positioned primitive remote-archive browsing reads a `.zip`'s central
    /// directory through, so it returns short only at end of file — ❗ a caller
    /// never loops for a network short read.
    pub(super) async fn read_range_impl(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let session = self.clone_session().await?;
        let file = session
            .sftp()
            .open(&remote)
            .await
            .map_err(|e| map_sftp_error(&e, &remote))?;

        let end = offset.saturating_add(len as u64);
        let handle = RemoteFile::new(file, Arc::from(remote.as_str()));
        let mut window = chunk_window(handle, offset, end, READ_WINDOW_DEPTH, CHUNK_BYTES);
        let mut out = Vec::with_capacity(len);
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

/// The producer half of a read stream: open, size, then chunks until the file,
/// the consumer, or a cancellation ends it.
///
/// Owns its `Arc<SshConnection>`, so a volume disconnected mid-copy finishes the
/// chunk it was reading rather than tearing the engine out from under it.
async fn produce_stream(
    session: Arc<SshConnection>,
    remote: String,
    depth: usize,
    from: Option<ReadFrom>,
    size_tx: tokio::sync::oneshot::Sender<Result<OpenedFile, VolumeError>>,
    chunk_tx: tokio::sync::mpsc::Sender<Result<Vec<u8>, VolumeError>>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let file = match session.sftp().open(&remote).await {
        Ok(file) => file,
        Err(e) => {
            let _ = size_tx.send(Err(map_sftp_error(&e, &remote)));
            return;
        }
    };

    // The `fstat` and the first chunk go out TOGETHER, so a small file costs one
    // round trip after the open instead of two. The first read asks for a whole
    // chunk without knowing the size yet, which is safe: reading past the end of
    // a file answers with fewer bytes, never with an error.
    // A carried-on read starts at its offset, and only once the `fstat` says
    // it's still the file the first read saw: the head chunk read beside it
    // is dropped otherwise, never handed out.
    let start = from.map_or(0, |from| from.offset);
    let mut sizing = RemoteFile::new(file, Arc::from(remote.as_str()));
    let mut head = sizing.clone();
    let (opened, first) = tokio::join!(sizing.stat(), head.read_at(start, CHUNK_BYTES));

    let opened = match opened {
        Ok(opened) => opened,
        Err(e) => {
            let _ = size_tx.send(Err(e));
            return;
        }
    };
    if let Some(from) = from
        && SftpVersion::of(&opened) != Some(from.version)
    {
        let _ = size_tx.send(Err(VolumeError::SourceChanged(remote.clone())));
        return;
    }
    let first = match first {
        Ok(bytes) => bytes,
        Err(e) => {
            let _ = size_tx.send(Err(e));
            return;
        }
    };
    let end = opened.read_end();
    if size_tx.send(Ok(opened)).is_err() {
        // The caller gave up before the size landed. Nothing to stream to.
        return;
    }

    // The first read may have been answered short, so the window picks up
    // wherever it actually got to rather than at a chunk boundary.
    let mut delivered = start + first.len() as u64;
    if !first.is_empty() && chunk_tx.send(Ok(first)).await.is_err() {
        return;
    }

    // A consumer slower than the link parks this loop inside `chunk_tx.send`,
    // which stops the window's in-flight reads being polled. That's safe rather
    // than a stall because the ENGINE has a read task of its own: it keeps
    // draining the channel and parking each answer in its response arena, so a
    // slow consumer costs `depth` buffered chunks and never blocks the SSH
    // connection the other operations share.
    let mut window = chunk_window(sizing, delivered, end, depth, CHUNK_BYTES);
    loop {
        let next = tokio::select! {
            biased;
            _ = &mut cancel_rx => {
                debug!("SftpVolume::open_read_stream({remote}): cancelled, bytes delivered: {delivered}", remote = LogPrivate::file_path(&remote));
                return;
            }
            next = window.next_chunk() => next,
        };
        let Some(chunk) = next else { return };
        match chunk {
            Err(e) => {
                let _ = chunk_tx.send(Err(e)).await;
                return;
            }
            Ok(chunk) => {
                let at_eof = chunk.at_eof;
                delivered += chunk.bytes.len() as u64;
                if !chunk.bytes.is_empty() && chunk_tx.send(Ok(chunk.bytes)).await.is_err() {
                    // The consumer dropped. Stop pumping; the handle closes when
                    // this task's `File` clones do.
                    return;
                }
                if at_eof {
                    // The file ended before its own size said it would, which a
                    // file being truncated under a copy really does. Report what
                    // exists rather than inventing the difference.
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "streams_test.rs"]
mod streams_test;

#[cfg(test)]
#[path = "streams_bench.rs"]
mod streams_bench;
