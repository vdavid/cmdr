//! Shared test-only helpers for the whole crate.
//!
//! A volume that never answers: [`WedgedVolume`]. One that answers, but slowly: [`SlowVolume`].
//! A volume that answers capability flags and refuses every read: [`CapabilityStub`].
//!
//! A scratch directory to write into: [`TestDir`]. Waiting for background work to land:
//! [`wait_until`] serves sync `#[test]`s, [`wait_until_async`] serves `#[tokio::test]`s. All three
//! live in `cmdr_fs::testing` (every crate in the workspace gets a scratch dir and waits the same
//! way) and are re-exported here so `crate::test_support::wait_until` keeps resolving. Don't
//! hand-roll a poll loop, and don't sleep a fixed span hoping the work landed: the sleep inside
//! those two helpers is the only sanctioned one in Rust test code.
//!
//! ❌ Don't build a fixture directory out of a compile-time-constant path
//! (`std::env::temp_dir().join("cmdr_foo_test")`): every process on the machine shares it. See
//! [`TestDir`] for the three ways that bites.
//!
//! ❌ A test that reaches the secret store, however indirectly, must open with
//! [`isolate_secrets`]. The store panics rather than falling back to the developer's real one, so
//! forgetting is loud instead of silent.
//!
//! ## Why the live-bytes counter is duplicated here
//!
//! [`heap_bytes_held`] and the `#[global_allocator]` behind it also exist, nearly line for line,
//! in `cmdr-index`'s own `test_support`. That is not an oversight to clean up. A binary gets
//! exactly ONE global allocator, so the counter has to live in the crate whose test binary is
//! doing the measuring, and this crate's test binary is a different one from `cmdr-index`'s.
//! Feature-gating a shared copy doesn't work either: every binary linking that crate would get a
//! second global allocator and fail to build.
//!
//! For parallel code, [`allocations_on_pool`] counts the allocations a dedicated rayon pool's
//! threads make: the per-thread counter can't see work that runs on rayon's workers.
//!
//! ❌ **Never let this become a no-op.** With no allocator installed, [`heap_bytes_held`] reports
//! 0 for everything and every memory guard passes while measuring nothing. That's why
//! `search/ranking/memory_tests.rs` asserts a non-zero measurement before it asserts a budget.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) use cmdr_fs::testing::{TestDir, wait_until, wait_until_async};

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    BatchScanResult, CopyScanResult, InMemoryVolume, ListingProgress, ScanConflict, SourceItemInfo, Volume, VolumeError,
};

/// Points the secret store at a scratch directory of this test's own, and returns it.
///
/// **Every test that reaches `crate::secrets::store()` needs this**, including the ones that get
/// there through `network::keychain`, `ai::api_keys`, or a `#[tauri::command]` that wraps either.
/// Without it the store has nowhere to go but the developer's real data dir
/// (`~/Library/Application Support/com.veszelovszki.cmdr/secrets.json` on macOS): the suite would
/// write live fixtures into someone's actual credentials, and reading them back on the next run
/// would fail every "nothing is stored yet" assertion. The test store refuses to operate without
/// `CMDR_DATA_DIR`, so a missing call panics at the first store access naming this helper.
///
/// **Bind the returned `TestDir` for the whole test** (`let _secrets = isolate_secrets();`).
/// Dropping it deletes the directory, so `let _ = …` would pull the store out from under the test.
/// Holding it is also what keeps `$TMPDIR` clean: the dir goes away when the test ends, instead of
/// one surviving per test per run.
///
/// Call it BEFORE the first store access. The scratch path is random, so no two tests (or two
/// concurrent suite runs) can collide on it.
#[must_use = "dropping the TestDir deletes the store's dir; bind it for the test's lifetime"]
pub(crate) fn isolate_secrets() -> TestDir {
    let dir = TestDir::new("secrets");
    // SAFETY: `std::env::set_var` is unsound only under concurrent env access. Each nextest test
    // runs in its own process, and `isolate_secrets` is called at the top of each test on that
    // process's single (main) thread before any code reads these vars, so no other thread can be
    // touching the environment here.
    unsafe {
        std::env::set_var("CMDR_DATA_DIR", dir.as_ref());
        std::env::set_var("CMDR_SECRET_STORE", "file");
    }
    dir
}

/// A volume that never answers: every read and scan future parks forever.
///
/// This is what a wedged mount looks like from inside the app: no error, no
/// cancel, no progress, no return. It's the fixture for every bound that exists
/// to survive one, because a real network drop isn't repeatable and a volume
/// that never answers is exactly repeatable and reaches the same code.
///
/// Name and root come from an `InMemoryVolume` so this is a real `Volume` rather
/// than a panic trap.
pub(crate) struct WedgedVolume {
    inner: InMemoryVolume,
}

impl WedgedVolume {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            inner: InMemoryVolume::new(name),
        }
    }
}

/// Every wedged method body: park, and never come back.
macro_rules! never_answers {
    () => {
        Box::pin(async move {
            std::future::pending::<()>().await;
            unreachable!("a wedged volume never answers")
        })
    };
}

