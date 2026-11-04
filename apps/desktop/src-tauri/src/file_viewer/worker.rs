//! Where the viewer runs its background jobs: the line-index upgrade and its timeout, the
//! search and its watchdog, the encoding rebuild, the tail catch-up, and the watcher
//! manager. The one place a viewer job gets a thread.
//!
//! Each job gets its own OS thread, as it always has: the jobs block on file reads, and
//! some (the watcher's subscribe) block on the OS for seconds, so a shared pool would let
//! one slow mount starve the rest. ❌ Never `std::thread::spawn` a viewer job anywhere
//! else; a host without threads (the browser) answers through here.

use std::thread;
use std::time::Duration;

/// A running job, which a caller may wait for.
pub(super) struct Job(thread::JoinHandle<()>);

impl Job {
    /// Waits for the job to finish. A job that panicked has nothing left to report: the
    /// panic already went to the log through the panic hook.
    pub(super) fn join(self) {
        let _ = self.0.join();
    }
}

/// Runs `job` in the background.
pub(super) fn spawn(job: impl FnOnce() + Send + 'static) -> Job {
    Job(thread::spawn(job))
}

/// Runs `job` once `delay` has passed. Nothing can call it off: a job that may no longer
/// be wanted checks its own flag when it runs.
pub(super) fn after(delay: Duration, job: impl FnOnce() + Send + 'static) {
    thread::spawn(move || {
        thread::sleep(delay);
        job();
    });
}
