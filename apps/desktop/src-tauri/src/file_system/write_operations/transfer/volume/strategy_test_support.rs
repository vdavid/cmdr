//! Shared fixtures and test doubles for the `volume/strategy.rs` test suites
//! (`volume/strategy_copy_tests.rs`, `volume/strategy_pause_tests.rs`,
//! `volume/strategy_yield_tests.rs`, `volume/strategy_stale_handle_tests.rs`).
//!
//! Holds the custom `Volume` / `VolumeReadStream` doubles every suite shares
//! plus the auto-yield tuning override. Items are `pub(super)` so the sibling
//! test modules (all children of the `volume::strategy` module) can reach them
//! through `super::test_support::…`. The override is also read by
//! `super::auto_yield_tuning()` in test builds.

use super::super::faulty_volume::forward_volume_methods;
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

/// Byte-offset flavor of the shared [`crate::file_system::write_operations::test_support::park_holds_at`],
/// so the copy suites can hand it an `AtomicU64` directly.
pub(super) async fn park_holds_at(seen: &std::sync::atomic::AtomicU64, what: &str) -> u64 {
    crate::file_system::write_operations::test_support::park_holds_at(|| seen.load(Ordering::SeqCst), what).await
}

pub(super) use crate::file_system::write_operations::test_support::PARK_WINDOW;

// ========================================================================
// Gated chunked source (mid-file pause): a multi-chunk volume copy whose
// stream emits one chunk per test-granted permit, so the controlling task
// holds it at an exact byte offset instead of racing a wall-clock timer.
// ========================================================================

pub(super) const SLOW_CHUNK_SIZE: usize = 64 * 1024;
pub(super) const SLOW_CHUNK_COUNT: usize = 30;

/// A read stream that yields `SLOW_CHUNK_COUNT` chunks of `SLOW_CHUNK_SIZE`
/// bytes, each one costing a permit from the test-owned chunk budget. The test
/// decides exactly how far the copy gets before it perturbs it, so "the pause
/// landed mid-file" is a fact rather than a timing bet.
pub(super) struct SlowChunkedStream {
    pub(super) chunks_left: usize,
    pub(super) fill: u8,
    pub(super) total: u64,
    pub(super) emitted: u64,
    /// Chunk budget: one permit per chunk. A closed semaphore ends the stream.
    pub(super) gate: Arc<tokio::sync::Semaphore>,
}

impl VolumeReadStream for SlowChunkedStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.chunks_left == 0 {
                return None;
            }
            match self.gate.acquire().await {
                Ok(permit) => permit.forget(),
                Err(_) => return None,
            }
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