impl Volume for WedgedVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        never_answers!()
    }

    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        never_answers!()
    }

    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        never_answers!()
    }

    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        never_answers!()
    }

    fn scan_for_copy<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        never_answers!()
    }

    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        _paths: &'a [PathBuf],
        _boundary: &'a cmdr_fs::volume::ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        never_answers!()
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        _items: &'a [SourceItemInfo],
        _dest: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        never_answers!()
    }
}

/// A volume that answers correctly, but only after a delay: on every metadata read
/// ([`new`](Self::new)), or on the rename alone ([`renaming_slowly`](Self::renaming_slowly)).
///
/// The slow-but-alive twin of [`WedgedVolume`]: a busy NAS holds one `stat` or listing for
/// seconds while its session stays healthy. The fixture for a bound that has to survive that
/// without mistaking it for a dead volume. Storage and the connection state come from the wrapped
/// `InMemoryVolume`, so a test decides whether the volume carries a live session.
pub(crate) struct SlowVolume {
    inner: InMemoryVolume,
    /// What every metadata read waits before answering.
    read_delay: std::time::Duration,
    /// What a `rename` waits before landing.
    rename_delay: std::time::Duration,
    /// What a new folder or file waits before landing.
    create_delay: std::time::Duration,
}

impl SlowVolume {
    /// Every metadata read takes `delay`; a rename or a create lands at once.
    pub(crate) fn new(inner: InMemoryVolume, delay: std::time::Duration) -> Self {
        Self {
            inner,
            read_delay: delay,
            rename_delay: std::time::Duration::ZERO,
            create_delay: std::time::Duration::ZERO,
        }
    }

    /// Reads answer at once; the rename itself is the one request the server holds.
    pub(crate) fn renaming_slowly(inner: InMemoryVolume, delay: std::time::Duration) -> Self {
        Self {
            inner,
            read_delay: std::time::Duration::ZERO,
            rename_delay: delay,
            create_delay: std::time::Duration::ZERO,
        }
    }

    /// Reads answer at once; creating a folder or file is what the server holds.
    pub(crate) fn creating_slowly(inner: InMemoryVolume, delay: std::time::Duration) -> Self {
        Self {
            inner,
            read_delay: std::time::Duration::ZERO,
            rename_delay: std::time::Duration::ZERO,
            create_delay: delay,
        }
    }
}

impl Volume for SlowVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn connection_state(&self) -> Option<crate::file_system::volume::ConnectionState> {
        self.inner.connection_state()
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // allowed-test-sleep: the fake latency IS the fixture
            tokio::time::sleep(self.read_delay).await;
            self.inner.list_directory(path, on_progress).await
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // allowed-test-sleep: the fake latency IS the fixture
            tokio::time::sleep(self.read_delay).await;
            self.inner.get_metadata(path).await
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            // allowed-test-sleep: the fake latency IS the fixture
            tokio::time::sleep(self.read_delay).await;
            self.inner.exists(path).await
        })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // allowed-test-sleep: the fake latency IS the fixture
            tokio::time::sleep(self.read_delay).await;
            self.inner.is_directory(path).await
        })
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // allowed-test-sleep: the fake latency IS the fixture
            tokio::time::sleep(self.rename_delay).await;
            self.inner.rename(from, to, force).await
        })
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // allowed-test-sleep: the fake latency IS the fixture
            tokio::time::sleep(self.create_delay).await;
            self.inner.create_directory(path).await
        })
    }

    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // allowed-test-sleep: the fake latency IS the fixture
            tokio::time::sleep(self.create_delay).await;
            self.inner.create_file(path, content).await
        })
    }
}

/// A volume that answers the two capability questions and nothing else.
///
/// The fixture for a seam that decides from flags alone: the favorites add gate
/// (`commands/favorites.rs`), the drag-locality seam (`commands/file_system/drag.rs`). Every I/O
/// method is `unreachable!`, which is the point: a stub that could be listed would invite a test
/// asserting something the seam under test doesn't own, and a seam that starts reading the disk
/// fails loudly here instead of quietly passing.
pub(crate) struct CapabilityStub {
    pub(crate) supports_local_fs_access: bool,
    pub(crate) paths_are_os_visible: bool,
}

/// Every stubbed method body: this seam doesn't get to ask.
macro_rules! never_asked {
    () => {
        unreachable!("a capability stub answers flags, never I/O")
    };
}

impl Volume for CapabilityStub {
    fn name(&self) -> &str {
        "stub"
    }

    fn root(&self) -> &Path {
        Path::new("/Volumes/stub")
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        never_asked!()
    }

    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        never_asked!()
    }

    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        never_asked!()
    }

    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        never_asked!()
    }

    fn supports_local_fs_access(&self) -> bool {
        self.supports_local_fs_access
    }

    fn paths_are_os_visible(&self) -> bool {
        self.paths_are_os_visible
    }
}

