//! Periodic DB housekeeping handlers run on the writer thread.
//!
//! Incremental vacuum reclaims free pages from deletes/rescans, and the WAL
//! checkpoint truncates the WAL file once readers permit. Both are fired by a
//! background timer (and the WAL checkpoint also right after a full scan); they
//! mutate no `entries` rows, so they don't bump the writer generation.

use std::cell::Cell;
use std::time::{Duration, Instant};

use crate::indexing::IndexFailureSignal;
use crate::indexing::store::IndexStoreError;
use crate::pluralize::pluralize;

// ── Busy-handler checkpoint suppression ──────────────────────────────

thread_local! {
    /// Set while THIS writer thread is inside [`handle_wal_checkpoint`]'s
    /// `PRAGMA wal_checkpoint(TRUNCATE)`, which deliberately waits readers out
    /// (to ~attempt 51, ~250 ms) before degrading to PASSIVE. The busy handler
    /// reads it to keep that expected wait at debug instead of escalating to warn.
    static IN_WAL_CHECKPOINT: Cell<bool> = const { Cell::new(false) };
}

/// RAII guard marking the writer thread as inside the WAL checkpoint's reader
/// wait. Resets the flag on drop, so a panic mid-checkpoint can't leave it stuck.
struct WalCheckpointGuard;

impl WalCheckpointGuard {
    fn enter() -> Self {
        IN_WAL_CHECKPOINT.with(|f| f.set(true));
        WalCheckpointGuard
    }
}

impl Drop for WalCheckpointGuard {
    fn drop(&mut self) {
        IN_WAL_CHECKPOINT.with(|f| f.set(false));
    }
}

// ── Busy episodes (one log line per lock wait) ───────────────────────

/// Last attempt the busy handler retries; past it, it returns `false` and SQLite
/// gives the write up with `SQLITE_BUSY`. At 5 ms per retry that caps a wait at
/// ~255 ms — short on purpose, since the writer thread stalls every queued write
/// behind it.
pub(super) const BUSY_GIVE_UP_ATTEMPT: i32 = 50;

/// One contention episode: SQLite couldn't take the write lock, so it drove the
/// busy handler in a retry loop until the lock came free (or the handler gave up).
#[derive(Clone, Copy)]
struct BusyEpisode {
    /// When the handler first fired for this locking event.
    started: Instant,
    /// Highest `attempt` the handler saw. Invocations = `peak_attempt + 1`.
    peak_attempt: i32,
    /// Whether any part of the wait happened inside the WAL checkpoint's
    /// deliberate reader wait. Captured here because the flag is already reset
    /// by the time the episode is flushed.
    in_checkpoint: bool,
}

thread_local! {
    /// The episode in progress on THIS writer thread, if any. Per-thread because
    /// each writer owns its own connection and drives its own busy handler.
    static BUSY_EPISODE: Cell<Option<BusyEpisode>> = const { Cell::new(None) };
}

/// Whether the writer's busy handler escalates to warn at `attempt`. Sustained
/// contention (>= 20 attempts, ~100 ms lock wait) is a genuine stall signal —
/// except during the WAL checkpoint's deliberate reader wait, which is working
/// as designed and stays at debug. Pure, so the policy is unit-testable.
pub(super) fn busy_handler_escalates(attempt: i32, in_checkpoint: bool) -> bool {
    attempt >= 20 && !in_checkpoint
}

/// The one line an episode emits: `(warn, message)`. Pure, so the shape and the
/// warn-vs-debug policy are unit-testable without touching the logger.
fn busy_episode_summary(peak_attempt: i32, waited: Duration, in_checkpoint: bool) -> (bool, String) {
    let attempts = pluralize((peak_attempt as u64) + 1, "attempt");
    let ms = waited.as_millis();
    let mut message = if peak_attempt > BUSY_GIVE_UP_ATTEMPT {
        format!("writer gave up waiting for the write lock after {ms} ms over {attempts}")
    } else {
        format!("writer waited {ms} ms over {attempts} for the write lock")
    };
    if in_checkpoint {
        message.push_str(" (WAL checkpoint reader wait)");
    }
    (busy_handler_escalates(peak_attempt, in_checkpoint), message)
}

