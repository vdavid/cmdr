//! Seedless ZIP generation over bounded byte channels.
//!
//! `zip` 8.6's `ZipWriter::new_stream` writes data descriptors instead of seeking
//! back into local headers. `finish` writes the central directory and returns the
//! underlying writer; success is not inferred from output-channel EOF. ZIP64 is
//! selected per entry, up front, from deflate's worst-case output for the planned
//! size, because stream mode cannot repair an undersized local header after bytes
//! have passed downstream (verified against installed `zip-8.6.0/src/write.rs`
//! and `types.rs::write_data_descriptor`, 2026-09-29).

use std::future::Future;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::file_system::volume::{StreamLength, VolumeError, VolumeReadStream};

const CHANNEL_CHUNKS: usize = 4;
const CHUNK_BYTES: usize = 128 * 1024;

pub(super) enum FreshZipSource {
    Local(PathBuf),
    Bytes(Vec<u8>),
    Remote(RemoteZipSource),
}

/// The producer's end of one remote entry's bounded async→sync bridge. The
/// producer parks in `blocking_recv`; a closed channel wakes it, and it reads
/// that as cancellation when one was requested, otherwise as channel loss.
pub(super) struct RemoteZipSource {
    rx: mpsc::Receiver<RemoteSourceMessage>,
}

/// The async feeder's end. Only the coordinator's feed task holds feeders, and
/// it drops every one of them when cancellation lands: channel close is what
/// wakes a producer parked on a source that stopped answering.
pub(super) struct RemoteZipFeeder {
    tx: mpsc::Sender<RemoteSourceMessage>,
}

enum RemoteSourceMessage {
    Chunk(Result<Vec<u8>, FreshZipError>),
    Complete,
}

pub(super) fn remote_source_bridge() -> (RemoteZipFeeder, RemoteZipSource) {
    let (tx, rx) = mpsc::channel(CHANNEL_CHUNKS);
    (RemoteZipFeeder { tx }, RemoteZipSource { rx })
}

impl RemoteZipFeeder {
    pub(super) async fn send(
        &self,
        chunk: Result<Vec<u8>, FreshZipError>,
        cancellation: &FreshZipCancellation,
    ) -> Result<(), FreshZipError> {
        match chunk {
            Ok(bytes) => {
                for bounded in bytes.chunks(CHUNK_BYTES) {
                    self.send_message(RemoteSourceMessage::Chunk(Ok(bounded.to_vec())), cancellation)
                        .await?;
                }
                Ok(())
            }
            Err(error) => {
                self.send_message(RemoteSourceMessage::Chunk(Err(error)), cancellation)
                    .await
            }
        }
    }

    /// Waits for queue room, or for cancellation, whichever comes first. A
    /// closed channel means the producer already ended; its own result is the
    /// causal one.
    async fn send_message(
        &self,
        message: RemoteSourceMessage,
        cancellation: &FreshZipCancellation,
    ) -> Result<(), FreshZipError> {
        tokio::select! {
            biased;
            () = cancellation.cancelled() => Err(FreshZipError::Cancelled),
            sent = self.tx.send(message) => sent.map_err(|_| FreshZipError::Cancelled),
        }
    }

    /// Marks the source as successfully exhausted. Dropping the feeder without
    /// this marker is channel loss, never EOF.
    pub(super) async fn finish(self, cancellation: &FreshZipCancellation) -> Result<(), FreshZipError> {
        self.send_message(RemoteSourceMessage::Complete, cancellation).await
    }
}

