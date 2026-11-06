//! A session's file read through its volume by position: a held random reader
//! (`Volume::open_random_reader`, SMB today), so a big file on a share is read by
//! positioned smb2 READs with no OS-mount open per fetch and no 256 MiB temp pull once
//! the mount is gone (#303), or the volume's `read_range` ([`RangeReader`]), which is how
//! a picture or PDF on any other server renders with no pull.
//!
//! The viewer core is sync and runs on blocking threads; each read bridges to the async
//! volume through `bridge.rs`.
//! `DETAILS.md` § "Byte sources".

use std::collections::VecDeque;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use log::debug;

use super::ViewerError;
use super::origin::ReplacementHook;
use super::source::{ByteSource, OpenedFile, file_name_of};
use super::volume_error::map_volume_error;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::rt::Runtime;
use cmdr_fs::volume::{Volume, VolumeError, VolumeRandomReader};

/// Bytes per cached block, and the smallest read the source asks the volume for.
///
/// One block answers a typical fetch (a 64 KiB row window plus the backward window of a
/// seek) in one round trip, and a fetch that walks on (a long-line file, up to the 2 MiB
/// chunk budget) costs one round trip per 256 KiB instead of one per 64 KiB. Small
/// enough that a jump on a slow link (10 Mbit/s: about 0.2 s) stays well inside the 2 s
/// fetch deadline.
pub(super) const BLOCK_BYTES: u64 = 256 * 1024;

/// Full blocks kept, most recent last: 4 MiB per session, room for the viewport, the
/// rows around it, and the open's head reads.
const CACHED_BLOCKS: usize = 16;

/// The volume's reader, closed on the async runtime once the last read holding it lets
/// go (a reopen swaps in a new one while an older read may still be running).
struct HeldReader {
    reader: Option<Box<dyn VolumeRandomReader>>,
    /// Where the close runs: the session's own runtime, never an ambient one (the last
    /// read may let go on a worker thread with none).
    runtime: Runtime,
}

impl HeldReader {
    fn new(reader: Box<dyn VolumeRandomReader>, runtime: &Runtime) -> Self {
        Self {
            reader: Some(reader),
            runtime: runtime.clone(),
        }
    }

    fn get(&self) -> &dyn VolumeRandomReader {
        self.reader.as_deref().expect("the reader is only taken in Drop")
    }
}

impl Drop for HeldReader {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take() {
            super::bridge::spawn_detached(&self.runtime, async move {
                if let Err(e) = reader.close().await {
                    debug!(
                        "viewer volume source: closing the reader failed: {e}",
                        e = cmdr_fs::log_error::LogError(&e)
                    );
                }
            });
        }
    }
}

/// Which of a volume's positioned reads a [`VolumeSource`] goes through.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PositionedRead {
    /// `Volume::open_random_reader`: one handle held for the session.
    Held,
    /// `Volume::read_range`: every read stands alone ([`RangeReader`]).
    Ranged,
}

/// [`VolumeRandomReader`] over `Volume::read_range`, for a volume that holds no handle.
/// The size comes from a stat at open, and reads clamp to it like any reader's.
struct RangeReader {
    volume: Arc<dyn Volume>,
    path: PathBuf,
    size: u64,
}

impl RangeReader {
    /// `NotSupported` when the stat doesn't say how big the file is: a reader that can't
    /// answer `size` can't serve a `Range` either, and the caller falls back to a pull.
    async fn open(volume: Arc<dyn Volume>, path: PathBuf) -> Result<Self, VolumeError> {
        let entry = volume.get_metadata(&path).await?;
        if entry.is_directory {
            return Err(VolumeError::IsADirectory(path.display().to_string()));
        }
        let size = entry.size.ok_or(VolumeError::NotSupported)?;
        Ok(Self { volume, path, size })
    }
}

impl VolumeRandomReader for RangeReader {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(
        &self,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + '_>> {
        Box::pin(async move {
            let mut bytes = self.volume.read_range(&self.path, offset, len).await?;
            bytes.truncate(self.size.saturating_sub(offset).min(len as u64) as usize);
            Ok(bytes)
        })
    }