/// Record one busy-handler invocation. Called from the handler itself, which
/// SQLite drives with `attempt` counting from 0 for each new locking event — so
/// a 0 both opens a fresh episode and proves the previous one is over (the write
/// either got its lock or gave up), which is when the previous one is logged.
pub(super) fn note_busy_attempt(attempt: i32) {
    if attempt == 0 {
        flush_busy_episode();
    }
    BUSY_EPISODE.with(|slot| {
        let in_checkpoint = IN_WAL_CHECKPOINT.with(|f| f.get());
        let episode = match slot.get() {
            Some(mut e) => {
                e.peak_attempt = e.peak_attempt.max(attempt);
                e.in_checkpoint |= in_checkpoint;
                e
            }
            None => BusyEpisode {
                started: Instant::now(),
                peak_attempt: attempt,
                in_checkpoint,
            },
        };
        slot.set(Some(episode));
    });
}

/// Close the open episode, if any, and log its one summary line. Called by the
/// writer loop after every message: the busy handler has no "you got the lock"
/// callback, so the episode is closed from the outside once the work that
/// contended is done. (A back-to-back episode within the same message is closed
/// by [`note_busy_attempt`]'s `attempt == 0` instead.)
pub(super) fn flush_busy_episode() {
    let Some(episode) = BUSY_EPISODE.with(|slot| slot.take()) else {
        return;
    };
    let (warn, message) = busy_episode_summary(episode.peak_attempt, episode.started.elapsed(), episode.in_checkpoint);
    if warn {
        log::warn!(target: "stall_probe::sqlite_busy", "{message}");
    } else {
        log::debug!(target: "stall_probe::sqlite_busy", "{message}");
    }
}

/// Cap thresholds for the tiered incremental-vacuum policy. Below `MIN`,
/// holding the write lock isn't worth the work. Between `MIN` and `BACKLOG`,
/// keep the original steady-state cap so concurrent operations barely notice.
/// Above `BACKLOG`, ramp the cap to drain backlogs (post-truncate, post-replay,
/// or DBs migrated from older versions that accumulated free pages) in tens of
/// minutes instead of hours.
const VACUUM_MIN_FREELIST: i64 = 1_000;
const VACUUM_STEADY_CAP: i64 = 2_000;
const VACUUM_BACKLOG_THRESHOLD: i64 = 20_000;
const VACUUM_BACKLOG_CAP: i64 = 20_000;

/// Pick the per-tick `incremental_vacuum` page cap given the current
/// `freelist_count`. Pure so it can be tested in isolation; the handler
/// just runs the SQL and logs.
///
/// Tiered cap: skip the no-op lock acquisition when the freelist is small;
/// hold the lock only as long as needed to drain real backlog. The 20K cap
/// (~80 MB at 4 KiB pages) is sized so a single tick fsyncs in ~100-300 ms
/// on SSD — long enough to make real progress but short enough that the
/// writer doesn't visibly stall behind it.
fn pick_vacuum_cap(freelist: i64) -> Option<i64> {
    if freelist < VACUUM_MIN_FREELIST {
        None
    } else if freelist < VACUUM_BACKLOG_THRESHOLD {
        Some(VACUUM_STEADY_CAP)
    } else {
        Some(VACUUM_BACKLOG_CAP)
    }
}

