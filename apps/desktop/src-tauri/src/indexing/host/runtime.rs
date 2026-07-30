//! The tokio runtime the index spawns background work onto.
//!
//! The host injects its runtime once at startup with [`set_runtime`], and every
//! `spawn` / `spawn_blocking` / `block_on` inside the three index subsystems goes
//! through [`spawn`], [`spawn_blocking`], and [`block_on`] here. That's the whole
//! seam: no index code names a runtime, and the index never builds one of its own.
//!
//! ## Why injected and not owned
//!
//! A crate-owned runtime would be a second thread pool competing with the app's for
//! the same cores, and the QoS story below only holds because there's exactly one.
//! Sharing the host's runtime is what keeps "the index yields to the UI" true.
//!
//! ## Thread QoS lives elsewhere, on purpose
//!
//! ❌ Nothing here lowers a thread's scheduling class, and nothing here should. The
//! heavy walking, writing, and reconciling threads are **dedicated** `std::thread`s
//! that call `cmdr_fs::thread_qos::set_current_thread_qos` at the top of their own
//! bodies; the class sticks to the thread for its whole life, which is why it can
//! never be set on a pooled tokio worker. So the runtime a task is spawned onto has
//! no bearing on QoS, and swapping runtimes can't quietly cost us the property that
//! keeps indexing in-process. `DETAILS.md` § "The runtime seam and thread QoS".
//!
//! ## The fallback runtime
//!
//! When nothing has been injected, the first call lazily builds one multi-threaded
//! runtime and keeps using it. This mirrors what `tauri::async_runtime` does for an
//! app that never calls `set`, so tests, benches, and tools behave exactly as they
//! did before. **The shipped app always injects**, at the top of `setup()`, before
//! any index work can start.

use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinHandle;

/// The host's runtime, injected once at startup.
static INJECTED: OnceLock<Handle> = OnceLock::new();

/// Built on first use only when nothing was injected. Never dropped: tasks spawned
/// onto it outlive any scope we could tie it to.
static FALLBACK: OnceLock<Runtime> = OnceLock::new();

/// A [`set_runtime`] call that arrived after the runtime was already chosen.
#[derive(Debug)]
pub(crate) struct RuntimeAlreadySet;

/// Tells the index which runtime to spawn background work onto.
///
/// Call once, before any indexing starts. A second call keeps the first handle and
/// reports [`RuntimeAlreadySet`] rather than panicking or switching runtimes
/// mid-flight; either of those would strand tasks that are already running.
pub(crate) fn set_runtime(handle: Handle) -> Result<(), RuntimeAlreadySet> {
    INJECTED.set(handle).map_err(|_| RuntimeAlreadySet)
}

/// The runtime background index work runs on: whatever the host injected, or the
/// lazily-built fallback described in the module docs.
pub(crate) fn handle() -> Handle {
    if let Some(injected) = INJECTED.get() {
        return injected.clone();
    }
    FALLBACK
        .get_or_init(|| {
            log::debug!(
                target: "indexing",
                "no runtime injected; building the index's own multi-threaded fallback"
            );
            Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build the index's fallback tokio runtime")
        })
        .handle()
        .clone()
}

/// Spawns an async task onto the index's runtime.
///
/// Safe to call from a synchronous context with no ambient runtime (a scan can start
/// from the app's synchronous `setup()` hook), because the handle is resolved rather
/// than inherited.
#[track_caller]
pub(crate) fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    handle().spawn(future)
}

/// Runs a blocking closure on the index runtime's blocking pool.
///
/// ❌ Don't lower the thread's QoS inside `f`: this is a pooled thread and the class
/// would leak onto whatever runs there next. See the module docs.
#[track_caller]
pub(crate) fn spawn_blocking<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    handle().spawn_blocking(f)
}

/// Drives `future` to completion on the index's runtime, blocking the caller.
///
/// Panics if called from inside an async context; wrap it in
/// `tokio::task::block_in_place` when the caller is already on a runtime thread.
#[track_caller]
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    handle().block_on(future)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seam has to work with nothing injected, because every test binary and
    /// every bench reaches it that way. If this regresses, the whole suite fails at
    /// once with a runtime panic rather than an assertion.
    #[test]
    fn spawns_without_an_injected_runtime() {
        let joined = block_on(async { spawn(async { 7_u8 }).await });
        assert_eq!(joined.expect("spawned task should join cleanly"), 7);
    }

    /// Injecting twice must not swap the runtime under tasks that are already
    /// running on the first one. Injecting the fallback's own handle keeps this test
    /// semantically inert for anything else in the binary: it names the runtime that
    /// was already in use.
    #[test]
    fn a_second_injection_loses_and_hands_its_handle_back() {
        set_runtime(handle()).expect("nothing else in this test binary injects");
        set_runtime(handle()).expect_err("a second injection must lose");
        // And the seam still spawns, on the runtime that won.
        let joined = block_on(async { spawn(async { 7_u8 }).await });
        assert_eq!(joined.expect("spawned task should join cleanly"), 7);
    }
}
