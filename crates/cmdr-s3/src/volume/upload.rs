//! The vocabulary both upload engines share: the one-PUT path in `writes.rs`
//! and the multipart engine in `multipart_upload.rs`. Where a write goes
//! ([`WriteTarget`]), the user's hold on it ([`Progress`]), how a stream is cut
//! into parts ([`PartReader`]), and how often a failed request goes again
//! ([`retry_after`], which `batch.rs` and `server_copy.rs` reuse).

use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use cmdr_fs::error_detail::ErrorDetail;
use cmdr_fs::pluralize::pluralize_grouped;
use cmdr_fs::volume::{StreamLength, StreamWriteProgress, VolumeError, VolumeReadStream, WriteMode};

use super::upload_body::PauseHold;
use crate::ops::{ObjectMetadata, Overwrite};

/// How often an upload reports progress while its body is in flight.
pub(super) const PROGRESS_TICK: Duration = Duration::from_millis(200);

/// Where a write goes and how it may treat what's there.
pub(super) struct WriteTarget<'a> {
    pub bucket: &'a str,
    pub key: &'a str,
    /// The server-side path, what every error names.
    pub remote: &'a str,
    pub mode: WriteMode,
    pub metadata: &'a ObjectMetadata,
}

/// What a write that fits one part does with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShortStream {
    /// One PUT: three requests for a small file is waste.
    OnePut,
    /// A one-part multipart upload, which publishes nothing until its
    /// completion: an overwrite on a server that may publish a cut-off PUT.
    /// An empty stream still goes as one PUT, having nothing to cut short.
    OnePart,
}

/// The overwrite a write mode asks of a conditional builder.
pub(super) fn overwrite_for(mode: WriteMode) -> Overwrite {
    match mode {
        WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => Overwrite::Refuse,
        WriteMode::CreateOrReplace => Overwrite::Replace,
    }
}

/// The user's hold on a write: the progress callback (where a Cancel
/// arrives), with what it shows held monotonic so a part sent again doesn't
/// move the bar backwards, and the operation's pause.
pub(super) struct Progress<'a> {
    report: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    expected: StreamLength,
    shown: AtomicU64,
    pub(super) pause: PauseHold,
}

impl<'a> Progress<'a> {
    pub(super) fn new(
        report: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
        expected: StreamLength,
        pause: PauseHold,
    ) -> Self {
        Self {
            report,
            expected,
            shown: AtomicU64::new(0),
            pause,
        }
    }

    /// Reports `sent` bytes (or the most shown so far, if more), answering
    /// whether the caller wants to stop.
    pub(super) fn at(&self, sent: u64) -> ControlFlow<()> {
        let shown = self.shown.fetch_max(sent, Ordering::Relaxed).max(sent);
        (self.report)(StreamWriteProgress {
            bytes_written: shown,
            expected_length: self.expected,
        })
    }
}

/// The source yielded a different byte count than it promised.
pub(super) fn size_mismatch(remote: &str, got: u64, size: u64) -> VolumeError {
    VolumeError::IoError {
        message: ErrorDetail::path_then(
            remote,
            format!(
                ": the source promised {} and yielded {}",
                pluralize_grouped(size, "byte"),
                pluralize_grouped(got, "byte")
            ),
        ),
        raw_os_error: None,
    }
}

/// How long to wait before sending a failed part again, after `attempt`
/// failures; `None` once it has failed four times.
pub(super) fn retry_after(attempt: u32) -> Option<Duration> {
    match attempt {
        1 => Some(Duration::from_secs(1)),
        2 => Some(Duration::from_secs(2)),
        3 => Some(Duration::from_secs(4)),
        _ => None,
    }
}

/// Cuts a stream into parts of exactly the size asked for, carrying a piece
/// that straddles a boundary into the next part.
pub(super) struct PartReader {
    stream: Box<dyn VolumeReadStream>,
    carry: Vec<u8>,
    ended: bool,
    /// Bytes handed out in parts so far.
    pub(super) taken: u64,
}

impl PartReader {
    pub(super) fn new(stream: Box<dyn VolumeReadStream>) -> Self {
        Self {
            stream,
            carry: Vec::new(),
            ended: false,
            taken: 0,
        }
    }

    /// The next `want` bytes, or fewer only at the source's end (empty when
    /// it had nothing left).
    #[cfg(test)]
    pub(super) async fn fill(&mut self, want: usize) -> Result<Vec<u8>, VolumeError> {
        self.fill_with(want, &mut || Ok(())).await
    }

    /// The next `want` bytes, as `fill` reads them, calling `between` after
    /// every piece it pulls and every [`PROGRESS_TICK`] while one is pending,
    /// so a slow or stalled source still reports progress and can be
    /// cancelled ([`Self::pull_with`]).
    pub(super) async fn fill_with(
        &mut self,
        want: usize,
        between: &mut (dyn FnMut() -> Result<(), VolumeError> + Send),
    ) -> Result<Vec<u8>, VolumeError> {
        while self.carry.len() < want && !self.ended {
            self.pull_with(between).await?;
        }
        let rest = self.carry.split_off(want.min(self.carry.len()));
        let part = std::mem::replace(&mut self.carry, rest);
        self.taken += part.len() as u64;
        Ok(part)
    }

    /// Whether the source has nothing left, reading one piece ahead to find
    /// out (kept for the next `fill`).
    #[cfg(test)]
    pub(super) async fn at_end(&mut self) -> Result<bool, VolumeError> {
        self.at_end_with(&mut || Ok(())).await
    }

    /// Whether the source has nothing left, as `at_end` finds out, calling
    /// `between` the way `fill_with` does: a source that stalls right at its
    /// end still reports progress and can be cancelled.
    pub(super) async fn at_end_with(
        &mut self,
        between: &mut (dyn FnMut() -> Result<(), VolumeError> + Send),
    ) -> Result<bool, VolumeError> {
        while self.carry.is_empty() && !self.ended {
            self.pull_with(between).await?;
        }
        Ok(self.carry.is_empty())
    }

    /// One `pull`, calling `between` every [`PROGRESS_TICK`] while it's
    /// pending and once after it lands. ❗ A `next_chunk` is dropped half-read
    /// only when `between` fails, which ends the upload: no later part reads
    /// past the lost bytes.
    async fn pull_with(
        &mut self,
        between: &mut (dyn FnMut() -> Result<(), VolumeError> + Send),
    ) -> Result<(), VolumeError> {
        let start = tokio::time::Instant::now() + PROGRESS_TICK;
        let mut tick = tokio::time::interval_at(start, PROGRESS_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let pull = self.pull();
        tokio::pin!(pull);
        loop {
            tokio::select! {
                pulled = &mut pull => {
                    pulled?;
                    break;
                }
                _ = tick.tick() => between()?,
            }
        }
        between()
    }

    /// Every byte out of the source so far, the read-ahead included.
    pub(super) fn read_so_far(&self) -> u64 {
        self.taken + self.carry.len() as u64
    }

    async fn pull(&mut self) -> Result<(), VolumeError> {
        match self.stream.next_chunk().await {
            Some(Ok(chunk)) => self.carry.extend_from_slice(&chunk),
            Some(Err(e)) => return Err(e),
            None => self.ended = true,
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "upload_test.rs"]
mod upload_test;