    fn close(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
}

fn open_reader(
    volume: &Arc<dyn Volume>,
    path: &Path,
    way: PositionedRead,
    runtime: &Runtime,
) -> Result<Box<dyn VolumeRandomReader>, VolumeError> {
    super::bridge::block_on(runtime, async {
        match way {
            PositionedRead::Held => volume.open_random_reader(path).await,
            PositionedRead::Ranged => {
                let reader = RangeReader::open(Arc::clone(volume), path.to_path_buf()).await?;
                Ok(Box::new(reader) as Box<dyn VolumeRandomReader>)
            }
        }
    })
}

/// A [`ByteSource`] over a volume's positioned reads, with a small block cache.
pub struct VolumeSource {
    volume: Arc<dyn Volume>,
    path: PathBuf,
    way: PositionedRead,
    runtime: Runtime,
    reader: Mutex<Arc<HeldReader>>,
    /// Full blocks only, keyed by block index. A short block (the file's tail) is
    /// never cached, so an append is read fresh.
    cache: Mutex<VecDeque<(u64, Arc<Vec<u8>>)>>,
    /// Told when the file changed under the reader ([`ReplacementHook`]).
    replaced: ReplacementHook,
    /// The hook was told already: a reload is on its way, and the reads still
    /// failing on this source are the same news.
    reported: AtomicBool,
}

impl VolumeSource {
    /// Opens `path` on `volume` and returns the source with the file's size. Every read
    /// bridges to the volume on `runtime`.
    pub fn open(
        volume: Arc<dyn Volume>,
        path: &Path,
        way: PositionedRead,
        runtime: &Runtime,
    ) -> Result<(Self, u64), VolumeError> {
        let reader = open_reader(&volume, path, way, runtime)?;
        let size = reader.size();
        Ok((
            Self {
                volume,
                path: path.to_path_buf(),
                way,
                runtime: runtime.clone(),
                reader: Mutex::new(Arc::new(HeldReader::new(reader, runtime))),
                cache: Mutex::new(VecDeque::new()),
                replaced: ReplacementHook::default(),
                reported: AtomicBool::new(false),
            },
            size,
        ))
    }

    /// This source, telling `hook` when its file changed under the reader.
    #[must_use]
    pub(super) fn reporting_replacement_to(mut self, hook: ReplacementHook) -> Self {
        self.replaced = hook;
        self
    }

    fn current(&self) -> Arc<HeldReader> {
        Arc::clone(&self.reader.lock_ignore_poison())
    }

    /// A fresh reader in place of `stale`, unless a concurrent read already swapped one
    /// in. Drops the cache: the file a fresh open sees may differ.
    fn reopen(&self, stale: &Arc<HeldReader>) -> Result<Arc<HeldReader>, ViewerError> {
        let fresh =
            open_reader(&self.volume, &self.path, self.way, &self.runtime).map_err(|e| map_volume_error(e, None))?;
        // Held before the race check, so a reader that loses it still gets closed.
        let fresh = Arc::new(HeldReader::new(fresh, &self.runtime));
        let mut slot = self.reader.lock_ignore_poison();
        if !Arc::ptr_eq(&slot, stale) {
            return Ok(Arc::clone(&slot));
        }
        *slot = fresh;
        self.cache.lock_ignore_poison().clear();
        Ok(Arc::clone(&slot))
    }

    /// One positioned read, reopening once on an error: a handle may not survive the
    /// share reconnecting under it. A file that changed under the reader is the
    /// exception: the session reloads instead ([`ReplacementHook`]).
    fn read_span(&self, reader: &Arc<HeldReader>, offset: u64, len: usize) -> Result<Vec<u8>, ViewerError> {
        match super::bridge::block_on(&self.runtime, reader.get().read_at(offset, len)) {
            Ok(bytes) => Ok(bytes),
            // The rows this session shows came from another version of the file, so a
            // fresh handle here would serve the new bytes under the old line index.
            Err(changed @ VolumeError::SourceChanged(_)) => {
                if !self.reported.swap(true, Ordering::AcqRel) {
                    self.replaced.fire();
                }
                Err(map_volume_error(changed, None))
            }
            Err(first) => {
                debug!(
                    "viewer volume source: read at {offset} failed ({first}); reopening {} once",
                    cmdr_fs::log_private::LogPrivate::file_path(&self.path),
                    first = cmdr_fs::log_error::LogError(&first)
                );
                let fresh = self.reopen(reader)?;
                super::bridge::block_on(&self.runtime, fresh.get().read_at(offset, len))
                    .map_err(|e| map_volume_error(e, None))
            }
        }
    }