// Live heap bytes accounted so far ON THIS THREAD. Thread-local rather than global so the
// harness's other threads can allocate freely without polluting a measurement: a plain
// `cargo test` runs `#[test]`s in parallel inside one process, where a global counter would be
// pure noise. `const`-initialised and `Drop`-free, so reading it never lazily allocates and never
// panics during thread teardown.
thread_local! {
    static LIVE_BYTES: Cell<i64> = const { Cell::new(0) };
    /// The allocation-event counter this thread reports into, if it belongs to a pool built by
    /// [`allocations_on_pool`]. `None` on every other thread, which then pays one TLS read per call.
    static ALLOC_EVENTS: Cell<Option<&'static AtomicU64>> = const { Cell::new(None) };
}

/// The test binary's allocator: `System`, plus a thread-local live-bytes counter.
///
/// Installed only under `cfg(test)`; the shipping binary installs
/// `cmdr_fs::process_memory::GLOBAL_ALLOC` (`main.rs`): the system allocator on macOS, mimalloc
/// on Linux or with the `mimalloc` feature.
///
/// Note for anyone comparing memory baselines: Rust test-run numbers are measured under THIS
/// allocator, with a counter on every call, so they aren't comparable with production figures.
struct CountingAllocator;

// SAFETY: every method forwards its arguments unchanged to `System`, whose `GlobalAlloc` impl is
// sound, so the pointer/layout contract is exactly `System`'s. The only added work is a
// thread-local counter bump on a `Drop`-free `Cell`, which never allocates and so can't re-enter
// the allocator.
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        account(layout.size() as i64);
        count_event();
        // SAFETY: `layout` is forwarded untouched from our caller, who upholds
        // `GlobalAlloc::alloc`'s contract.
        unsafe { std::alloc::System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        account(layout.size() as i64);
        count_event();
        // SAFETY: `layout` is forwarded untouched from our caller, who upholds
        // `GlobalAlloc::alloc_zeroed`'s contract.
        unsafe { std::alloc::System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        account(new_size as i64 - layout.size() as i64);
        count_event();
        // SAFETY: `ptr`/`layout`/`new_size` are forwarded untouched from our caller, who upholds
        // `GlobalAlloc::realloc`'s contract (the block came from this allocator, which is
        // `System`).
        unsafe { std::alloc::System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        account(-(layout.size() as i64));
        // SAFETY: `ptr`/`layout` are forwarded untouched from our caller, who upholds
        // `GlobalAlloc::dealloc`'s contract (the block came from this allocator, which is
        // `System`).
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static COUNTING_ALLOCATOR: CountingAllocator = CountingAllocator;

/// Record an allocator event against the current thread, tolerating a thread whose TLS is already
/// torn down (`try_with`), so instrumenting the allocator can never turn a teardown into a panic.
fn account(bytes: i64) {
    let _ = LIVE_BYTES.try_with(|live| live.set(live.get() + bytes));
}

/// Record one allocation or reallocation against the counter of the pool this thread belongs to.
fn count_event() {
    let _ = ALLOC_EVENTS.try_with(|counter| {
        if let Some(counter) = counter.get() {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    });
}

/// Run `body` inside a fresh `threads`-wide rayon pool, and report how many allocations and
/// reallocations the pool's threads made while it ran.
///
/// The per-thread [`heap_bytes_held`] can't answer this for parallel code: a `par_iter` does its
/// work on rayon's workers, never on the calling thread. Counting only this pool's threads, into a
/// counter of its own, keeps the rest of the harness out of the number: a plain `cargo test` runs
/// `#[test]`s in parallel inside one process, and they'd all land in a global counter.
///
/// `body` runs through `install`, so its own work AND every nested `par_iter` run on the pool.
/// The number includes whatever setup `body` does, so keep per-query setup out of it or budget
/// for it. Events, not bytes: this is the instrument for "does the hot loop allocate per item?".
pub(crate) fn allocations_on_pool<R: Send>(threads: usize, body: impl FnOnce() -> R + Send) -> (R, u64) {
    // Leaked on purpose: a worker's TLS points at it, and a worker can outlive this call by a
    // moment while the pool winds down. One `AtomicU64` per measurement is a fine price.
    let counter: &'static AtomicU64 = Box::leak(Box::new(AtomicU64::new(0)));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .start_handler(move |_| ALLOC_EVENTS.with(|slot| slot.set(Some(counter))))
        .build()
        .expect("a test rayon pool should build");
    // The workers may still be starting (their own stacks and TLS allocate); wait for all of them
    // to report in so their startup isn't counted as the body's.
    pool.broadcast(|_| ());
    counter.store(0, Ordering::Relaxed);
    let out = pool.install(body);
    let events = counter.load(Ordering::Relaxed);
    (out, events)
}

/// Run `body` and report the heap bytes its result STILL HOLDS on this thread, alongside the
/// result itself.
///
/// The number is requested bytes (the `Layout` sizes), not the allocator's rounded-up block
/// sizes, so it's a floor on real residency and is stable across allocators. Anything `body`
/// allocated and freed nets out, which is the point: this answers "how big is the thing you just
/// built", the question a resident-memory budget is written against.
pub(crate) fn heap_bytes_held<R>(body: impl FnOnce() -> R) -> (R, i64) {
    let before = LIVE_BYTES.with(Cell::get);
    let out = body();
    let after = LIVE_BYTES.with(Cell::get);
    (out, after - before)
}
