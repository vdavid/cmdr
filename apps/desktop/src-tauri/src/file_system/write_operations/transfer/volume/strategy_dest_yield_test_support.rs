//! Test doubles for the DESTINATION-side foreground yield (the upload path,
//! local → SMB), used by `strategy_dest_yield_tests.rs`.
//!
//! Both doubles are write destinations that pull the source stream chunk-by-chunk
//! the way `SmbVolume::write_from_stream` does, which is what drives the wrapping
//! `CheckpointStream`'s per-chunk checkpoint and so the destination arm. They
//! differ in one thing: whether they opt into the yield at all.
//!
//! Sources, gates, tuning guards, and every other double live in the general
//! `strategy_test_support.rs`.

use std::future::Future;
use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use cmdr_fs::volume::host::activity::BusyVolumes;

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{ListingProgress, Volume, VolumeError, VolumeReadStream};
use crate::ignore_poison::IgnorePoison;

/// The share this double is uploading to. One id, because one double serves one
/// upload; the per-volume scoping of the signal is covered where it lives
/// (`cmdr_fs::volume::host::activity`).
pub(super) const BUSY_DEST_SHARE: &str = "test://dest-yield/share";

/// The quiet window the double reads its signal through, mirroring `cmdr-smb`'s
/// `TRANSFER_FOREGROUND_IDLE_THRESHOLD`. The value can't affect a test's timing:
/// `BusyVolumes` reports a busy share as zero-quiet and a free one as never
/// touched, so the park ends on the scripted EVENT and never on a clock.
const BUSY_DEST_QUIET_WINDOW: Duration = Duration::from_millis(500);

/// A write destination that opts into `supports_foreground_yield_as_destination`
/// and answers both foreground questions through the SAME composed rule and
/// event-driven wait `SmbVolume` uses. The test-double of an `SmbVolume` upload
/// target: its `write_from_stream` pulls chunks (driving the wrapping
/// `CheckpointStream`'s per-chunk checkpoint, hence the destination arm) and
/// lands them in `written`, so a test can check the assembled bytes equal a
/// non-yielded upload exactly.
///
/// A test drives the share with `becomes_busy` / `goes_quiet` (a navigation) or
/// `takes_a_lease` / `releases_a_lease` (a listing running), and the parked upload
/// wakes on that change rather than on any tick.
pub(super) struct ForegroundBusyDest {
    /// The share's foreground signal, scripted by the test.
    pub(super) share: Arc<BusyVolumes>,
    /// How many times the arm has probed `foreground_pending`. A park that
    /// re-probed while parked would be a poll loop; a count that stays put across
    /// a park window is how a test sees that it isn't.
    pub(super) probes: Arc<AtomicUsize>,
    /// How many times the arm has entered the share's WAIT. This is the structural
    /// difference between waiting for the listing to end and re-asking whether it
    /// has: a park that never enters the wait is a poll loop.
    pub(super) waits: Arc<AtomicUsize>,
    /// Everything `write_from_stream` has written, in order: the assembled file.
    pub(super) written: Arc<StdMutex<Vec<u8>>>,
    /// The most bytes this double lands in ONE compound frame, standing in for
    /// `SmbVolume`'s negotiated `max_write_size`. `0` ⇒ nothing is single-shot.
    max_write: u64,
}

/// The double's copy of `cmdr-smb`'s `fits_one_compound_write` (private there).
/// Both `write_is_single_shot` and `write_from_stream` below branch on it, which
/// is the contract every real backend owes: disagreement would leave a truncated
/// file under the user's real filename.
fn fits_one_compound_write(max_write: u64, size: u64) -> bool {
    size > 0 && size <= max_write
}

impl ForegroundBusyDest {
    /// A quiet share whose writes are never single-shot: every upload takes the
    /// streaming shape, holding a write handle open across the whole transfer.
    pub(super) fn quiet(written: Arc<StdMutex<Vec<u8>>>) -> (Arc<Self>, Arc<BusyVolumes>) {
        Self::quiet_with_max_write(written, 0)
    }

    /// A quiet share that lands any write up to `max_write` bytes in one compound
    /// frame, the way `SmbVolume` does below its negotiated `max_write_size`.
    pub(super) fn quiet_with_max_write(
        written: Arc<StdMutex<Vec<u8>>>,
        max_write: u64,
    ) -> (Arc<Self>, Arc<BusyVolumes>) {
        let share = Arc::new(BusyVolumes::new());
        let dest = Arc::new(Self {
            share: Arc::clone(&share),
            probes: Arc::new(AtomicUsize::new(0)),
            waits: Arc::new(AtomicUsize::new(0)),
            written,
            max_write,
        });
        (dest, share)
    }
}