/// Minimal source volume whose `open_read_stream` returns a `SlowChunkedStream`
/// on the shared chunk budget. Non-local + streaming so `copy_single_path`
/// routes through the streaming pipe (and thus the `CheckpointStream` wrapper).
pub(super) struct SlowSource {
    pub(super) gate: Arc<tokio::sync::Semaphore>,
}

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
        let gate = Arc::clone(&self.gate);
        Box::pin(async move {
            Ok(Box::new(SlowChunkedStream {
                chunks_left: SLOW_CHUNK_COUNT,
                fill: 0xCD,
                total: (SLOW_CHUNK_COUNT * SLOW_CHUNK_SIZE) as u64,
                emitted: 0,
                gate,
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
// Flaky destination (the retry suite): fails the first N writes, then works.
// ========================================================================

/// A destination that fails its first `fail_writes` `write_from_stream` calls
/// with `error` and delegates every call after that to a real `InMemoryVolume`.
///
/// Two details make it worth more than a counter:
///
/// - **It leaves litter.** A failing attempt writes the bytes it "received" to
///   the target path and does NOT remove them, so the suite can prove that
///   `StagedWrite::abandon` clears the previous attempt's partial rather than
///   relying on a well-behaved backend to have done it. Real backends do clean up
///   after themselves; a wedged one may not.
/// - **It is a real volume underneath**, so staging, the landing rename, and the
///   final content are all observable end to end.
pub(crate) struct FlakyDest {
    pub(crate) inner: Arc<crate::file_system::volume::InMemoryVolume>,
    pub(crate) fail_writes: usize,
    pub(crate) error: VolumeError,
    pub(crate) calls: AtomicUsize,
    /// When set, only writes whose target file name STARTS WITH this string are
    /// eligible to fail (a staged write targets `<name>.cmdr-tmp-<uuid>`, so the
    /// prefix is how a test names one child of a merge). `None` ⇒ every write.
    pub(crate) fail_only_named: Option<String>,
}

impl FlakyDest {
    /// A destination that fails `fail_writes` times with `error`, then works.
    pub(crate) fn new(fail_writes: usize, error: VolumeError) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(
                crate::file_system::volume::InMemoryVolume::new("flaky-dest").with_space_info(10_000_000, 10_000_000),
            ),
            fail_writes,
            error,
            calls: AtomicUsize::new(0),
            fail_only_named: None,
        })
    }

    /// Narrows the failures to one file, named by the prefix of its final name.
    pub(crate) fn only_for(mut self: Arc<Self>, name: &str) -> Arc<Self> {
        Arc::get_mut(&mut self)
            .expect("the fixture is not shared yet")
            .fail_only_named = Some(name.to_owned());
        self
    }

    pub(crate) fn write_calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// Everything sitting in the destination root, by name.
    pub(crate) async fn names(&self) -> Vec<String> {
        self.inner
            .list_directory(Path::new("/"), None)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.name)
            .collect()
    }

    /// The bytes at `path`, or `None` when nothing is there.
    pub(crate) async fn read(&self, path: &str) -> Option<Vec<u8>> {
        let mut stream = self.inner.open_read_stream(Path::new(path)).await.ok()?;
        let mut buf = Vec::new();
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            buf.extend_from_slice(&chunk);
        }
        Some(buf)
    }
}

impl Volume for FlakyDest {
    forward_volume_methods!(
        inner => name,
        root,
        max_concurrent_ops,
        list_directory,
        get_metadata,
        exists,
        is_directory,
        create_directory,
        create_directory_all,
        create_file,
        delete,
        rename,
        get_space_info,
        open_read_stream,
        create_directory_errors_on_existing_dir,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let eligible = self.fail_only_named.as_ref().is_none_or(|name| {
            dest.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(name.as_str()))
        });
        let attempt = if eligible {
            self.calls.fetch_add(1, Ordering::SeqCst)
        } else {
            usize::MAX
        };
        Box::pin(async move {
            if attempt >= self.fail_writes {
                return self.inner.write_from_stream(dest, size, stream, on_progress).await;
            }
            // Drain one chunk and leave it on disk, unremoved: a wedged backend
            // that never got to its own cleanup. The suite asserts our staging
            // clears it before the next attempt.
            let mut partial = Vec::new();
            if let Some(Ok(chunk)) = stream.next_chunk().await {
                partial.extend_from_slice(&chunk);
            }
            let _ = self.inner.delete(dest).await;
            let _ = self.inner.create_file(dest, &partial).await;
            Err(self.error.clone())
        })
    }
}

// ========================================================================
// A volume that lies: `delete` recurses, against the trait contract.
// ========================================================================

/// Wraps an `InMemoryVolume` but makes `delete` recursive, so a guard that
/// leans on `Volume::delete`'s "a single file or **empty** directory" contract
/// has something to fail against.
///
/// Every shipping backend honors that contract, and
/// `cmdr_fs::volume::conformance::assert_delete_leaves_a_non_empty_dir_intact`
/// keeps it that way. This double is for the backend that doesn't exist yet: a
/// guard that only survives because a promise held is a guard that breaks the
/// day someone writes a new `Volume`. Any code path that must not over-delete
/// gets pointed at this volume and has to protect the user's data by itself.
pub(crate) struct RecursiveDeleteVolume {
    inner: Arc<crate::file_system::volume::InMemoryVolume>,
}

impl RecursiveDeleteVolume {
    pub(crate) fn wrapping(inner: Arc<crate::file_system::volume::InMemoryVolume>) -> Arc<Self> {
        Arc::new(Self { inner })
    }
}

