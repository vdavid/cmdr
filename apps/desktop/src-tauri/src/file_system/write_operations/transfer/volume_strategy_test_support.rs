//! Shared fixtures and test doubles for the `volume_strategy.rs` test suites
//! (`volume_strategy_copy_tests.rs`, `volume_strategy_pause_tests.rs`,
//! `volume_strategy_yield_tests.rs`, `volume_strategy_stale_handle_tests.rs`).
//!
//! Holds the custom `Volume` / `VolumeReadStream` doubles every suite shares
//! plus the auto-yield tuning override. Items are `pub(super)` so the sibling
//! test modules (all children of the `volume_strategy` module) can reach them
//! through `super::test_support::…`. The override is also read by
//! `super::auto_yield_tuning()` in test builds.

use super::*;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{ListingProgress, Volume, VolumeError, VolumeReadStream};
use crate::ignore_poison::IgnorePoison;

pub(super) fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(200)))
}

// ========================================================================
// Slow chunked source (mid-file pause): a multi-chunk volume copy whose
// stream sleeps per chunk so a pause from the controlling task lands mid-file.
// ========================================================================

pub(super) const SLOW_CHUNK_SIZE: usize = 64 * 1024;
pub(super) const SLOW_CHUNK_COUNT: usize = 30;
/// Per-chunk delay so the whole transfer spans ~120 ms — wide enough that a
/// pause from the controlling task reliably lands between two chunks, short
/// enough to keep the test from lingering across other globally-stateful tests.
pub(super) const SLOW_CHUNK_DELAY: Duration = Duration::from_millis(4);

/// A read stream that yields `SLOW_CHUNK_COUNT` chunks of `SLOW_CHUNK_SIZE`
/// bytes, sleeping `SLOW_CHUNK_DELAY` before each, so a multi-chunk copy spans a
/// real wall-clock window for pause/cancel to land mid-stream.
pub(super) struct SlowChunkedStream {
    pub(super) chunks_left: usize,
    pub(super) fill: u8,
    pub(super) total: u64,
    pub(super) emitted: u64,
}

impl VolumeReadStream for SlowChunkedStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.chunks_left == 0 {
                return None;
            }
            tokio::time::sleep(SLOW_CHUNK_DELAY).await;
            self.chunks_left -= 1;
            self.emitted += SLOW_CHUNK_SIZE as u64;
            Some(Ok(vec![self.fill; SLOW_CHUNK_SIZE]))
        })
    }

    fn total_size(&self) -> u64 {
        self.total
    }

    fn bytes_read(&self) -> u64 {
        self.emitted
    }
}

/// Minimal source volume whose `open_read_stream` returns a `SlowChunkedStream`.
/// Non-local + streaming so `copy_single_path` routes through the streaming
/// pipe (and thus the `CheckpointStream` wrapper).
pub(super) struct SlowSource;

impl Volume for SlowSource {
    fn name(&self) -> &str {
        "slow-source"
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
        Box::pin(async { true })
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
    fn open_read_stream<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async {
            Ok(Box::new(SlowChunkedStream {
                chunks_left: SLOW_CHUNK_COUNT,
                fill: 0xCD,
                total: (SLOW_CHUNK_COUNT * SLOW_CHUNK_SIZE) as u64,
                emitted: 0,
            }) as Box<dyn VolumeReadStream>)
        })
    }
}

// ========================================================================
// Stale-destination-handle double.
// ========================================================================

/// Destination volume that rejects the first `write_from_stream` with
/// `StaleDestinationHandle` (a re-keyed MTP folder handle) and accepts the
/// second. Proves the transfer engine re-opens the source and retries once
/// rather than surfacing the stale-handle error to the user.
pub(super) struct FailOnceStaleDest {
    pub(super) calls: AtomicUsize,
}