pub(super) fn handle_incremental_vacuum(conn: &rusqlite::Connection, signal: &IndexFailureSignal) {
    let free = match conn.pragma_query_value(None, "freelist_count", |row| row.get::<_, i64>(0)) {
        Ok(n) => n,
        Err(e) => {
            signal.note(&IndexStoreError::from(e), "freelist_count query");
            return;
        }
    };

    let Some(cap) = pick_vacuum_cap(free) else {
        return;
    };

    if let Err(e) = crate::sqlite_util::run_incremental_vacuum(conn, Some(cap)) {
        signal.note(&IndexStoreError::from(e), "incremental_vacuum");
    } else {
        log::debug!(
            "Writer: incremental_vacuum reclaimed up to {cap} of {}",
            pluralize(free as u64, "free page")
        );
    }
}

/// Periodically TRUNCATE the WAL file so its high-water mark doesn't sit on
/// disk indefinitely. SQLite's `wal_autocheckpoint` runs in PASSIVE mode and
/// only moves pages from WAL to the main file; it never shrinks the file
/// itself. After a big scan the WAL can balloon to 1+ GB, and without an
/// explicit TRUNCATE that file size persists until the next app restart.
///
/// TRUNCATE blocks waiting for readers, invoking this connection's busy handler
/// (installed in `writer/mod.rs::spawn`) while it waits. That handler — NOT the
/// `busy_timeout = 5000` pragma, which installing a `busy_handler` overrides —
/// caps the wait at ~250 ms (it sleeps 5 ms per retry and gives up at attempt 51),
/// after which the call degrades to PASSIVE semantics (busy code = 1 in the return
/// tuple): pages still get checkpointed, the file just doesn't shrink this time.
/// Next tick tries again. No error path needed. The short cap is deliberate: this
/// runs on the writer thread, so a multi-second block would stall every live write
/// queued behind it.
///
/// A `WalCheckpointGuard` brackets the TRUNCATE, so the busy handler stays at
/// debug for the whole reader wait rather than escalating to warn past attempt 20
/// — a persistent reader here is working-as-designed, not a stall. Every OTHER
/// writer contention still warns past attempt 20.
/// Run the periodic WAL checkpoint, or park it until the open batch transaction
/// commits. Returns whether the checkpoint was parked.
///
/// `PRAGMA wal_checkpoint(TRUNCATE)` cannot run inside a transaction: SQLite
/// refuses it with `SQLITE_LOCKED` ("database table is locked"). A journal replay
/// wraps its entire run in one `BeginTransaction`, so an undeferred maintenance
/// tick fails on every tick for the whole replay — and the WAL grows unchecked
/// exactly when write volume is highest. Parking it means the truncate happens
/// once, right after the commit, which is when it can actually reclaim the space.
pub(super) fn request_wal_checkpoint(conn: &rusqlite::Connection, signal: &IndexFailureSignal, deferred: &mut bool) {
    if conn.is_autocommit() {
        handle_wal_checkpoint(conn, signal);
    } else {
        log::debug!("Writer: wal_checkpoint deferred until the open transaction commits");
        *deferred = true;
    }
}

/// Run a checkpoint parked by [`request_wal_checkpoint`], if any. Called after a
/// commit. Re-checks `is_autocommit`: a COMMIT that itself failed leaves the
/// transaction open, and the checkpoint stays parked for the next chance rather
/// than reporting the same non-error again.
pub(super) fn run_deferred_wal_checkpoint(
    conn: &rusqlite::Connection,
    signal: &IndexFailureSignal,
    deferred: &mut bool,
) {
    if *deferred && conn.is_autocommit() {
        *deferred = false;
        handle_wal_checkpoint(conn, signal);
    }
}

