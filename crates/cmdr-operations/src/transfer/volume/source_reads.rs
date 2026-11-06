//! What the engine learns from watching a source's reads: which side failed,
//! and whether the source delivered the length it declared.
//!
//! A destination's `write_from_stream` pulls the source's chunks itself, so a
//! read that fails mid-file comes back as the WRITE's error, with nothing in it
//! saying which volume broke. [`watch`] wraps the source stream before it's
//! handed over and remembers whether one of its reads failed, so the engine
//! labels the failure with the side that actually refused
//! (`transfer_error.rs::PathedVolumeError`).
//!
//! The same wrapper holds the source to its declared length (#538):
//! `StreamLength::Known(n)` promises exactly `n` bytes, and a destination that
//! copies until EOF would otherwise land a short or overlong file under the
//! user's name as done (and a move would then delete the original). A stream
//! that runs past `n` fails on the chunk that crosses it, and one that ends
//! short fails in place of its EOF, both as `SourceChanged`, so the
//! destination's write fails, drops its partial, and nothing lands. The check
//! sits in the stream rather than after the write so it reaches a SINGLE-SHOT
//! write too, which drains its source before anything touches the real name.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::transfer_error::{PathRole, PathedVolumeError};
use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses};
use cmdr_fs::pluralize::pluralize_grouped;
use cmdr_fs::volume::{ScanStop, StreamLength, VolumeError, VolumeReadStream};

/// Whether a watched source stream failed a read. Shared with the stream, which
/// the destination owns once the write starts.
pub(super) struct SourceReads(Arc<AtomicBool>);

impl SourceReads {
    /// For a write that reads no stream (a link's target is already in hand):
    /// nothing it can fail on is the source's.
    pub(super) fn unwatched() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// For a source that failed before any stream existed (a resume refused at
    /// its open): the failure is the source's.
    pub(super) fn failed() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }

    /// Labels a failed `write_from_stream` with the side that broke: the source,
    /// at `source_path`, when one of its reads failed, else the destination, at
    /// `dest_path`.
    pub(super) fn label(&self, error: VolumeError, source_path: &Path, dest_path: &Path) -> PathedVolumeError {
        if self.0.load(Ordering::Acquire) {
            PathedVolumeError::new(PathRole::Source, source_path, error)
        } else {
            PathedVolumeError::new(PathRole::Destination, dest_path, error)
        }
    }
}

/// Wraps `stream`, read from `source_path`, so a failed read is remembered and
/// a stream that doesn't deliver `declared` fails. Everything else forwards
/// untouched (a wrapper stream forwards its inner stream's answers).
pub(super) fn watch(
    stream: Box<dyn VolumeReadStream>,
    source_path: &Path,
    declared: StreamLength,
) -> (Box<dyn VolumeReadStream>, SourceReads) {
    let failed = Arc::new(AtomicBool::new(false));
    let watched = Box::new(WatchedStream {
        inner: stream,
        failed: Arc::clone(&failed),
        source_path: source_path.display().to_string(),
        declared,
        delivered: 0,
    });
    (watched, SourceReads(failed))
}

struct WatchedStream {
    inner: Box<dyn VolumeReadStream>,
    failed: Arc<AtomicBool>,
    /// What a `SourceChanged` names.
    source_path: String,
    declared: StreamLength,
    /// Bytes handed out so far.
    delivered: u64,
}

impl WatchedStream {
    /// The chunk as the destination should see it: a stream that broke its
    /// declared length turns into `SourceChanged`.
    fn held_to_length(&mut self, chunk: Option<Result<Vec<u8>, VolumeError>>) -> Option<Result<Vec<u8>, VolumeError>> {
        let StreamLength::Known(declared) = self.declared else {
            return chunk;
        };
        match chunk {
            Some(Ok(bytes)) => {
                self.delivered = self.delivered.saturating_add(bytes.len() as u64);
                if self.delivered > declared {
                    return Some(Err(self.length_broken(declared)));
                }
                Some(Ok(bytes))
            }
            None if self.delivered < declared => Some(Err(self.length_broken(declared))),
            other => other,
        }
    }

    fn length_broken(&self, declared: u64) -> VolumeError {
        log::warn!(
            target: "copy",
            "source_reads: {} declared {} and delivered {}{}; failing the file rather than landing it",
            cmdr_fs::log_private::LogPrivate::file_path(&self.source_path),
            pluralize_grouped(declared, "byte"),
            pluralize_grouped(self.delivered, "byte"),
            if self.delivered > declared { " and more" } else { "" },
        );
        VolumeError::SourceChanged(self.source_path.clone())
    }
}

impl VolumeReadStream for WatchedStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let chunk = self.inner.next_chunk().await;
            let chunk = self.held_to_length(chunk);
            if matches!(chunk, Some(Err(_))) {
                self.failed.store(true, Ordering::Release);
            }
            chunk
        })
    }

    fn total_size(&self) -> StreamLength {
        self.inner.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.inner.bytes_read()
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        self.inner.modified_at()
    }

    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        self.inner.metadata(wanted)
    }

    fn metadata_outline(&self, wanted: MetadataClasses) -> CarriedMetadata {
        self.inner.metadata_outline(wanted)
    }

    fn stop_signal(&self) -> ScanStop {
        self.inner.stop_signal()
    }

    fn cancel_and_release(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.inner.cancel_and_release()
    }
}