impl Volume for FailOnceStaleDest {
    fn name(&self) -> &str {
        "fail-once-stale-dest"
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
    fn write_from_stream<'a>(
        &'a self,
        _dest: &'a Path,
        size: u64,
        _stream: Box<dyn VolumeReadStream>,
        _on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let attempt = self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if attempt == 0 {
                Err(VolumeError::StaleDestinationHandle("/Documents".to_string()))
            } else {
                Ok(size)
            }
        })
    }
}

// ========================================================================
// MTP-shaped "releasing" source (bounded-window park-in-place) doubles.
// ========================================================================

pub(super) const REL_TOTAL: usize = 200 * 1024; // 200 KiB, well over one chunk
pub(super) const REL_CHUNK: usize = 16 * 1024;
pub(super) const REL_CHUNK_DELAY: Duration = Duration::from_millis(4);

/// Records what a `ReleasingSource` did, so a test can assert the stream is
/// opened exactly once (no reopen) and `cancel_and_release` is never called (no
/// release) under the bounded-window park-in-place model.
#[derive(Default)]
pub(super) struct RelLog {
    /// Offsets at which a stream was opened. The bounded-window model opens once
    /// at offset 0 and never reopens, so this should always be `[0]`.
    pub(super) opens: Vec<u64>,
    /// Number of times `cancel_and_release` ran. Should always be 0 now — the
    /// copy wrapper parks in place between windows, never releasing the source.
    pub(super) releases: usize,
}

/// A stream over the synthetic `[offset, REL_TOTAL)` byte range. The byte at
/// absolute position `p` is `(p % 256) as u8`, so the assembled destination can
/// be checked against that pattern regardless of where reopens happened.
pub(super) struct ReleasingStream {
    // `log` and `released` ARE read — in `cancel_and_release` below, reachable via the
    // `dyn VolumeReadStream` vtable (stable compiles them as used). The nightly
    // `cargo-udeps` build mis-flags fields read only inside a boxed async trait-method
    // body as dead, so allow it here rather than fail CI on a toolchain quirk.
    #[allow(dead_code, reason = "read in cancel_and_release; nightly cargo-udeps false positive")]
    pub(super) log: Arc<StdMutex<RelLog>>,
    pub(super) pos: u64, // absolute position of the next byte to emit
    pub(super) emitted_here: u64,
    #[allow(dead_code, reason = "read in cancel_and_release; nightly cargo-udeps false positive")]
    pub(super) released: bool,
    /// Optional test-controlled chunk budget. When `Some`, `next_chunk` consumes
    /// one permit before emitting each chunk, so a test can hold the stream at an
    /// exact byte offset (deterministic pause-point control) instead of racing a
    /// wall-clock timer against the stream. `None` = ungated (the default).
    pub(super) gate: Option<Arc<tokio::sync::Semaphore>>,
}

impl VolumeReadStream for ReleasingStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.pos >= REL_TOTAL as u64 {
                return None;
            }
            if let Some(gate) = &self.gate {
                // Wait for the test to release this window; a closed semaphore ends the stream.
                match gate.acquire().await {
                    Ok(permit) => permit.forget(),
                    Err(_) => return None,
                }
            }
            tokio::time::sleep(REL_CHUNK_DELAY).await;
            let start = self.pos;
            let end = (start + REL_CHUNK as u64).min(REL_TOTAL as u64);
            let chunk: Vec<u8> = (start..end).map(|p| (p % 256) as u8).collect();
            self.pos = end;
            self.emitted_here += chunk.len() as u64;
            Some(Ok(chunk))
        })
    }

    fn total_size(&self) -> u64 {
        REL_TOTAL as u64
    }

    fn bytes_read(&self) -> u64 {
        self.emitted_here
    }

    fn cancel_and_release(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            if !self.released {
                self.released = true;
                self.log.lock_ignore_poison().releases += 1;
            }
        })
    }
}

