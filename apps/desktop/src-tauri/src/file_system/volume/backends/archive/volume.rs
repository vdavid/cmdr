//! `ArchiveVolume`: a read-only [`Volume`] over a zip file, built on the
//! decoupled archive reading core in this module.
//!
//! The volume presents the archive as if it were a folder: `root()` is the real
//! `.zip` path, and inner entries live under it (`/path/to/foo.zip/dir/a.txt`),
//! so the transparent path bar renders for free. Browsing reads the cached
//! central-directory index ([`ArchiveIndex`]); extraction streams one entry at a
//! time through an [`ArchiveEntryReader`] wrapped as a [`VolumeReadStream`].
//!
//! It is read-only for now: every mutation method returns
//! [`VolumeError::NotSupported`] until zip mutation (add/delete/rename via
//! temp+rename) lands. Registration, refcounting, and LRU eviction are the
//! routing layer's job; this type is headless and constructed directly.
//!
//! ## Parent volume
//!
//! An `ArchiveVolume` holds an `Arc<dyn Volume>` **parent** (the volume that
//! physically stores the `.zip`) plus the archive's path. The parent is the seam
//! for two things a read-only archive can't answer itself:
//!
//! - [`lane_key`](ArchiveVolume::lane_key) returns the **parent's** lane key, so
//!   the operation manager serializes archive work against other work on the same
//!   device/session (an archive on one SMB share must share that share's lane).
//! - [`get_space_info`](ArchiveVolume::get_space_info) delegates to the parent:
//!   any archive edit (the planned temp+rename mutation) builds on the parent drive, so the
//!   parent's free space is the honest constraint — and delegating avoids
//!   reporting `available = 0`, which the pre-copy space check reads as "disk
//!   full" and would use to block a paste.
//!
//! Only a **local** parent is exercised now: the backing bytes come from a
//! [`LocalFileSource`] opened over the archive path. When remote-backed archives
//! land, a remote parent supplies bytes by implementing [`ArchiveByteSource`]
//! over its ranged reads, with no change to the index/reader layer.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use tokio::task::spawn_blocking;

use super::{
    ArchiveByteSource, ArchiveEntryReader, ArchiveError, ArchiveIndex, ArchiveIndexCache, ArchiveNode, LocalFileSource,
};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    CopyScanResult, LaneKey, ListingProgress, SpaceInfo, Volume, VolumeError, VolumeReadStream,
};

/// A read-only [`Volume`] that presents a zip archive as a browsable folder.
pub struct ArchiveVolume {
    /// The volume physically holding the `.zip`. Source of the shared lane key
    /// and the space-info answer; later also the byte source for a remote parent.
    parent: Arc<dyn Volume>,
    /// Absolute path of the `.zip` on the parent volume. This is `root()`, and
    /// every inner entry's path joins under it.
    archive_path: PathBuf,
    /// Display name: the archive file's own name.
    name: String,
    /// Parsed-index cache keyed by `(path, size, mtime)`. Shared as `Arc` so the
    /// blocking parse can run inside `spawn_blocking`; an external edit to the
    /// `.zip` (size/mtime change) is a natural miss and re-parse.
    cache: Arc<ArchiveIndexCache>,
}

