//! Implicit write batching for the writer loop.
//!
//! In autocommit every message pays its own COMMIT plus a WAL frame write, and on
//! the live path that IS most of the write cost: 31.1 µs per row in autocommit
//! against 7.0 µs for the same rows inside one transaction
//! (`store/tests/insert_throughput_probe.rs`, debug build, 2026-08-03), matching a
//! production stack profile that put 628 of 918 samples under `propagate_delta_by_id`
//! in `sqlite3VdbeHalt` → `vdbeCommit` → `pagerWalFrames` → `walWriteOneFrame` →
//! `pwrite`.
//!
//! So the loop coalesces work that is **already queued**: the first mutation opens
//! `BEGIN IMMEDIATE`, every mutation behind it joins, and the batch commits the
//! moment the queue runs dry. An empty queue commits exactly as eagerly as autocommit
//! did, so batching adds NO latency of its own; only a genuine backlog batches, and
//! that backlog was uncommitted anyway. A time-boxed transaction would instead hold
//! uncommitted work while idle: a strictly worse trade for no gain.
//!
//! The crash window is bounded by the same reasoning plus the two caps below. Losing
//! a bounded amount of recent LIVE index work is acceptable by design (the index is a
//! disposable cache and the reconciler resyncs drift — `indexing/CLAUDE.md` § "Rebuild,
//! don't migrate"); the `dir_stats` ledger's paid/unpaid invariant is not, which is why
//! `MarkLedgerUnpaid` is a [`BatchRole::Barrier`].
//!
//! Depth, including why each variant lands where it does: `DETAILS.md` § "Implicit
//! write batching".

use std::time::{Duration, Instant};

use super::maintenance::run_deferred_wal_checkpoint;
use super::{IndexFailureSignal, ProbeStats, WriteMessage};

/// Hard cap on how many messages one implicit batch may absorb. The queue running
/// dry is the normal close; this only bounds a sustained flood. 512 already amortizes
/// the per-message fsync ~512×, so raising it buys nothing measurable while widening
/// both the crash window and the uncommitted-rows-invisible-to-readers window.
const MAX_MESSAGES: usize = 512;

/// Hard cap on how long one implicit batch may stay open. Deliberately far below the
/// network scanner's 2 s `SCAN_COMMIT_INTERVAL`: that one wraps a bulk stream nobody
/// is watching, while these are LIVE mutations whose rows stay invisible to every read
/// connection (enrichment, the UI) until the commit. 250 ms keeps that lag under the
/// threshold where a just-copied file would visibly show its old size.
const MAX_DURATION: Duration = Duration::from_millis(250);

/// How the writer loop treats a message with respect to the implicit batch.
///
/// Exhaustively matched in [`role`], with no catch-all arm on purpose: a new
/// `WriteMessage` variant must be a deliberate decision here, not a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BatchRole {
    /// A per-message DB mutation that pays a COMMIT of its own in autocommit. Opens
    /// an implicit batch, or joins the open one.
    Mutation,
    /// Must be handled with no implicit batch open, so the batch is committed FIRST.
    /// Either the message's contract requires durability (a `Flush` reply means
    /// "committed"; an `EmitDirUpdated` makes the UI refetch), it replies with a value
    /// a caller compares against another connection, it manages the transaction itself
    /// (`BeginTransaction` / `CommitTransaction` — SQLite has no nested transactions),
    /// or it simply cannot run inside one (`PRAGMA wal_checkpoint` fails `SQLITE_LOCKED`,
    /// `PRAGMA incremental_vacuum` frees nothing).
    Barrier,
    /// Neither opens a batch nor closes one: it joins an open batch when there is one,
    /// and runs in autocommit when there isn't.
    Neutral,
}

/// Which [`BatchRole`] a message plays.
pub(super) fn role(msg: &WriteMessage) -> BatchRole {
    match msg {
        // Single-entry live mutations plus the small meta writes that ride alongside
        // them. These are the messages that pay one COMMIT + WAL frame write per row
        // today, so they're the whole point of the batch.
        WriteMessage::UpsertEntryV2 { .. }
        | WriteMessage::MoveEntryV2 { .. }
        | WriteMessage::DeleteEntryById(_)
        | WriteMessage::DeleteSubtreeById(_)
        | WriteMessage::DeleteDescendantsById(_)
        | WriteMessage::PropagateDeltaById { .. }
        | WriteMessage::PropagateMinSubtreeEpoch(_)
        | WriteMessage::MarkDirsListed { .. }
        | WriteMessage::MarkDirsUnreadable { .. }
        | WriteMessage::ClearAbandonedIfDue
        | WriteMessage::UpdateLastEventId(_)
        | WriteMessage::UpdateMeta { .. }
        | WriteMessage::DeleteMeta(_)
        | WriteMessage::BumpCurrentEpoch => BatchRole::Mutation,

        // A reply, an emit, or an explicit transaction: each one's contract is that
        // everything before it is already durable (or that no transaction is open).
        WriteMessage::Flush(_)
        | WriteMessage::EmitDirUpdated(_)
        | WriteMessage::BeginTransaction
        | WriteMessage::CommitTransaction
        | WriteMessage::Shutdown => BatchRole::Barrier,
        #[cfg(test)]
        WriteMessage::GetEntryCount(_) => BatchRole::Barrier,

        // Maintenance that cannot run inside a transaction at all, and the truncate
        // that ends with an inline `incremental_vacuum`.
        WriteMessage::TruncateData | WriteMessage::IncrementalVacuum | WriteMessage::WalCheckpoint => {
            BatchRole::Barrier
        }

        // Bulk recomputes: already amortized (they rewrite whole swathes of
        // `dir_stats` in one message), and each emits progress or `DirsUpdated` events
        // whose whole job is to make committed sizes visible. Running them in
        // autocommit keeps them byte-for-byte what they were before batching.
        WriteMessage::ComputeAllAggregates { .. }
        | WriteMessage::ComputePartialAggregates { .. }
        | WriteMessage::ComputeSubtreeAggregates { .. }
        | WriteMessage::BackfillMissingDirStats
        | WriteMessage::PayLedgerIfUnpaid => BatchRole::Barrier,

        // The ledger debt is recorded DURABLY before the first suppressed write, so a
        // walk that dies leaves the next launch a marker to heal from. Batching it
        // would make the marker's durability depend on a COMMIT that hasn't run yet,
        // and the ledger's paid/unpaid invariant is the one thing here we don't trade
        // for throughput (`CLAUDE.md` § the dir_stats ledger). It's sent once per bulk
        // walk, so autocommit costs nothing.
        WriteMessage::MarkLedgerUnpaid => BatchRole::Barrier,

        // `InsertEntriesV2` already wraps its ~2000 rows in ONE savepoint, so
        // autocommit costs it a single fsync per 2000 rows — the batch would buy
        // ~nothing and would let a full scan's stream balloon one transaction to
        // hundreds of thousands of rows (and the WAL with it). It still JOINS an open
        // batch so a mixed live stream doesn't thrash.
        WriteMessage::InsertEntriesV2(_) => BatchRole::Neutral,

        // Pure control messages: no DB write to batch either way.
        WriteMessage::SetDeltaPropagation(_) | WriteMessage::ArmLedgerHealLatch => BatchRole::Neutral,
    }
}