/// An MTP-shaped source that serves the offset-pattern stream and counts any
/// `cancel_and_release` — the test-double of `MtpVolume` for the pause tests. It
/// does NOT opt into foreground yield (the default), so it also doubles as the
/// "non-yield-capable source" in `non_mtp_source_never_auto_yields_for_foreground`.
pub(super) struct ReleasingSource {
    pub(super) log: Arc<StdMutex<RelLog>>,
    /// Optional chunk-budget gate handed to every stream this source opens; see
    /// [`ReleasingStream::gate`]. `None` = ungated (the default for most tests).
    pub(super) gate: Option<Arc<tokio::sync::Semaphore>>,
}

impl Volume for ReleasingSource {
    fn name(&self) -> &str {
        "releasing-source"
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
        Box::pin(async { true })
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
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.open_read_stream_at_offset(path, 0)
    }
    fn open_read_stream_at_offset<'a>(
        &'a self,
        _path: &'a Path,
        offset: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let log = Arc::clone(&self.log);
        let gate = self.gate.clone();
        Box::pin(async move {
            log.lock_ignore_poison().opens.push(offset);
            Ok(Box::new(ReleasingStream {
                log: Arc::clone(&log),
                pos: offset,
                emitted_here: 0,
                released: false,
                gate,
            }) as Box<dyn VolumeReadStream>)
        })
    }
}

/// The reference bytes the destination must end up holding.
pub(super) fn rel_expected_bytes() -> Vec<u8> {
    (0..REL_TOTAL as u64).map(|p| (p % 256) as u8).collect()
}

// ========================================================================
// Foreground auto-yield doubles and tuning override.
// ========================================================================

thread_local! {
    /// Per-test override of `(debounce, min_progress_floor, dest_yield_hard_cap)`.
    /// `None` ⇒ production constants. Set via [`AutoYieldTuningGuard`] and cleared
    /// on drop.
    static AUTO_YIELD_TUNING: std::cell::Cell<Option<(Duration, u64, Duration)>> = const { std::cell::Cell::new(None) };
}

/// Read by `super::auto_yield_tuning()` in test builds; production returns `None`.
pub(super) fn auto_yield_tuning_override() -> Option<(Duration, u64, Duration)> {
    AUTO_YIELD_TUNING.with(|c| c.get())
}

/// RAII guard that installs an auto-yield tuning override for the current thread
/// and restores the previous value on drop. The copy runs on a tokio task; these
/// tests use a CURRENT-THREAD runtime so the spawned copy shares this thread's
/// thread-local (a multi-thread runtime would not see it).
///
/// The source-arm suites ([`AutoYieldTuningGuard::new`]) don't exercise the
/// destination cap, so they get a generous default cap; the destination-arm suite
/// sets a short cap via [`AutoYieldTuningGuard::with_dest_cap`].
pub(super) struct AutoYieldTuningGuard {
    prev: Option<(Duration, u64, Duration)>,
}

impl AutoYieldTuningGuard {
    pub(super) fn new(debounce: Duration, floor: u64) -> Self {
        // A long default cap: the source-arm tests never park on the destination,
        // so the cap is inert for them.
        Self::with_dest_cap(debounce, floor, Duration::from_secs(3600))
    }

    /// Install a tuning override that also sets the destination-side hard cap, for
    /// the destination-yield suite.
    pub(super) fn with_dest_cap(debounce: Duration, floor: u64, dest_hard_cap: Duration) -> Self {
        let prev = AUTO_YIELD_TUNING.with(|c| c.replace(Some((debounce, floor, dest_hard_cap))));
        Self { prev }
    }
}

impl Drop for AutoYieldTuningGuard {
    fn drop(&mut self) {
        AUTO_YIELD_TUNING.with(|c| c.set(self.prev));
    }
}

/// An MTP-shaped source that opts into foreground auto-yield, serving the
/// offset-pattern stream — the test-double of `MtpVolume` + its device priority
/// gate. The `foreground` flag is the controllable equivalent of the gate's
/// `foreground_pending`.
pub(super) struct YieldingSource {
    pub(super) log: Arc<StdMutex<RelLog>>,
    /// When `true`, `foreground_pending()` reports a foreground op is waiting.
    pub(super) foreground: Arc<AtomicBool>,
}

