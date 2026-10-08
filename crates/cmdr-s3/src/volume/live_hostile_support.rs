//! What the hostile live cells share (`live_hostile_test.rs`,
//! `live_hostile_failure_test.rs`): a generated source that never holds a big
//! object whole, a byte-for-byte read-back, cancelling write and copy hooks,
//! and the per-provider miss collector that fails a cell once, at the end.

use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use cmdr_fs::volume::{ServerCopyProgress, StreamLength, Volume, VolumeError, VolumeReadStream, WriteMode};

use super::S3Volume;
use super::live_support::{Live, MIB, report};
use super::testing::{BytesSource, distant_mtime};

pub(super) fn at(volume: &S3Volume, key: &str) -> PathBuf {
    volume.root().join(key)
}

/// The byte at `index` of [`pattern`]`(_, tag)`, so a big object is checked
/// without holding it.
pub(super) fn pattern_byte(index: u64, tag: u8) -> u8 {
    tag.wrapping_add((index % 251) as u8)
}

/// [`pattern`] bytes generated a MiB at a time, never held whole, optionally
/// sleeping before each piece so an upload stays mid-flight.
pub(super) struct PatternSource {
    len: u64,
    offset: u64,
    tag: u8,
    pace: Option<Duration>,
}

impl PatternSource {
    pub(super) fn new(len: u64, tag: u8) -> Self {
        Self {
            len,
            offset: 0,
            tag,
            pace: None,
        }
    }

    pub(super) fn paced(mut self, pace: Duration) -> Self {
        self.pace = Some(pace);
        self
    }
}

impl VolumeReadStream for PatternSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.offset >= self.len {
                return None;
            }
            if let Some(pace) = self.pace {
                tokio::time::sleep(pace).await;
            }
            let end = (self.offset + MIB as u64).min(self.len);
            let piece = (self.offset..end).map(|i| pattern_byte(i, self.tag)).collect();
            self.offset = end;
            Some(Ok(piece))
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.len)
    }

    fn bytes_read(&self) -> u64 {
        self.offset
    }

    fn modified_at(&self) -> Option<SystemTime> {
        Some(distant_mtime())
    }
}

/// Streams `path` back and compares it with [`pattern`]`(len, tag)` byte by
/// byte: `None` when intact, else what differed.
pub(super) async fn pattern_mismatch(volume: &S3Volume, path: &Path, len: u64, tag: u8) -> Option<String> {
    let mut stream = match volume.open_read_stream(path).await {
        Ok(stream) => stream,
        Err(e) => return Some(format!("open: {e:?}")),
    };
    let mut offset = 0u64;
    while let Some(piece) = stream.next_chunk().await {
        let piece = match piece {
            Ok(piece) => piece,
            Err(e) => return Some(format!("read at {offset}: {e:?}")),
        };
        for byte in piece {
            if offset >= len {
                return Some(format!("longer than {len}"));
            }
            if byte != pattern_byte(offset, tag) {
                return Some(format!("byte {offset} differs")); // allowed-pluralize-noun: an offset, not a count
            }
            offset += 1;
        }
    }
    (offset != len).then(|| format!("{offset} bytes of {len}")) // allowed-pluralize-noun: a test diagnostic about a short read
}

pub(super) async fn write(volume: &S3Volume, path: &Path, mode: WriteMode, bytes: Vec<u8>) -> Result<u64, VolumeError> {
    let source = BytesSource::new(bytes).modified_at(distant_mtime());
    let length = source.total_size();
    volume
        .write_from_stream(path, mode, length, Box::new(source), &|_| ControlFlow::Continue(()))
        .await
}

/// A write whose progress hook answers `Break` once `stop` says so.
pub(super) async fn write_cancelled_when(
    volume: &S3Volume,
    path: &Path,
    mode: WriteMode,
    source: Box<dyn VolumeReadStream>,
    stop: impl Fn(u64) -> bool + Sync,
) -> Result<u64, VolumeError> {
    let length = source.total_size();
    volume
        .write_from_stream(path, mode, length, source, &|progress| {
            if stop(progress.bytes_written) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .await
}

/// A server-side copy's hook: cancels at checkpoint number `cancel_at` (1 is
/// before the upload is created), or when `advanced` passes `cancel_after`
/// bytes; counts what it saw.
pub(super) struct CopyHook {
    pub(super) cancel_at: Option<u32>,
    pub(super) cancel_after: Option<u64>,
    pub(super) checkpoints: AtomicU32,
    pub(super) done: AtomicU64,
}

impl CopyHook {
    pub(super) fn new(cancel_at: Option<u32>, cancel_after: Option<u64>) -> Self {
        Self {
            cancel_at,
            cancel_after,
            checkpoints: AtomicU32::new(0),
            done: AtomicU64::new(0),
        }
    }
}

impl ServerCopyProgress for CopyHook {
    fn advanced(&self, done: u64, _total: u64) -> ControlFlow<()> {
        self.done.store(done, Ordering::Relaxed);
        match self.cancel_after {
            Some(after) if done >= after => ControlFlow::Break(()),
            _ => ControlFlow::Continue(()),
        }
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        let n = self.checkpoints.fetch_add(1, Ordering::Relaxed) + 1;
        let stop = self.cancel_at == Some(n);
        Box::pin(async move {
            if stop {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
    }
}

/// What one provider got wrong in a cell; empty means it passed.
pub(super) struct Misses<'a> {
    pub(super) live: &'a Live,
    pub(super) misses: Vec<String>,
}

impl<'a> Misses<'a> {
    pub(super) fn new(live: &'a Live) -> Self {
        Self {
            live,
            misses: Vec::new(),
        }
    }

    /// Records `what` as a miss unless `ok`, and prints it either way.
    pub(super) fn check(&mut self, ok: bool, what: &str, finding: impl std::fmt::Display) {
        let line = format!("{finding}");
        report(self.live, what, if ok { line.clone() } else { format!("MISS {line}") });
        if !ok {
            self.misses.push(format!("{what}: {line}"));
        }
    }
}

/// Fails the cell with every provider's misses at once.
pub(super) fn verdict_of(all: Vec<(String, Vec<String>)>) {
    let failed: Vec<String> = all
        .into_iter()
        .filter(|(_, misses)| !misses.is_empty())
        .map(|(name, misses)| format!("[{name}]\n  {}", misses.join("\n  ")))
        .collect();
    assert!(failed.is_empty(), "misses:\n{}", failed.join("\n"));
}

/// The unfinished uploads under `prefix`, or `Ok(empty)` on R2 (whose
/// bucket-scoped key lists none, ever).
pub(super) async fn leftover_uploads(live: &Live, prefix: &str) -> Result<Vec<(String, String)>, String> {
    live.uploads_under(&live.client(), prefix).await
}

/// The ledger's open records under `prefix` for this volume's account.
pub(super) fn ledger_open(volume: &S3Volume, prefix: &str) -> usize {
    volume.inner.ledger.open_under(&volume.inner.account(), prefix).len()
}
