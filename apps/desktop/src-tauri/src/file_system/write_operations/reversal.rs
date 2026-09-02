//! What a cancelled operation's reversal is allowed to touch, and how it says
//! what it left alone.
//!
//! Reversing a transfer deletes files and renames items back. Both acts are
//! destructive and both land at a path the operation may have written hours
//! earlier, so each one is decided by rechecking the ledger's snapshot
//! ([`super::ledger`]) against what's at the path RIGHT NOW. Still the entry
//! this operation wrote ⇒ act. Changed, or unprovable ⇒ leave it, and say why.
//!
//! **Recheck immediately before acting.** ❌ Never verify a batch and then act
//! on it: a verification that sat while other items were processed no longer
//! authorizes anything. `operation_log/DETAILS.md` explains why the same rule
//! shapes the history engine's loop.
//!
//! The vocabulary is deliberately the history engine's: [`verify_snapshot`]
//! decides what "this file changed" means for both, and the verdicts are
//! [`SkipReason`]s, so a user meets one set of answers whether they cancelled a
//! transfer or pressed Roll back in history.

use std::fs;
use std::io;
use std::path::Path;

use crate::operation_log::rollback::{ItemResult, SkipTally, SnapshotVerdict, verify_snapshot};
use crate::operation_log::types::SkipReason;

use super::ledger::{CopyTransaction, WrittenFile, WrittenIdentity};
use super::types::{CancelRollback, CancelRollbackOutcome};

/// What a reversal found at a path its ledger claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Recheck {
    /// Still the entry this operation put here. Go ahead.
    Act,
    /// Nothing is there any more: the end state the reversal wanted already
    /// holds, so this is an idempotent success rather than something to report.
    AlreadyGone,
    /// Leave it, and report this reason.
    Skip(SkipReason),
}

impl Recheck {
    /// Translate a snapshot verdict into the reversal's decision.
    fn of(verdict: SnapshotVerdict) -> Self {
        match PresentRecheck::of(verdict) {
            PresentRecheck::Act => Self::Act,
            PresentRecheck::Skip(reason) => Self::Skip(reason),
        }
    }
}

/// The same decision for an entry the caller has ALREADY found present, so
/// "nothing is there" can't be one of the answers.
///
/// It's a separate type rather than a [`Recheck`] whose `AlreadyGone` never
/// happens: a caller that fetched the entry itself has already answered that
/// question, and an arm it can't reach reads to the next author as a case
/// somebody forgot to think about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresentRecheck {
    /// Still the entry this operation put here. Go ahead.
    Act,
    /// Leave it, and report this reason.
    Skip(SkipReason),
}

impl PresentRecheck {
    fn of(verdict: SnapshotVerdict) -> Self {
        match verdict {
            SnapshotVerdict::Match => Self::Act,
            SnapshotVerdict::Drift => Self::Skip(SkipReason::Drift),
            SnapshotVerdict::Unverifiable => Self::Skip(SkipReason::UnverifiablePrecondition),
        }
    }
}

/// Is what's at `file.path` right now still what this operation put there?
///
/// Read with `symlink_metadata`, matching how the ledger recorded it: a copied
/// symlink that dangles has to be FOUND here, and `metadata` would follow it to
/// its missing target and call the link absent.
pub(crate) fn recheck_local(file: &WrittenFile) -> Recheck {
    // A partial has no complete file to recognize and, by construction, no size
    // either — and nothing but this operation can plausibly own a destination
    // path that never held a complete file. See [`WrittenIdentity::OwnPartial`].
    if file.identity == WrittenIdentity::OwnPartial {
        return Recheck::Act;
    }
    let live = match fs::symlink_metadata(&file.path) {
        Ok(live) => live,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Recheck::AlreadyGone,
        // The stat itself refused (a permission change, a dead mount). Nothing
        // is provable, so fail safe.
        Err(_) => return Recheck::Skip(SkipReason::UnverifiablePrecondition),
    };
    match file.identity {
        // Handled above; repeated here so a new variant can't fall through.
        WrittenIdentity::OwnPartial => Recheck::Act,
        WrittenIdentity::Unverifiable => Recheck::Skip(SkipReason::UnverifiablePrecondition),
        WrittenIdentity::VolumeFile { .. } => {
            // A volume entry in a local ledger is a bookkeeping bug, not a file
            // to delete on a guess.
            log::warn!(
                "reversal: {} was recorded through a volume backend but is being rechecked locally, leaving it",
                file.path.display()
            );
            Recheck::Skip(SkipReason::UnverifiablePrecondition)
        }
        WrittenIdentity::LocalDir { node } => match WrittenIdentity::node_of_stat(&live) {
            // A directory's own size shifts as children come and go, so the node
            // id is the whole check.
            Some(live_node) if live_node == node => Recheck::Act,
            Some(_) => Recheck::Skip(SkipReason::Drift),
            None => Recheck::Skip(SkipReason::UnverifiablePrecondition),
        },
        WrittenIdentity::LocalFile { size, node } => match WrittenIdentity::node_of_stat(&live) {
            Some(live_node) if live_node != node => Recheck::Skip(SkipReason::Drift),
            Some(_) => Recheck::of(verify_snapshot(Some(size as i64), None, Some(live.len()), None)),
            None => Recheck::Skip(SkipReason::UnverifiablePrecondition),
        },
    }
}