impl Volume for RecursiveDeleteVolume {
    forward_volume_methods!(inner => name, root, list_directory, get_metadata, exists, is_directory);

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    /// Recursive delete: contractually wrong, but plausible for some backends.
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if self.inner.is_directory(path).await.unwrap_or(false) {
                let entries = self.inner.list_directory(path, None).await?;
                for entry in entries {
                    let child = PathBuf::from(&entry.path);
                    // Recurse: child might also be a non-empty directory.
                    Box::pin(self.delete(&child)).await.ok();
                }
            }
            self.inner.delete(path).await
        })
    }
}

// ========================================================================
// Source that won't give up one file (the move's source-delete phase).
// ========================================================================

/// A source volume that deletes like a real backend: it refuses to remove a
/// directory that still holds entries, and it refuses outright to delete the one
/// file named by `undeletable`.
///
/// Both halves matter. The named file is the leaf a user can act on; the
/// non-empty-directory refusal is what turns that leaf into the parent's
/// `ENOTEMPTY` on the way out, which is the failure the report used to name
/// instead. `InMemoryVolume` alone can't stage this: its `delete` drops a
/// directory entry whether or not the directory still has children.
pub(crate) struct UndeletableSource {
    inner: Arc<crate::file_system::volume::InMemoryVolume>,
    /// File name (exact) whose delete always fails.
    undeletable: String,
    error: VolumeError,
}

impl UndeletableSource {
    pub(crate) fn new(undeletable: &str, error: VolumeError) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(
                crate::file_system::volume::InMemoryVolume::new("undeletable-source")
                    .with_space_info(10_000_000, 10_000_000),
            ),
            undeletable: undeletable.to_owned(),
            error,
        })
    }
}

impl Volume for UndeletableSource {
    // The scan/conflict surface a SOURCE volume needs comes along here too: the
    // trait's defaults are `NotSupported`, which fails the transfer long before
    // its delete phase.
    forward_volume_methods!(
        inner => name,
        root,
        max_concurrent_ops,
        list_directory,
        get_metadata,
        exists,
        is_directory,
        create_directory,
        create_directory_all,
        create_file,
        rename,
        get_space_info,
        open_read_stream,
        create_directory_errors_on_existing_dir,
        write_from_stream,
        supports_export,
        operations_are_local,
        supports_local_fs_access,
        scan_for_copy,
        scan_for_conflicts,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy() == self.undeletable.as_str())
            {
                return Err(self.error.clone());
            }
            // A real backend's `delete` is "file or EMPTY directory"; refuse a
            // directory that still has contents, the way `remove_dir` does.
            if self.inner.is_directory(path).await.unwrap_or(false)
                && !self.inner.list_directory(path, None).await?.is_empty()
            {
                return Err(VolumeError::IoError {
                    message: "Directory not empty".to_string(),
                    raw_os_error: None,
                });
            }
            self.inner.delete(path).await
        })
    }
}

/// A destination whose FIRST write never returns — the wedge shape that has no
/// deadline anywhere and cost a user a force-quit on 2026-07-31 — and whose
/// second write works normally.
///
/// It is the fixture for the M4.2 → M4.1 handoff: nothing about this write will
/// ever produce an error on its own, so the only thing that can end it is the
/// watchdog, and the only thing that can then save the file is the retry.
pub(super) struct WedgedThenWorkingDest {
    pub(super) inner: Arc<crate::file_system::volume::InMemoryVolume>,
    pub(super) calls: AtomicUsize,
    /// Set once the wedged write has actually been entered, so a test waits on
    /// the state it needs instead of guessing at a duration.
    pub(super) wedged: Arc<AtomicBool>,
}

impl WedgedThenWorkingDest {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(
                crate::file_system::volume::InMemoryVolume::new("wedged-dest").with_space_info(10_000_000, 10_000_000),
            ),
            calls: AtomicUsize::new(0),
            wedged: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(super) fn write_calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Volume for WedgedThenWorkingDest {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.delete(path)
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.rename(from, to, force)
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream(path)
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let attempt = self.calls.fetch_add(1, Ordering::SeqCst);
        let wedged = Arc::clone(&self.wedged);
        Box::pin(async move {
            if attempt > 0 {
                return self.inner.write_from_stream(dest, size, stream, on_progress).await;
            }
            wedged.store(true, Ordering::SeqCst);
            // Never returns, never errors, never reports a byte. Only the
            // watchdog can end this.
            std::future::pending::<()>().await;
            unreachable!("a pending future never resolves")
        })
    }
}

