//! Shared test-only helpers for the whole crate.
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
//! ## Why the live-bytes counter is duplicated here
//!
//! [`heap_bytes_held`] and the `#[global_allocator]` behind it also exist, nearly line for line,
//! in `cmdr-index`'s own `test_support`. That is not an oversight to clean up. A binary gets
//! exactly ONE global allocator, so the counter has to live in the crate whose test binary is
//! doing the measuring, and this crate's test binary is a different one from `cmdr-index`'s.
//! Feature-gating a shared copy doesn't work either: every binary linking that crate would get a
//! second global allocator and fail to build.
//!
//! ❌ **Never let this become a no-op.** With no allocator installed, [`heap_bytes_held`] reports
//! 0 for everything and every memory guard passes while measuring nothing. That's why
//! `search/ranking/memory_tests.rs` asserts a non-zero measurement before it asserts a budget.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::pin::Pin;

pub(crate) use cmdr_fs::testing::{TestDir, wait_until, wait_until_async};

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    BatchScanResult, CopyScanResult, InMemoryVolume, ListingProgress, ScanConflict, SourceItemInfo, Volume, VolumeError,
};

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

    fn scan_for_copy_batch_with_progress<'a>(
        &'a self,
        _paths: &'a [PathBuf],
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
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

// Live heap bytes accounted so far ON THIS THREAD. Thread-local rather than global so the
// harness's other threads can allocate freely without polluting a measurement: a plain
// `cargo test` runs `#[test]`s in parallel inside one process, where a global counter would be
// pure noise. `const`-initialised and `Drop`-free, so reading it never lazily allocates and never
// panics during thread teardown.
thread_local! {
    static LIVE_BYTES: Cell<i64> = const { Cell::new(0) };
}

/// The test binary's allocator: `System`, plus a thread-local live-bytes counter.
///
/// Installed only under `cfg(test)`; the shipping binary keeps mimalloc (`main.rs`).
///
/// Note for anyone comparing memory baselines: Rust test-run numbers are measured under THIS
/// allocator, not mimalloc, so they aren't comparable with production figures.
struct CountingAllocator;

// SAFETY: every method forwards its arguments unchanged to `System`, whose `GlobalAlloc` impl is
// sound, so the pointer/layout contract is exactly `System`'s. The only added work is a
// thread-local counter bump on a `Drop`-free `Cell`, which never allocates and so can't re-enter
// the allocator.
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        account(layout.size() as i64);
        // SAFETY: `layout` is forwarded untouched from our caller, who upholds
        // `GlobalAlloc::alloc`'s contract.
        unsafe { std::alloc::System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        account(layout.size() as i64);
        // SAFETY: `layout` is forwarded untouched from our caller, who upholds
        // `GlobalAlloc::alloc_zeroed`'s contract.
        unsafe { std::alloc::System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        account(new_size as i64 - layout.size() as i64);
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