pub(super) struct FreshZipEntry {
    pub name: String,
    pub source: FreshZipSource,
    pub size: u64,
    pub is_directory: bool,
    pub modified: Option<std::time::SystemTime>,
    pub unix_mode: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FreshZipProgress {
    pub entries_done: usize,
    pub source_bytes_done: u64,
}

/// Returns `true` when the producer must stop after reporting this boundary.
pub(super) type FreshZipProgressObserver = Arc<dyn Fn(FreshZipProgress) -> bool + Send + Sync>;

/// The pipeline's ONE cancellation source. For a managed operation it's a child
/// of the operation's `backend_cancel`, so a user's Cancel and every internal
/// stop (the destination dropping its stream, the coordinator shutting the
/// pipeline down) land on the same token. Every participant watches it: the
/// producer at each checkpoint and pause, the remote feed around each source
/// read, the output stream on each pull, and the destination through its write
/// callback.
#[derive(Clone)]
pub(super) struct FreshZipCancellation {
    token: CancellationToken,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl FreshZipCancellation {
    /// A standalone source, for a producer no operation owns.
    #[cfg(test)]
    pub(super) fn new(wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        Self {
            token: CancellationToken::new(),
            wake,
        }
    }

    /// Follows `operation` (the op's tier-1 `backend_cancel`), plus its own
    /// internal requests. `wake` releases a producer parked on the pause gate
    /// when the request is internal; an operation cancel wakes the gate itself.
    pub(super) fn for_operation(operation: &CancellationToken, wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        Self {
            token: operation.child_token(),
            wake,
        }
    }

    pub(super) fn request(&self) {
        self.token.cancel();
        if let Some(wake) = &self.wake {
            wake();
        }
    }

    pub(super) fn is_requested(&self) -> bool {
        self.token.is_cancelled()
    }

    pub(super) async fn cancelled(&self) {
        self.token.cancelled().await;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FreshZipError {
    Cancelled,
    Source { entry: String, message: String },
    CountMismatch { entry: String, expected: u64, actual: u64 },
    Zip(String),
    OutputClosed,
    SourceChannelClosed,
    WorkerStart(String),
    ProducerPanicked,
}

pub(super) struct FreshZipOutput {
    stream: FreshZipStream,
    completion: FreshZipCompletion,
}

/// What crosses the output queue. The producer sends `End` only after the
/// central directory is written and flushed, so a channel that closes without
/// it is a failed or cancelled ZIP, never a complete one.
enum OutputMessage {
    Chunk(Vec<u8>),
    End,
}

pub(super) struct FreshZipStream {
    rx: mpsc::Receiver<OutputMessage>,
    bytes_read: u64,
    ended: bool,
    cancellation: FreshZipCancellation,
}

pub(super) struct FreshZipCompletion {
    terminal: oneshot::Receiver<Result<u64, FreshZipError>>,
    join: Option<std::thread::JoinHandle<()>>,
    cancellation: FreshZipCancellation,
}

impl FreshZipOutput {
    /// The next output chunk; `None` at the producer's `End` and on any failure,
    /// which [`finish`](Self::finish) then names.
    #[cfg(test)]
    pub(super) async fn next_chunk(&mut self) -> Option<Vec<u8>> {
        self.stream.next_chunk().await.and_then(Result::ok)
    }

    /// Waits for the explicit producer outcome and joins its blocking worker.
    /// A closed terminal channel is a producer failure, never successful EOF.
    #[cfg(test)]
    pub(super) async fn finish(self) -> Result<u64, FreshZipError> {
        self.completion.finish().await
    }

    /// Separates the destination-consumed stream from the producer's explicit
    /// terminal result. The coordinator retains the latter while a backend owns
    /// the former, so destination EOF can never stand in for producer success.
    pub(super) fn into_parts(self) -> (FreshZipStream, FreshZipCompletion) {
        (self.stream, self.completion)
    }
}

impl VolumeReadStream for FreshZipStream {
    /// EOF only after the producer's explicit `End`. A cancellation, or a queue
    /// that closes without `End`, reaches the destination as an error, so the
    /// backend drops its partial instead of closing it as a finished file.
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.ended {
                return None;
            }
            let message = tokio::select! {
                biased;
                () = self.cancellation.cancelled() => None,
                message = self.rx.recv() => message,
            };
            match message {
                Some(OutputMessage::Chunk(chunk)) => {
                    self.bytes_read += chunk.len() as u64;
                    Some(Ok(chunk))
                }
                Some(OutputMessage::End) => {
                    self.ended = true;
                    None
                }
                None if self.cancellation.is_requested() => {
                    Some(Err(VolumeError::Cancelled("fresh ZIP cancelled".to_string())))
                }
                None => Some(Err(VolumeError::IoError {
                    message: "the ZIP producer stopped before the archive was complete".to_string(),
                    raw_os_error: None,
                })),
            }
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Unknown
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        // A ZIP being made right now: no source date to keep.
        None
    }
}

impl Drop for FreshZipStream {
    /// A destination that lets go before `End` stopped consuming a live ZIP:
    /// stop the producer rather than leave it parked on a full queue.
    fn drop(&mut self) {
        if !self.ended {
            self.cancellation.request();
        }
        self.rx.close();
    }
}

impl FreshZipCompletion {
    pub(super) async fn finish(mut self) -> Result<u64, FreshZipError> {
        let terminal = (&mut self.terminal).await;
        let joined = match self.join.take() {
            Some(join) => tokio::task::spawn_blocking(move || join.join())
                .await
                .map_err(|_| FreshZipError::ProducerPanicked)
                .and_then(|outcome| outcome.map_err(|_| FreshZipError::ProducerPanicked)),
            None => Ok(()),
        };
        joined?;
        terminal.map_err(|_| FreshZipError::ProducerPanicked)?
    }

    /// Cancels a producer whose destination stopped consuming, closes that
    /// endpoint first so a full bounded queue wakes, then joins off the runtime.
    #[cfg(test)]
    pub(super) async fn shutdown(self, stream: FreshZipStream) -> Result<u64, FreshZipError> {
        self.cancellation.request();
        drop(stream);
        self.finish().await
    }
}

impl Drop for FreshZipCompletion {
    fn drop(&mut self) {
        // Joining may block and `Drop` can run on an async worker. The managed
        // coordinator owns explicit `finish` / `shutdown`; Drop only requests
        // cancellation, and only for a producer nobody joined.
        if self.join.is_some() {
            self.cancellation.request();
        }
    }
}

#[cfg(test)]
pub(super) fn spawn_fresh_zip(
    entries: Vec<FreshZipEntry>,
    level: Option<i64>,
) -> Result<FreshZipOutput, FreshZipError> {
    spawn_fresh_zip_with_progress(entries, level, None, FreshZipCancellation::new(None))
}

pub(super) fn spawn_fresh_zip_with_progress(
    entries: Vec<FreshZipEntry>,
    level: Option<i64>,
    progress: Option<FreshZipProgressObserver>,
    cancellation: FreshZipCancellation,
) -> Result<FreshZipOutput, FreshZipError> {
    let (output_tx, output_rx) = mpsc::channel(CHANNEL_CHUNKS);
    let (terminal_tx, terminal_rx) = oneshot::channel();
    let worker_cancellation = cancellation.clone();
    let join = std::thread::Builder::new()
        .name("fresh-zip-producer".into())
        .spawn(move || {
            let result = produce(entries, level, output_tx, &worker_cancellation, progress.as_ref());
            let _ = terminal_tx.send(result);
        })
        .map_err(|error| FreshZipError::WorkerStart(error.to_string()))?;
    Ok(FreshZipOutput {
        stream: FreshZipStream {
            rx: output_rx,
            bytes_read: 0,
            ended: false,
            cancellation: cancellation.clone(),
        },
        completion: FreshZipCompletion {
            terminal: terminal_rx,
            join: Some(join),
            cancellation,
        },
    })
}

struct ChannelWriter {
    tx: mpsc::Sender<OutputMessage>,
    pending: Vec<u8>,
    written: u64,
    cancellation: FreshZipCancellation,
}

impl ChannelWriter {
    fn flush_pending(&mut self) -> std::io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let chunk = std::mem::replace(&mut self.pending, Vec::with_capacity(CHUNK_BYTES));
        self.send(OutputMessage::Chunk(chunk))
    }

    /// Queues one message, waiting for room. A closed queue means the
    /// destination stopped reading.
    fn send(&self, message: OutputMessage) -> std::io::Result<()> {
        if self.cancellation.is_requested() {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }
        self.tx
            .blocking_send(message)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "ZIP destination closed"))
    }
}

