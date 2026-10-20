//! Moving bytes in and out of a local file: the chunked read stream the backend
//! hands out, the ranged read beside it, and the durable streaming write every
//! cross-volume copy landing on local disk goes through, with the metadata the
//! source carried (`cmdr_fs::metadata`, the one place the ordering lives).
//!
//! Split out the way the MTP backend's `mtp/streams.rs` is: byte movement has
//! its own vocabulary (chunk sizes, `pread` short reads, fdatasync) and reads as
//! one subject rather than three more `Volume` methods. A trait impl can't span
//! files, so the trait side stays in `local_posix.rs` and the work lives here.

use super::LocalPosixVolume;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{
    CarriedMetadata, LandedMetadata, MetadataClasses, MetadataReport, StagedItem, StagedTimes, UserFlags, apply_landed,
    apply_staged, default_file_mode, default_folder_mode, routed_mode,
};
use cmdr_fs::volume::{StreamLength, StreamWriteProgress, StreamWritten, WriteMode};
use cmdr_fs::volume::{VolumeError, VolumeReadStream};
use std::future::Future;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;
use std::pin::Pin;
use tokio::task::spawn_blocking;

/// Streaming reader for `LocalPosixVolume` files.
///
/// Reads the file in 1 MiB chunks on the blocking thread pool via
/// `tokio::task::spawn_blocking`. Each `next_chunk` call hands the file handle
/// to the blocking pool, reads one chunk, and returns ownership along with the
/// data.
struct LocalPosixReadStream {
    file: Option<std::fs::File>,
    total_size: u64,
    bytes_read: u64,
    /// The `stat` the open already took: the date, the creation date, the
    /// mode, and the flags, for nothing.
    stat: std::fs::Metadata,
}

/// 1 MiB chunks, matching `chunked_copy.rs`'s constant.
const LOCAL_STREAM_CHUNK_SIZE: usize = 1024 * 1024;

impl VolumeReadStream for LocalPosixReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let mut file = self.file.take()?;

            let (file_ret, result) = spawn_blocking(move || {
                use std::io::Read;
                let mut buf = vec![0u8; LOCAL_STREAM_CHUNK_SIZE];
                let n = match file.read(&mut buf) {
                    Ok(n) => n,
                    // No path: the handle is already open, so `ENOENT` and `EACCES`
                    // can't reach here; a mid-stream read failure is an `IoError`.
                    Err(e) => return (file, Err(VolumeError::from_io_without_path(&e))),
                };
                buf.truncate(n);
                (file, Ok(buf))
            })
            .await
            .expect("spawn_blocking read-chunk closure doesn't panic and the task is uncancelable");

            match result {
                Ok(buf) if buf.is_empty() => {
                    // EOF: drop the file handle.
                    drop(file_ret);
                    None
                }
                Ok(buf) => {
                    self.bytes_read += buf.len() as u64;
                    self.file = Some(file_ret);
                    Some(Ok(buf))
                }
                Err(e) => {
                    drop(file_ret);
                    Some(Err(e))
                }
            }
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.total_size)
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        self.stat.modified().ok()
    }

    /// Off the open's `stat`, plus the xattrs off the fd when the destination
    /// keeps them (one `flistxattr` for a file with none).
    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        match &self.file {
            Some(file) => CarriedMetadata::of_local_file_wanting(file, &self.stat, wanted),
            // Past EOF the fd is gone; the stat still answers.
            None => CarriedMetadata::of_local_stat(&self.stat).restricted_to(wanted.without(MetadataClasses::XATTRS)),
        }
    }

    /// The same classes, the xattrs by name only.
    fn metadata_outline(&self, wanted: MetadataClasses) -> CarriedMetadata {
        match &self.file {
            Some(file) => CarriedMetadata::outline_of_local_file(file, &self.stat, wanted),
            None => self.metadata(wanted),
        }
    }
}

