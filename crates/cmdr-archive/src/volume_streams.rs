//! The stream adapters behind [`ArchiveVolume`](super::ArchiveVolume): the
//! reading core's readers wrapped as the `Volume` trait's streams, and the
//! parent-backed byte source a remote archive reads through.
//!
//! None of them touches `ArchiveVolume`'s fields; they depend only on the
//! reading core's types and [`to_volume_error`].

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::to_volume_error;
use crate::{ArchiveByteSource, ArchiveEntryReader, SubtreeExtractReader};
use cmdr_fs::volume::{ExtractedFile, SequentialExtract, Volume, VolumeError, VolumeReadStream};

/// Wraps an [`ArchiveEntryReader`] as a [`VolumeReadStream`], mapping the core's
/// [`ArchiveError`](crate::ArchiveError) to [`VolumeError`] and, for a resumed read, discarding the
/// leading `skip_remaining` decompressed bytes before yielding.
///
/// A compressed entry has no random access, so a non-zero start offset means
/// "decompress from the beginning and drop the prefix" — correct, if not cheap.
/// Nothing calls the at-offset path with a non-zero offset today (see the trait
/// docs), so the common `skip_remaining == 0` path never drops anything.
pub(super) struct ArchiveVolumeReadStream {
    pub(super) reader: ArchiveEntryReader,
    pub(super) skip_remaining: u64,
    /// Decompressed bytes handed to the consumer (this segment), for progress.
    pub(super) delivered: u64,
    /// The entry's date as the archive's index recorded it.
    pub(super) modified_at: Option<SystemTime>,
}

/// An archive's recorded Unix-seconds date as a `SystemTime`. A pre-1970 date is
/// dropped, the same as the listing does (`node_to_entry`), so the stream and
/// the listing never disagree.
pub(super) fn recorded_date(unix_secs: Option<i64>) -> Option<SystemTime> {
    let secs = u64::try_from(unix_secs?).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(secs))
}

impl VolumeReadStream for ArchiveVolumeReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            loop {
                match self.reader.next_chunk().await {
                    Some(Ok(mut chunk)) => {
                        if self.skip_remaining > 0 {
                            let drop_count = (self.skip_remaining as usize).min(chunk.len());
                            self.skip_remaining -= drop_count as u64;
                            chunk.drain(..drop_count);
                            if chunk.is_empty() {
                                continue;
                            }
                        }
                        self.delivered += chunk.len() as u64;
                        return Some(Ok(chunk));
                    }
                    Some(Err(err)) => return Some(Err(to_volume_error(err))),
                    None => return None,
                }
            }
        })
    }

    /// The entry's FULL uncompressed size (not the remaining tail), so a resumed
    /// transfer's progress stays anchored to the whole file, per the trait.
    fn total_size(&self) -> cmdr_fs::volume::StreamLength {
        cmdr_fs::volume::StreamLength::Known(self.reader.total_size())
    }

    fn bytes_read(&self) -> u64 {
        self.delivered
    }

    fn modified_at(&self) -> Option<SystemTime> {
        self.modified_at
    }
}

/// The [`SequentialExtract`] over an archive subtree: the [`Volume`] adapter for
/// the reading core's one-pass [`SubtreeExtractReader`]. Maps the core's
/// [`ArchiveError`](crate::ArchiveError) to [`VolumeError`] and each core member's inner path back to a
/// full source path (`archive_path/inner`, matching what `list_directory`
/// reports, so the copy planner's per-file lookup keys line up).
///
/// The core reader is shared behind an `Arc<tokio::sync::Mutex<…>>` so
/// [`current_stream`](SequentialExtract::current_stream) can hand out an OWNED
/// [`VolumeReadStream`] (what `write_from_stream` takes) that still pulls from the
/// one decoder. Usage is strictly serial (advance, then drain the member's
/// stream, then advance), so the mutex is never contended — it's there for
/// ownership and `Send`, not concurrency.
pub(super) struct ArchiveSequentialExtract {
    reader: Arc<tokio::sync::Mutex<SubtreeExtractReader>>,
    archive_path: PathBuf,
    /// Uncompressed size of the member the last `next_file` returned, so
    /// `current_stream` can report `total_size()` without touching the reader.
    current_size: u64,
    /// That member's date, for the same reason.
    current_modified_at: Option<SystemTime>,
}

