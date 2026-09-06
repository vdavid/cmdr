//! Moving bytes in and out of a local file: the chunked read stream the backend
//! hands out, the ranged read beside it, and the durable streaming write every
//! cross-volume copy landing on local disk goes through.
//!
//! Split out the way the MTP backend's `mtp/streams.rs` is: byte movement has
//! its own vocabulary (chunk sizes, `pread` short reads, fdatasync) and reads as
//! one subject rather than three more `Volume` methods. A trait impl can't span
//! files, so the trait side stays in `local_posix.rs` and the work lives here.

use super::super::{VolumeError, VolumeReadStream};
use super::LocalPosixVolume;
use std::future::Future;
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

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
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

    /// Streams `stream` into `dest`, then `sync_data`s the file (and
    /// best-effort fsyncs the parent dir) before reporting success.
    pub(super) fn write_from_stream_impl<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
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

            // Open destination file on the blocking pool.
            let dest_for_open = dest_abs.clone();
            let mut file = spawn_blocking(move || std::fs::File::create(&dest_for_open))
                .await
                .expect("spawn_blocking File::create closure doesn't panic and the task is uncancelable")
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

                if on_progress(bytes_written, size) == std::ops::ControlFlow::Break(()) {
                    // Drop the file handle and try to clean up the partial file.
                    drop(file);
                    let partial = dest_abs.clone();
                    let _ = spawn_blocking(move || std::fs::remove_file(&partial)).await;
                    return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
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
            // local-FS chunked copy path already has (`transfer/chunked_copy.rs`
            // → `dst_file.sync_data()`): a crash mid-batch leaves earlier files
            // safe.
            //
            // Best-effort on error, matching `durability::flush_created_destinations`:
            // a failed `sync_data` is logged under `target: "write_durability"`,
            // NOT propagated. The bytes are written either way, and failing a
            // completed multi-GB transfer at the final fsync is worse UX than
            // accepting a small durability-window risk on a filesystem that
            // can't sync.
            let dest_for_sync = dest_abs.clone();
            file = spawn_blocking(move || {
                use std::io::Write;
                // Userspace flush first (harmless no-op on a raw File, but
                // correct if the writer is ever wrapped in a BufWriter).
                let _ = file.flush();
                if let Err(e) = file.sync_data() {
                    log::warn!(
                        target: "write_durability",
                        "write_from_stream: fdatasync failed for {}: {e}",
                        dest_for_sync.display()
                    );
                }
                file
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
                        parent.display()
                    ),
                })
                .await;
            }

            // No `notify_mutation` call here: `LocalPosixVolume`'s mutation
            // methods (create_file/delete/rename) all rely on FSEvents to
            // patch the cache, and FSEvents is reliable on local FS. The
            // SMB / MTP overrides need it because their out-of-band
            // notification channels can lose events; we don't.
            Ok(bytes_written)
        })
    }
}
