//! Explicit writer transactions: the guard that opens one and can't leave it open,
//! and the writer-side watch that says so if one stays open anyway.
//!
//! An explicit `BEGIN IMMEDIATE` makes a bulk run (a journal replay, a network
//! walk) one fsync instead of thousands. Its cost is that every write behind it is
//! invisible to every read connection until the `COMMIT`. A sender that returns
//! without committing leaves the writer inside that transaction for the rest of the
//! process: every later write lands in it, no reader ever sees one, the WAL grows
//! without a checkpoint, and the stored replay position never advances, so the next
//! launch replays the same span and leaks again. That's why nothing outside this
//! module can send the raw messages: the [`TransactionToken`] they carry is only
//! minted here, by [`WriterTransaction`], which commits on drop.

use std::time::{Duration, Instant};

use super::{IndexWriter, WriteMessage};
use crate::indexing::store::IndexStoreError;

/// Proof that a `BeginTransaction` / `CommitTransaction` came from a
/// [`WriterTransaction`]. Constructible only inside `writer`, so a caller can't open
/// a transaction it isn't forced to close.
#[derive(Debug)]
pub struct TransactionToken(pub(super) ());

/// One explicit transaction on a volume's writer. Commits when dropped, so every
/// exit path (an early return, a `?`, a dropped future) closes it.
///
/// Drop COMMITS rather than rolls back: every write a bulk run sends is a fact read
/// off the disk, so keeping what landed is right even when the run gives up early,
/// and whatever runs next (a fallback rescan, the live loop) builds on it.
#[must_use = "an explicit transaction commits when dropped; bind it for the span it should cover"]
pub(crate) struct WriterTransaction {
    writer: IndexWriter,
    open: bool,
}

impl IndexWriter {
    /// Open an explicit transaction that lasts until the returned guard commits or
    /// drops.
    pub(crate) fn begin_transaction(&self) -> Result<WriterTransaction, IndexStoreError> {
        self.send(WriteMessage::BeginTransaction(TransactionToken(())))?;
        Ok(WriterTransaction {
            writer: self.clone(),
            open: true,
        })
    }
}

impl WriterTransaction {
    /// Commit now, reporting a writer that's gone.
    pub(crate) fn commit(mut self) -> Result<(), IndexStoreError> {
        self.finish()
    }

    /// Commit now if still open, keeping the guard: for a run that has to commit
    /// before what it sends next (marks and aggregates run in autocommit), at a
    /// point where it still holds the guard. Idempotent.
    pub(crate) fn finish(&mut self) -> Result<(), IndexStoreError> {
        if !self.open {
            return Ok(());
        }
        self.open = false;
        self.writer.send(WriteMessage::CommitTransaction(TransactionToken(())))
    }

    /// Commit what's landed so far and keep going in a fresh transaction, so a long
    /// walk's rows become visible in steps.
    pub(crate) fn commit_and_continue(&mut self) -> Result<(), IndexStoreError> {
        self.finish()?;
        self.writer.send(WriteMessage::BeginTransaction(TransactionToken(())))?;
        self.open = true;
        Ok(())
    }

    /// Whether this guard still holds an open transaction.
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
}

impl Drop for WriterTransaction {
    fn drop(&mut self) {
        if self.open
            && let Err(e) = self.writer.send(WriteMessage::CommitTransaction(TransactionToken(())))
        {
            // The writer is gone, and its connection with it: nothing is left open.
            log::debug!("Writer transaction: commit on drop found the writer gone: {e}");
        }
    }
}

/// How long an explicit transaction may stay open before the writer says so. A
/// network walk commits every 2 s and a healthy replay finishes in seconds to a
/// minute or two, so five minutes is past anything legitimate.
pub(super) const LONG_TRANSACTION_WARN_AFTER: Duration = Duration::from_secs(5 * 60);

/// How often a transaction that's still open repeats the warning.
pub(super) const LONG_TRANSACTION_WARN_EVERY: Duration = Duration::from_secs(15 * 60);

/// The writer's own watch over explicit transactions. The guard makes a leak
/// unrepresentable for a sender; this catches whatever the guard can't, like a
/// bulk run that hangs while holding one. Read off the connection, so it can't
/// disagree with what SQLite actually has open.
pub(super) struct LongTransactionWatch {
    /// When the current explicit transaction was first seen open, and when the
    /// watch last warned about it.
    open: Option<OpenTransaction>,
}

struct OpenTransaction {
    since: Instant,
    last_warned: Option<Instant>,
    /// Messages the writer handled inside it, for the warning.
    messages: u64,
}

impl LongTransactionWatch {
    pub(super) fn new() -> Self {
        Self { open: None }
    }

    /// Note the writer's state after a message (or an idle wakeup). `explicit_open`
    /// is whether an explicit transaction is open on the connection right now;
    /// `handled_one` whether this call follows a message.
    pub(super) fn observe(&mut self, volume_id: &str, explicit_open: bool, handled_one: bool) {
        self.observe_at(volume_id, explicit_open, handled_one, Instant::now());
    }

    fn observe_at(&mut self, volume_id: &str, explicit_open: bool, handled_one: bool, now: Instant) -> bool {
        if !explicit_open {
            self.open = None;
            return false;
        }
        let open = self.open.get_or_insert(OpenTransaction {
            since: now,
            last_warned: None,
            messages: 0,
        });
        if handled_one {
            open.messages += 1;
        }
        let age = now.saturating_duration_since(open.since);
        let due = match open.last_warned {
            None => age >= LONG_TRANSACTION_WARN_AFTER,
            Some(at) => now.saturating_duration_since(at) >= LONG_TRANSACTION_WARN_EVERY,
        };
        if due {
            open.last_warned = Some(now);
            log::warn!(
                "Writer: an explicit transaction on '{volume_id}' has been open for {} min and holds {} \
                 messages; none of them are visible to readers until it commits",
                age.as_secs() / 60,
                open.messages,
            );
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_transaction_never_warns() {
        let mut watch = LongTransactionWatch::new();
        let start = Instant::now();
        assert!(!watch.observe_at("root", true, true, start));
        assert!(!watch.observe_at("root", true, true, start + Duration::from_secs(60)));
        assert!(!watch.observe_at("root", false, true, start + Duration::from_secs(61)));
    }

    #[test]
    fn a_transaction_left_open_warns_once_then_at_the_repeat_interval() {
        let mut watch = LongTransactionWatch::new();
        let start = Instant::now();
        watch.observe_at("root", true, true, start);
        assert!(watch.observe_at("root", true, true, start + LONG_TRANSACTION_WARN_AFTER));
        assert!(
            !watch.observe_at(
                "root",
                true,
                true,
                start + LONG_TRANSACTION_WARN_AFTER + Duration::from_secs(60)
            ),
            "a warning per message would flood the log"
        );
        assert!(watch.observe_at(
            "root",
            true,
            true,
            start + LONG_TRANSACTION_WARN_AFTER + LONG_TRANSACTION_WARN_EVERY
        ));
    }

    #[test]
    fn a_commit_starts_the_clock_over() {
        let mut watch = LongTransactionWatch::new();
        let start = Instant::now();
        watch.observe_at("root", true, true, start);
        watch.observe_at("root", false, false, start + Duration::from_secs(240));
        assert!(
            !watch.observe_at("root", true, true, start + LONG_TRANSACTION_WARN_AFTER),
            "a fresh transaction is only seconds old"
        );
    }
}
