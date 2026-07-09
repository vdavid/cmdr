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
use std::sync::{Arc, Mutex};

use tokio::task::spawn_blocking;
use zeroize::Zeroizing;

use super::{
    ArchiveByteSource, ArchiveEntryReader, ArchiveError, ArchiveFormat, ArchiveIndex, ArchiveIndexCache, ArchiveNode,
    DEFAULT_TAIL_CACHE_LEN, LocalFileSource, SubtreeExtractReader, TailCachedSource,
};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    CopyScanResult, ExtractedFile, LaneKey, ListingProgress, SequentialExtract, SpaceInfo, Volume, VolumeError,
    VolumeReadStream,
};
use crate::ignore_poison::IgnorePoison;

/// A read-only [`Volume`] that presents a zip archive as a browsable folder.
pub struct ArchiveVolume {
    /// The volume physically holding the `.zip`. Source of the shared lane key
    /// and the space-info answer; later also the byte source for a remote parent.
    parent: Arc<dyn Volume>,
    /// Absolute path of the `.zip` on the parent volume. This is `root()`, and
    /// every inner entry's path joins under it.
    archive_path: PathBuf,
    /// Which archive format (zip / tar+codec / 7z) — decided from the path at
    /// resolve time and threaded into every parse. Also drives the sequential
    /// access class ([`ArchiveFormat::is_sequential`]).
    format: ArchiveFormat,
    /// Display name: the archive file's own name.
    name: String,
    /// Parsed-index cache keyed by `(path, size, mtime)`. Shared as `Arc` so the
    /// blocking parse can run inside `spawn_blocking`; an external edit to the
    /// `.zip` (size/mtime change) is a natural miss and re-parse.
    cache: Arc<ArchiveIndexCache>,
    /// Live content watch on the backing `.zip`. `None` until
    /// [`start_content_watch`](Self::start_content_watch) runs (the routing layer
    /// starts it once, when the volume first registers), or when the watch can't
    /// be established. Its presence is what [`listing_is_watched`](Volume::listing_is_watched)
    /// reports; dropping it (on LRU eviction) stops the OS watch.
    watch: Mutex<Option<super::watch::ArchiveContentWatch>>,
    /// The password for a password-protected archive, remembered for the lifetime
    /// of THIS `ArchiveVolume` instance. `VolumeManager::resolve` mints one per
    /// archive and LRU-caches it, so this is exactly "remember for this archive"
    /// — and it's gone when the LRU evicts (a re-minted instance starts empty, so
    /// the frontend re-prompts). `Zeroizing` wipes the bytes on drop. A wrong
    /// password never persists: `set_password` overwrites, and a detected wrong
    /// attempt clears it (see [`clear_password_if_wrong`](Self::clear_password_if_wrong)).
    password: Mutex<Option<Zeroizing<String>>>,
}