/// A source whose `open_read_stream` never returns — the `OpeningSource` half of
/// the wedge shape, where a device round trip to open the file hangs before a
/// single byte has moved.
///
/// The serial driver awaits each file directly, so nothing above this can end the
/// wait; only the hard-abort tier can (`volume/strategy_abort_tests.rs`).
pub(super) struct WedgedOpenSource {
    /// Set once the open has actually been entered, so a test waits on the state
    /// it needs instead of guessing at a duration.
    pub(super) opening: Arc<AtomicBool>,
}

impl Volume for WedgedOpenSource {
    fn name(&self) -> &str {
        "wedged-open-source"
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
        let opening = Arc::clone(&self.opening);
        Box::pin(async move {
            opening.store(true, Ordering::SeqCst);
            // Never returns, never errors. Only the hard abort can end this.
            std::future::pending::<()>().await;
            unreachable!("a pending future never resolves")
        })
    }
}

/// A destination that behaves like every real backend does on a COOPERATIVE
/// cancel: it notices through `on_progress`, removes the partial it was writing,
/// and reports `Cancelled` — and it records that it did.
///
/// That self-cleanup is the reason tier 1 exists and the reason writes are NOT
/// raced against `backend_cancel`. `own_cleanup_ran` is what a regression would
/// leave `false`.
pub(super) struct TierOneWitnessDest {
    pub(super) inner: Arc<crate::file_system::volume::InMemoryVolume>,
    /// Set when the backend removed its own partial after observing the cancel.
    pub(super) own_cleanup_ran: Arc<AtomicBool>,
    /// Bytes handed to the destination so far, so a test can wait on "the write
    /// is really under way" instead of racing it.
    pub(super) written: Arc<std::sync::atomic::AtomicU64>,
}

impl TierOneWitnessDest {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(
                crate::file_system::volume::InMemoryVolume::new("tier-one-witness-dest")
                    .with_space_info(10_000_000, 10_000_000),
            ),
            own_cleanup_ran: Arc::new(AtomicBool::new(false)),
            written: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        })
    }

    /// Everything sitting in the destination root, by name.
    pub(super) async fn names(&self) -> Vec<String> {
        self.inner
            .list_directory(Path::new("/"), None)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.name)
            .collect()
    }

    /// Replaces whatever is at `path` with `data`, so a growing write is visible
    /// at the write path chunk by chunk (`InMemoryVolume::create_file` refuses to
    /// overwrite an existing entry).
    async fn publish(&self, path: &Path, data: &[u8]) -> Result<(), VolumeError> {
        let _ = self.inner.delete(path).await;
        self.inner.create_file(path, data).await
    }
}

impl Volume for TierOneWitnessDest {
    forward_volume_methods!(
        inner => name,
        root,
        max_concurrent_ops,
        list_directory,
        get_metadata,
        exists,
        is_directory,
        create_directory,
        create_directory_all,
        create_file,
        delete,
        rename,
        get_space_info,
        open_read_stream,
        create_directory_errors_on_existing_dir,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut data: Vec<u8> = Vec::new();
            self.publish(dest, &data).await?;
            while let Some(chunk) = stream.next_chunk().await {
                data.extend_from_slice(&chunk?);
                self.publish(dest, &data).await?;
                self.written.store(data.len() as u64, Ordering::SeqCst);
                if on_progress(data.len() as u64, size).is_break() {
                    // What every real backend does on a cooperative cancel: drop
                    // the handle and remove the partial it was writing.
                    let _ = self.inner.delete(dest).await;
                    self.own_cleanup_ran.store(true, Ordering::SeqCst);
                    return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                }
            }
            Ok(data.len() as u64)
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
            // The modeled cost of an MTP/SMB read window: without it a 200 KiB copy finishes before
            // the controlling task can raise foreground or pause it, so the yield and pause arms
            // would never be exercised.
            // allowed-test-sleep: simulated per-window device latency; the modeled cost, not a wait.
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
                // The DOUBLE's own park, standing in for the real per-device gate's poll: production
                // behavior being simulated, not a test waiting for background work.
                // allowed-test-sleep: the fake device gate's poll interval; simulated production behavior.
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
