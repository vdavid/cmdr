//! Chunked file copy with cancellation support for network filesystems.
//!
//! macOS `copyfile()` ignores `COPYFILE_QUIT` on network filesystems - the syscall
//! continues until buffered I/O drains. This module provides a chunked read/write
//! alternative that checks cancellation between chunks, allowing immediate response
//! to user cancellation requests.

use super::super::state::IntentCell;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::Ordering;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, LandedMetadata, StagedItem, StagedTimes, apply_staged};

use super::super::overwrite::Staged;
use super::super::types::WriteOperationError;

/// Progress callback for chunked copy operations.
/// Called after each chunk with (bytes_copied_so_far, total_bytes).
pub(crate) type ChunkedCopyProgressFn<'a> = &'a dyn Fn(u64, u64);

/// Chunk size for network file copies (1MB).
/// This provides a good balance between:
/// - Cancellation responsiveness (checked every 1MB)
/// - I/O efficiency (not too many small writes)
const CHUNK_SIZE: usize = 1024 * 1024;

// ============================================================================
// Network filesystem detection
// ============================================================================

/// Detects if the given path is on a network filesystem.
///
/// Returns `true` for SMB, NFS, AFP, and WebDAV filesystems.
/// Returns `false` for local filesystems (APFS, HFS+, etc.) or if detection fails.
///
/// On macOS, copy strategy uses `is_same_apfs_volume` instead (see `copy_strategy.rs`).
/// This function is only used on Linux.
#[cfg(target_os = "linux")]
pub(crate) fn is_network_filesystem(path: &Path) -> bool {
    cmdr_local::linux_mounts::is_network_filesystem_linux(path)
}

// ============================================================================
// Chunked copy with metadata
// ============================================================================

/// Copies a file using chunked read/write with cancellation checks.
///
/// This is used for network filesystems where `copyfile()` doesn't respond
/// to cancellation in a timely manner. The copy checks for cancellation
/// between each 1MB chunk, allowing near-instant response to cancel requests
/// on a responsive filesystem. Against a hung mount the read and write below
/// block without bound and no timeout can be put on them, so a deadline holder
/// abandons the thread instead: `../../DETAILS.md` § "A local blocking read or
/// write cannot be given a timeout".
///
/// `dest` (the staged temp) is created owner-only, so a private file is never
/// readable by others while its bytes stream. After the data and its flush, on
/// the open fd (`cmdr_fs::metadata::apply_staged`): the xattrs, the source's
/// exact mode, then birth, mtime, and atime in one call.
///
/// The ACL and the user flags don't go on the temp: Locked and a `deny delete`
/// ACL refuse the landing rename, so they come back in [`Staged::landed`] for
/// the landing to put on (`overwrite::stage_and_land_file`).
///
/// The optional progress callback is called after each chunk with
/// (bytes_copied_so_far, total_bytes).
pub(crate) fn chunked_copy_with_metadata(
    source: &Path,
    dest: &Path,
    cancelled: &IntentCell,
    progress_callback: Option<ChunkedCopyProgressFn>,
) -> Result<Staged, WriteOperationError> {
    log::debug!(
        "chunked_copy: starting chunked copy from {} to {}",
        LogPrivate::file_path(source),
        LogPrivate::file_path(dest)
    );

    let read_error = |e: std::io::Error, what: &str| WriteOperationError::ReadError {
        path: source.display().to_string(),
        message: format!("Failed to {what}: {e}").into(),
    };
    let mut src_file = std::fs::File::open(source).map_err(|e| read_error(e, "open source file"))?;
    // One `fstat` answers the size, the dates, the mode, and the flags.
    let src_meta = src_file.metadata().map_err(|e| read_error(e, "read source metadata"))?;

    let mut dst_file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(dest)
        .map_err(|e| WriteOperationError::WriteError {
            path: dest.display().to_string(),
            message: format!("Failed to create destination file: {}", e).into(),
        })?;

    let bytes = copy_data_chunked(
        &mut src_file,
        &mut dst_file,
        source,
        dest,
        cancelled,
        src_meta.len(),
        progress_callback,
    )?;

    // Flush the file's data pages durably before signalling success.
    //
    // `sync_all` defeating cancellation only applies *during* the chunk loop;
    // at this point we've left the loop, there's nothing left to cancel, and
    // the next thing the caller does is emit `write-complete`. Without this,
    // the user can pull a USB drive after "Copy finished" and lose a file
    // that lived only in the OS page cache.
    //
    // ❗ Before the dates, not after: on a macOS SMB mount, writes still
    // buffered when the dates go on reach the server at the handle's real
    // close (the landing rename), and the server stamps that close's time as
    // the mtime (verified against the Docker Samba share, macOS 27.0,
    // 2026-10-11: a time-set before the flush read back the rename's time).
    cmdr_fs::durable::sync_file_data(&dst_file).map_err(|e| WriteOperationError::WriteError {
        path: dest.display().to_string(),
        message: format!("Couldn't flush destination to disk: {}", e).into(),
    })?;

    // Best effort: a refused class is logged and the copy stands (invariant 1).
    let carried = CarriedMetadata::of_local_file(&src_file, &src_meta);
    let times = StagedTimes {
        modified: src_meta.modified().ok(),
        accessed: src_meta.accessed().ok(),
    };
    let report = apply_staged(StagedItem::File(&dst_file), &carried, times);
    if report.any_refused() {
        log::warn!(
            "chunked_copy: {} took only some of the metadata of {}: {report:?}",
            LogPrivate::file_path(dest),
            LogPrivate::file_path(source)
        );
    }

    log::debug!(
        "chunked_copy: completed {} bytes from {} to {}",
        bytes,
        LogPrivate::file_path(source),
        LogPrivate::file_path(dest)
    );

    Ok(Staged {
        bytes,
        landed: LandedMetadata::of_source_acl(source).with_flags(carried.flags),
    })
}

