//! The deferred `dir_stats` repair queue: drift the writer knows about but
//! can't fix right now.
//!
//! A `dir_stats` ancestor walk ([`super::delta`], [`super::repair`]) can fail
//! mid-chain on a transient DB error — `SQLITE_BUSY` after the busy handler
//! gives up at attempt 51, a read that couldn't be served. The walk stops there,
//! so that ancestor and everything above it keep a stale balance. Logging a
//! warning and moving on makes that drift permanent and silent, which is exactly
//! what the ledger's "never clamp a bad delta into place" rule exists to
//! prevent.
//!
//! So every failing walk hands the id whose chain needs fixing to this queue,
//! and the writer drains it on a later tick, when the DB is likely writable
//! again. Retrying inline is pointless (the DB is locked *now*) and a re-entrant
//! repair inside a failing walk is worse.
//!
//! Writer-thread only: it's created in `writer_loop`, threaded through the
//! handlers next to the failure signal, and drained on the same thread — so
//! interior mutability needs no locking. See `indexing/DETAILS.md` § "The
//! dir_stats ledger".

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use crate::pluralize::pluralize;

use super::repair::repair_dir_stats_upward;

/// How many distinct ids the queue holds. Ancestor chains overlap heavily, so
/// one stuck episode usually queues a handful of ids, not thousands; this is a
/// memory ceiling for a pathological run, not a working size.
const MAX_PENDING: usize = 1_024;

/// How many drain passes one id gets before the writer gives up on it. A repair
/// that keeps failing is failing for a reason that a 12th attempt won't change
/// (a dead disk trips the failure signal and stops the writer anyway), and an
/// unbounded retry would keep walking the chain on every idle tick forever.
const MAX_ATTEMPTS: u32 = 5;

/// Ids whose `dir_stats` chain is known to have drifted because a propagation
/// read or write failed, waiting for a drain on a later writer tick.
///
/// Deduped (ancestor chains overlap heavily) and bounded. The value is the
/// number of drain passes the id has already survived; see [`MAX_ATTEMPTS`].
pub(super) struct DeferredRepairs {
    pending: RefCell<BTreeMap<i64, u32>>,
    /// `true` while [`drain`](Self::drain) is running, so a re-queue from a
    /// still-failing repair carries the id's attempt count forward instead of
    /// re-announcing the episode.
    draining: Cell<bool>,
    /// Attempt count to stamp on ids queued during the current drain pass.
    current_attempt: Cell<u32>,
    /// Ids dropped because the queue was full, since the last drain. Counted, not
    /// logged per id.
    dropped: Cell<u64>,
}

impl DeferredRepairs {
    pub(super) fn new() -> Self {
        Self {
            pending: RefCell::new(BTreeMap::new()),
            draining: Cell::new(false),
            current_attempt: Cell::new(0),
            dropped: Cell::new(0),
        }
    }

