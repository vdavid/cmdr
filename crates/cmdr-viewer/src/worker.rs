//! Where the viewer runs its background jobs: the line-index upgrade and its timeout, the
//! search and its watchdog, the encoding rebuild, the tail catch-up, and the watcher
//! manager. The one place a viewer job gets a thread.
//!
//! Each job gets its own OS thread, as it always has: the jobs block on file reads, and
//! some (the watcher's subscribe) block on the OS for seconds, so a shared pool would let
//! one slow mount starve the rest. ❌ Never `std::thread::spawn` a viewer job anywhere
//! else; a host without threads (the browser) answers through here.
//!
//! In the browser a page has one thread: [`spawn`] runs the job INLINE and hands back a
//! finished [`Job`], and [`after`] waits on the page's timer. So a browser job must never
//! wait for another job's progress (the search watchdog is native-only), and nothing may
//! scan a whole big file inside one call (`DETAILS.md` § "The browser build").
#![allow(
    clippy::disallowed_methods,
    reason = "this module is the one place a viewer job gets a thread"
)]

use std::time::Duration;

/// A running job, which a caller may wait for.
#[cfg(not(target_family = "wasm"))]
pub(super) struct Job(std::thread::JoinHandle<()>);

/// A job that already ran (the browser runs every job inline).
#[cfg(target_family = "wasm")]
pub(super) struct Job;

impl Job {
    /// Waits for the job to finish. A job that panicked has nothing left to report: the
    /// panic already went to the log through the panic hook.
    pub(super) fn join(self) {
        #[cfg(not(target_family = "wasm"))]
        let _ = self.0.join();
    }
}

/// Runs `job` in the background.
#[cfg(not(target_family = "wasm"))]
pub(super) fn spawn(job: impl FnOnce() + Send + 'static) -> Job {
    Job(std::thread::spawn(job))
}

/// Runs `job` right here: a page has no other thread (module docs).
#[cfg(target_family = "wasm")]
pub(super) fn spawn(job: impl FnOnce() + Send + 'static) -> Job {
    job();
    Job
}

/// Runs `job` once `delay` has passed. Nothing can call it off: a job that may no longer
/// be wanted checks its own flag when it runs.
#[cfg(not(target_family = "wasm"))]
pub(super) fn after(delay: Duration, job: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        job();
    });
}

/// Runs `job` on the page's event loop once `delay` has passed.
#[cfg(target_family = "wasm")]
pub(super) fn after(delay: Duration, job: impl FnOnce() + Send + 'static) {
    drop(cmdr_fs::rt::Runtime::default().spawn(async move {
        cmdr_fs::rt::time::sleep(delay).await;
        job();
    }));
}