impl LocalPosixVolume {
    /// Opens `path` for chunked reading, refusing a directory.
    #[allow(
        clippy::type_complexity,
        reason = "carries the trait method's own signature, which returns a pinned boxed future by design"
    )]
    pub(super) fn open_read_stream_impl<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                let metadata = std::fs::metadata(&abs_path).map_err(|e| VolumeError::from_io_at(&e, &abs_path))?;
                if metadata.is_dir() {
                    return Err(VolumeError::IoError {
                        message: "Cannot stream a directory".into(),
                        raw_os_error: None,
                    });
                }
                let total_size = metadata.len();
                let file = std::fs::File::open(&abs_path).map_err(|e| VolumeError::from_io_at(&e, &abs_path))?;
                Ok(Box::new(LocalPosixReadStream {
                    file: Some(file),
                    total_size,
                    bytes_read: 0,
                    stat: metadata,
                }) as Box<dyn VolumeReadStream>)
            })
            .await
            .expect("spawn_blocking open_read_stream closure doesn't panic and the task is uncancelable")
        })
    }

    /// One `pread` window, looped until it's full or the file ends.
    pub(super) fn read_range_impl<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                use std::os::unix::fs::FileExt;
                let file = std::fs::File::open(&abs_path).map_err(|e| VolumeError::from_io_at(&e, &abs_path))?;
                let mut buf = vec![0u8; len];
                let mut filled = 0usize;
                // `read_at` is a `pread`; it may short-read, so loop until the
                // window is full or the file ends (a read at/past EOF returns 0).
                while filled < len {
                    let n = file
                        .read_at(&mut buf[filled..], offset + filled as u64)
                        .map_err(|e| VolumeError::from_io_at(&e, &abs_path))?;
                    if n == 0 {
                        break;
                    }
                    filled += n;
                }
                buf.truncate(filled);
                Ok(buf)
            })
            .await
            .expect("spawn_blocking read_range closure doesn't panic and the task is uncancelable")
        })
    }

    /// Streams `stream` into `dest`, then `sync_data`s the file, puts the
    /// source's metadata on it, and best-effort fsyncs the parent dir before
    /// reporting success.
    ///
    /// The source's user flags aren't put on: Locked would refuse the landing
    /// rename, so the caller applies them after it (`Volume::apply_metadata`).
    pub(super) fn write_from_stream_impl<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        let dest_abs = self.resolve(dest);
        Box::pin(async move {
            // Ensure parent directory exists
            if let Some(parent) = dest_abs.parent() {
                let parent = parent.to_path_buf();
                let parent_for_error = parent.clone();
                spawn_blocking(move || std::fs::create_dir_all(&parent))
                    .await
                    .expect("spawn_blocking create_dir_all closure doesn't panic and the task is uncancelable")
                    .map_err(|e| VolumeError::from_io_at(&e, &parent_for_error))?;
            }

            // Asked once, before the first chunk: everything a local file keeps
            // but the flags (above). A routed source's mode is folded through
            // the umask, never put on as is.
            let carried = stream.metadata(MetadataClasses::ALL.without(MetadataClasses::FLAGS));
            let landed_mode = carried.mode.and_then(|mode| routed_mode(mode, default_file_mode()));
            let carried = CarriedMetadata {
                mode: landed_mode,
                ..carried
            };

            // Open destination file on the blocking pool. `CreateNew` is
            // `O_EXCL`: a file already at the name is refused atomically and
            // left alone, before anything of ours exists to clean up. A file
            // whose source has a mode is created owner-only and gets that mode
            // after its last byte, so a private file is never readable by
            // others while it streams; one without keeps the umask's default.
            let dest_for_open = dest_abs.clone();
            let mut file = spawn_blocking(move || {
                let mut options = std::fs::OpenOptions::new();
                options.write(true);
                if landed_mode.is_some() {
                    options.mode(0o600);
                }
                match mode {
                    WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => options.create_new(true),
                    WriteMode::CreateOrReplace => options.create(true).truncate(true),
                };
                options.open(&dest_for_open)
            })
            .await
            .expect("spawn_blocking open closure doesn't panic and the task is uncancelable")
            .map_err(|e| VolumeError::from_io_at(&e, &dest_abs))?;

            let mut bytes_written = 0u64;
            while let Some(chunk_result) = stream.next_chunk().await {
                let chunk = chunk_result?;
                if chunk.is_empty() {
                    continue;
                }
                let chunk_len = chunk.len() as u64;

                // Write the chunk on the blocking pool.
                let (file_ret, write_res) = spawn_blocking(move || {
                    use std::io::Write;
                    let res = file.write_all(&chunk);
                    (file, res)
                })
                .await
                .expect("spawn_blocking write_all closure doesn't panic and the task is uncancelable");
                file = file_ret;
                write_res.map_err(|e| VolumeError::from_io_at(&e, &dest_abs))?;

                bytes_written += chunk_len;

                if on_progress(StreamWriteProgress {
                    bytes_written,
                    expected_length: length,
                }) == std::ops::ControlFlow::Break(())
                {
                    // Drop the file handle and try to clean up the partial file.
                    drop(file);
                    let partial = dest_abs.clone();
                    let _ = spawn_blocking(move || std::fs::remove_file(&partial)).await;
                    return Err(VolumeError::Cancelled("Operation cancelled by user".into()));
                }
            }

            // Make the file durable before signalling success. A bare
            // `file.flush()` is a userspace no-op on a raw `std::fs::File`, so
            // without `sync_data` the bytes would live only in the OS page
            // cache. A cross-volume copy/move landing on a local disk (MTP →
            // Local, SMB → Local, USB import) all flows through this method, so
            // reporting "complete" here without an fdatasync would let the user
            // eject / sleep and lose data (on a move, from both sides). This
            // gives the same "durable as each file completes" property the
            // local-FS chunked copy path already has (`transfer/chunked_copy.rs`):
            // a crash mid-batch leaves earlier files safe.
            //
            // Best-effort on error, matching `durability::flush_created_destinations`:
            // a failed sync is logged under `target: "write_durability"`, NOT
            // propagated. The bytes are written either way, and failing a
            // completed multi-GB transfer at the final fsync is worse UX than
            // accepting a small durability-window risk on a filesystem that
            // can't sync.
            //
            // Then the source's metadata, on the open handle, after the last
            // byte AND the flush: xattrs, the mode, then birth and mtime in one
            // call (`apply_staged`). ❗ Flush first: on a macOS SMB mount, writes
            // still buffered when the dates go on reach the server at the
            // handle's real close, which stamps its own mtime. Best effort: the
            // bytes are the copy.
            let times = StagedTimes {
                modified: stream.modified_at(),
                accessed: None,
            };
            let dest_for_sync = dest_abs.clone();
            let carried_for_apply = carried.clone();
            let (file, report) = spawn_blocking(move || {
                use std::io::Write;
                // Userspace flush first (harmless no-op on a raw File, but
                // correct if the writer is ever wrapped in a BufWriter).
                let _ = file.flush();
                if let Err(e) = cmdr_fs::durable::sync_file_data(&file) {
                    log::warn!(
                        target: "write_durability",
                        "write_from_stream: fdatasync failed for {}: {e}",
                        LogPrivate::file_path(&dest_for_sync)
                    );
                }
                let report = apply_staged(StagedItem::File(&file), &carried_for_apply, times);
                if report.any_refused() {
                    log::warn!(
                        target: "local_posix",
                        "write_from_stream: {} took only some of the source's metadata: {report:?}",
                        LogPrivate::file_path(&dest_for_sync)
                    );
                }
                (file, report)
            })
            .await
            .expect("spawn_blocking sync_data closure doesn't panic and the task is uncancelable");
            drop(file);

            // Best-effort: fsync the parent directory so the new file's
            // directory entry (the create) is durable too. Some filesystems
            // reject directory fsync; log and continue.
            if let Some(parent) = dest_abs.parent() {
                let parent = parent.to_path_buf();
                let _ = spawn_blocking(move || match std::fs::File::open(&parent).and_then(|d| d.sync_all()) {
                    Ok(()) => {}
                    Err(e) => log::debug!(
                        target: "write_durability",
                        "write_from_stream: parent dir fsync skipped for {}: {e}",
                        LogPrivate::dir_path(&parent)
                    ),
                })
                .await;
            }

            // No `notify_mutation` call here: `LocalPosixVolume`'s mutation
            // methods (create_file/delete/rename) all rely on FSEvents to
            // patch the cache, and FSEvents is reliable on local FS. The
            // SMB / MTP overrides need it because their out-of-band
            // notification channels can lose events; we don't.
            Ok(StreamWritten::with_report(
                bytes_written,
                &report,
                &carried,
                times.modified,
            ))
        })
    }
}