impl ArchiveVolume {
    /// Builds a read-only archive volume over `archive_path`, backed by `parent`.
    ///
    /// Cheap and infallible: the central directory is parsed lazily on first use
    /// (and cached). The routing layer confirms the file is a real,
    /// supported archive at navigation time before constructing one.
    pub fn new(parent: Arc<dyn Volume>, archive_path: PathBuf) -> Self {
        let name = archive_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            parent,
            archive_path,
            name,
            cache: Arc::new(ArchiveIndexCache::new()),
        }
    }

    /// Maps a path from the volume's namespace to the archive-inner path the
    /// index keys on (`/`-separated, no surrounding slashes, root is `""`).
    ///
    /// Accepts the archive root itself (empty / `.`), an absolute path carrying
    /// the archive-path prefix (`/path/to/foo.zip/dir` → `dir`), or an
    /// already-inner relative path. The index re-trims slashes, so a rough
    /// result is fine.
    fn inner_path(&self, path: &Path) -> String {
        if path.as_os_str().is_empty() || path == Path::new(".") {
            return String::new();
        }
        let relative = path.strip_prefix(&self.archive_path).unwrap_or(path);
        let slashed = relative.to_string_lossy().replace('\\', "/");
        slashed.trim_matches('/').to_string()
    }

    /// Loads the parsed index and opens a fresh byte source for streaming, both
    /// blocking. `offset` is the decompressed byte offset the returned stream
    /// starts at (0 = the whole entry).
    #[allow(
        clippy::type_complexity,
        reason = "mirrors the VolumeReadStream trait method's pinned-boxed-future return shape"
    )]
    fn open_stream<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let inner = self.inner_path(path);
        let cache = Arc::clone(&self.cache);
        let archive_path = self.archive_path.clone();
        Box::pin(async move {
            // Parse (cached) and open the local byte source off the async
            // executor — both stat/read the real file.
            let (index, source) = parse_blocking(
                move || -> Result<(Arc<ArchiveIndex>, Arc<dyn ArchiveByteSource>), VolumeError> {
                    let index = cache.index_for_local(&archive_path).map_err(to_volume_error)?;
                    let source: Arc<dyn ArchiveByteSource> = Arc::new(LocalFileSource::open(&archive_path)?);
                    Ok((index, source))
                },
            )
            .await?;

            // `open_read` only looks the entry up and spawns the decompress
            // producer (which itself uses `spawn_blocking`), so it's cheap here.
            let reader = index.open_read(&inner, source).map_err(to_volume_error)?;
            Ok(Box::new(ArchiveVolumeReadStream {
                reader,
                skip_remaining: offset,
                delivered: 0,
            }) as Box<dyn VolumeReadStream>)
        })
    }
}

