//! The browser build's executor and clock: the page's own event loop.
//!
//! - **Spawning** queues the future on the page's microtask queue
//!   (`wasm_bindgen_futures::spawn_local`), so a task runs as soon as the JS call
//!   that spawned it returns, with no thread and no pump to drive.
//! - **Blocking work** runs inline: there's no pool to hand it to, and the only
//!   blocking work the engine does there (the overlay gather, the stall probe's
//!   symlink walk) is in-memory and instant in the browser.
//! - **Time** is `performance.now()` (`web-time`) and sleeping is `setTimeout`,
//!   so a 50 ms diff flush is a real 50 ms the page never blocks on.
//!
//! `DETAILS.md` § "The browser build" has the why.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use tokio::sync::oneshot;
use wasm_bindgen::prelude::*;

pub(crate) use web_time::Instant;

/// The browser build's stand-in for `tokio::runtime::Handle`, with the two
/// methods the engine calls on it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Runtime {
    _private: (),
}

impl Runtime {
    /// Runs `future` on the page's event loop; the handle hears its output.
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (answer, handle) = oneshot::channel();
        wasm_bindgen_futures::spawn_local(async move {
            // A dropped handle detaches the task, as tokio's does.
            let _ = answer.send(future.await);
        });
        JoinHandle(handle)
    }

    /// Runs `work` right here: the browser has no blocking pool (module docs).
    pub(crate) fn spawn_blocking<F, R>(&self, work: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R,
    {
        let (answer, handle) = oneshot::channel();
        let _ = answer.send(work());
        JoinHandle(handle)
    }
}

/// A spawned task's output, like `tokio::task::JoinHandle`: dropping it detaches
/// the task, never cancels it.
pub(crate) struct JoinHandle<T>(oneshot::Receiver<T>);

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx).map_err(|_| JoinError)
    }
}

/// The task ended without an answer (it panicked, which in the browser build
/// aborts the module anyway).
#[derive(Debug)]
pub(crate) struct JoinError;

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the task ended without an answer")
    }
}

/// The subset of `tokio::time` the engine uses, on the page's clock.
pub(crate) mod time {
    use std::future::Future;
    use std::time::Duration;

    pub(crate) use web_time::Instant;

    /// Waits `duration` without blocking the page.
    pub(crate) async fn sleep(duration: Duration) {
        super::Sleep::new(duration).await;
    }

    /// Waits until `deadline` (at once when it's past).
    pub(crate) async fn sleep_until(deadline: Instant) {
        sleep(deadline.saturating_duration_since(Instant::now())).await;
    }

    /// `future`'s output, or [`Elapsed`] once `duration` passes first. Like
    /// tokio's, the timeout drops `future`: callers wrap a spawned task's handle,
    /// which detaches.
    pub(crate) async fn timeout<F: Future>(duration: Duration, future: F) -> Result<F::Output, Elapsed> {
        tokio::select! {
            biased;
            output = future => Ok(output),
            () = sleep(duration) => Err(Elapsed),
        }
    }

    /// The deadline passed before the future answered.
    #[derive(Debug)]
    pub(crate) struct Elapsed;
}

#[wasm_bindgen]
extern "C" {
    // The global ones, so this works in a window, a worker, and Node alike.
    #[wasm_bindgen(js_name = setTimeout)]
    fn set_timeout(handler: &JsValue, timeout_ms: f64) -> JsValue;
    #[wasm_bindgen(js_name = clearTimeout)]
    fn clear_timeout(id: f64);
}

/// `setTimeout` beyond `2^31 - 1` ms fires at once, so a longer wait is capped
/// there (24.8 days; the engine's longest is the six-hour reaper sweep).
const MAX_TIMEOUT_MS: f64 = 2_147_483_647.0;

#[derive(Default)]
struct SleepState {
    fired: bool,
    waker: Option<Waker>,
}

/// One `setTimeout`. `Send`, unlike a `JsFuture`, because it holds no JS value:
/// the callback is handed to JS whole, and only the timer's number id stays.
struct Sleep {
    state: Arc<Mutex<SleepState>>,
    /// The id to clear on drop: a number in a browser. Node answers an object,
    /// which this doesn't keep, so there a dropped sleep fires into nothing.
    timer: Option<f64>,
}

impl Sleep {
    fn new(duration: Duration) -> Self {
        let state = Arc::new(Mutex::new(SleepState::default()));
        let fired = Arc::clone(&state);
        let callback = Closure::once_into_js(move || {
            let waker = {
                let mut state = fired.lock_ignore_poison();
                state.fired = true;
                state.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        });
        let ms = (duration.as_secs_f64() * 1000.0).min(MAX_TIMEOUT_MS);
        let timer = set_timeout(&callback, ms).as_f64();
        Self { state, timer }
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut state = self.state.lock_ignore_poison();
        if state.fired {
            Poll::Ready(())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        let fired = self.state.lock_ignore_poison().fired;
        if !fired && let Some(timer) = self.timer {
            clear_timeout(timer);
        }
    }
}