impl Write for ChannelWriter {
    fn write(&mut self, mut buf: &[u8]) -> std::io::Result<usize> {
        let original = buf.len();
        while !buf.is_empty() {
            let room = CHUNK_BYTES - self.pending.len();
            let take = room.min(buf.len());
            self.pending.extend_from_slice(&buf[..take]);
            self.written += take as u64;
            buf = &buf[take..];
            if self.pending.len() == CHUNK_BYTES {
                self.flush_pending()?;
            }
        }
        Ok(original)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flush_pending()
    }
}

fn produce(
    entries: Vec<FreshZipEntry>,
    level: Option<i64>,
    tx: mpsc::Sender<OutputMessage>,
    cancellation: &FreshZipCancellation,
    progress: Option<&FreshZipProgressObserver>,
) -> Result<u64, FreshZipError> {
    let output = ChannelWriter {
        tx,
        pending: Vec::with_capacity(CHUNK_BYTES),
        written: 0,
        cancellation: cancellation.clone(),
    };
    let mut zip = ZipWriter::new_stream(output);
    let mut source_bytes_done = 0;
    let mut entries_done = 0;
    for mut entry in entries {
        checkpoint(cancellation)?;
        let mut options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(level.map(|value| value.clamp(1, 9)))
            .large_file(needs_zip64(entry.size));
        if let Some(mode) = entry.unix_mode {
            options = options.unix_permissions(mode);
        }
        // A source with no mtime is dated now; the helper owns the ZIP convention
        // (local DOS time plus the exact UTC second), shared with the mutator.
        let modified = entry.modified.unwrap_or_else(std::time::SystemTime::now);
        let options = cmdr_archive::mutator::with_entry_mtime(options, modified);
        if entry.is_directory {
            zip.add_directory(entry.name.trim_end_matches('/'), options)
                .map_err(map_zip)?;
            entries_done += 1;
            report_progress(progress, entries_done, source_bytes_done)?;
            continue;
        }
        zip.start_file(entry.name.clone(), options).map_err(map_zip)?;
        // One byte past the plan is enough to know the source grew (a live log):
        // stop there instead of reading on to an EOF that may never come.
        let read_limit = entry.size.saturating_add(1);
        let actual = match &mut entry.source {
            FreshZipSource::Local(path) => {
                let mut file = std::fs::File::open(path).map_err(|error| FreshZipError::Source {
                    entry: entry.name.clone(),
                    message: error.to_string(),
                })?;
                copy_reader(
                    &mut (&mut file).take(read_limit),
                    &mut zip,
                    &entry.name,
                    cancellation,
                    &mut source_bytes_done,
                    entries_done,
                    progress,
                )?
            }
            FreshZipSource::Bytes(bytes) => copy_reader(
                &mut bytes.as_slice().take(read_limit),
                &mut zip,
                &entry.name,
                cancellation,
                &mut source_bytes_done,
                entries_done,
                progress,
            )?,
            FreshZipSource::Remote(source) => {
                let mut actual = 0;
                loop {
                    checkpoint(cancellation)?;
                    match source.rx.blocking_recv() {
                        Some(RemoteSourceMessage::Chunk(Ok(bytes))) => {
                            actual += bytes.len() as u64;
                            if actual > entry.size {
                                return Err(FreshZipError::CountMismatch {
                                    entry: entry.name.clone(),
                                    expected: entry.size,
                                    actual,
                                });
                            }
                            source_bytes_done += bytes.len() as u64;
                            zip.write_all(&bytes).map_err(map_io)?;
                            report_progress(progress, entries_done, source_bytes_done)?;
                        }
                        Some(RemoteSourceMessage::Chunk(Err(error))) => return Err(error),
                        Some(RemoteSourceMessage::Complete) => break,
                        None if cancellation.is_requested() => return Err(FreshZipError::Cancelled),
                        None => return Err(FreshZipError::SourceChannelClosed),
                    }
                }
                actual
            }
        };
        if actual != entry.size {
            return Err(FreshZipError::CountMismatch {
                entry: entry.name,
                expected: entry.size,
                actual,
            });
        }
        entries_done += 1;
        report_progress(progress, entries_done, source_bytes_done)?;
    }
    let mut output = zip.finish().map_err(map_zip)?.into_inner();
    output.flush_pending().map_err(map_io)?;
    output.send(OutputMessage::End).map_err(map_io)?;
    Ok(output.written)
}