fn handle_wal_checkpoint(conn: &rusqlite::Connection, signal: &IndexFailureSignal) {
    let _guard = WalCheckpointGuard::enter();
    // `PRAGMA wal_checkpoint(TRUNCATE)` returns a single row with three
    // columns: (busy, log_size, checkpointed). `busy = 0` means everything
    // got checkpointed AND the file was truncated; `busy = 1` means at least
    // one reader was still on the WAL so the file couldn't shrink (pages
    // were still copied to the main file). Either is a success from the
    // caller's POV — only a SQL error means something is actually wrong.
    let result: rusqlite::Result<(i64, i64, i64)> = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    });
    match result {
        Ok((0, log_size, checkpointed)) => {
            log::debug!(
                "Writer: wal_checkpoint TRUNCATE done ({checkpointed} of {})",
                pluralize(log_size as u64, "page")
            );
        }
        Ok((_, log_size, checkpointed)) => {
            // Busy: readers blocking the truncate. Pages still got written
            // to the main file; the WAL file just didn't shrink this tick.
            log::debug!(
                "Writer: wal_checkpoint partial ({checkpointed} of {}, blocked by readers)",
                pluralize(log_size as u64, "page")
            );
        }
        Err(e) => {
            signal.note(&IndexStoreError::from(e), "wal_checkpoint");
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
    use crate::indexing::writer::tests::setup_db;
    use crate::indexing::writer::{IndexWriter, WriteMessage};

    // ── Busy-episode summary ─────────────────────────────────────────

    /// One line per episode carries what the per-attempt ladder used to spread
    /// over dozens of lines: how long the writer waited and how many retries it
    /// took. Short contention is routine, so it stays at debug.
    #[test]
    fn busy_episode_summary_reports_total_wait_and_attempts() {
        let (warn, msg) = busy_episode_summary(4, Duration::from_millis(25), false);
        assert!(!warn, "a five-attempt wait is routine contention");
        assert_eq!(msg, "writer waited 25 ms over 5 attempts for the write lock");
    }

    /// Sustained contention (the old attempt >= 20 warn threshold) still warns,
    /// now once per episode instead of once per attempt.
    #[test]
    fn busy_episode_summary_warns_on_sustained_contention() {
        let (warn, msg) = busy_episode_summary(26, Duration::from_millis(340), false);
        assert!(warn, "27 attempts is a genuine stall signal");
        assert_eq!(msg, "writer waited 340 ms over 27 attempts for the write lock");
    }

    /// The WAL checkpoint's TRUNCATE deliberately waits readers out to ~attempt
    /// 51, so its episode stays at debug and says so.
    #[test]
    fn busy_episode_summary_stays_quiet_during_the_checkpoint_reader_wait() {
        let (warn, msg) = busy_episode_summary(50, Duration::from_millis(255), true);
        assert!(!warn, "the checkpoint's reader wait is working as designed");
        assert!(
            msg.ends_with("(WAL checkpoint reader wait)"),
            "the checkpoint context belongs in the line, got: {msg}"
        );
    }

    /// Past the retry cap the handler returns false and SQLite gives up, which
    /// the summary must say outright — that's the difference between "we waited"
    /// and "the write didn't get the lock".
    #[test]
    fn busy_episode_summary_says_it_gave_up_past_the_retry_cap() {
        let (warn, msg) = busy_episode_summary(BUSY_GIVE_UP_ATTEMPT + 1, Duration::from_millis(260), false);
        assert!(warn);
        assert_eq!(
            msg,
            "writer gave up waiting for the write lock after 260 ms over 52 attempts"
        );
    }

    // ── DB hygiene tests ─────────────────────────────────────────────

    /// The tier policy is the safety-critical part of the vacuum logic:
    /// regressing it would either thrash the writer lock (cap too aggressive
    /// in steady state) or let the freelist grow unbounded (cap missing on
    /// backlog). Lock the thresholds with explicit cases either side of each
    /// boundary plus the steady-state band's interior.
    #[test]
    fn pick_vacuum_cap_skips_below_min() {
        assert_eq!(pick_vacuum_cap(0), None);
        assert_eq!(pick_vacuum_cap(VACUUM_MIN_FREELIST - 1), None);
    }

    #[test]
    fn pick_vacuum_cap_uses_steady_band_for_modest_backlog() {
        assert_eq!(pick_vacuum_cap(VACUUM_MIN_FREELIST), Some(VACUUM_STEADY_CAP));
        assert_eq!(pick_vacuum_cap(VACUUM_BACKLOG_THRESHOLD - 1), Some(VACUUM_STEADY_CAP));
    }

    #[test]
    fn pick_vacuum_cap_ramps_to_backlog_cap_for_large_backlog() {
        assert_eq!(pick_vacuum_cap(VACUUM_BACKLOG_THRESHOLD), Some(VACUUM_BACKLOG_CAP));
        assert_eq!(pick_vacuum_cap(1_000_000), Some(VACUUM_BACKLOG_CAP));
    }

    /// The capped `incremental_vacuum` handler must reclaim the FULL per-tick
    /// cap, not a single page. `PRAGMA incremental_vacuum(N)` frees one page per
    /// `sqlite3_step()`, so a single `execute_batch` step drains one page
    /// regardless of the cap — which stranded the freelist, draining it one page
    /// per 30 s tick. Build a multi-thousand-page freelist with a subtree delete
    /// (which, unlike `TruncateData`, does not self-vacuum), then assert one
    /// `IncrementalVacuum` reclaims far more than a single page.
    #[test]
    fn handle_incremental_vacuum_reclaims_capped_batch() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // A directory to hang the children off, then a large batch of children.
        // Deleting the subtree frees a few thousand pages onto the freelist with
        // no auto-drain (rows pack ~20/page, so it takes tens of thousands to
        // clear the vacuum's 1 000-page MIN and reach the 2 000-page steady cap).
        let dir_id = 100;
        let mut entries: Vec<EntryRow> = vec![EntryRow {
            id: dir_id,
            parent_id: ROOT_ID,
            name: "subtree".to_string(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }];
        entries.extend((0..60_000).map(|i| EntryRow {
            id: 101 + i,
            parent_id: dir_id,
            name: format!("test-entry-with-a-reasonably-long-name-{i:08}"),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(4096),
            physical_size: Some(4096),
            modified_at: None,
            inode: None,
        }));
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::DeleteSubtreeById(dir_id)).unwrap();
        writer.flush_blocking().unwrap();

        let probe = IndexStore::open_read_connection(&db_path).unwrap();
        let free_before: i64 = probe
            .pragma_query_value(None, "freelist_count", |row| row.get(0))
            .unwrap();
        drop(probe);
        // The freelist must comfortably exceed a single page for the reclaim
        // assertion below to be unambiguous.
        assert!(
            free_before > 1_000,
            "test setup: expected a multi-thousand-page freelist, got {free_before}"
        );

        writer.send(WriteMessage::IncrementalVacuum).unwrap();
        writer.flush_blocking().unwrap();

        let probe = IndexStore::open_read_connection(&db_path).unwrap();
        let free_after: i64 = probe
            .pragma_query_value(None, "freelist_count", |row| row.get(0))
            .unwrap();

        let reclaimed = free_before - free_after;
        assert!(
            reclaimed >= 1_000,
            "one IncrementalVacuum must reclaim the per-tick cap, not a single page; \
             before={free_before}, after={free_after}, reclaimed={reclaimed}"
        );

        writer.shutdown();
    }

    /// After a full `TruncateData`, the uncapped post-truncate vacuum must drain
    /// the whole freelist (not one page), so the file returns its space instead
    /// of stranding thousands of dead pages.
    #[test]
    fn truncate_drains_freelist() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Long names so each row touches its own page; 5000 rows ≥ several
        // thousand pages freed by the truncate's DELETE.
        let entries: Vec<EntryRow> = (0..5000)
            .map(|i| EntryRow {
                id: 100 + i,
                parent_id: ROOT_ID,
                name: format!("test-entry-with-a-reasonably-long-name-{i:08}"),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(4096),
                physical_size: Some(4096),
                modified_at: None,
                inode: None,
            })
            .collect();
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::TruncateData).unwrap();
        writer.flush_blocking().unwrap();

        // No reader pinned a snapshot during the truncate, so the uncapped drain
        // reclaims every freed page. A residual near zero is expected; thousands
        // would mean the vacuum only stepped once.
        let probe = IndexStore::open_read_connection(&db_path).unwrap();
        let free_after: i64 = probe
            .pragma_query_value(None, "freelist_count", |row| row.get(0))
            .unwrap();
        assert!(
            free_after < 100,
            "TruncateData's uncapped vacuum must drain the freelist; residual={free_after}"
        );

        writer.shutdown();
    }

    /// A maintenance tick that lands mid-batch (the replay wraps its whole run in
    /// one `BeginTransaction`) must not call the TRUNCATE at all: SQLite refuses a
    /// checkpoint inside an open transaction with `SQLITE_LOCKED`, so every tick
    /// would report a storage error that isn't one. The checkpoint is deferred to
    /// the commit instead, so the WAL still gets truncated — which is the point of
    /// the tick.
    #[test]
    fn wal_checkpoint_defers_out_of_an_open_transaction() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        writer.send(WriteMessage::BeginTransaction).unwrap();
        let entries: Vec<EntryRow> = (0..2000)
            .map(|i| EntryRow {
                id: 300 + i,
                parent_id: ROOT_ID,
                name: format!("deferred-checkpoint-entry-{i:08}"),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1024),
                physical_size: Some(1024),
                modified_at: None,
                inode: None,
            })
            .collect();
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();

        // The maintenance tick, arriving while the batch transaction is open.
        writer.send(WriteMessage::WalCheckpoint).unwrap();
        writer.flush_blocking().unwrap();

        assert_eq!(
            writer.failure_signal().note_count(),
            0,
            "a checkpoint skipped because a transaction is open is not a storage error"
        );

        let wal_path = format!("{}-wal", db_path.display());
        let wal_size_before = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
        assert!(wal_size_before > 0, "test setup: expected a grown WAL, got {wal_size_before}"); // allowed-pluralize-noun: assertion-failure-only message guarded by `> 0`

        writer.send(WriteMessage::CommitTransaction).unwrap();
        writer.flush_blocking().unwrap();

        let wal_size_after = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
        assert!(
            wal_size_after < wal_size_before,
            "the deferred checkpoint must run once the transaction commits; \
             before={wal_size_before}, after={wal_size_after}"
        );

        writer.shutdown();
    }

    /// End-to-end check: after inserts have grown the WAL, `WalCheckpoint`
    /// shrinks the on-disk WAL file. The WAL file is `db_path` + "-wal";
    /// after a successful TRUNCATE checkpoint with no readers, it should
    /// drop to zero bytes (or a small header).
    #[test]
    fn handle_wal_checkpoint_truncates_wal_file() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Grow the WAL with a non-trivial insert batch.
        let entries: Vec<EntryRow> = (0..2000)
            .map(|i| EntryRow {
                id: 200 + i,
                parent_id: ROOT_ID,
                name: format!("wal-test-entry-{i:08}"),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1024),
                physical_size: Some(1024),
                modified_at: None,
                inode: None,
            })
            .collect();
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        let wal_path = format!("{}-wal", db_path.display());
        let wal_size_before = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
        assert!(
            wal_size_before > 0,
            "expected WAL file to have grown after 2000 inserts; got {} bytes",
            wal_size_before // allowed-pluralize-noun: assertion-failure-only message; the assertion is `> 0`, so when it fires `wal_size_before == 0` and "0 bytes" reads correctly
        );

        writer.send(WriteMessage::WalCheckpoint).unwrap();
        writer.flush_blocking().unwrap();

        let wal_size_after = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
        assert!(
            wal_size_after < wal_size_before,
            "WalCheckpoint should shrink the WAL file; before={wal_size_before}, after={wal_size_after}"
        );

        writer.shutdown();
    }
}