/// Is what the backend reports at a volume path still what this operation wrote?
///
/// `live_size` comes off the entry the caller already fetched, which is why the
/// answer is a [`PresentRecheck`]: a path the backend says is gone never reaches
/// here, so nothing pays a second round trip to re-ask.
pub(crate) fn recheck_volume(file: &WrittenFile, live_size: Option<u64>) -> PresentRecheck {
    match file.identity {
        WrittenIdentity::OwnPartial => PresentRecheck::Act,
        WrittenIdentity::VolumeFile { size } => {
            PresentRecheck::of(verify_snapshot(Some(size as i64), None, live_size, None))
        }
        // No volume backend reports a node id, so a local identity can't be
        // rechecked here. Reaching this is a bookkeeping bug; fail safe.
        WrittenIdentity::LocalFile { .. } | WrittenIdentity::LocalDir { .. } => {
            log::warn!(
                "reversal: {} was recorded as a local entry but is being rechecked through a volume, leaving it",
                file.path.display()
            );
            PresentRecheck::Skip(SkipReason::UnverifiablePrecondition)
        }
        WrittenIdentity::Unverifiable => PresentRecheck::Skip(SkipReason::UnverifiablePrecondition),
    }
}

/// Remove one local file this operation wrote, if it's still that file.
pub(crate) fn remove_local_file(file: &WrittenFile) -> ItemResult {
    match recheck_local(file) {
        Recheck::AlreadyGone => ItemResult::Skipped(SkipReason::AlreadyGone),
        Recheck::Skip(reason) => ItemResult::Skipped(reason),
        Recheck::Act => match fs::remove_file(&file.path) {
            Ok(()) => ItemResult::Reversed,
            Err(e) if e.kind() == io::ErrorKind::NotFound => ItemResult::Skipped(SkipReason::AlreadyGone),
            Err(e) => {
                log::warn!("reversal: couldn't remove {}: {e}", file.path.display());
                ItemResult::Skipped(SkipReason::Failed)
            }
        },
    }
}

/// Reverse a local copy's whole ledger: every file it still claims, newest
/// first, then the directories it created deepest-first and empty-only.
///
/// The error-cleanup path's reversal. It rechecks like the Rollback button does
/// — deleting a file somebody else has modified is wrong whatever brought Cmdr
/// here — while everything this operation actually wrote still goes, so the
/// half-copied tree the cleanup exists to remove is removed. ❌ Not the panic
/// net: `CopyTransaction`'s `Drop` runs its own unconditional sweep, for the
/// reason written there.
///
/// The caller must still commit the transaction; this has already removed
/// whatever it removed.
pub(crate) fn reverse_copy_transaction(transaction: &mut CopyTransaction) -> ReversalTally {
    let mut tally = ReversalTally::default();
    while let Some(file) = transaction.pop_file() {
        tally.record(remove_local_file(&file), &file.path);
    }
    for dir in transaction.created_dirs.iter().rev() {
        tally.record(remove_local_dir_if_empty(dir), dir);
    }
    tally
}

