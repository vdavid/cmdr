//! Where the sync viewer core meets the async volumes: a read through a volume's
//! positioned reader, its stat, and the reader's close. The one place the viewer blocks on
//! a future or hands one off, always on the runtime its `Viewer` was handed.
//!
//! The viewer core is sync and runs on blocking threads (a command's `spawn_blocking`, or
//! one of the viewer's own jobs, `worker.rs`), never inside an async task: blocking on a
//! future from inside one panics. ❌ Never call [`block_on`] from async code, and ❌ never
//! resolve an ambient runtime here (`Handle::current()`): a reader drops on a plain
//! worker thread, which has none.

use std::future::Future;

use cmdr_fs::rt::Runtime;

/// Runs `future` to completion on `runtime`, blocking this thread meanwhile.
pub(super) fn block_on<F: Future>(runtime: &Runtime, future: F) -> F::Output {
    runtime.block_on(future)
}

/// Hands `future` to `runtime` and returns at once; nobody waits for it.
pub(super) fn spawn_detached(runtime: &Runtime, future: impl Future<Output = ()> + Send + 'static) {
    drop(runtime.spawn(future));
}
