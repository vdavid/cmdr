//! Shared test-only helpers for the whole crate.
//!
//! Waiting for background work to land: [`wait_until`] serves sync `#[test]`s,
//! [`wait_until_async`] serves `#[tokio::test]`s. Both poll a condition to a deadline
//! and panic when it never holds, so a wait can't silently pass. Don't hand-roll a poll loop, and
//! don't sleep a fixed span hoping the work landed: the sleep inside these two helpers is the only
//! sanctioned one in Rust test code.
//!
//! Measuring allocation shape: [`count_allocations`] reports how many heap allocations a closure
//! made on the calling thread, so a test can pin "this walk doesn't allocate per row" — the
//! invariant behind the index-walk memory work.

use std::cell::Cell;
use std::future::Future;
use std::panic::Location;
use std::time::{Duration, Instant};

/// How often we re-check the condition: short enough that a satisfied wait returns promptly, long
/// enough that a cheap predicate doesn't spin a core.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Polls `condition` until it holds, panicking after `timeout` if it never does.
///
/// `description` finishes the sentence "timed out after 2s waiting for …", so phrase it as a noun
/// phrase: `"the ByteSeek to LineIndex upgrade to finish"`.
///
/// ❌ Don't call this from an `async` test: `std::thread::sleep` blocks the runtime worker and
/// deadlocks a current-thread scheduler. Use [`wait_until_async`] there.
#[track_caller]
pub(crate) fn wait_until(timeout: Duration, description: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return;
        }
        assert!(Instant::now() < deadline, "{}", timed_out(timeout, description));
        // allowed-test-sleep: the sanctioned poll interval; every sync test wait routes through here
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// The async twin of [`wait_until`], for `#[tokio::test]`s.
///
/// Deadline and poll both run on tokio's clock, so a `start_paused` runtime auto-advances through
/// the waiting instead of burning wall-clock.
///
/// This is a plain `fn` returning a future rather than an `async fn` on purpose: `#[track_caller]`
/// doesn't reach through the future an `async fn` generates, so we capture the call site eagerly
/// and put it in the panic message instead.
#[track_caller]
pub(crate) fn wait_until_async<'a>(
    timeout: Duration,
    description: &'a str,
    mut condition: impl FnMut() -> bool + 'a,
) -> impl Future<Output = ()> + 'a {
    let caller = Location::caller();
    async move {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if condition() {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{} (at {caller})",
                timed_out(timeout, description)
            );
            // allowed-test-sleep: the sanctioned poll interval; every async test wait routes through here
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

fn timed_out(timeout: Duration, description: &str) -> String {
    format!("timed out after {timeout:.1?} waiting for {description}")
}

// ── Counting allocations (memory-shape regression guards) ────────────────────────

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_already_true_condition_returns_without_waiting() {
        let started = Instant::now();
        wait_until(Duration::from_secs(30), "an always-true condition", || true);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_condition_that_turns_true_later_is_picked_up() {
        let mut polls = 0;
        wait_until(Duration::from_secs(5), "the third poll", || {
            polls += 1;
            polls >= 3
        });
        assert_eq!(polls, 3);
    }

    #[test]
    #[should_panic(expected = "timed out after 20.0ms waiting for a condition that never holds")]
    fn a_condition_that_never_holds_panics_with_the_description() {
        wait_until(Duration::from_millis(20), "a condition that never holds", || false);
    }

    #[tokio::test]
    async fn an_already_true_condition_returns_without_waiting_async() {
        let started = Instant::now();
        wait_until_async(Duration::from_secs(30), "an always-true condition", || true).await;
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn a_condition_that_turns_true_later_is_picked_up_async() {
        let mut polls = 0;
        wait_until_async(Duration::from_secs(5), "the third poll", || {
            polls += 1;
            polls >= 3
        })
        .await;
        assert_eq!(polls, 3);
    }

    #[tokio::test]
    #[should_panic(expected = "timed out after 20.0ms waiting for a condition that never holds")]
    async fn a_condition_that_never_holds_panics_with_the_description_async() {
        wait_until_async(Duration::from_millis(20), "a condition that never holds", || false).await;
    }

    #[test]
    fn the_timeout_message_names_the_budget_and_the_condition() {
        assert_eq!(
            timed_out(Duration::from_secs(2), "the upgrade to finish"),
            "timed out after 2.0s waiting for the upgrade to finish"
        );
    }
}
