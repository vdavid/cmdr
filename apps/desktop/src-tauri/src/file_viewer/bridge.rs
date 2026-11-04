//! Where the sync viewer core meets the async volumes: a read through a volume's
//! positioned reader, its stat, and the reader's close. The one place the viewer blocks on
//! a future or hands one off.
//!
//! The viewer core is sync and runs on blocking threads (a command's `spawn_blocking`, or
//! one of the viewer's own jobs, `worker.rs`), never inside an async task: blocking on a
//! future from inside one panics. ❌ Never call [`block_on`] from async code.

use std::future::Future;

/// Runs `future` to completion on the app's runtime, blocking this thread meanwhile.
pub(super) fn block_on<F: Future>(future: F) -> F::Output {
    tauri::async_runtime::block_on(future)
}

/// Hands `future` to the app's runtime and returns at once; nobody waits for it.
pub(super) fn spawn_detached(future: impl Future<Output = ()> + Send + 'static) {
    tauri::async_runtime::spawn(future);
}