/// Test seam: parks a chunked copy of one source part-way through the file, so
/// a test can pull the drive out from under a transfer that is mid-write.
///
/// ❗ Process-global, not thread-local like the other seams here: the engine runs
/// inside `spawn_blocking`, so the copy is on a different thread from the test
/// that installed the park. Each park names the SOURCE it holds, so a sibling
/// test's copy running at the same moment never parks on it. The guard disarms
/// on drop, and the parked copy gives up on its own after [`PARK_CAP`] so a test
/// that dies can't wedge the suite.
#[cfg(test)]
pub(crate) mod chunk_park {
    // Only the macOS real-detach lane arms the park; elsewhere the copy's own hook is all
    // that's read, and `-D unused` is fatal on the Linux build.
    #![cfg_attr(
        not(target_os = "macos"),
        expect(dead_code, reason = "only the macOS real-detach lane arms the park")
    )]

    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use cmdr_fs::ignore_poison::IgnorePoison;

    /// How long a parked copy waits for its release before carrying on anyway.
    const PARK_CAP: Duration = Duration::from_secs(30);
    const NEVER: u64 = u64::MAX;

    /// Every armed park, each naming the source it holds.
    static ARMED: Mutex<Vec<Arc<Park>>> = Mutex::new(Vec::new());

    struct Park {
        source: PathBuf,
        after: AtomicU64,
        parked_at: AtomicU64,
        is_parked: AtomicBool,
        released: AtomicBool,
    }

    /// Arms the park. Uninstalls it on drop, released or not.
    pub(crate) struct ChunkPark(Arc<Park>);

    /// Parks the next chunked copy of `source` once it has written at least
    /// `bytes`.
    pub(crate) fn park_after(source: &Path, bytes: u64) -> ChunkPark {
        let park = Arc::new(Park {
            source: source.to_path_buf(),
            after: AtomicU64::new(bytes),
            parked_at: AtomicU64::new(0),
            is_parked: AtomicBool::new(false),
            released: AtomicBool::new(false),
        });
        ARMED.lock_ignore_poison().push(Arc::clone(&park));
        ChunkPark(park)
    }

    impl ChunkPark {
        /// Blocks until the copy is parked, and answers how many bytes it had
        /// written. `None` if nothing parked within `timeout`.
        pub(crate) fn wait_until_parked(&self, timeout: Duration) -> Option<u64> {
            let deadline = Instant::now() + timeout;
            while Instant::now() < deadline {
                if self.0.is_parked.load(Ordering::SeqCst) {
                    return Some(self.0.parked_at.load(Ordering::SeqCst));
                }
                // allowed-test-sleep: polling a flag another THREAD sets is the
                // subject. The park is what this caller is waiting to observe, so
                // there's no condition `wait_until` could ask about instead.
                std::thread::sleep(Duration::from_millis(10));
            }
            None
        }

        /// Lets the parked copy carry on.
        pub(crate) fn release(&self) {
            self.0.released.store(true, Ordering::SeqCst);
        }
    }

    impl Drop for ChunkPark {
        fn drop(&mut self) {
            ARMED.lock_ignore_poison().retain(|park| !Arc::ptr_eq(park, &self.0));
            self.0.released.store(true, Ordering::SeqCst);
        }
    }

    pub(super) fn park_if_due(source: &Path, written: u64) {
        let due = ARMED
            .lock_ignore_poison()
            .iter()
            .find(|park| park.source == source && written >= park.after.load(Ordering::SeqCst))
            .cloned();
        let Some(park) = due else {
            return;
        };
        // One park per arming: the copy that got here owns it.
        if park.after.swap(NEVER, Ordering::SeqCst) == NEVER {
            return;
        }
        park.parked_at.store(written, Ordering::SeqCst);
        park.is_parked.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + PARK_CAP;
        while !park.released.load(Ordering::SeqCst) && Instant::now() < deadline {
            // allowed-test-sleep: this IS the park. Holding the copy still
            // mid-file is the whole point, and `PARK_CAP` above keeps a dead
            // test from wedging the suite.
            std::thread::sleep(Duration::from_millis(10));
        }
        park.is_parked.store(false, Ordering::SeqCst);
    }
}