impl Volume for ForegroundBusyDest {
    fn name(&self) -> &str {
        "foreground-busy-dest"
    }
    fn root(&self) -> &Path {
        Path::new("/")
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }
    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }
    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(false) })
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    /// Landing a staged write. This double keeps no path-keyed storage, so the
    /// rename is a no-op — but it must report SUCCESS: a `NotSupported` here
    /// would send `stream_pipe_file` down its can't-stage fallback and write the
    /// file twice, which is not what any of these suites are measuring.
    fn rename<'a>(
        &'a self,
        _from: &'a Path,
        _to: &'a Path,
        _force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn supports_foreground_yield_as_destination(&self) -> bool {
        true
    }
    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.probes.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            cmdr_fs::volume::host::activity::volume_busy_for_user(&*self.share, BUSY_DEST_SHARE, BUSY_DEST_QUIET_WINDOW)
        })
    }
    fn wait_until_foreground_idle<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        self.waits.fetch_add(1, Ordering::SeqCst);
        // The same wait `SmbVolume` takes, over the same composed rule: it resolves
        // on the scripted change, never on a tick.
        Box::pin(cmdr_fs::volume::host::activity::wait_until_volume_free(
            &*self.share,
            BUSY_DEST_SHARE,
            BUSY_DEST_QUIET_WINDOW,
        ))
    }
    fn write_is_single_shot<'a>(&'a self, size: u64) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let single_shot = fits_one_compound_write(self.max_write, size);
        Box::pin(async move { single_shot })
    }
    fn write_from_stream<'a>(
        &'a self,
        _dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let written = Arc::clone(&self.written);
        let single_shot = fits_one_compound_write(self.max_write, size);
        Box::pin(async move {
            // Mirror the SMB write loop: pull a chunk (this drives the wrapping
            // `CheckpointStream`'s checkpoint, where the destination arm parks),
            // then fire progress and honor cancellation. The compound path drains
            // the whole source into a buffer and lands it in one frame at the end,
            // so nothing is open on the destination during the drain; the
            // streaming path lands each chunk as it arrives, holding a handle open
            // across every checkpoint.
            let mut buffered = Vec::new();
            let mut bytes_written = 0u64;
            while let Some(chunk) = stream.next_chunk().await {
                let chunk = chunk?;
                bytes_written += chunk.len() as u64;
                if single_shot {
                    buffered.extend_from_slice(&chunk);
                } else {
                    written.lock_ignore_poison().extend_from_slice(&chunk);
                }
                if on_progress(bytes_written, size).is_break() {
                    return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                }
            }
            if single_shot {
                written.lock_ignore_poison().extend_from_slice(&buffered);
            }
            Ok(bytes_written)
        })
    }
}

/// A write destination that does NOT opt into the destination-side yield and
/// whose `foreground_pending()` PANICS if ever called. The destination arm
/// short-circuits on `supports_foreground_yield_as_destination()` BEFORE probing
/// `foreground_pending`, so this hard-fails on a regression that lets a
/// non-opting target (a local disk, in-memory, or an MTP upload) reach the park.
pub(super) struct PanicIfProbedDest {
    /// The assembled file, so the copy still verifies byte-exact.
    pub(super) written: Arc<StdMutex<Vec<u8>>>,
}

impl Volume for PanicIfProbedDest {
    fn name(&self) -> &str {
        "panic-if-probed-dest"
    }
    fn root(&self) -> &Path {
        Path::new("/")
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }
    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }
    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(false) })
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    /// Landing a staged write. This double keeps no path-keyed storage, so the
    /// rename is a no-op — but it must report SUCCESS: a `NotSupported` here
    /// would send `stream_pipe_file` down its can't-stage fallback and write the
    /// file twice, which is not what any of these suites are measuring.
    fn rename<'a>(
        &'a self,
        _from: &'a Path,
        _to: &'a Path,
        _force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    // supports_foreground_yield_as_destination() stays at the trait default (false).
    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async {
            panic!("destination arm probed foreground_pending on a NON-opting destination: enable-switch regression");
        })
    }
    fn write_from_stream<'a>(
        &'a self,
        _dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let written = Arc::clone(&self.written);
        Box::pin(async move {
            let mut bytes_written = 0u64;
            while let Some(chunk) = stream.next_chunk().await {
                let chunk = chunk?;
                written.lock_ignore_poison().extend_from_slice(&chunk);
                bytes_written += chunk.len() as u64;
                if on_progress(bytes_written, size).is_break() {
                    return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                }
            }
            Ok(bytes_written)
        })
    }
}
