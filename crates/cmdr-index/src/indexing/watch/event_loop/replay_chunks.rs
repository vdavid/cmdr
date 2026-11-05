//! How a cold-start replay commits: in bounded chunks, so the WAL can be
//! checkpointed while the replay runs and a crash resumes from the last chunk.
//!
//! One transaction for a whole replay kept every write in the WAL until the end:
//! no checkpoint can move a frame an open transaction still owns, so the WAL grew
//! with the replay. On a nearly full disk that grew it until SQLite hit
//! `SQLITE_FULL` (cmdr-reports#100: a 1.1 M-event replay, eight minutes of parked
//! checkpoints, about 1.8 GB of free space gone). A commit every
//! [`ReplayChunks::commit_every_paths`] bounds it, because SQLite's own
//! autocheckpoint runs at each commit and the 30 s `TRUNCATE` parked behind a
//! chunk runs at the next one.
//!
//! What makes a chunk boundary SAFE to crash at (`watch/DETAILS.md` § "A replay
//! commits in chunks"):
//!
//! - **The journal position only moves when a dedup window has drained whole.**
//!   Inside a window, events are merged by path and processed in no particular
//!   order, so a commit mid-window carries rows but keeps the previous window's
//!   `last_event_id`. A crash there replays the window again, which is harmless:
//!   processing re-stats every path, so a path processed twice lands at the same
//!   row, updated in place.
//! - **The deferred rescans ride the same commit as the position.** A
//!   `MustScanSubDirs` the replay has seen runs only after the replay, so a
//!   position past its event with the rescan only in memory would lose it for
//!   good. Each window's commit writes the set (`REPLAY_DEFERRED_RESCANS_KEY`) in
//!   the transaction that moves `last_event_id`, and the next replay loads it.

use std::collections::HashSet;

use rusqlite::Connection;

use crate::indexing::IndexPathSpace;
use crate::indexing::store::{IndexStore, REPLAY_DEFERRED_RESCANS_KEY};
use crate::indexing::writer::{IndexWriter, WriteMessage, WriterTransaction};

/// Distinct paths a replay processes between two commits. Each processed path is
/// one upsert or delete plus its `dir_stats` ancestor chain, so a chunk this size
/// keeps the WAL in the tens of megabytes, next to `journal_size_limit` (64 MiB)
/// and the 16 MiB autocheckpoint, rather than growing with the replay.
pub(crate) const REPLAY_COMMIT_EVERY_PATHS: usize = 10_000;

/// The two sizes a replay chunks by. Production uses [`ReplayChunks::SHIPPING`];
/// tests shrink both to put a boundary between a handful of events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReplayChunks {
    /// Distinct paths the dedup window holds before it drains (and the journal
    /// position moves).
    pub(crate) window_paths: usize,
    /// Distinct paths processed between two commits, inside or across windows.
    pub(crate) commit_every_paths: usize,
}

impl ReplayChunks {
    /// What a real replay runs with.
    pub(crate) const SHIPPING: Self = Self {
        window_paths: super::replay_cost::REPLAY_DEDUP_WINDOW_PATHS,
        commit_every_paths: REPLAY_COMMIT_EVERY_PATHS,
    };
}

/// The replay's explicit transaction, committed in chunks.
///
/// Holds the [`WriterTransaction`] guard, so every exit path still commits (the
/// guard commits on drop, an early return or an abort included).
pub(super) struct ReplayCommits {
    transaction: Option<WriterTransaction>,
    commit_every: usize,
    uncommitted: usize,
    /// How many deferred rescans the last window's commit wrote down. The set only
    /// grows during the replay, so an unchanged length means an unchanged set and
    /// the window skips rewriting it.
    persisted_rescans: usize,
}

impl ReplayCommits {
    /// Open the replay's first transaction. `rescans_on_disk` is how many deferred
    /// rescans the store already holds (the ones [`load_deferred_rescans`] found).
    pub(super) fn open(writer: &IndexWriter, chunks: ReplayChunks, rescans_on_disk: usize) -> Self {
        let transaction = match writer.begin_transaction() {
            Ok(transaction) => Some(transaction),
            Err(e) => {
                log::warn!("Replay: BeginTransaction send failed: {e}");
                None
            }
        };
        Self {
            transaction,
            commit_every: chunks.commit_every_paths.max(1),
            uncommitted: 0,
            persisted_rescans: rescans_on_disk,
        }
    }

    /// One path processed. Commits once a chunk's worth have been, keeping the
    /// journal position where it is: a window's ids say nothing about which of its
    /// paths are done.
    pub(super) fn note_processed(&mut self) {
        self.uncommitted += 1;
        if self.uncommitted >= self.commit_every {
            self.commit_and_continue();
        }
    }

    /// A dedup window drained whole: write down how far the journal got and the
    /// rescans it owes, and commit them together.
    pub(super) fn close_window(
        &mut self,
        writer: &IndexWriter,
        space: &IndexPathSpace,
        position: JournalPosition,
        pending_rescans: &HashSet<String>,
    ) {
        if pending_rescans.len() != self.persisted_rescans {
            let value = encode_deferred_rescans(space, pending_rescans);
            if let Err(e) = writer.send(WriteMessage::UpdateMeta {
                key: REPLAY_DEFERRED_RESCANS_KEY.to_string(),
                value,
            }) {
                log::warn!("Replay: writing the deferred rescans failed to send: {e}");
            } else {
                self.persisted_rescans = pending_rescans.len();
            }
        }
        position.send(writer);
        self.commit_and_continue();
    }

