//! Streaming, chunk-by-chunk reads of a single archive entry.
//!
//! Decompression is CPU-bound, so it must not run on the async executor
//! (project principle 3: never block the runtime). The design mirrors the SMB
//! backend's channel-backed read stream (`backends/DETAILS.md` § Pattern B):
//!
//! - A `spawn_blocking` producer owns the byte source and drives rc-zip's
//!   [`EntryFsm`], which reads compressed bytes and decompresses them, entirely
//!   off the executor.
//! - It pushes decompressed chunks (≤ [`CHUNK_SIZE`]) through a bounded channel,
//!   so peak memory is `capacity × CHUNK_SIZE` regardless of entry size — never
//!   the whole entry (project principle 5, and the trait's "must stream" rule).
//! - The consumer ([`ArchiveEntryReader::next_chunk`]) awaits the channel.
//!   Dropping the reader drops the receiver; the producer's next `send` fails
//!   and it stops — that's the cancel path, no extra signalling needed.
//!
//! Concurrency: each reader owns its own `EntryFsm` and read offset over a
//! shared `Arc<dyn ArchiveByteSource>`; `read_at` is a positioned read with no
//! shared cursor, so concurrent entry reads don't contend.

use std::sync::Arc;

use rc_zip::Entry;
use rc_zip::fsm::{EntryFsm, FsmResult};
use tokio::sync::mpsc;

use super::error::ArchiveError;
use super::source::ArchiveByteSource;

/// Max decompressed bytes handed back per chunk. Also the producer's decompress
/// scratch size, so a single `next_chunk` never yields more than this.
const CHUNK_SIZE: usize = 128 * 1024;

/// Channel depth. Peak in-flight decompressed memory is `CHANNEL_CAPACITY ×
/// CHUNK_SIZE` (~512 KiB), independent of the entry's uncompressed size.
const CHANNEL_CAPACITY: usize = 4;

/// A streaming reader over one decompressed archive entry.
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

impl ArchiveEntryReader {
    /// Spawns the blocking producer for `entry` and returns the reader. The
    /// producer starts immediately and fills the channel up to its capacity,
    /// then parks on backpressure until the consumer pulls.
    pub(super) fn spawn(source: Arc<dyn ArchiveByteSource>, entry: Entry) -> Self {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        let total_size = entry.uncompressed_size;

        tokio::task::spawn_blocking(move || run_producer(source, entry, tx));

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

    /// Returns the next decompressed chunk, `None` at end of entry, or an error
    /// if decompression or the byte source failed. After `None` or an error the
    /// reader is spent; don't call again.
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

/// The blocking producer: drives the entry state machine to completion, sending
/// each decompressed chunk over `tx`. Runs on a `spawn_blocking` thread.
fn run_producer(source: Arc<dyn ArchiveByteSource>, entry: Entry, tx: mpsc::Sender<Result<Vec<u8>, ArchiveError>>) {
    // The entry data begins at its local header; the fsm parses that header
    // then the compressed data as one forward stream.
    let mut offset = entry.header_offset;
    let mut fsm = EntryFsm::new(Some(entry), None);
    let mut out = vec![0u8; CHUNK_SIZE];
    // The fsm reads ahead (its buffer always has spare room), so it asks to read
    // past the entry's own bytes into the central directory — and reaches the
    // real file end even for a complete entry. So EOF alone isn't truncation;
    // only EOF *plus* a `process` that then makes no progress is.
    let mut at_eof = false;

    loop {
        if fsm.wants_read() && !at_eof {
            let space = fsm.space();
            let n = match source.read_at(offset, space) {
                Ok(n) => n,
                Err(err) => {
                    // Best-effort: if the consumer is gone, the send fails and
                    // we simply stop.
                    let _ = tx.blocking_send(Err(ArchiveError::from(err)));
                    return;
                }
            };
            if n == 0 {
                at_eof = true;
            } else {
                offset += n as u64;
                fsm.fill(n);
            }
        }

        match fsm.process(&mut out) {
            Ok(FsmResult::Continue((next, outcome))) => {
                fsm = next;
                if outcome.bytes_written > 0 {
                    if tx.blocking_send(Ok(out[..outcome.bytes_written].to_vec())).is_err() {
                        // Consumer dropped the reader: cancel.
                        return;
                    }
                } else if at_eof && outcome.bytes_read == 0 {
                    // At EOF with no bytes consumed or produced: the fsm still
                    // wants input it can't get — the entry is truncated. Bail
                    // with a typed error instead of spinning.
                    let _ = tx.blocking_send(Err(ArchiveError::Corrupt(
                        "archive entry data is truncated".to_string(),
                    )));
                    return;
                }
            }
            Ok(FsmResult::Done(_)) => return,
            Err(err) => {
                let _ = tx.blocking_send(Err(ArchiveError::from(err)));
                return;
            }
        }
    }
}