impl ArchiveSequentialExtract {
    pub(super) fn new(reader: SubtreeExtractReader, archive_path: PathBuf) -> Self {
        Self {
            reader: Arc::new(tokio::sync::Mutex::new(reader)),
            archive_path,
            current_size: 0,
            current_modified_at: None,
        }
    }
}

impl SequentialExtract for ArchiveSequentialExtract {
    fn next_file(&mut self) -> Pin<Box<dyn Future<Output = Result<Option<ExtractedFile>, VolumeError>> + Send + '_>> {
        Box::pin(async move {
            let mut reader = self.reader.lock().await;
            match reader.next_member().await.map_err(to_volume_error)? {
                Some(member) => {
                    self.current_size = member.size;
                    self.current_modified_at = recorded_date(member.modified);
                    Ok(Some(ExtractedFile {
                        source_path: self.archive_path.join(&member.inner_path),
                        size: member.size,
                    }))
                }
                None => Ok(None),
            }
        })
    }

    fn current_stream(&self) -> Box<dyn VolumeReadStream> {
        Box::new(MemberStream {
            reader: Arc::clone(&self.reader),
            total: self.current_size,
            delivered: 0,
            modified_at: self.current_modified_at,
        })
    }
}

/// A [`VolumeReadStream`] over ONE member of the shared one-pass extractor. Pulls
/// the current member's chunks from the shared core reader until it ends (the
/// core's `next_chunk` returns `None` at the member boundary), then reports EOF.
struct MemberStream {
    reader: Arc<tokio::sync::Mutex<SubtreeExtractReader>>,
    total: u64,
    delivered: u64,
    modified_at: Option<SystemTime>,
}

impl VolumeReadStream for MemberStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let mut reader = self.reader.lock().await;
            match reader.next_chunk().await {
                Ok(Some(chunk)) => {
                    self.delivered += chunk.len() as u64;
                    Some(Ok(chunk))
                }
                Ok(None) => None,
                Err(err) => Some(Err(to_volume_error(err))),
            }
        })
    }

    fn total_size(&self) -> cmdr_fs::volume::StreamLength {
        cmdr_fs::volume::StreamLength::Known(self.total)
    }

    fn bytes_read(&self) -> u64 {
        self.delivered
    }

    fn modified_at(&self) -> Option<SystemTime> {
        self.modified_at
    }
}

/// An [`ArchiveByteSource`] backed by a parent [`Volume`]'s ranged read, for a
/// zip that lives on a REMOTE backend (direct SMB or MTP), where there's no
/// local file to `pread`.
///
/// The core's `read_at` is **blocking** (the parse and decompress run on
/// `spawn_blocking`), but a `Volume`'s [`read_range`](Volume::read_range) is
/// async. This bridges the two: it captures the tokio runtime handle at
/// construction (on the async executor, in
/// [`open_remote_source`](super::ArchiveVolume::open_remote_source)) and `block_on`s
/// the parent's `read_range` from inside the blocking read. That's sound because
/// `read_at` only ever runs on a `spawn_blocking` thread — never a runtime worker
/// — so `block_on` doesn't reenter the executor (the same bridge the viewer's
/// archive extractor uses). Shared as `Arc` across concurrent reads; `read_at`
/// takes `&self` with no shared cursor, so parallel entry reads are independent.
pub(super) struct VolumeByteSource {
    parent: Arc<dyn Volume>,
    /// Absolute path of the `.zip` on the parent volume.
    path: PathBuf,
    /// The archive's size, from the parent's metadata at construction. A read at
    /// or past it returns EOF, matching a local `pread`.
    size: u64,
    handle: tokio::runtime::Handle,
}

impl VolumeByteSource {
    pub(super) fn new(parent: Arc<dyn Volume>, path: PathBuf, size: u64) -> Self {
        Self {
            parent,
            path,
            size,
            handle: tokio::runtime::Handle::current(),
        }
    }
}

impl ArchiveByteSource for VolumeByteSource {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() || offset >= self.size {
            return Ok(0);
        }
        // Clamp the request to the known size so read-ahead past EOF (rc-zip's
        // fsm always leaves buffer room) doesn't ask the backend for bytes that
        // don't exist.
        let want = buf.len().min((self.size - offset) as usize);
        let parent = Arc::clone(&self.parent);
        let path = self.path.clone();
        let data = self
            .handle
            .block_on(async move { parent.read_range(&path, offset, want).await })
            .map_err(|err| std::io::Error::other(err.to_string()))?;
        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }
}