    /// The replay reached `HistoryDone`: the final position goes in, the deferred
    /// rescans come off the disk (they're the live drain's from here), and the last
    /// chunk commits.
    pub(super) fn finish(mut self, writer: &IndexWriter, position: JournalPosition) {
        position.send(writer);
        self.forget_deferred_rescans(writer);
        self.commit();
    }

    /// The replay gave up for a walk that reads the whole volume (a purged journal,
    /// the cost budget, the RAM guard): the rescans it deferred are covered by that
    /// walk, so they come off the disk with the last chunk.
    pub(super) fn abandon(mut self, writer: &IndexWriter) {
        self.forget_deferred_rescans(writer);
        self.commit();
    }

    /// The replay stopped mid-way (its stream closed before `HistoryDone`): commit
    /// what landed, and leave the stored position and the deferred rescans where the
    /// last drained window put them.
    pub(super) fn interrupt(self) {
        self.commit();
    }

    fn forget_deferred_rescans(&mut self, writer: &IndexWriter) {
        if let Err(e) = writer.send(WriteMessage::DeleteMeta(REPLAY_DEFERRED_RESCANS_KEY.to_string())) {
            log::warn!("Replay: clearing the deferred rescans failed to send: {e}");
        }
        self.persisted_rescans = 0;
    }

    fn commit_and_continue(&mut self) {
        self.uncommitted = 0;
        if let Some(transaction) = self.transaction.as_mut()
            && let Err(e) = transaction.commit_and_continue()
        {
            log::warn!("Replay: chunk commit failed to send: {e}");
        }
    }

    fn commit(mut self) {
        if let Some(transaction) = self.transaction.take()
            && let Err(e) = transaction.commit()
        {
            log::warn!("Replay: CommitTransaction send failed: {e}");
        }
    }
}

/// How far the replay got in the journal, against where it started.
#[derive(Debug, Clone, Copy)]
pub(super) struct JournalPosition {
    /// The id of the last event the replay has fully processed.
    pub(super) last_event_id: u64,
    /// The stored id the replay started from.
    pub(super) since_event_id: u64,
}

impl JournalPosition {
    /// Send the position, unless the replay hasn't moved past where it started (a
    /// stored id never goes backwards).
    fn send(self, writer: &IndexWriter) {
        if self.last_event_id > self.since_event_id
            && let Err(e) = writer.send(WriteMessage::UpdateLastEventId(self.last_event_id))
        {
            log::warn!("Replay: UpdateLastEventId send failed: {e}");
        }
    }
}

/// The deferred rescans an earlier replay wrote down and never handed off, as the
/// absolute paths the replay keeps. An unreadable or malformed value reads as none:
/// losing a rescan hint costs a stale subtree until the next walk, and refusing the
/// replay over it would cost the whole journal.
pub(super) fn load_deferred_rescans(conn: &Connection, space: &IndexPathSpace) -> HashSet<String> {
    let stored = match IndexStore::get_meta(conn, REPLAY_DEFERRED_RESCANS_KEY) {
        Ok(Some(stored)) => stored,
        Ok(None) => return HashSet::new(),
        Err(e) => {
            log::warn!("Replay: reading the deferred rescans failed: {e}");
            return HashSet::new();
        }
    };
    match serde_json::from_str::<Vec<String>>(&stored) {
        Ok(relative) => relative.iter().map(|path| space.absolute_of(path)).collect(),
        Err(e) => {
            log::warn!("Replay: the stored deferred rescans don't parse ({e}); dropping them");
            HashSet::new()
        }
    }
}

/// The set as the store keeps it: index-relative, so a volume that comes back at a
/// different mount point still finds its rescans, and sorted, so the same set
/// always writes the same value.
fn encode_deferred_rescans(space: &IndexPathSpace, pending_rescans: &HashSet<String>) -> String {
    let mut relative: Vec<String> = pending_rescans
        .iter()
        .filter_map(|absolute| space.index_relative(absolute))
        .collect();
    relative.sort();
    // A `Vec<String>` always serializes.
    serde_json::to_string(&relative).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deferred_rescans_round_trip_through_their_stored_form() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("chunks.db");
        let store = IndexStore::open(&db_path).expect("open store");
        let space = IndexPathSpace::mount_rooted("/Volumes/Backup");
        let pending: HashSet<String> = ["/Volumes/Backup/a/b".to_string(), "/Volumes/Backup/c".to_string()].into();

        let conn = IndexStore::open_write_connection(&db_path).expect("write connection");
        IndexStore::update_meta(
            &conn,
            REPLAY_DEFERRED_RESCANS_KEY,
            &encode_deferred_rescans(&space, &pending),
        )
        .expect("store");
        drop(store);

        let remounted = IndexPathSpace::mount_rooted("/Volumes/Backup 1");
        let reader = IndexStore::open_read_connection(&db_path).expect("read connection");
        assert_eq!(
            load_deferred_rescans(&reader, &remounted),
            ["/Volumes/Backup 1/a/b".to_string(), "/Volumes/Backup 1/c".to_string()].into(),
            "stored index-relative, so a remounted volume finds them under its new root"
        );
    }

    #[test]
    fn a_malformed_stored_value_reads_as_no_rescans() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("chunks.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let conn = IndexStore::open_write_connection(&db_path).expect("write connection");
        IndexStore::update_meta(&conn, REPLAY_DEFERRED_RESCANS_KEY, "not json").expect("store");
        assert!(load_deferred_rescans(&conn, &IndexPathSpace::root()).is_empty());
    }
}