fn copy_reader(
    reader: &mut dyn Read,
    output: &mut impl Write,
    entry: &str,
    cancellation: &FreshZipCancellation,
    source_bytes_done: &mut u64,
    entries_done: usize,
    progress: Option<&FreshZipProgressObserver>,
) -> Result<u64, FreshZipError> {
    let mut buffer = [0u8; CHUNK_BYTES];
    let mut total = 0;
    loop {
        checkpoint(cancellation)?;
        let count = reader.read(&mut buffer).map_err(|error| FreshZipError::Source {
            entry: entry.to_string(),
            message: error.to_string(),
        })?;
        if count == 0 {
            return Ok(total);
        }
        total += count as u64;
        *source_bytes_done += count as u64;
        output.write_all(&buffer[..count]).map_err(map_io)?;
        report_progress(progress, entries_done, *source_bytes_done)?;
    }
}

fn report_progress(
    observer: Option<&FreshZipProgressObserver>,
    entries_done: usize,
    source_bytes_done: u64,
) -> Result<(), FreshZipError> {
    if let Some(observer) = observer
        && observer(FreshZipProgress {
            entries_done,
            source_bytes_done,
        })
    {
        return Err(FreshZipError::Cancelled);
    }
    Ok(())
}

fn checkpoint(cancellation: &FreshZipCancellation) -> Result<(), FreshZipError> {
    if cancellation.is_requested() {
        Err(FreshZipError::Cancelled)
    } else {
        Ok(())
    }
}