/// Remove a directory this operation created, but only once it's empty.
///
/// **Establishes the emptiness itself** rather than reading it off `remove_dir`'s
/// refusal, so a non-empty directory is reported as the honest `DirNotEmpty`
/// instead of a generic failure. A directory holding something the operation
/// didn't write (a file the user dropped in, a neighbour a drifted sibling kept
/// alive) stays, which is the whole point.
pub(crate) fn remove_local_dir_if_empty(dir: &Path) -> ItemResult {
    let mut entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return ItemResult::Skipped(SkipReason::AlreadyGone),
        Err(_) => return ItemResult::Skipped(SkipReason::Failed),
    };
    if entries.next().is_some() {
        return ItemResult::Skipped(SkipReason::DirNotEmpty);
    }
    match fs::remove_dir(dir) {
        Ok(()) => ItemResult::Reversed,
        Err(e) if e.kind() == io::ErrorKind::NotFound => ItemResult::Skipped(SkipReason::AlreadyGone),
        Err(_) => ItemResult::Skipped(SkipReason::Failed),
    }
}

/// Where the two draining counters stand after `processed` of `total` ledger
/// entries. Both are interpolated over the ledger rather than decremented, so
/// they reach zero together at the end of the walk however many entries the
/// reversal actually removed — the bar lands on its end whether an entry was
/// removed, left alone, or refused to go.
pub(crate) fn drained(files_at_cancel: usize, bytes_at_cancel: u64, processed: u32, total: usize) -> (usize, u64) {
    if total == 0 || processed as usize >= total {
        return (0, 0);
    }
    let left = 1.0 - f64::from(processed) / total as f64;
    (
        (files_at_cancel as f64 * left) as usize,
        (bytes_at_cancel as f64 * left) as u64,
    )
}

/// What one reversal did, accumulated item by item.
///
/// Mirrors the history engine's accumulator, minus the journal writes: an
/// in-flight reversal has no rows to update, it reports on the
/// `write-cancelled` event instead.
// DEFAULT-OK: a tally nobody has recorded into is a reversal that has processed
// nothing, which is exactly what every zero here says.
#[derive(Debug, Default)]
pub(crate) struct ReversalTally {
    reversed: u32,
    skipped: u32,
    skips: SkipTally,
    canceled: bool,
}

impl ReversalTally {
    /// Count one item. `path` is where the reversal found it, so a report can
    /// name the file rather than only the reason.
    pub(crate) fn record(&mut self, result: ItemResult, path: &Path) {
        match result {
            ItemResult::Reversed => self.reversed += 1,
            // The end state already held, so this counts as undone and is worth
            // nothing to a user reading what got left behind.
            ItemResult::Skipped(reason) if reason.counts_as_reversed() => self.reversed += 1,
            ItemResult::Skipped(reason) => {
                self.skipped += 1;
                self.skips.record(reason, path);
            }
        }
    }

    /// The user stopped the reversal partway, so the ledger still claims files.
    pub(crate) fn mark_canceled(&mut self) {
        self.canceled = true;
    }

    /// How many items this reversal has walked past so far, undone or not — what
    /// the progress bar advances on, so it always reaches its end.
    pub(crate) fn processed(&self) -> u32 {
        self.reversed + self.skipped
    }

    /// Fold into the payload the `write-cancelled` event carries.
    ///
    /// The three outcomes mirror `operation_log::rollback::resolve_final_state`:
    /// a stop before the reversal reached a single item undid nothing; a stop
    /// after it did, or a full pass that skipped something, is partial; a full
    /// pass with no skips is the complete undo (vacuously so for an empty
    /// ledger, which is honest — everything this operation wrote is gone).
    pub(crate) fn into_cancel_rollback(self) -> CancelRollback {
        let outcome = if self.canceled {
            if self.processed() == 0 {
                CancelRollbackOutcome::NotRolledBack
            } else {
                CancelRollbackOutcome::PartiallyRolledBack
            }
        } else if self.skipped == 0 {
            CancelRollbackOutcome::RolledBack
        } else {
            CancelRollbackOutcome::PartiallyRolledBack
        };
        CancelRollback {
            outcome,
            reversed: self.reversed,
            skips: self.skips.into_breakdowns(),
            // The abandoned-write sweep runs OUTSIDE the ledger walk, so its
            // leftovers are attached by the caller that ran it
            // (`CancelRollback::with_staged_leftovers`).
            staged_leftovers: None,
        }
    }
}

#[cfg(test)]
#[path = "reversal_tests.rs"]
mod tests;