/// [`Volume::apply_metadata`](cmdr_fs::volume::Volume::apply_metadata)
/// for a local item that has landed: the xattrs, the routed mode folded against
/// the item's kind, birth and mtime in one call, then the flags last (Locked
/// refuses every later write).
pub(super) fn apply_metadata_at(
    path: &Path,
    carried: &CarriedMetadata,
    modified: Option<std::time::SystemTime>,
) -> MetadataReport {
    // The kind decides the default the mode folds against; an item that can't
    // be stat'ed keeps the mode it has.
    let mode = carried.mode.and_then(|mode| {
        let default = match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() => default_folder_mode(),
            Ok(_) => default_file_mode(),
            Err(e) => {
                log::debug!(target: "local_posix", "apply_metadata: couldn't stat {}: {e}", LogPrivate::path(path));
                return None;
            }
        };
        routed_mode(mode, default)
    });
    let staged = CarriedMetadata {
        mode,
        flags: UserFlags::NONE,
        ..carried.clone()
    };
    let mut report = if staged.is_empty() && modified.is_none() {
        MetadataReport::ABSENT
    } else {
        apply_staged(
            StagedItem::Path(path),
            &staged,
            StagedTimes {
                modified,
                accessed: None,
            },
        )
    };
    if !carried.flags.is_empty() {
        report.flags = apply_landed(path, &LandedMetadata::default().with_flags(carried.flags)).flags;
    }
    if report.any_refused() {
        log::warn!(
            target: "local_posix",
            "apply_metadata: {} took only some of the source's metadata: {report:?}",
            LogPrivate::path(path)
        );
    }
    report
}