impl Volume for ArchiveVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.archive_path
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// The **parent's** lane key: archive work shares the physical device's lane
    /// so the operation manager can't run it in parallel with other work on the
    /// same mount, USB pipe, or SMB session. Never the archive path.
    fn lane_key(&self) -> LaneKey {
        self.parent.lane_key()
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        let inner = self.inner_path(path);
        let cache = Arc::clone(&self.cache);
        let archive_path = self.archive_path.clone();
        let volume_name = self.name.clone();
        Box::pin(async move {
            let entries = parse_blocking(move || -> Result<Vec<FileEntry>, VolumeError> {
                let index = cache.index_for_local(&archive_path).map_err(to_volume_error)?;
                let nodes = index
                    .list(&inner)
                    .ok_or_else(|| VolumeError::NotFound(archive_path.join(&inner).display().to_string()))?;
                Ok(nodes
                    .iter()
                    .map(|node| node_to_entry(&archive_path, &volume_name, node))
                    .collect())
            })
            .await?;

            // One cumulative progress tick, as the trait asks (backends call
            // `on_progress` at least once; the archive listing is atomic so
            // there's nothing incremental to report).
            if let Some(callback) = on_progress {
                let mut progress = ListingProgress::default();
                for entry in &entries {
                    if entry.is_directory {
                        progress.dirs += 1;
                    } else {
                        progress.files += 1;
                        progress.bytes += entry.size.unwrap_or(0);
                    }
                }
                callback(progress);
            }
            Ok(entries)
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        let inner = self.inner_path(path);
        let cache = Arc::clone(&self.cache);
        let archive_path = self.archive_path.clone();
        let volume_name = self.name.clone();
        Box::pin(async move {
            parse_blocking(move || -> Result<FileEntry, VolumeError> {
                let index = cache.index_for_local(&archive_path).map_err(to_volume_error)?;
                let node = index
                    .get(&inner)
                    .ok_or_else(|| VolumeError::NotFound(archive_path.join(&inner).display().to_string()))?;
                Ok(node_to_entry(&archive_path, &volume_name, &node))
            })
            .await
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let inner = self.inner_path(path);
        let cache = Arc::clone(&self.cache);
        let archive_path = self.archive_path.clone();
        Box::pin(async move {
            parse_blocking(move || -> Result<bool, VolumeError> {
                Ok(match cache.index_for_local(&archive_path) {
                    Ok(index) => index.exists(&inner),
                    // An unreadable archive has no browsable entries.
                    Err(_) => false,
                })
            })
            .await
            // A parse panic can't confirm existence: report not-exists.
            .unwrap_or(false)
        })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        let inner = self.inner_path(path);
        let cache = Arc::clone(&self.cache);
        let archive_path = self.archive_path.clone();
        Box::pin(async move {
            parse_blocking(move || -> Result<bool, VolumeError> {
                let index = cache.index_for_local(&archive_path).map_err(to_volume_error)?;
                index
                    .is_directory(&inner)
                    .ok_or_else(|| VolumeError::NotFound(archive_path.join(&inner).display().to_string()))
            })
            .await
        })
    }

    // ---- Read-only: mutations are unsupported until zip mutation lands ------

    // `create_file`, `create_directory`, `delete`, `rename`, and
    // `write_from_stream` inherit the trait's `NotSupported` default. Only
    // `create_directory_all` is overridden: its default would walk `exists()`
    // and return `Ok(())` for an already-present directory, silently claiming
    // success on a read-only volume. Every mutation is pinned by
    // `volume_test.rs::every_mutation_is_unsupported`.
    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    // ---- Extract-out: streaming reads + copy scanning -----------------------

    fn supports_export(&self) -> bool {
        true
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    /// One at a time for now (the plan pins a single stream in flight against an
    /// archive for the read-only phase). The core supports concurrent independent
    /// reads, so raising this later is a one-line change.
    fn max_concurrent_ops(&self) -> usize {
        1
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        let inner = self.inner_path(path);
        let cache = Arc::clone(&self.cache);
        let archive_path = self.archive_path.clone();
        Box::pin(async move {
            parse_blocking(move || -> Result<CopyScanResult, VolumeError> {
                let index = cache.index_for_local(&archive_path).map_err(to_volume_error)?;
                let node = index
                    .get(&inner)
                    .ok_or_else(|| VolumeError::NotFound(archive_path.join(&inner).display().to_string()))?;

                // A single file: one entry, its uncompressed size.
                if !node.is_dir {
                    let size = node.size.unwrap_or(0);
                    return Ok(CopyScanResult {
                        file_count: 1,
                        dir_count: 0,
                        total_bytes: size,
                        // No hardlinks in a zip: the two footprints are equal.
                        dedup_bytes: size,
                        top_level_is_directory: false,
                    });
                }

                // A directory: walk the subtree via the index's per-dir child
                // lists. Counts and byte totals come from the central directory —
                // no decompression during the scan. The top-level dir isn't
                // counted (matches `LocalPosixVolume`).
                let mut file_count = 0;
                let mut dir_count = 0;
                let mut total_bytes = 0u64;
                let mut pending = vec![inner];
                while let Some(dir) = pending.pop() {
                    let Some(children) = index.list(&dir) else { continue };
                    for child in children {
                        if child.is_dir {
                            dir_count += 1;
                            pending.push(child.path);
                        } else {
                            file_count += 1;
                            total_bytes += child.size.unwrap_or(0);
                        }
                    }
                }
                Ok(CopyScanResult {
                    file_count,
                    dir_count,
                    total_bytes,
                    dedup_bytes: total_bytes,
                    top_level_is_directory: true,
                })
            })
            .await
        })
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.open_stream(path, 0)
    }

    fn open_read_stream_at_offset<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.open_stream(path, offset)
    }

    // ---- Capability flags: set explicitly, don't inherit defaults -----------

    /// `None`: an archive isn't a local FS path. Inner entries aren't reachable
    /// via `std::fs`, so there's no `copyfile(2)` fast path to advertise.
    fn local_path(&self) -> Option<PathBuf> {
        None
    }

    /// `false`: inner paths can't be stat'd or read via `std::fs` (they live
    /// inside the zip), so the legacy synthetic-diff path must be skipped.
    fn supports_local_fs_access(&self) -> bool {
        false
    }

    /// `None`: a read-only archive's space never changes, so don't poll it (the
    /// trait default `Some(2s)` would poll pointlessly).
    fn space_poll_interval(&self) -> Option<std::time::Duration> {
        None
    }

    /// Delegates to the parent volume. An archive isn't a disk with its own free
    /// space; any edit (temp+rename) lands on the parent drive, so the
    /// parent's space is the honest, non-blocking answer.
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.parent.get_space_info().await })
    }

    // `listing_is_watched` stays the `false` default: there's no live watcher on
    // the archive yet (live watching adds one later). A backend without a real
    // watcher must not claim listing freshness.
}

/// Wraps an [`ArchiveEntryReader`] as a [`VolumeReadStream`], mapping the core's
/// [`ArchiveError`] to [`VolumeError`] and, for a resumed read, discarding the
/// leading `skip_remaining` decompressed bytes before yielding.
///
/// A compressed entry has no random access, so a non-zero start offset means
/// "decompress from the beginning and drop the prefix" — correct, if not cheap.
/// Nothing calls the at-offset path with a non-zero offset today (see the trait
/// docs), so the common `skip_remaining == 0` path never drops anything.
struct ArchiveVolumeReadStream {
    reader: ArchiveEntryReader,
    skip_remaining: u64,
    /// Decompressed bytes handed to the consumer (this segment), for progress.
    delivered: u64,
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
    fn total_size(&self) -> u64 {
        self.reader.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.delivered
    }
}