impl ArchiveVolume {
    /// Builds a read-only archive volume over `archive_path` (a `format` archive),
    /// backed by `parent`.
    ///
    /// Cheap and infallible: the directory is parsed lazily on first use (and
    /// cached). The routing layer confirms the file is a real, supported archive
    /// (extension + magic) and picks the `format` before constructing one.
    pub fn new(parent: Arc<dyn Volume>, archive_path: PathBuf, format: ArchiveFormat) -> Self {
        let name = archive_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            parent,
            archive_path,
            format,
            name,
            cache: Arc::new(ArchiveIndexCache::new()),
            watch: Mutex::new(None),
            password: Mutex::new(None),
        }
    }

    /// Stores the password for this archive, overwriting any previous one (so a
    /// fresh attempt replaces a rejected password). Called by the
    /// `set_archive_password` IPC command after the frontend prompts.
    pub fn set_password(&self, password: String) {
        *self.password.lock_ignore_poison() = Some(Zeroizing::new(password));
    }

    /// Forgets any stored password (the `clear_archive_password` IPC command, and
    /// the internal wrong-password reset).
    pub fn clear_password(&self) {
        *self.password.lock_ignore_poison() = None;
    }

    /// A cloned snapshot of the stored password to move into the blocking read
    /// closure. Cloning keeps it `Zeroizing`, so the transient copy is wiped when
    /// the closure ends.
    fn password_snapshot(&self) -> Option<Zeroizing<String>> {
        self.password.lock_ignore_poison().clone()
    }

    /// Starts the live content watch on the backing `.zip` so an external edit
    /// (an editor rewriting it, a `cp` over it, a future in-app mutation's final
    /// rename) refreshes any open listing inside the zip.
    ///
    /// Called once by the routing layer ([`VolumeManager::resolve`]) when this
    /// volume first registers, so repeated resolves of an already-registered
    /// archive don't churn watchers; a no-op if a watch is already live.
    /// `parent_volume_id` is the drive id the listing cache keys on, threaded
    /// through so the refresh re-resolves back to this archive.
    ///
    /// [`VolumeManager::resolve`]: crate::file_system::volume::VolumeManager::resolve
    pub fn start_content_watch(&self, parent_volume_id: &str) {
        let mut watch = self.watch.lock_ignore_poison();
        if watch.is_some() {
            return;
        }
        *watch = super::watch::start_watch(
            self.archive_path.clone(),
            parent_volume_id.to_string(),
            Arc::clone(&self.cache),
        );
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

    /// Whether the parent stores the `.zip` on the local filesystem — a plain
    /// local drive OR an OS-mounted share (both served by `LocalPosixVolume`,
    /// which reports `supports_local_fs_access() = true`). Then the fast local
    /// path applies: `std::fs` stat + a `LocalFileSource` `pread`. Otherwise the
    /// archive is remote-backed (direct SMB / MTP) and every read flows through
    /// the parent volume's `read_range` (a [`VolumeByteSource`]).
    fn parent_is_local(&self) -> bool {
        self.parent.supports_local_fs_access()
    }

    /// Builds a parent-backed byte source for a REMOTE archive, plus the
    /// freshness fields the index cache keys on.
    ///
    /// The size and mtime come from the parent volume's metadata (a remote `.zip`
    /// can't be `std::fs`-stat'd). Async because it does one metadata round-trip
    /// to the backend; the returned [`VolumeByteSource`] then reads ranges lazily.
    async fn open_remote_source(&self) -> Result<(u64, Option<i128>, Arc<dyn ArchiveByteSource>), VolumeError> {
        let meta = self.parent.get_metadata(&self.archive_path).await?;
        let size = meta.size.unwrap_or(0);
        // `FileEntry::modified_at` is Unix SECONDS; widen to nanos for the cache
        // key. Second granularity is enough to catch an external edit (size also
        // guards), and it round-trips the same way on every browse.
        let mtime_nanos = meta.modified_at.map(|s| i128::from(s) * 1_000_000_000);
        let raw: Arc<dyn ArchiveByteSource> = Arc::new(VolumeByteSource::new(
            Arc::clone(&self.parent),
            self.archive_path.clone(),
            size,
        ));
        // Cache the file's tail so the central-directory parse costs one ranged
        // read of the backend, not many small ones (entry reads mid-file fall
        // through to the raw parent-backed source).
        let source: Arc<dyn ArchiveByteSource> = Arc::new(TailCachedSource::new(raw, DEFAULT_TAIL_CACHE_LEN));
        Ok((size, mtime_nanos, source))
    }

    /// Loads the parsed central-directory index (cached), picking the local fast
    /// path or the remote parent-backed path by [`parent_is_local`](Self::parent_is_local).
    /// The pure tree queries the callers run on the returned index don't block, so
    /// only the parse itself is off-executor.
    async fn index(&self) -> Result<Arc<ArchiveIndex>, VolumeError> {
        let cache = Arc::clone(&self.cache);
        let archive_path = self.archive_path.clone();
        let format = self.format;
        // Needed only for a HEADER-encrypted 7z, whose metadata is encrypted; every
        // other archive parses (browses) with no password. `None` on a header-
        // encrypted 7z fails the parse with `Encrypted` → `NeedsPassword`, the
        // browse-time prompt.
        let password = self.password_snapshot();
        if self.parent_is_local() {
            parse_blocking(move || {
                cache
                    .index_for_local(&archive_path, format, password.as_deref().map(String::as_str))
                    .map_err(to_volume_error)
            })
            .await
        } else {
            let (size, mtime_nanos, source) = self.open_remote_source().await?;
            parse_blocking(move || {
                cache
                    .index_for_source(
                        &archive_path,
                        size,
                        mtime_nanos,
                        source,
                        format,
                        password.as_deref().map(String::as_str),
                    )
                    .map_err(to_volume_error)
            })
            .await
        }
    }

    /// Loads the index AND a byte source for streaming an entry's bytes (local
    /// `LocalFileSource` or remote [`VolumeByteSource`]). Used by
    /// [`open_stream`](Self::open_stream); the query-only methods use
    /// [`index`](Self::index), which skips opening a source.
    async fn load(&self) -> Result<(Arc<ArchiveIndex>, Arc<dyn ArchiveByteSource>), VolumeError> {
        let cache = Arc::clone(&self.cache);
        let archive_path = self.archive_path.clone();
        let format = self.format;
        let password = self.password_snapshot();
        if self.parent_is_local() {
            parse_blocking(
                move || -> Result<(Arc<ArchiveIndex>, Arc<dyn ArchiveByteSource>), VolumeError> {
                    let index = cache
                        .index_for_local(&archive_path, format, password.as_deref().map(String::as_str))
                        .map_err(to_volume_error)?;
                    let source: Arc<dyn ArchiveByteSource> = Arc::new(LocalFileSource::open(&archive_path)?);
                    Ok((index, source))
                },
            )
            .await
        } else {
            let (size, mtime_nanos, source) = self.open_remote_source().await?;
            let parse_source = Arc::clone(&source);
            let path_for_parse = archive_path.clone();
            let index = parse_blocking(move || {
                cache
                    .index_for_source(
                        &path_for_parse,
                        size,
                        mtime_nanos,
                        parse_source,
                        format,
                        password.as_deref().map(String::as_str),
                    )
                    .map_err(to_volume_error)
            })
            .await?;
            Ok((index, source))
        }
    }

    /// Loads the index + byte source and opens a streaming reader for `path`.
    /// `offset` is the decompressed byte offset the returned stream starts at
    /// (0 = the whole entry).
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
        Box::pin(async move {
            let (index, source) = self.load().await?;
            // `open_read` only looks the entry up and spawns the decompress
            // producer (which itself uses `spawn_blocking`), so it's cheap here.
            // With no password an encrypted entry fails fast here (`Encrypted` →
            // `NeedsPassword`); a WRONG password surfaces later, during streaming
            // (the decrypt/verify runs inside the producer), as the same typed
            // signal — the frontend retries after `set_archive_password`.
            let password = self.password_snapshot();
            let reader = index
                .open_read(&inner, source, password.as_deref().map(String::as_str))
                .map_err(to_volume_error)?;
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
        let archive_path = self.archive_path.clone();
        let volume_name = self.name.clone();
        Box::pin(async move {
            let index = self.index().await?;
            let nodes = index
                .list(&inner)
                .ok_or_else(|| VolumeError::NotFound(archive_path.join(&inner).display().to_string()))?;
            let entries: Vec<FileEntry> = nodes
                .iter()
                .map(|node| node_to_entry(&archive_path, &volume_name, node))
                .collect();

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
        let archive_path = self.archive_path.clone();
        let volume_name = self.name.clone();
        Box::pin(async move {
            let index = self.index().await?;
            let node = index
                .get(&inner)
                .ok_or_else(|| VolumeError::NotFound(archive_path.join(&inner).display().to_string()))?;
            Ok(node_to_entry(&archive_path, &volume_name, &node))
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let inner = self.inner_path(path);
        Box::pin(async move {
            // An unreadable archive (parse error or a lost remote) has no
            // browsable entries.
            match self.index().await {
                Ok(index) => index.exists(&inner),
                Err(_) => false,
            }
        })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        let inner = self.inner_path(path);
        let archive_path = self.archive_path.clone();
        Box::pin(async move {
            let index = self.index().await?;
            index
                .is_directory(&inner)
                .ok_or_else(|| VolumeError::NotFound(archive_path.join(&inner).display().to_string()))
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

    /// A compressed tar or 7z is sequential-access (see
    /// [`Volume::extraction_is_sequential`] and [`ArchiveFormat::is_sequential`]);
    /// a plain `.tar` and a zip are random-access.
    fn extraction_is_sequential(&self, _path: &Path) -> bool {
        self.format.is_sequential()
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        let inner = self.inner_path(path);
        let archive_path = self.archive_path.clone();
        Box::pin(async move {
            let index = self.index().await?;
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

            // A directory: walk the subtree via the index's per-dir child lists.
            // Counts and byte totals come from the central directory — no
            // decompression during the scan. The top-level dir isn't counted
            // (matches `LocalPosixVolume`).
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

    /// Opens the one-pass subtree extractor for a compressed tar / solid 7z, so a
    /// bulk extract decodes the stream ONCE instead of once per file. Parses the
    /// index (cached) and opens a byte source, then hands both to
    /// [`ArchiveIndex::open_subtree_extract`]. The returned
    /// [`ArchiveSequentialExtract`] yields the subtree's files in archive order.
    fn open_sequential_extract<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn SequentialExtract>, VolumeError>> + Send + 'a>> {
        let inner = self.inner_path(path);
        let archive_path = self.archive_path.clone();
        Box::pin(async move {
            let (index, source) = self.load().await?;
            // Sequential extract serves compressed tar / solid 7z only (a random-
            // access zip never routes here). A content-encrypted 7z is decryptable,
            // so the per-archive password is threaded; a plaintext tar ignores it.
            let password = self.password_snapshot();
            let reader = index.open_subtree_extract(&inner, source, password.as_deref().map(String::as_str));
            Ok(Box::new(ArchiveSequentialExtract::new(reader, archive_path)) as Box<dyn SequentialExtract>)
        })
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

    /// `true` only while the content watch is genuinely live (established by
    /// [`start_content_watch`](Self::start_content_watch) and not yet dropped by
    /// LRU eviction). If the watch failed to establish, this stays `false`, so a
    /// listing never claims freshness the backend can't back.
    fn listing_is_watched(&self, _path: &Path) -> bool {
        self.watch.lock_ignore_poison().is_some()
    }
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

/// The [`SequentialExtract`] over an archive subtree: the [`Volume`] adapter for
/// the reading core's one-pass [`SubtreeExtractReader`]. Maps the core's
/// [`ArchiveError`] to [`VolumeError`] and each core member's inner path back to a
/// full source path (`archive_path/inner`, matching what `list_directory`
/// reports, so the copy planner's per-file lookup keys line up).
///
/// The core reader is shared behind an `Arc<tokio::sync::Mutex<…>>` so
/// [`current_stream`](SequentialExtract::current_stream) can hand out an OWNED
/// [`VolumeReadStream`] (what `write_from_stream` takes) that still pulls from the
/// one decoder. Usage is strictly serial (advance, then drain the member's
/// stream, then advance), so the mutex is never contended — it's there for
/// ownership and `Send`, not concurrency.
struct ArchiveSequentialExtract {
    reader: Arc<tokio::sync::Mutex<SubtreeExtractReader>>,
    archive_path: PathBuf,
    /// Uncompressed size of the member the last `next_file` returned, so
    /// `current_stream` can report `total_size()` without touching the reader.
    current_size: u64,
}

impl ArchiveSequentialExtract {
    fn new(reader: SubtreeExtractReader, archive_path: PathBuf) -> Self {
        Self {
            reader: Arc::new(tokio::sync::Mutex::new(reader)),
            archive_path,
            current_size: 0,
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

    fn total_size(&self) -> u64 {
        self.total
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

/// An [`ArchiveByteSource`] backed by a parent [`Volume`]'s ranged read, for a
/// zip that lives on a REMOTE backend (direct SMB or MTP), where there's no
/// local file to `pread`.
///
/// The core's `read_at` is **blocking** (the parse and decompress run on
/// `spawn_blocking`), but a `Volume`'s [`read_range`](Volume::read_range) is
/// async. This bridges the two: it captures the tokio runtime handle at
/// construction (on the async executor, in
/// [`open_remote_source`](ArchiveVolume::open_remote_source)) and `block_on`s
/// the parent's `read_range` from inside the blocking read. That's sound because
/// `read_at` only ever runs on a `spawn_blocking` thread — never a runtime worker
/// — so `block_on` doesn't reenter the executor (the same bridge the viewer's
/// archive extractor uses). Shared as `Arc` across concurrent reads; `read_at`
/// takes `&self` with no shared cursor, so parallel entry reads are independent.
struct VolumeByteSource {
    parent: Arc<dyn Volume>,
    /// Absolute path of the `.zip` on the parent volume.
    path: PathBuf,
    /// The archive's size, from the parent's metadata at construction. A read at
    /// or past it returns EOF, matching a local `pread`.
    size: u64,
    handle: tokio::runtime::Handle,
}

impl VolumeByteSource {
    fn new(parent: Arc<dyn Volume>, path: PathBuf, size: u64) -> Self {
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
/// path-aware callers keep working. Encryption maps to the typed
/// `NeedsPassword { wrong_attempt }` the frontend prompts on; the rejection
/// family (not-a-zip / unsupported / too-large) collapses to
/// `NotSupported`, and a broken/faulted read to `IoError`: this is the
/// mid-browse **backstop**. The user-facing "damaged archive" friendly copy
/// (`ListingErrorReason::ArchiveUnreadable`) is produced downstream at the
/// listing seam (`listing/streaming.rs`), from the path + this collapsed error
/// kind — not from the fine `ArchiveError` distinction, which stops here. A
/// single combined message covers the whole family, so recovering the
/// distinction isn't needed (matches the viewer's one archive-unreadable copy).
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
        // Password-protected: a typed signal the frontend prompts on (distinct
        // from the rejection family below so it never reads as "damaged"). No
        // password tried yet vs. a rejected one:
        ArchiveError::Encrypted => VolumeError::NeedsPassword { wrong_attempt: false },
        ArchiveError::WrongPassword => VolumeError::NeedsPassword { wrong_attempt: true },
        // The rejection family — "this backend can't or won't serve this
        // archive": not-a-zip, an unsupported codec, or a synthesized tree past
        // the node-count DoS cap. All collapse to `NotSupported`.
        ArchiveError::NotAnArchive | ArchiveError::Unsupported(_) | ArchiveError::TooLarge(_) => {
            VolumeError::NotSupported
        }
    }
}

#[cfg(test)]
#[path = "volume_test.rs"]
mod volume_test;
