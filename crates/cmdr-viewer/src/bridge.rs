//! Where the sync viewer core meets the async volumes: a read through a volume's
//! positioned reader, its stat, and the reader's close. The one place the viewer blocks on
//! a future or hands one off, always on the runtime its `Viewer` was handed.
//!
//! The viewer core is sync and runs on blocking threads (a command's `spawn_blocking`, or
//! one of the viewer's own jobs, `worker.rs`), never inside an async task: blocking on a
//! future from inside one panics. ❌ Never call [`block_on`] from async code, and ❌ never
//! resolve an ambient runtime here (`Handle::current()`): a reader drops on a plain
//! worker thread, which has none.
//!
//! In the browser a page can't block, so [`block_on`] polls the future ONCE: an in-memory
//! volume's unpaced read is ready on its first poll. One that isn't (a paced or networked
//! volume) answers `NotSupported`, never a hang or a panic.

use std::future::Future;

use cmdr_fs::rt::Runtime;
use cmdr_fs::volume::VolumeError;

/// Runs `future` to completion on `runtime`, blocking this thread meanwhile.
#[cfg(not(target_family = "wasm"))]
pub(super) fn block_on<T, F>(runtime: &Runtime, future: F) -> Result<T, VolumeError>
where
    F: Future<Output = Result<T, VolumeError>>,
{
    runtime.block_on(future)
}

/// Polls `future` once and answers it, or `NotSupported` when it isn't ready (module docs).
#[cfg(target_family = "wasm")]
pub(super) fn block_on<T, F>(_runtime: &Runtime, future: F) -> Result<T, VolumeError>
where
    F: Future<Output = Result<T, VolumeError>>,
{
    poll_once(future)
}

/// The browser's answer, built on every target so native tests pin it.
#[cfg_attr(
    not(target_family = "wasm"),
    allow(dead_code, reason = "the browser's `block_on`; native tests pin it")
)]
pub(super) fn poll_once<T, F>(future: F) -> Result<T, VolumeError>
where
    F: Future<Output = Result<T, VolumeError>>,
{
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(answer) => answer,
        std::task::Poll::Pending => {
            log::warn!("viewer bridge: a volume call wasn't ready on its first poll; a page can't wait for it");
            Err(VolumeError::NotSupported)
        }
    }
}

/// Hands `future` to `runtime` and returns at once; nobody waits for it.
pub(super) fn spawn_detached(runtime: &Runtime, future: impl Future<Output = ()> + Send + 'static) {
    drop(runtime.spawn(future));
}