impl Volume for YieldingSource {
    fn name(&self) -> &str {
        "yielding-source"
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
        Box::pin(async { true })
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
    fn supports_foreground_yield(&self) -> bool {
        true
    }
    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let flag = Arc::clone(&self.foreground);
        Box::pin(async move { flag.load(Ordering::SeqCst) })
    }
    fn wait_until_foreground_idle<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        // The double's foreground signal is owned by the test, which clears it to
        // simulate the foreground op draining. Poll it the way the real per-device
        // gate parks until `foreground_pending == 0`.
        let flag = Arc::clone(&self.foreground);
        Box::pin(async move {
            while flag.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.open_read_stream_at_offset(path, 0)
    }
    fn open_read_stream_at_offset<'a>(
        &'a self,
        _path: &'a Path,
        offset: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let log = Arc::clone(&self.log);
        Box::pin(async move {
            log.lock_ignore_poison().opens.push(offset);
            Ok(Box::new(ReleasingStream {
                log: Arc::clone(&log),
                pos: offset,
                emitted_here: 0,
                released: false,
                gate: None,
            }) as Box<dyn VolumeReadStream>)
        })
    }
}

/// A yield-capable MTP-shaped source whose `foreground_pending()` is ALWAYS
/// false and whose `wait_until_foreground_idle()` PANICS if ever called. The
/// auto-yield arm parks (its only caller) only after `foreground_pending()`
/// returns true, so with no foreground pending the arm must short-circuit and
/// never touch this method. A panic here means the copy yielded to ITSELF.
pub(super) struct NeverPendingYieldSource {
    pub(super) opens: Arc<StdMutex<Vec<u64>>>,
}

impl Volume for NeverPendingYieldSource {
    fn name(&self) -> &str {
        "never-pending-yield-source"
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
        Box::pin(async { true })
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
    fn supports_foreground_yield(&self) -> bool {
        true
    }
    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }
    fn wait_until_foreground_idle<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {
            panic!("auto-yield parked despite no foreground pending — self-yield livelock regression");
        })
    }
    fn open_read_stream<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let opens = Arc::clone(&self.opens);
        Box::pin(async move {
            opens.lock_ignore_poison().push(0);
            Ok(Box::new(ReleasingStream {
                log: Arc::new(StdMutex::new(RelLog::default())),
                pos: 0,
                emitted_here: 0,
                released: false,
                gate: None,
            }) as Box<dyn VolumeReadStream>)
        })
    }
}

// ========================================================================
// SMB-shaped WRITE destination that opts into the bounded destination-side
// foreground yield (the upload path). Its `write_from_stream` drains the source
// stream chunk-by-chunk (exactly like `SmbVolume::write_from_stream_impl`'s
// streaming loop), collecting the bytes so a test can assert byte-exactness
// across a destination park, while `foreground` stands in for the per-share
// `foreground_pending` signal.
// ========================================================================

/// A write destination that opts into `supports_foreground_yield_as_destination`
/// and serves a controllable `foreground_pending`. The test-double of an
/// `SmbVolume` upload target: its `write_from_stream` pulls chunks in a loop
/// (driving the wrapping `CheckpointStream`'s per-chunk checkpoint, hence the
/// destination arm) and appends them to `written`, so a test can check the
/// assembled bytes equal a non-yielded upload exactly.
pub(super) struct ForegroundBusyDest {
    /// When `true`, `foreground_pending()` reports the user is browsing this share.
    pub(super) foreground: Arc<AtomicBool>,
    /// Everything `write_from_stream` has written, in order: the assembled file.
    pub(super) written: Arc<StdMutex<Vec<u8>>>,
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
    fn supports_foreground_yield_as_destination(&self) -> bool {
        true
    }
    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let flag = Arc::clone(&self.foreground);
        Box::pin(async move { flag.load(Ordering::SeqCst) })
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
            // Mirror the SMB streaming write loop: pull a chunk (this drives the
            // wrapping `CheckpointStream`'s checkpoint, where the destination arm
            // parks), append it, then fire progress and honor cancellation.
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