    /// Remember that `id`'s ancestor chain needs repairing. `reason` names the
    /// failing step and rides the one warning this episode emits.
    ///
    /// The empty → non-empty crossing logs at `warn`: this is drift telemetry
    /// and should normally stay silent, so a steadily-firing line means the DB
    /// is genuinely unwritable. Every later id in the same episode is silent.
    pub(super) fn queue(&self, id: i64, reason: &str) {
        let mut pending = self.pending.borrow_mut();
        let was_empty = pending.is_empty();
        if pending.len() >= MAX_PENDING && !pending.contains_key(&id) {
            // Full: keep the ids already queued (each one is proof of drift we
            // still owe a repair) and count the newcomer. Its chain stays drifted
            // until a full aggregate or a backfill heals it.
            self.dropped.set(self.dropped.get() + 1);
            return;
        }
        pending.entry(id).or_insert_with(|| self.current_attempt.get());
        if was_empty && !self.draining.get() {
            log::warn!(
                target: "indexing::writer",
                "dir_stats repair deferred: {reason} failed for id={id}; the chain above it is drifted until the queue drains",
            );
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.pending.borrow().is_empty()
    }

    /// Forget everything queued. Sent by `TruncateData`: the tables are gone, so
    /// the queued ids name rows that no longer exist.
    pub(super) fn clear(&self) {
        self.pending.borrow_mut().clear();
        self.dropped.set(0);
    }

    /// Repair every queued chain. Runs on the writer thread, outside any explicit
    /// transaction (see the drain point in `writer_loop`).
    ///
    /// An id whose repair fails again re-queues itself from inside
    /// [`repair_dir_stats_upward`] with its attempt count bumped, so nothing is
    /// dropped on the floor; an id that exhausts [`MAX_ATTEMPTS`] is given up on
    /// with one warning. One line per drain, never per id.
    pub(super) fn drain(&self, conn: &rusqlite::Connection) {
        let batch = std::mem::take(&mut *self.pending.borrow_mut());
        if batch.is_empty() {
            return;
        }
        let attempted = batch.len();
        let dropped_full = self.dropped.replace(0);

        self.draining.set(true);
        for (id, attempts) in batch {
            self.current_attempt.set(attempts + 1);
            repair_dir_stats_upward(conn, id, self);
        }
        self.current_attempt.set(0);
        self.draining.set(false);

        // Whatever is queued now failed again during this pass.
        let mut pending = self.pending.borrow_mut();
        let exhausted: Vec<i64> = pending
            .iter()
            .filter(|(_, attempts)| **attempts >= MAX_ATTEMPTS)
            .map(|(id, _)| *id)
            .collect();
        for id in &exhausted {
            pending.remove(id);
        }
        let still_pending = pending.len();
        drop(pending);

        log::debug!(
            target: "indexing::writer",
            "dir_stats deferred repair drained: attempted={attempted} still_pending={still_pending} given_up={} dropped_when_full={dropped_full}",
            exhausted.len(),
        );
        if !exhausted.is_empty() || dropped_full > 0 {
            log::warn!(
                target: "indexing::writer",
                "dir_stats repair gave up on {} (attempt limit {MAX_ATTEMPTS}) and dropped {} more with a full queue; those subtrees read stale until the next full aggregate",
                pluralize(exhausted.len() as u64, "id"),
                dropped_full,
            );
        }
    }

    /// The queued ids (test-only observable).
    #[cfg(test)]
    pub(super) fn pending_ids(&self) -> Vec<i64> {
        self.pending.borrow().keys().copied().collect()
    }

    /// Ids dropped because the queue was full, since the last drain (test-only).
    #[cfg(test)]
    pub(super) fn dropped_count(&self) -> u64 {
        self.dropped.get()
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
    use crate::indexing::stress_test_helpers::check_db_consistency;
    use crate::indexing::writer::delta::propagate_delta_by_id;
    use crate::indexing::writer::tests::setup_db;
    use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};
    use crate::test_support::wait_until;

    /// Reject every `dir_stats` write for `entry_id`, with a real SQLite failure
    /// (`RAISE(ABORT)`) rather than a mocked store: the propagation walk sees the
    /// same `Err` shape a `database is locked` gives it, and the trigger is
    /// deterministic — no threads, no timing.
    fn block_dir_stats_writes(conn: &rusqlite::Connection, entry_id: i64) {
        conn.execute_batch(&format!(
            "CREATE TRIGGER block_ds_insert BEFORE INSERT ON dir_stats WHEN NEW.entry_id = {entry_id}
             BEGIN SELECT RAISE(ABORT, 'dir_stats write blocked'); END;
             CREATE TRIGGER block_ds_update BEFORE UPDATE ON dir_stats WHEN NEW.entry_id = {entry_id}
             BEGIN SELECT RAISE(ABORT, 'dir_stats write blocked'); END;",
        ))
        .expect("install blocking triggers");
    }

    fn unblock_dir_stats_writes(conn: &rusqlite::Connection) {
        conn.execute_batch("DROP TRIGGER block_ds_insert; DROP TRIGGER block_ds_update;")
            .expect("drop blocking triggers");
    }

    fn dir_entry(id: i64, parent_id: i64, name: &str) -> EntryRow {
        EntryRow {
            id,
            parent_id,
            name: name.into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }
    }

    fn file_entry(id: i64, parent_id: i64, name: &str, size: u64) -> EntryRow {
        EntryRow {
            id,
            parent_id,
            name: name.into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(size),
            physical_size: Some(size),
            modified_at: None,
            inode: None,
        }
    }

    /// Build ROOT → A(10) → B(20) → f21(700), aggregated correctly, and return a
    /// write connection plus the writer that owns the DB.
    fn seed_chain() -> (IndexWriter, std::path::PathBuf, tempfile::TempDir) {
        let (db_path, dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();
        let entries = vec![
            dir_entry(10, ROOT_ID, "A"),
            dir_entry(20, 10, "B"),
            file_entry(21, 20, "f21", 700),
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer
            .send(WriteMessage::ComputeAllAggregates {
                source: AggSource::Maps,
            })
            .unwrap();
        writer.flush_blocking().unwrap();
        (writer, db_path, dir)
    }

    /// A propagation whose ancestor WRITE fails must leave that id queued for
    /// repair (not just warn), and a later drain must repair the chain from the
    /// committed children rather than leaving it stale.
    #[test]
    fn failed_ancestor_write_is_queued_and_a_later_drain_repairs_the_chain() {
        let (writer, db_path, _dir) = seed_chain();

        // A new 300-byte file lands under B. The entry row is committed; the
        // ancestor walk is what we're testing, so it runs by hand below.
        writer
            .send(WriteMessage::InsertEntriesV2(vec![file_entry(22, 20, "f22", 300)]))
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let repairs = DeferredRepairs::new();

        // A's `dir_stats` write fails; B's succeeds.
        block_dir_stats_writes(&conn, 10);
        propagate_delta_by_id(&conn, 20, 300, 300, 1, 0, &repairs);

        let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(
            (a.recursive_logical_size, a.recursive_file_count),
            (700, 1),
            "A kept its stale balance: the write failed"
        );
        assert_eq!(repairs.pending_ids(), vec![10], "the drifted chain must be queued");

        // The DB is writable again; the drain heals A and everything above it.
        unblock_dir_stats_writes(&conn);
        repairs.drain(&conn);

        assert!(repairs.is_empty(), "a successful drain empties the queue");
        let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(
            (a.recursive_logical_size, a.recursive_file_count),
            (1000, 2),
            "A must be repaired from its committed children"
        );
        check_db_consistency(&conn);

        writer.shutdown();
    }

    /// A failed ancestor READ must never write: collapsing `Err` into "no row"
    /// makes the positive-delta branch materialize a row holding only the delta,
    /// converting a transient read failure into permanently wrong sizes (the
    /// "never clamp a lie into place" rule).
    #[test]
    fn failed_ancestor_read_never_writes_a_partial_row() {
        let (writer, db_path, _dir) = seed_chain();
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Make A's row unreadable as stats: 'x' has no INTEGER affinity conversion,
        // so `get::<_, u64>` fails the way a busy/IO read failure does. The raw
        // value doubles as the tamper-evidence below.
        conn.execute(
            "UPDATE dir_stats SET recursive_logical_size = 'x' WHERE entry_id = ?1",
            rusqlite::params![10],
        )
        .unwrap();

        let repairs = DeferredRepairs::new();
        propagate_delta_by_id(&conn, 20, 300, 300, 1, 0, &repairs);

        // Read it back untyped: pre-fix this came back as an INTEGER holding just
        // the delta, the transient read failure baked into the ledger forever.
        let raw: rusqlite::types::Value = conn
            .query_row(
                "SELECT recursive_logical_size FROM dir_stats WHERE entry_id = ?1",
                rusqlite::params![10],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            raw,
            rusqlite::types::Value::Text("x".into()),
            "a failed read must leave the stored aggregate untouched"
        );
        assert_eq!(repairs.pending_ids(), vec![10], "the unread chain must be queued");

        writer.shutdown();
    }

    /// End to end through the real writer loop: a `PropagateDeltaById` whose
    /// ancestor write fails leaves the chain drifted, and the writer heals it on
    /// its own once the DB accepts writes again — no further propagation needed.
    ///
    /// The blocking triggers live in the schema, so they reject the WRITER's own
    /// connection too, which is what makes this exercise the production path.
    #[test]
    fn the_writer_loop_drains_deferred_repairs_once_the_db_accepts_writes() {
        let (writer, db_path, _dir) = seed_chain();
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        block_dir_stats_writes(&conn, 10);

        writer
            .send(WriteMessage::InsertEntriesV2(vec![file_entry(22, 20, "f22", 300)]))
            .unwrap();
        writer
            .send(WriteMessage::PropagateDeltaById {
                entry_id: 20,
                logical_size_delta: 300,
                physical_size_delta: 300,
                file_count_delta: 1,
                dir_count_delta: 0,
            })
            .unwrap();
        writer.flush_blocking().unwrap();
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, 10)
                .unwrap()
                .unwrap()
                .recursive_logical_size,
            700,
            "A is drifted while its writes are rejected"
        );

        unblock_dir_stats_writes(&conn);
        // Any later message gives the loop its next tick; the drain runs at the
        // end of that iteration, just after the flush reply.
        writer.flush_blocking().unwrap();
        wait_until(
            std::time::Duration::from_secs(2),
            "the writer to repair the drifted chain without being asked",
            || {
                IndexStore::get_dir_stats_by_id(&conn, 10)
                    .unwrap()
                    .map(|s| s.recursive_logical_size)
                    == Some(1000)
            },
        );
        check_db_consistency(&conn);

        writer.shutdown();
    }

    /// Overflow policy: past `MAX_PENDING` distinct ids the queue keeps what it
    /// has (each entry is proof of drift we still owe a repair) and counts the
    /// newcomers instead of growing without limit.
    #[test]
    fn queue_is_bounded_and_counts_what_it_drops() {
        let repairs = DeferredRepairs::new();
        for id in 1..=(MAX_PENDING as i64 + 50) {
            repairs.queue(id, "test");
        }
        assert_eq!(repairs.pending_ids().len(), MAX_PENDING, "the queue is bounded");
        assert_eq!(repairs.dropped_count(), 50, "dropped ids are counted, never silent");

        // Re-queueing an id that's already in the full queue is not a drop.
        repairs.queue(1, "test");
        assert_eq!(repairs.dropped_count(), 50);
    }

    /// A drain that fails again keeps the id queued (never dropped on the floor),
    /// and gives up after `MAX_ATTEMPTS` passes rather than retrying forever.
    #[test]
    fn a_failing_drain_requeues_then_gives_up_after_max_attempts() {
        let (writer, db_path, _dir) = seed_chain();
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let repairs = DeferredRepairs::new();

        // A's writes stay blocked for the whole test, so every drain fails.
        block_dir_stats_writes(&conn, 10);
        // Drift A so the repair actually wants to write (an agreeing row would
        // short-circuit before touching the DB).
        conn.execute(
            "UPDATE dir_stats SET recursive_logical_size = 1 WHERE entry_id = ?1",
            rusqlite::params![20],
        )
        .unwrap();
        repairs.queue(10, "test");

        for attempt in 1..MAX_ATTEMPTS {
            repairs.drain(&conn);
            assert_eq!(
                repairs.pending_ids(),
                vec![10],
                "attempt {attempt}: a failed repair must stay queued"
            );
        }
        repairs.drain(&conn);
        assert!(
            repairs.is_empty(),
            "after MAX_ATTEMPTS the writer gives up instead of retrying forever"
        );

        unblock_dir_stats_writes(&conn);
        writer.shutdown();
    }
}