/// Whether an entry of `size` uncompressed bytes could need ZIP64 sizes. Stream
/// mode commits to the choice in the local header, before a byte is compressed,
/// and `zip` refuses the data descriptor once the COMPRESSED size passes 4 GiB,
/// so the decision uses deflate's worst-case growth, not the input size.
fn needs_zip64(size: u64) -> bool {
    deflate_bound(size) >= zip::ZIP64_BYTES_THR
}

/// zlib's conservative deflate upper bound, valid for any window and memory
/// setting: `n + ceil(n/8) + ceil(n/64) + 5`. Incompressible input can really
/// grow by an eighth: level 1 codes literals with 9-bit static Huffman
/// (verified against `zlib-rs` 0.6.5 `deflate::bound`, the backend `zip` 8.6's
/// `deflate` feature selects, 2026-09-29). It costs a 20-byte ZIP64 extra on
/// entries between roughly 3.76 and 4 GiB.
fn deflate_bound(size: u64) -> u64 {
    size.saturating_add(size.saturating_add(7) >> 3)
        .saturating_add(size.saturating_add(63) >> 6)
        .saturating_add(5)
}

fn map_zip(error: zip::result::ZipError) -> FreshZipError {
    FreshZipError::Zip(error.to_string())
}

fn map_io(error: std::io::Error) -> FreshZipError {
    match error.kind() {
        std::io::ErrorKind::Interrupted => FreshZipError::Cancelled,
        std::io::ErrorKind::BrokenPipe => FreshZipError::OutputClosed,
        _ => FreshZipError::Zip(error.to_string()),
    }
}

#[cfg(test)]
#[path = "fresh_zip_tests.rs"]
mod tests;
