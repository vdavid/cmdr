//! Streaming, chunk-by-chunk reads of a single archive entry — format-agnostic.
//!
//! Decompression is CPU-bound, so it must not run on the async executor
//! (project principle 3: never block the runtime). The design mirrors the SMB
//! backend's channel-backed read stream (`backends/DETAILS.md` § Pattern B):
//!
//! - A `spawn_blocking` producer owns the byte source and decodes the entry
//!   entirely off the executor (the per-format producer lives in that format's
//!   module — `zip.rs`, `tar.rs`, `sevenz.rs`).
//! - It pushes decoded chunks (≤ [`CHUNK_SIZE`]) through a bounded channel, so
//!   peak memory is `capacity × CHUNK_SIZE` regardless of entry size — never the
//!   whole entry (project principle 5, and the trait's "must stream" rule).
//! - The consumer ([`ArchiveEntryReader::next_chunk`]) awaits the channel.
//!   Dropping the reader drops the receiver; the producer's next `send` fails and
//!   it stops — that's the cancel path, no extra signalling needed.
//!
//! Concurrency: each reader owns its decode state and read offset over a shared
//! `Arc<dyn ArchiveByteSource>`; `read_at` is a positioned read with no shared
//! cursor, so concurrent entry reads don't contend.

use std::io::Read;

use tokio::sync::mpsc;

use super::error::ArchiveError;

/// Max decoded bytes handed back per chunk. Also the producer's scratch size, so
/// a single `next_chunk` never yields more than this.
pub(super) const CHUNK_SIZE: usize = 128 * 1024;

/// Channel depth. Peak in-flight decoded memory is `CHANNEL_CAPACITY ×
/// CHUNK_SIZE` (~512 KiB), independent of the entry's uncompressed size.
const CHANNEL_CAPACITY: usize = 4;

/// A streaming reader over one decoded archive entry.
///
/// Yields chunks via [`next_chunk`](Self::next_chunk) until `None` (end of
/// entry) or an error. Reports the entry's full uncompressed size up front and
/// tracks bytes delivered so far, matching what a volume read stream needs.
#[derive(Debug)]
pub struct ArchiveEntryReader {
    rx: mpsc::Receiver<Result<Vec<u8>, ArchiveError>>,
    total_size: u64,
    bytes_read: u64,
    finished: bool,
}

/// The producer's send side: a chunk sink with backpressure. `send` returns
/// `false` once the consumer has dropped the reader (the cancel signal); the
/// producer then stops. Blocking, so it's only ever used from the
/// `spawn_blocking` producer thread.
pub(super) struct ChunkTx(mpsc::Sender<Result<Vec<u8>, ArchiveError>>);

impl ChunkTx {
    /// Sends one decoded chunk. Returns `false` if the consumer dropped the
    /// reader — the producer must stop (that's the cancel path).
    pub(super) fn send(&self, chunk: Vec<u8>) -> bool {
        self.0.blocking_send(Ok(chunk)).is_ok()
    }

    /// Reports a decode / byte-source failure to the consumer (best-effort — if
    /// the consumer is gone the send fails and it's a no-op).
    pub(super) fn send_err(&self, err: ArchiveError) {
        let _ = self.0.blocking_send(Err(err));
    }
}

impl ArchiveEntryReader {
    /// Spawns a blocking `producer` that decodes an entry of `total_size`
    /// uncompressed bytes, sending chunks through the returned reader's channel.
    /// The producer starts immediately and fills the channel up to capacity,
    /// then parks on backpressure until the consumer pulls.
    pub(super) fn spawn_with<F>(total_size: u64, producer: F) -> Self
    where
        F: FnOnce(ChunkTx) + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        tokio::task::spawn_blocking(move || producer(ChunkTx(tx)));
        Self {
            rx,
            total_size,
            bytes_read: 0,
            finished: false,
        }
    }

    /// The full uncompressed size of the entry, in bytes.
    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    /// Bytes delivered to the consumer so far.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Returns the next decoded chunk, `None` at end of entry, or an error if
    /// decoding or the byte source failed. After `None` or an error the reader is
    /// spent; don't call again.
    pub async fn next_chunk(&mut self) -> Option<Result<Vec<u8>, ArchiveError>> {
        if self.finished {
            return None;
        }
        match self.rx.recv().await {
            Some(Ok(chunk)) => {
                self.bytes_read += chunk.len() as u64;
                Some(Ok(chunk))
            }
            Some(Err(err)) => {
                self.finished = true;
                Some(Err(err))
            }
            None => {
                self.finished = true;
                None
            }
        }
    }
}

/// Pumps a decoded byte stream (`reader`) into the chunk sink in ≤ [`CHUNK_SIZE`]
/// blocks, so peak memory stays bounded regardless of the entry's size. Used by
/// the tar and 7z producers, whose codecs expose a pull-model [`Read`]. Stops
/// early (returns) if the consumer drops the reader — the cancel path. A read
/// error is forwarded as a typed [`ArchiveError`].
///
/// `limit`, when `Some`, caps the number of bytes pumped: a tar member's data is
/// followed by padding and the next header in the same stream, so the caller
/// must stop at the member's exact size rather than reading to EOF.
pub(super) fn pump_read(mut reader: impl Read, tx: &ChunkTx, limit: Option<u64>) {
    let mut remaining = limit;
    let mut buf = vec![0u8; CHUNK_SIZE];
    loop {
        let want = match remaining {
            Some(0) => return,
            Some(n) => (n.min(CHUNK_SIZE as u64)) as usize,
            None => CHUNK_SIZE,
        };
        match reader.read(&mut buf[..want]) {
            Ok(0) => return,
            Ok(n) => {
                if let Some(rem) = remaining.as_mut() {
                    *rem -= n as u64;
                }
                if !tx.send(buf[..n].to_vec()) {
                    return; // consumer dropped the reader: cancel
                }
            }
            Err(err) => {
                tx.send_err(ArchiveError::from(err));
                return;
            }
        }
    }
}
