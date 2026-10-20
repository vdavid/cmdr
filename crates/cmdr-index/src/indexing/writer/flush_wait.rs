//! What the writer thread is working on right now, and since when, so a caller
//! waiting on it can say what it's stuck behind.
//!
//! The writer's own slow-message WARN (`SLOW_MESSAGE` in `mod.rs`) fires only once
//! a message ENDS, so a message that never ends never names itself. A flush that
//! has waited [`STALL_REPORT_INTERVAL`] reads this and says it instead, once per
//! interval, for as long as it keeps waiting. It never gives up: a single message
//! can legitimately run for minutes (a full aggregate over millions of rows), and
//! a waiter that walked away would leave the work it was waiting for unfinished.
//! The point is that a wedge (GitHub issue #564) names itself in a plain INFO log.

use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use cmdr_fs::ignore_poison::IgnorePoison;

use crate::indexing::store::IndexStoreError;

/// How long a flush waits between "still waiting" lines.
pub(super) const STALL_REPORT_INTERVAL: Duration = Duration::from_secs(30);

/// Where a `Flush` reply goes: an async waiter's oneshot, or a blocking waiter's
/// channel. The blocking one is a std channel because it has to wait with a
/// timeout, so it can say what it's waiting on.
pub enum FlushReply {
    /// `IndexWriter::flush`'s.
    Async(oneshot::Sender<()>),
    /// `IndexWriter::flush_blocking`'s.
    Blocking(mpsc::SyncSender<()>),
}

impl FlushReply {
    pub(super) fn send(self) {
        // The waiter may have gone (a dropped task, a test's timeout); nothing to tell.
        match self {
            FlushReply::Async(tx) => drop(tx.send(())),
            FlushReply::Blocking(tx) => drop(tx.send(())),
        }
    }
}

impl From<oneshot::Sender<()>> for FlushReply {
    fn from(tx: oneshot::Sender<()>) -> Self {
        FlushReply::Async(tx)
    }
}

/// Await an async flush's reply, reporting each [`STALL_REPORT_INTERVAL`] it takes.
pub(super) async fn wait_async(
    mut reply: oneshot::Receiver<()>,
    busy_on: &BusyOn,
    volume_id: &str,
) -> Result<(), IndexStoreError> {
    let started = Instant::now();
    loop {
        match tokio::time::timeout(STALL_REPORT_INTERVAL, &mut reply).await {
            Ok(answer) => return answer.map_err(|_| dropped_reply()),
            Err(_) => busy_on.report_wait(volume_id, started.elapsed()),
        }
    }
}

/// Block on a flush's reply, reporting each `interval` it takes.
pub(super) fn wait_blocking(
    reply: &mpsc::Receiver<()>,
    interval: Duration,
    busy_on: &BusyOn,
    volume_id: &str,
) -> Result<(), IndexStoreError> {
    let started = Instant::now();
    loop {
        match reply.recv_timeout(interval) {
            Ok(()) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => busy_on.report_wait(volume_id, started.elapsed()),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(dropped_reply()),
        }
    }
}

fn dropped_reply() -> IndexStoreError {
    IndexStoreError::Io(std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "Writer thread dropped flush reply",
    ))
}

/// The message the writer thread is inside, if any. Written twice per message by
/// the writer (an uncontended lock), read only by a waiter that has already waited
/// a whole [`STALL_REPORT_INTERVAL`].
#[derive(Default)]
pub(crate) struct BusyOn {
    current: Mutex<Option<(&'static str, Instant)>>,
}

impl BusyOn {
    pub(super) fn start(&self, kind: &'static str) {
        *self.current.lock_ignore_poison() = Some((kind, Instant::now()));
    }

    pub(super) fn end(&self) {
        *self.current.lock_ignore_poison() = None;
    }

    /// The message the writer is on and how long it has been on it, or `None`
    /// while it's between messages.
    pub(crate) fn snapshot(&self) -> Option<(&'static str, Duration)> {
        self.current
            .lock_ignore_poison()
            .map(|(kind, since)| (kind, since.elapsed()))
    }

    /// The "still waiting" line for a flush that has waited `waited` so far.
    pub(super) fn report_wait(&self, volume_id: &str, waited: Duration) {
        log::warn!("{}", wait_line(volume_id, waited, self.snapshot()));
    }
}

/// The text of [`BusyOn::report_wait`], apart so a test can read it.
fn wait_line(volume_id: &str, waited: Duration, busy_on: Option<(&'static str, Duration)>) -> String {
    match busy_on {
        Some((kind, on_it)) => format!(
            "Waited {} s for '{volume_id}''s index writer to flush; it has been on one {kind} for {} s",
            waited.as_secs(),
            on_it.as_secs(),
        ),
        None => format!(
            "Waited {} s for '{volume_id}''s index writer to flush; it's between messages, so its queue is long",
            waited.as_secs(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_snapshot_names_the_message_until_it_ends() {
        let busy = BusyOn::default();
        assert!(busy.snapshot().is_none());
        busy.start("DeleteSubtreeById");
        assert_eq!(busy.snapshot().map(|(kind, _)| kind), Some("DeleteSubtreeById"));
        busy.end();
        assert!(busy.snapshot().is_none());
    }

    /// A sink that parks the writer inside its first emit until the test lets go:
    /// a stand-in for a message that never ends.
    struct ParksTheWriter {
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl crate::EventSink for ParksTheWriter {
        fn emit(&self, _event: crate::IndexEvent) {
            let _ = self.release.lock_ignore_poison().recv();
        }
    }

    /// The scan start's wait (`flush_blocking`) behind a writer stuck in one
    /// message: it keeps waiting, interval after interval, while the writer says
    /// which message holds it, and it answers as soon as the message ends.
    #[test]
    fn a_flush_behind_a_stuck_message_keeps_waiting_and_can_name_it() {
        use crate::indexing::writer::tests::setup_db;
        use crate::indexing::writer::{IndexWriter, WriteMessage};

        let (db_path, _dir) = setup_db();
        let (release, parked) = mpsc::channel();
        let sink = std::sync::Arc::new(ParksTheWriter {
            release: Mutex::new(parked),
        });
        let writer = IndexWriter::spawn(&db_path, sink).unwrap();

        writer
            .send(WriteMessage::EmitDirUpdated(vec!["/a".to_string()]))
            .unwrap();
        let (answered_tx, answered) = mpsc::channel();
        let flusher = writer.clone();
        std::thread::spawn(move || {
            let _ = answered_tx.send(flusher.flush_blocking_reporting_every_for_test(Duration::from_millis(20)));
        });

        // Several report intervals go by: the flush neither returns nor gives up.
        assert!(answered.recv_timeout(Duration::from_millis(200)).is_err());
        assert_eq!(writer.busy_on().map(|(kind, _)| kind), Some("EmitDirUpdated"));

        release.send(()).unwrap();
        answered
            .recv_timeout(Duration::from_secs(5))
            .expect("the flush answers once the message ends")
            .unwrap();
        drop(release);
        writer.shutdown();
    }

    #[test]
    fn the_wait_line_names_the_volume_the_message_and_both_durations() {
        let line = wait_line(
            "root",
            Duration::from_secs(60),
            Some(("DeleteSubtreeById", Duration::from_secs(61))),
        );
        assert_eq!(
            line,
            "Waited 60 s for 'root''s index writer to flush; it has been on one DeleteSubtreeById for 61 s"
        );
    }
}