/// Copies file data in chunks, checking cancellation between each chunk.
fn copy_data_chunked(
    src_file: &mut std::fs::File,
    dst_file: &mut std::fs::File,
    source: &Path,
    dest: &Path,
    cancelled: &IntentCell,
    source_size: u64,
    progress_callback: Option<ChunkedCopyProgressFn>,
) -> Result<u64, WriteOperationError> {
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut total_bytes = 0u64;

    loop {
        // Check cancellation BEFORE each read
        if cancelled.is_cancelled() {
            log::debug!(
                "chunked_copy: cancellation detected after {} bytes, cleaning up",
                total_bytes
            );
            // Clean up partial file in background (may block on network mounts)
            super::super::cancellable::remove_file_in_background(dest.to_path_buf());
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".into(),
            });
        }

        // Test seam: hold the copy open mid-file so a test can take the drive
        // away under it. No-op in production, which installs no park.
        #[cfg(test)]
        chunk_park::park_if_due(source, total_bytes);

        let bytes_read = src_file.read(&mut buffer).map_err(|e| WriteOperationError::ReadError {
            path: source.display().to_string(),
            message: format!("Failed to read from source: {}", e).into(),
        })?;

        if bytes_read == 0 {
            break; // EOF
        }

        dst_file
            .write_all(&buffer[..bytes_read])
            .map_err(|e| WriteOperationError::WriteError {
                path: dest.display().to_string(),
                message: format!("Failed to write to destination: {}", e).into(),
            })?;

        total_bytes += bytes_read as u64;

        // Report progress after each chunk
        if let Some(cb) = progress_callback {
            cb(total_bytes, source_size);
        }
    }

    Ok(total_bytes)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use cmdr_fs::testing::TestDir;
    use std::fs;
    use std::sync::Arc;

    fn create_temp_dir(name: &str) -> TestDir {
        TestDir::new(&format!("chunked_copy_test_{}", name))
    }

    #[test]
    fn test_chunked_copy_basic() {
        let temp_dir = create_temp_dir("basic");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "Hello, chunked copy!").unwrap();

        let cancelled = IntentCell::running();
        let result = chunked_copy_with_metadata(&src, &dst, &cancelled, None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().bytes, 20); // "Hello, chunked copy!" is 20 bytes
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "Hello, chunked copy!");
    }

    #[test]
    fn test_chunked_copy_cancellation() {
        let temp_dir = create_temp_dir("cancel");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        // Create a file larger than CHUNK_SIZE to ensure we hit the cancellation check
        let large_content = "x".repeat(CHUNK_SIZE + 1000);
        fs::write(&src, &large_content).unwrap();

        // Pre-cancelled
        let cancelled = IntentCell::reading(crate::OperationIntent::Stopped);
        let result = chunked_copy_with_metadata(&src, &dst, &cancelled, None);

        assert!(matches!(result, Err(WriteOperationError::Cancelled { .. })));
        // Partial file cleanup is now async/best-effort (fires on a detached thread),
        // so we don't assert file absence here; it may or may not be deleted yet.
    }

    #[test]
    fn test_chunked_copy_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = create_temp_dir("perms");
        let src = temp_dir.join("source.sh");
        let dst = temp_dir.join("dest.sh");

        fs::write(&src, "#!/bin/bash").unwrap();
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();

        let cancelled = IntentCell::running();
        let result = chunked_copy_with_metadata(&src, &dst, &cancelled, None);

        assert!(result.is_ok());
        let dst_perms = fs::metadata(&dst).unwrap().permissions().mode();
        assert_eq!(dst_perms & 0o777, 0o755);
    }

    #[test]
    fn test_chunked_copy_empty_file() {
        let temp_dir = create_temp_dir("empty");
        let src = temp_dir.join("empty.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "").unwrap();

        let cancelled = IntentCell::running();
        let result = chunked_copy_with_metadata(&src, &dst, &cancelled, None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().bytes, 0);
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "");
    }

    // ------------------------------------------------------------------
    // Metadata-preservation pins (cargo-mutants survivors)
    //
    // Replacing the metadata step with a no-op is undetectable unless a test
    // checks that its side effects happen. The permissions axis is covered by
    // test_chunked_copy_preserves_permissions; below, timestamps and xattrs.
    // The full per-class matrix is `metadata_matrix_tests.rs`.
    // ------------------------------------------------------------------

    #[test]
    fn chunked_copy_preserves_mtime() {
        // Kills: dropping the dates from the staged metadata. Without them the
        // destination's mtime is its creation time (now), not the source's.
        let temp_dir = create_temp_dir("mtime");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");
        fs::write(&src, "stamped").unwrap();

        // Stamp source with a clearly-in-the-past mtime.
        let target_mtime = filetime::FileTime::from_unix_time(1_600_000_000, 0); // 2020-09-13
        filetime::set_file_mtime(&src, target_mtime).unwrap();

        let cancelled = IntentCell::running();
        chunked_copy_with_metadata(&src, &dst, &cancelled, None).unwrap();

        let dst_meta = fs::metadata(&dst).unwrap();
        let dst_mtime = filetime::FileTime::from_last_modification_time(&dst_meta);
        // Allow nanosecond drift but require the seconds to round-trip exactly.
        assert_eq!(
            dst_mtime.unix_seconds(),
            target_mtime.unix_seconds(),
            "chunked copy must preserve source mtime"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn chunked_copy_preserves_user_xattrs() {
        // Kills: dropping the xattrs from the staged metadata.
        // On macOS, user-namespace xattrs roundtrip on APFS tmpdirs.
        let temp_dir = create_temp_dir("xattr");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");
        fs::write(&src, "with-xattr").unwrap();

        let key = "user.cmdr_test_xattr";
        let val = b"sentinel-value-12345";
        if let Err(e) = xattr::set(&src, key, val) {
            // Some CI / encrypted-disk setups disallow user xattrs. Skip in
            // that case rather than failing: the test is opportunistic.
            log::warn!("chunked_copy_preserves_user_xattrs: setting xattr failed: {e}; skipping");
            return;
        }

        let cancelled = IntentCell::running();
        chunked_copy_with_metadata(&src, &dst, &cancelled, None).unwrap();

        let roundtripped = xattr::get(&dst, key).unwrap();
        assert_eq!(
            roundtripped.as_deref(),
            Some(val.as_slice()),
            "chunked copy must preserve user xattrs"
        );
    }

    #[test]
    fn chunked_copy_byte_total_equals_source_size_for_multi_chunk_input() {
        // Pins the `total_bytes += bytes_read` accumulator against `-=` and
        // `*=` mutants. Existing basic test only copies 20 bytes (single
        // chunk); the multi-chunk progress test asserts callback bytes but
        // also tolerates a `*=` mutant when bytes_read == bytes_read. By
        // checking the final return value against the file size for >1MB
        // input, we kill both `+=` mutants definitively.
        let temp_dir = create_temp_dir("total-bytes");
        let src = temp_dir.join("source.bin");
        let dst = temp_dir.join("dest.bin");

        // 3.5 MB → 4 chunks of varying sizes. Picking a non-power-of-two
        // total avoids `*= total` collapsing on chance.
        let payload = vec![0xAB_u8; CHUNK_SIZE * 3 + 12345];
        let expected = payload.len() as u64;
        fs::write(&src, &payload).unwrap();

        let cancelled = IntentCell::running();
        let result = chunked_copy_with_metadata(&src, &dst, &cancelled, None).unwrap();
        assert_eq!(
            result.bytes, expected,
            "returned byte count must equal source length over multiple chunks"
        );
        assert_eq!(fs::metadata(&dst).unwrap().len(), expected);
    }

    #[test]
    fn test_chunked_copy_progress_callback() {
        use std::sync::atomic::AtomicU64;

        let temp_dir = create_temp_dir("progress");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        // Create a file larger than CHUNK_SIZE to ensure multiple callbacks
        let large_content = "x".repeat(CHUNK_SIZE * 2 + 1000);
        let expected_size = large_content.len() as u64;
        fs::write(&src, &large_content).unwrap();

        let cancelled = IntentCell::running();
        let callback_count = Arc::new(AtomicU64::new(0));
        let last_bytes = Arc::new(AtomicU64::new(0));

        let callback_count_clone = Arc::clone(&callback_count);
        let last_bytes_clone = Arc::clone(&last_bytes);
        let progress_cb = move |bytes_done: u64, total: u64| {
            callback_count_clone.fetch_add(1, Ordering::Relaxed);
            last_bytes_clone.store(bytes_done, Ordering::Relaxed);
            assert_eq!(total, expected_size);
        };

        let result = chunked_copy_with_metadata(&src, &dst, &cancelled, Some(&progress_cb));

        assert!(result.is_ok());
        assert_eq!(result.unwrap().bytes, expected_size);
        // Should have been called at least 3 times (for 3 chunks)
        assert!(callback_count.load(Ordering::Relaxed) >= 3);
        // Last callback should report all bytes
        assert_eq!(last_bytes.load(Ordering::Relaxed), expected_size);
    }
}