/// The writer loop's implicit `BEGIN IMMEDIATE` … `COMMIT` around already-queued work.
pub(super) struct ImplicitBatch {
    /// Whether THIS loop opened the transaction currently on the connection.
    ///
    /// Never read on its own: [`is_open`](Self::is_open) ANDs it with
    /// `conn.is_autocommit()`, because a failed statement can roll the transaction back
    /// underneath us, and a stale `true` would make the next `COMMIT` fail. The flag is
    /// still needed alongside the connection state to tell OUR transaction from an
    /// explicit `BeginTransaction`, which we must never commit or nest inside.
    opened: bool,
    /// When the open batch began (meaningless while closed).
    started: Instant,
    /// Messages absorbed by the open batch, against [`MAX_MESSAGES`].
    messages: usize,
}

impl ImplicitBatch {
    pub(super) fn new() -> Self {
        Self {
            opened: false,
            started: Instant::now(),
            messages: 0,
        }
    }

    /// Whether our implicit batch is open, per the CONNECTION rather than the flag
    /// alone. A rolled-back transaction reads as closed, which is what it is.
    pub(super) fn is_open(&self, conn: &rusqlite::Connection) -> bool {
        self.opened && !conn.is_autocommit()
    }

    /// Open a batch for a mutation, unless a transaction is already open.
    ///
    /// The `is_autocommit` guard is what keeps an implicit batch from nesting inside
    /// the scan path's explicit `BeginTransaction`: SQLite has no nested transactions,
    /// so a `BEGIN` inside a `BEGIN` errors. When one is already open the mutation just
    /// rides it, which is exactly what that transaction is for.
    pub(super) fn begin(&mut self, conn: &rusqlite::Connection) {
        if !conn.is_autocommit() {
            return;
        }
        if let Err(e) = conn.execute_batch("BEGIN IMMEDIATE") {
            log::warn!("Index writer: implicit BEGIN IMMEDIATE failed: {e}");
            return;
        }
        self.opened = true;
        self.started = Instant::now();
        self.messages = 0;
    }

    /// Count a message the open batch absorbed.
    pub(super) fn note_message(&mut self) {
        self.messages += 1;
    }

    /// Whether the open batch should close now. `queue_empty` is the primary and
    /// overwhelmingly common reason: with nothing queued behind us there is nothing
    /// left to coalesce, so holding the transaction would only add latency. The two
    /// caps bound a sustained flood that never lets the queue drain.
    pub(super) fn should_close(&self, queue_empty: bool) -> bool {
        queue_empty || self.messages >= MAX_MESSAGES || self.started.elapsed() >= MAX_DURATION
    }

    /// Commit the open batch (if any) and run a WAL checkpoint that a maintenance tick
    /// parked while it was open. A no-op when no implicit batch is open, so callers can
    /// close unconditionally.
    pub(super) fn close(
        &mut self,
        conn: &rusqlite::Connection,
        probe: &mut ProbeStats,
        signal: &IndexFailureSignal,
        deferred_checkpoint: &mut bool,
    ) {
        if !self.opened {
            return;
        }
        if conn.is_autocommit() {
            // An error rolled the batch back underneath us; there is nothing to commit
            // and `COMMIT` here would just report "no transaction is active".
            self.opened = false;
            return;
        }

        let t = Instant::now();
        let result = conn.execute_batch("COMMIT");
        let elapsed = t.elapsed();
        probe.time_in_commit += elapsed;
        probe.transaction_commits += 1;

        // Follow the CONNECTION, not the `COMMIT`'s return value: a COMMIT that failed
        // with the transaction still open must stay ours to retry, or every later write
        // lands in a transaction nothing ever closes.
        self.opened = !conn.is_autocommit();
        if let Err(e) = result {
            log::warn!(
                "Index writer: implicit COMMIT of {} queued messages failed: {e}",
                self.messages
            );
        } else {
            log::trace!(
                "Writer: committed an implicit batch of {} messages ({} ms)",
                self.messages,
                elapsed.as_millis()
            );
        }

        run_deferred_wal_checkpoint(conn, signal, deferred_checkpoint);
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
#[cfg(test)]
mod throughput_probe;
