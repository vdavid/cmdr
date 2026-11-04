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
//! `crates/cmdr-fs/DETAILS.md` § "The browser build" has the why.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use crate::ignore_poison::IgnorePoison;
use tokio::sync::oneshot;
use wasm_bindgen::prelude::*;

pub use web_time::Instant;

/// The browser build's stand-in for `tokio::runtime::Handle`, with the two
/// methods the engine calls on it.
// DEFAULT-OK: it carries no state; every `Runtime` is the page's one event loop.
#[derive(Debug, Clone, Default)]
pub struct Runtime {
    _private: (),
}

impl Runtime {
    /// Runs `future` on the page's event loop; the handle hears its output.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
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
    pub fn spawn_blocking<F, R>(&self, work: F) -> JoinHandle<R>
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
pub struct JoinHandle<T>(oneshot::Receiver<T>);

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx).map_err(|_| JoinError { id: None })
    }
}

/// The task ended without an answer: aborted, or (from a `JoinHandle`) gone
/// without sending one. A panic aborts the whole module in the browser build
/// (`panic = "abort"`), so `is_panic` is always `false` here.
#[derive(Debug)]
pub struct JoinError {
    id: Option<task::Id>,
}

impl JoinError {
    /// The task it was, when a [`task::JoinSet`] reports it.
    ///
    /// # Panics
    ///
    /// Never for an error a `JoinSet` hands out; a bare `JoinHandle`'s error
    /// carries no id (tokio's always does, which is why its `id` isn't optional).
    pub fn id(&self) -> task::Id {
        self.id.expect("a JoinSet's error names its task")
    }

    /// Never: a panic aborts the module before anyone could hear it.
    pub fn is_panic(&self) -> bool {
        false
    }

    /// Whether it was aborted.
    pub fn is_cancelled(&self) -> bool {
        self.id.is_some()
    }
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.id.is_some() {
            f.write_str("the task was aborted")
        } else {
            f.write_str("the task ended without an answer")
        }
    }
}

/// The wall clock as `std`'s `SystemTime`: `std`'s own `now` panics here, so the
/// page's clock (`web-time`) is read and re-based onto `std`'s epoch.
pub fn wall_now() -> std::time::SystemTime {
    let since_epoch = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default();
    std::time::UNIX_EPOCH + since_epoch
}

/// Runs `future` on the page's event loop (the browser's ambient executor).
pub fn spawn_ambient<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    Runtime::default().spawn(future)
}

/// A set of spawned tasks and a cooperative yield, on the page's event loop.
pub mod task {
    use std::collections::HashMap;
    use std::future::Future;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use futures_util::future::{AbortHandle as Abort, abortable};
    use tokio::sync::mpsc;

    use super::{JoinError, Runtime};

    /// A task's identity within the page, like `tokio::task::Id`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Id(u64);

