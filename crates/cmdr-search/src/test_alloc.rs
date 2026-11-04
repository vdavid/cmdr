//! A counting global allocator for this crate's tests: the allocation budgets of a scan
//! (`engine/tests/allocations.rs`) and the arena and weight-map memory shapes.

use std::cell::Cell;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};

/// Live heap bytes across the whole process, accounted only while a [`heap_bytes_held`]
/// measurement is open ([`MEASUREMENTS_OPEN`]), so the rest of the suite pays one relaxed load per
/// allocation and nothing else.
///
/// Process-wide rather than per-thread because the thing measured is often built on several
/// threads: a parallel build allocates on rayon's workers and frees scratch on the caller, so a
/// per-thread count sees only the frees (`tests::bytes_allocated_on_a_worker_still_count`).
static PROCESS_LIVE_BYTES: AtomicI64 = AtomicI64::new(0);

/// How many [`heap_bytes_held`] measurements are running right now.
static MEASUREMENTS_OPEN: AtomicUsize = AtomicUsize::new(0);

// `const`-initialised and `Drop`-free, so reading it never lazily allocates and never panics
// during thread teardown.
thread_local! {
    /// The allocation-event counter this thread reports into, if it belongs to a pool built by
    /// [`allocations_on_pool`]. `None` on every other thread, which then pays one TLS read per call.
    static ALLOC_EVENTS: Cell<Option<&'static AtomicU64>> = const { Cell::new(None) };
}

/// The test binary's allocator: `System`, plus a live-bytes counter and a per-pool event counter.
///
/// This crate's own test binary's: a binary gets exactly ONE global allocator, so the
/// counter lives in the crate whose tests do the measuring (the app keeps its own copy in
/// `apps/desktop/src-tauri/src/test_support.rs`, and `cmdr-index` another).
///
/// Note for anyone comparing memory baselines: Rust test-run numbers are measured under THIS
/// allocator, with a counter on every call, so they aren't comparable with production figures.
struct CountingAllocator;

// SAFETY: every method forwards its arguments unchanged to `System`, whose `GlobalAlloc` impl is
// sound, so the pointer/layout contract is exactly `System`'s. The only added work is relaxed
// atomics and a `Drop`-free thread-local read, which never allocate and so can't re-enter the
// allocator.
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

/// Record an allocator event's bytes, but only while a measurement is open. Atomics only, so it
/// never allocates and can't re-enter the allocator, on any thread, at any point of its life.
fn account(bytes: i64) {
    if MEASUREMENTS_OPEN.load(Ordering::Relaxed) > 0 {
        PROCESS_LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed);
    }
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
/// [`heap_bytes_held`] can't answer this: it nets bytes, and a hot loop that allocates and frees
/// per item nets to zero. Counting only this pool's threads, into a
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

/// Run `body` and report the heap bytes its result STILL HOLDS, alongside the result itself,
/// counting every thread `body` puts to work (rayon's workers included).
///
/// The number is requested bytes (the `Layout` sizes), not the allocator's rounded-up block
/// sizes, so it's a floor on real residency and is stable across allocators. Anything `body`
/// allocated and freed nets out, which is the point: this answers "how big is the thing you just
/// built", the question a resident-memory budget is written against.
///
/// ❗ Process-wide: another test allocating in the same process lands in the number. nextest
/// runs each test in a process of its own; under a plain `cargo test`, run it alone (`--exact`).
pub(crate) fn heap_bytes_held<R>(body: impl FnOnce() -> R) -> (R, i64) {
    MEASUREMENTS_OPEN.fetch_add(1, Ordering::SeqCst);
    let before = PROCESS_LIVE_BYTES.load(Ordering::SeqCst);
    let out = body();
    let after = PROCESS_LIVE_BYTES.load(Ordering::SeqCst);
    MEASUREMENTS_OPEN.fetch_sub(1, Ordering::SeqCst);
    (out, after - before)
}

#[cfg(test)]
mod tests {
    use super::heap_bytes_held;

    /// What `body` builds on a rayon worker and hands back still counts: the arena's
    /// parallel load allocates its segments on the workers and frees them on the caller,
    /// so a per-thread count saw only the frees and reported a negative arena.
    #[test]
    fn bytes_allocated_on_a_worker_still_count() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("a test rayon pool should build");
        let (held, bytes) = heap_bytes_held(|| pool.install(|| vec![0u8; 1 << 20]));
        assert!(
            bytes >= held.len() as i64,
            "measured {bytes} B for a {} B buffer",
            held.len()
        );
    }
}
