//! The persisted ledger's writer: one thread per open log, and the only code
//! that writes to it.
//!
//! **Why a thread.** Records arrive from the per-file hot path of every copy,
//! which runs on an async runtime worker. A `write(2)` there, under the
//! ledger's `std` mutex, is file I/O on a thread the whole app shares, and every
//! other copy task queues on the mutex behind it. It's microseconds on a healthy
//! disk, but nothing bounds it (ERR-PTRGY put a 100 s stall inside this
//! neighborhood; the ledger was a suspect and never confirmed). So the store
//! only queues the bytes and this thread writes them, in the order they were
//! queued, which keeps compaction's rewrite consistent with the appends around
//! it.
//!
//! **What it costs, and only for a temp.** A TEMP's record reaches the kernel a
//! moment after `register` returns. A crash in that window loses the record and
//! leaves a visible `.cmdr-tmp-*` holding nothing but incoming bytes: litter,
//! never data. The name-based directory scan (`reap_stale_transfer_temps`,
//! files over an hour old, at the next cross-volume transfer into that folder)
//! reaps it, or the user deletes it.
//!
//! **Every other kind waits** (`in_flight_temps::track` / `track_in`): an
//! aside or a staging folder can hold the user's only copy, and its caller
//! records it BEFORE the rename that puts the original there. So `track` queues
//! its line with a confirmation ([`LogWriter::append_then`]) and returns only
//! once the writer has tried it: written means in the page cache, which a
//! process crash or a kill can't take. The answer says whether it was, so a
//! caller that must not act on an unrecorded copy (a landing about to clear the
//! way) refuses instead. That's exactly the inline
//! write it replaced, and deliberately no `fsync`: one per aside would make a
//! 10,000-file overwrite pay 10,000 of them.
//! The directory scan never touches an aside (it matches `.cmdr-tmp-` only), so
//! a lost aside record would leave the original stranded under a scratch name;
//! the wait is what rules that out. Pinned by
//! `in_flight_temps_tests.rs::an_aside_is_on_disk_before_track_returns_while_a_temp_only_queues`.
//!
//! The quit teardown waits for the queue to drain ([`LogWriter::barrier`]), so
//! an ordinary quit loses nothing.

use super::super::sync_primitives::{mpsc, thread};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// What the writer thread is asked to do, in order.
enum Op {
    /// Append one line, and tell `written` (when there is one) whether it
    /// reached the kernel.
    Append {
        line: Vec<u8>,
        written: Option<Box<dyn FnOnce(bool) + Send>>,
    },
    /// Replace the log with these lines: compaction.
    Rewrite(Vec<u8>),
    /// Run `then` once everything queued before it has reached the kernel.
    Barrier(Box<dyn FnOnce() + Send>),
}

/// The open log, owned by its writer thread.
///
/// Dropping it closes the queue and joins the thread, so every line queued
/// before the drop is written by the time the drop returns. Production never
/// drops one (the process ledger lives for the process); a test's ledger does,
/// which is what makes its log readable afterwards.
pub(super) struct LogWriter {
    /// `Some` until `Drop`, which takes it to close the queue before joining.
    queue: Option<mpsc::Sender<Op>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl LogWriter {
    /// Starts the writer thread for `log`, which must be open in append mode
    /// at `path`.
    pub(super) fn spawn(log: File, path: PathBuf) -> std::io::Result<Self> {
        let (queue, ops) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("cmdr-temp-ledger".to_string())
            .spawn(move || write_all_queued(log, &path, &ops))?;
        Ok(Self {
            queue: Some(queue),
            thread: Some(thread),
        })
    }

    /// Queues one line. Best-effort, like every write to this log.
    pub(super) fn append(&self, line: Vec<u8>) {
        self.send(Op::Append { line, written: None });
    }

    /// Queues one line, and has the writer run `then` once it tried it: `true`
    /// when the line reached the kernel, `false` when the write failed. Answers
    /// `false` when the writer is gone, and `then` never runs.
    pub(super) fn append_then(&self, line: Vec<u8>, then: impl FnOnce(bool) + Send + 'static) -> bool {
        self.queue.as_ref().is_some_and(|queue| {
            queue
                .send(Op::Append {
                    line,
                    written: Some(Box::new(then)),
                })
                .is_ok()
        })
    }

    /// Queues a rewrite of the whole log to `contents`.
    pub(super) fn rewrite(&self, contents: Vec<u8>) {
        self.send(Op::Rewrite(contents));
    }

    /// Queues a marker: the writer runs `then` once every line queued before it
    /// is written. `false` when the
    /// writer is gone, and `then` never runs.
    ///
    /// A callback rather than a channel so a sync caller and an async one can
    /// each wait their own way, with the ledger's mutex released.
    pub(super) fn barrier(&self, then: impl FnOnce() + Send + 'static) -> bool {
        self.queue
            .as_ref()
            .is_some_and(|queue| queue.send(Op::Barrier(Box::new(then))).is_ok())
    }

    fn send(&self, op: Op) {
        let sent = self.queue.as_ref().is_some_and(|queue| queue.send(op).is_ok());
        if !sent {
            log::debug!(target: "copy", "couldn't record an in-flight leftover: the ledger's writer is gone");
        }
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        drop(self.queue.take());
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            log::warn!(target: "copy", "the in-flight temp ledger's writer panicked; its last records may be missing");
        }
    }
}

/// The writer thread's body: every op, in order, until the queue closes.
fn write_all_queued(mut log: File, path: &Path, ops: &mpsc::Receiver<Op>) {
    for op in ops {
        match op {
            Op::Append { line, written } => {
                let reached = match log.write_all(&line) {
                    Ok(()) => true,
                    Err(e) => {
                        log::debug!(target: "copy", "couldn't record an in-flight leftover: {e}");
                        false
                    }
                };
                if let Some(written) = written {
                    written(reached);
                }
            }
            Op::Rewrite(contents) => match swap_in(path, &contents) {
                Ok(compacted) => log = compacted,
                Err(e) => {
                    log::debug!(target: "copy", "couldn't compact the in-flight temp ledger, keeping it whole: {e}")
                }
            },
            Op::Barrier(then) => then(),
        }
    }
}

/// Writes `contents` beside the log at `path` and renames it over the log,
/// answering the new log's handle, positioned at its end for the appends after.
///
/// ❗ A rename, ❌ never a truncate and a rewrite of the open log: a process
/// killed between those two calls leaves an empty log, and the next launch
/// forgets every record, an aside holding the user's original included. Here a
/// kill at any point leaves the whole old log or the whole new one; a scratch
/// file a kill strands is overwritten by the next compaction. On any failure the
/// old log stays as it is, uncompacted and still right, and keeps taking appends.
fn swap_in(path: &Path, contents: &[u8]) -> std::io::Result<File> {
    let mut scratch_name = path.as_os_str().to_owned();
    scratch_name.push(".compacting");
    let scratch = PathBuf::from(scratch_name);
    let swapped = File::create(&scratch)
        .and_then(|mut fresh| fresh.write_all(contents).map(|()| fresh))
        .and_then(|fresh| std::fs::rename(&scratch, path).map(|()| fresh));
    if swapped.is_err() {
        let _ = std::fs::remove_file(&scratch);
    }
    swapped
}