    impl std::fmt::Display for Id {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.0.fmt(f)
        }
    }

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    /// What `spawn_on` answers: the task's id, and a way to abort it alone.
    pub struct AbortHandle {
        id: Id,
        abort: Abort,
    }

    impl AbortHandle {
        /// The task's id.
        pub fn id(&self) -> Id {
            self.id
        }

        /// Stops the task at its next poll; its join reports `is_cancelled`.
        pub fn abort(&self) {
            self.abort.abort();
        }
    }

    type Ended<T> = (Id, Option<T>);

    /// `tokio::task::JoinSet`'s surface on the page's event loop: spawned tasks,
    /// joined in the order they finish. Like tokio's, dropping the set ABORTS
    /// every task still in it (a `spawn_local` task on its own would run on).
    pub struct JoinSet<T> {
        ended_tx: mpsc::UnboundedSender<Ended<T>>,
        ended_rx: mpsc::UnboundedReceiver<Ended<T>>,
        /// Every task spawned and not yet joined, finished or not (tokio's `len`).
        live: HashMap<Id, Abort>,
    }

    impl<T: 'static> Default for JoinSet<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T: 'static> JoinSet<T> {
        /// An empty set.
        pub fn new() -> Self {
            let (ended_tx, ended_rx) = mpsc::unbounded_channel();
            Self {
                ended_tx,
                ended_rx,
                live: HashMap::new(),
            }
        }

        /// Spawns `future` into the set.
        pub fn spawn_on<F>(&mut self, future: F, runtime: &Runtime) -> AbortHandle
        where
            F: Future<Output = T> + 'static,
        {
            let id = Id(NEXT_ID.fetch_add(1, Ordering::Relaxed));
            let (future, abort) = abortable(future);
            let ended = self.ended_tx.clone();
            // The set keeps the join; the runtime's own handle is dropped (detached).
            drop(runtime.spawn(async move {
                // A closed set was dropped, which already aborted this task.
                let _ = ended.send((id, future.await.ok()));
            }));
            self.live.insert(id, abort.clone());
            AbortHandle { id, abort }
        }

        /// How many tasks are in the set (finished but not yet joined included).
        pub fn len(&self) -> usize {
            self.live.len()
        }

        /// Whether the set holds no task.
        pub fn is_empty(&self) -> bool {
            self.live.is_empty()
        }

        /// The next task to finish, with its id; `None` once the set is empty.
        pub async fn join_next_with_id(&mut self) -> Option<Result<(Id, T), JoinError>> {
            if self.live.is_empty() {
                return None;
            }
            // The set holds a sender, so this never sees a closed channel.
            let (id, output) = self.ended_rx.recv().await?;
            self.live.remove(&id);
            Some(output.map(|value| (id, value)).ok_or(JoinError { id: Some(id) }))
        }

        /// Asks every task in the set to stop; each still joins, as cancelled.
        pub fn abort_all(&self) {
            for abort in self.live.values() {
                abort.abort();
            }
        }
    }

    impl<T> Drop for JoinSet<T> {
        fn drop(&mut self) {
            for abort in self.live.values() {
                abort.abort();
            }
        }
    }

    /// Hands the page back to the browser for one macrotask (`setTimeout(0)`), so
    /// it paints, takes input, and fires timers before this task continues. A
    /// microtask yield (tokio's, with no runtime here) would wake at once and
    /// starve all three.
    pub async fn yield_now() {
        super::Sleep::new(Duration::ZERO).await;
    }
}

/// The subset of `tokio::time` the engine uses, on the page's clock.
pub mod time {
    use std::future::Future;
    use std::time::Duration;

    pub use web_time::Instant;

    /// The current instant on the page's clock.
    pub fn now() -> Instant {
        Instant::now()
    }

    /// Waits `duration` without blocking the page.
    pub async fn sleep(duration: Duration) {
        super::Sleep::new(duration).await;
    }

    /// Waits until `deadline` (at once when it's past).
    pub async fn sleep_until(deadline: Instant) {
        sleep(deadline.saturating_duration_since(Instant::now())).await;
    }

    /// `future`'s output, or [`Elapsed`] once `duration` passes first. Like
    /// tokio's, the timeout drops `future`: callers wrap a spawned task's handle,
    /// which detaches.
    pub async fn timeout<F: Future>(duration: Duration, future: F) -> Result<F::Output, Elapsed> {
        tokio::select! {
            biased;
            output = future => Ok(output),
            () = sleep(duration) => Err(Elapsed),
        }
    }

    /// `future`'s output, or [`Elapsed`] once `deadline` passes first.
    pub async fn timeout_at<F: Future>(deadline: Instant, future: F) -> Result<F::Output, Elapsed> {
        tokio::select! {
            biased;
            output = future => Ok(output),
            () = sleep_until(deadline) => Err(Elapsed),
        }
    }

    /// The deadline passed before the future answered.
    #[derive(Debug)]
    pub struct Elapsed;

    impl std::fmt::Display for Elapsed {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("deadline has elapsed")
        }
    }

    /// A ticker whose late ticks shift the schedule rather than burst, like
    /// tokio's `interval` with `MissedTickBehavior::Delay`. The first tick is
    /// immediate.
    pub fn interval_delayed(period: Duration) -> Interval {
        Interval { period, next: None }
    }

    /// What [`interval_delayed`] answers.
    pub struct Interval {
        period: Duration,
        next: Option<Instant>,
    }

    impl Interval {
        /// Waits for the next tick and answers when it fired.
        pub async fn tick(&mut self) -> Instant {
            if let Some(next) = self.next {
                sleep_until(next).await;
            }
            let fired = Instant::now();
            self.next = Some(fired + self.period);
            fired
        }
    }
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

// DEFAULT-OK: a sleep that just started hasn't fired and has no waker yet.
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
