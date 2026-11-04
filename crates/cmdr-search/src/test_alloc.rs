//! A counting global allocator for this crate's tests: the allocation budgets of a scan
//! (`engine/tests/allocations.rs`) and the arena and weight-map memory shapes.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

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
/// This crate's own test binary's: a binary gets exactly ONE global allocator, so the
/// counter lives in the crate whose tests do the measuring (the app keeps its own copy in
/// `apps/desktop/src-tauri/src/test_support.rs`, and `cmdr-index` another).
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
