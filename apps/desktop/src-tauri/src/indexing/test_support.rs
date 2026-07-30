//! Allocation-shape measurement for the index subsystems' memory guards.
//!
//! [`count_allocations`] reports how many heap allocations a closure made on the calling
//! thread and [`heap_bytes_held`] how many bytes its result still holds, so a test can pin
//! "this walk doesn't allocate per row" — the invariant behind the index-walk memory work.
//!
//! ## Why it lives here rather than in a shared crate
//!
//! The counters are fed by a `#[global_allocator]`, and a binary gets exactly one of those.
//! So the harness has to sit in the crate whose test binary is measuring, and it cannot move
//! to `cmdr-fs`: every binary linking that crate — including the shipped app, which uses
//! mimalloc — would get a second global allocator and fail to build. Feature-gating doesn't
//! save it either, since a dev-dependency's features unify with the normal ones for the same
//! package in a workspace test build.
//!
//! **Consequence to carry into the crate move**: `search/ranking/memory_tests.rs` stays
//! app-side and reaches in here for [`heap_bytes_held`]. Once this code is a separate crate,
//! the app's test binary no longer contains this allocator, so that test measures nothing
//! unless the app grows its own copy. Duplicating ~80 lines of allocator forwarding is the
//! only shape that works; it isn't an oversight to clean up.

use std::cell::Cell;

// Heap allocations, and live heap bytes, accounted so far ON THIS THREAD. Thread-local
// rather than global so the harness's other threads can allocate freely without polluting a
// measurement: a plain `cargo test` runs `#[test]`s in parallel inside one process, where
// global counters would be pure noise. Both are `const`-initialised and `Drop`-free, so
// reading them never lazily allocates and never panics during thread teardown.
thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static LIVE_BYTES: Cell<i64> = const { Cell::new(0) };
}

/// The test binary's allocator: `System`, plus a thread-local allocation counter.
///
/// Installed only under `cfg(test)`; the shipping binary keeps mimalloc (`main.rs`). It
/// exists so a test can assert the SHAPE of a hot path's allocations ("this walk must not
/// allocate per directory"), which is what the index-walk memory runaways were made of.
///
/// Note for anyone comparing memory baselines: Rust test-run numbers are measured under
/// THIS allocator, not mimalloc, so they aren't comparable with production figures.
struct CountingAllocator;

// SAFETY: every method forwards its arguments unchanged to `System`, whose `GlobalAlloc`
// impl is sound, so the pointer/layout contract is exactly `System`'s. The only added work
// is a thread-local counter bump on a `Drop`-free `Cell`, which never allocates and so
// can't re-enter the allocator.
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        account(1, layout.size() as i64);
        // SAFETY: `layout` is forwarded untouched from our caller, who upholds
        // `GlobalAlloc::alloc`'s contract.
        unsafe { std::alloc::System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        account(1, layout.size() as i64);
        // SAFETY: `layout` is forwarded untouched from our caller, who upholds
        // `GlobalAlloc::alloc_zeroed`'s contract.
        unsafe { std::alloc::System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        account(1, new_size as i64 - layout.size() as i64);
        // SAFETY: `ptr`/`layout`/`new_size` are forwarded untouched from our caller, who
        // upholds `GlobalAlloc::realloc`'s contract (the block came from this allocator,
        // which is `System`).
        unsafe { std::alloc::System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        account(0, -(layout.size() as i64));
        // SAFETY: `ptr`/`layout` are forwarded untouched from our caller, who upholds
        // `GlobalAlloc::dealloc`'s contract (the block came from this allocator, which is
        // `System`).
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static COUNTING_ALLOCATOR: CountingAllocator = CountingAllocator;

/// Record an allocator event against the current thread, tolerating a thread whose TLS is
/// already torn down (`try_with`), so instrumenting the allocator can never turn a teardown
/// into a panic.
fn account(allocations: u64, bytes: i64) {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + allocations));
    let _ = LIVE_BYTES.try_with(|live| live.set(live.get() + bytes));
}

/// Run `body` and report how many heap allocations it made on THIS thread, alongside its
/// return value.
///
/// Use it to pin a hot path's allocation SHAPE (per-item vs amortised), never an exact
/// number: assert a generous bound that a per-item regression blows through, so the test
/// documents the invariant instead of the allocator's current internals.
pub(crate) fn count_allocations<R>(body: impl FnOnce() -> R) -> (R, u64) {
    let before = ALLOCATIONS.with(Cell::get);
    let out = body();
    let after = ALLOCATIONS.with(Cell::get);
    (out, after.saturating_sub(before))
}

/// Run `body` and report the heap bytes its result STILL HOLDS on this thread, alongside
/// the result itself.
///
/// The number is requested bytes (the `Layout` sizes), not the allocator's rounded-up block
/// sizes, so it's a floor on real residency and is stable across allocators. Anything `body`
/// allocated and freed nets out, which is the point: this answers "how big is the thing you
/// just built", the question a resident-memory budget is written against.
pub(crate) fn heap_bytes_held<R>(body: impl FnOnce() -> R) -> (R, i64) {
    let before = LIVE_BYTES.with(Cell::get);
    let out = body();
    let after = LIVE_BYTES.with(Cell::get);
    (out, after - before)
}