/// Runs a blocking archive-parse closure, mapping a `JoinError` — a PANIC inside
/// the closure — to a typed [`VolumeError`] instead of `expect`-aborting the
/// whole process.
///
/// This DELIBERATELY differs from the repo's usual expect-on-join idiom
/// (`spawn_blocking(...).await.expect("…the task is uncancelable")`): every path
/// here parses UNTRUSTED bytes, and a malformed archive can panic deep in the
/// zip decoder. That must surface as a handled error on the one path being read,
/// never take the app down. The closure is uncancelable, so the only `JoinError`
/// is a panic.
async fn parse_blocking<T, F>(closure: F) -> Result<T, VolumeError>
where
    F: FnOnce() -> Result<T, VolumeError> + Send + 'static,
    T: Send + 'static,
{
    match spawn_blocking(closure).await {
        Ok(result) => result,
        Err(join_err) => Err(VolumeError::IoError {
            message: join_err.to_string(),
            raw_os_error: None,
        }),
    }
}

/// Maps an [`ArchiveNode`] onto a [`FileEntry`]. The full path joins the inner
/// path under the archive path, so it renders transparently as
/// `/path/to/foo.zip/inner`. The archive root (`""`) carries the archive's own
/// name and path.
fn node_to_entry(archive_path: &Path, volume_name: &str, node: &ArchiveNode) -> FileEntry {
    let (name, full_path) = if node.path.is_empty() {
        (volume_name.to_string(), archive_path.to_path_buf())
    } else {
        (node.name.clone(), archive_path.join(&node.path))
    };
    FileEntry {
        size: node.size,
        // `ArchiveNode::modified` is Unix seconds, matching `FileEntry`; a
        // negative (pre-1970) timestamp is dropped rather than wrapped.
        modified_at: node.modified.and_then(|secs| u64::try_from(secs).ok()),
        // The archive listing is complete in one pass — no deferred metadata.
        extended_metadata_loaded: true,
        ..FileEntry::new(
            name,
            full_path.to_string_lossy().into_owned(),
            node.is_dir,
            node.is_symlink,
        )
    }
}

/// Maps the reading core's [`ArchiveError`] onto [`VolumeError`], typed only (no
/// message-string classification, per `no-string-matching`).
///
/// The path-shaped errors map to their native `VolumeError` twins so
/// path-aware callers keep working. The rejection family
/// (not-a-zip / encrypted / unsupported / too-large) collapses to
/// `NotSupported`, and a broken/faulted read to `IoError`: this is the
/// mid-browse **backstop**. The user-facing "not a real archive" / "encrypted"
/// friendly copy is produced at the routing boundary, straight from the raw
/// `ArchiveError` at navigation time — not from this mapping — so nothing
/// downstream needs to recover the fine distinction from a `VolumeError`.
fn to_volume_error(err: ArchiveError) -> VolumeError {
    // EXHAUSTIVE on purpose (no wildcard): a new `ArchiveError` variant must fail
    // to compile here and force a conscious mapping, rather than silently
    // defaulting to `NotSupported`. A future non-rejection variant (say a
    // transient source error once remote-backed archives land) would be
    // mis-served by a catch-all.
    // This is the repo's compile-time-tripwire convention (see `analytics.rs`).
    match err {
        // Path-shaped errors keep their native `VolumeError` twins so path-aware
        // callers keep working.
        ArchiveError::NotFound(path) => VolumeError::NotFound(path),
        ArchiveError::IsADirectory(path) => VolumeError::IsADirectory(path),
        // A structurally broken zip or a live byte-source fault: a serious I/O
        // condition on the path being read.
        ArchiveError::Corrupt(message) | ArchiveError::Io(message) => VolumeError::IoError {
            message,
            raw_os_error: None,
        },
        // The rejection family — "this backend can't or won't serve this
        // archive": not-a-zip, encrypted, an unsupported codec, or a synthesized
        // tree past the node-count DoS cap. All collapse to `NotSupported`.
        ArchiveError::NotAnArchive
        | ArchiveError::Encrypted
        | ArchiveError::Unsupported(_)
        | ArchiveError::TooLarge(_) => VolumeError::NotSupported,
    }
}

#[cfg(test)]
#[path = "volume_test.rs"]
mod volume_test;