    fn cached(&self, block: u64) -> Option<Arc<Vec<u8>>> {
        self.cache
            .lock_ignore_poison()
            .iter()
            .find(|(index, _)| *index == block)
            .map(|(_, data)| Arc::clone(data))
    }

    fn remember(&self, block: u64, data: Arc<Vec<u8>>) {
        let mut cache = self.cache.lock_ignore_poison();
        cache.retain(|(index, _)| *index != block);
        if cache.len() == CACHED_BLOCKS {
            cache.pop_front();
        }
        cache.push_back((block, data));
    }

    /// Blocks `first..=last`, fetching every missing one in ONE read. A block shorter
    /// than [`BLOCK_BYTES`] is the file's tail; nothing after it is returned.
    fn blocks(&self, reader: &Arc<HeldReader>, first: u64, last: u64) -> Result<Vec<Arc<Vec<u8>>>, ViewerError> {
        let have: Vec<Option<Arc<Vec<u8>>>> = (first..=last).map(|block| self.cached(block)).collect();
        let missing_from = have.iter().position(Option::is_none);
        let Some(missing_from) = missing_from else {
            return Ok(have.into_iter().flatten().collect());
        };
        let missing_to = have.iter().rposition(Option::is_none).unwrap_or(missing_from);
        let span_first = first + missing_from as u64;
        let span_blocks = (missing_to - missing_from + 1) as u64;
        let fetched = self.read_span(reader, span_first * BLOCK_BYTES, (span_blocks * BLOCK_BYTES) as usize)?;

        let mut out = Vec::new();
        for (i, cached) in have.into_iter().enumerate() {
            let block = first + i as u64;
            let data = match cached {
                Some(data) if block < span_first || block >= span_first + span_blocks => data,
                _ => {
                    let start = ((block - span_first) * BLOCK_BYTES) as usize;
                    let end = (start + BLOCK_BYTES as usize).min(fetched.len());
                    let data = Arc::new(fetched.get(start..end).unwrap_or_default().to_vec());
                    if data.len() == BLOCK_BYTES as usize {
                        self.remember(block, Arc::clone(&data));
                    }
                    data
                }
            };
            let short = data.len() < BLOCK_BYTES as usize;
            out.push(data);
            if short {
                break;
            }
        }
        Ok(out)
    }
}

impl ByteSource for VolumeSource {
    /// ❗ A request past the reader's size means the caller knows the file grew (the
    /// backends never read past their `total_bytes`, and tail mode moves that), and a
    /// held handle answers only up to the size it opened at, so it reopens first.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, ViewerError> {
        if buf.is_empty() {
            return Ok(0);
        }
        let end = offset + buf.len() as u64;
        let mut reader = self.current();
        if end > reader.get().size() {
            reader = self.reopen(&reader)?;
        }
        let first = offset / BLOCK_BYTES;
        let last = (end - 1) / BLOCK_BYTES;
        let mut filled = 0usize;
        for (i, data) in self.blocks(&reader, first, last)?.iter().enumerate() {
            let block_start = (first + i as u64) * BLOCK_BYTES;
            let from = (offset + filled as u64 - block_start) as usize;
            if from >= data.len() {
                break;
            }
            let n = (data.len() - from).min(buf.len() - filled);
            buf[filled..filled + n].copy_from_slice(&data[from..from + n]);
            filled += n;
            if filled == buf.len() {
                break;
            }
        }
        Ok(filled)
    }
}

/// The file at `path` on `volume`, as a session's [`OpenedFile`], telling `replaced`
/// when the file changes under its reader.
pub(super) fn open_on_volume(
    volume: &Arc<dyn Volume>,
    path: &Path,
    way: PositionedRead,
    runtime: &Runtime,
    replaced: &ReplacementHook,
) -> Result<OpenedFile, VolumeError> {
    let (source, size) = VolumeSource::open(Arc::clone(volume), path, way, runtime)?;
    Ok(OpenedFile {
        source: Arc::new(source.reporting_replacement_to(replaced.clone())),
        total_bytes: size,
        file_name: file_name_of(path),
    })
}

/// The file's size now, from the volume's stat.
pub(super) fn size_on_volume(volume: &Arc<dyn Volume>, path: &Path, runtime: &Runtime) -> Result<u64, ViewerError> {
    let entry = super::bridge::block_on(runtime, volume.get_metadata(path)).map_err(|e| map_volume_error(e, None))?;
    Ok(entry.size.unwrap_or(0))
}
