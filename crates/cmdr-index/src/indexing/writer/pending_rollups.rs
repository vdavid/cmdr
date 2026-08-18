//! The routine ancestor roll-up queue: one repair per BURST of subtree
//! aggregates, not one per subtree.
//!
//! A finished frontier-root walk ends in `ComputeSubtreeAggregates`, whose
//! handler owes the subtree root's ancestors a recompute. Doing that inline made
//! covering a wide directory quadratic: a stopped walk turns every unwalked
//! child of a `W`-child directory into a frontier root of its own, and each of
//! the `W` roots then recomputed that one parent from all `W` of its children
//! (`docs/notes/wide-dir-scaling-2026-08-18.md`).
//!
//! So the handler queues the ancestor here instead, and `writer_loop` drains the
//! queue at its caught-up point. The queue is a SET, so a burst of `W` roots
//! sharing one parent costs one roll-up rather than `W`.
//!
//! **Why deferring is safe.** [`super::repair::repair_dir_stats_upward`]
//! recomputes each level from its committed children: it writes an ABSOLUTE
//! value, never a relative one, so it can't double-count whatever landed while
//! it waited, and running it later can only make it see MORE of the truth. It is
//! idempotent and order-independent for the same reason. Every writer of this
//! DB is this one thread, so nothing can interleave a write between the queue
//! and the drain. See `DETAILS.md` § "The dir_stats ledger" for the full
//! argument, including what reads the stale row in between.
//!
//! **❌ Not [`super::deferred_repair::DeferredRepairs`].** That queue is drift
//! TELEMETRY: it warns on its first entry, caps at 1,024 ids, and gives up on an
//! id after five attempts. All three are wrong here — this is the routine path,
//! so the warning would fire on every first index, and a dropped roll-up is a
//! permanently wrong size rather than a missed retry.
//!
//! **❌ The trigger stays the caught-up point** — never a timer, a size cap, or
//! "every N messages". An empty queue is what makes a recompute-from-children see
//! final rows, and it is also what makes the coalescing self-limiting: the writer
//! only pays for a roll-up when it has nothing else to do, so a burst costs about
//! what the work that filled it cost.
//!
//! Writer-thread only, like the queue next door: created in `writer_loop`,
//! threaded through the handlers, drained on the same thread, so interior
//! mutability needs no locking.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use cmdr_fs::pluralize::pluralize;

use crate::indexing::events::{EventSink, emit_dir_updated};

use super::deferred_repair::DeferredRepairs;
use super::repair::repair_dir_stats_upward;

/// Ancestors a finished subtree aggregate owes a roll-up, waiting for the
/// writer's caught-up point.
///
/// Deduped and UNBOUNDED: one entry per distinct parent, which is what makes the
/// wide-directory burst collapse to a single walk, and 8 bytes an id even when a
/// whole phase's parents pile up. ❌ Don't cap it — an id dropped here is a
/// directory whose size stays wrong until a full aggregate rebuilds the volume.
pub(super) struct PendingRollups {
    pending: RefCell<BTreeSet<i64>>,
    /// How many ancestor walks this writer has run out of the queue. One per
    /// drained id, so `walks / roots` IS the coalescing factor a scaling guard
    /// reads. Shared with `IndexWriter` because the guard runs on another thread.
    walks: Arc<AtomicU64>,
}

impl PendingRollups {
    pub(super) fn new(walks: Arc<AtomicU64>) -> Self {
        Self {
            pending: RefCell::new(BTreeSet::new()),
            walks,
        }
    }

    /// Remember that `id`'s chain owes a roll-up. Cheap and silent: this fires
    /// once per frontier root a first index covers.
    pub(super) fn queue(&self, id: i64) {
        self.pending.borrow_mut().insert(id);
    }

    /// Roll every queued ancestor up, and report whether any row actually moved
    /// (the caller turns that into one refresh for the panes).
    ///
    /// Takes the set first, so a repair that hands its id to `repairs` on a
    /// transient DB error can't be re-entered here. Order doesn't matter for
    /// correctness — each walk recomputes from committed children and stops
    /// where a level already agrees — so overlapping chains cost a short-circuit
    /// each, not a rewrite.
    pub(super) fn drain(&self, conn: &rusqlite::Connection, repairs: &DeferredRepairs) -> bool {
        let ids = std::mem::take(&mut *self.pending.borrow_mut());
        if ids.is_empty() {
            return false;
        }
        let t = Instant::now();
        let queued = ids.len();
        let mut changed = false;
        for id in ids {
            changed |= repair_dir_stats_upward(conn, id, repairs);
        }
        self.walks.fetch_add(queued as u64, Ordering::Relaxed);
        log::debug!(
            target: "indexing::writer",
            "Ancestor roll-up: {} rolled up in {}ms{}",
            pluralize(queued as u64, "ancestor"),
            t.elapsed().as_millis(),
            if changed { "" } else { " (nothing moved)" },
        );
        changed
    }
}

/// Settle the `dir_stats` ledger: roll up the ancestors a burst of subtree
/// aggregates left owing, then repair whatever chains a failed propagation queued.
///
/// Both want the writer's caught-up point (`DETAILS.md` § "The caught-up point")
/// and both are idempotent, so the exit paths call this again on the way out.
/// `is_autocommit()` keeps them out of an open `BeginTransaction` batch, where the
/// tree is only half written: rolling ancestors up from a partial state and then
/// dropping the id would bake that half-state in.
///
/// Returns whether any row moved, so a caller can refresh the panes exactly when
/// there is something to see.
pub(super) fn settle_the_ledger(
    conn: &rusqlite::Connection,
    rollups: &PendingRollups,
    repairs: &DeferredRepairs,
) -> bool {
    if !conn.is_autocommit() {
        return false;
    }
    let changed = rollups.drain(conn, repairs);
    if !repairs.is_empty() {
        repairs.drain(conn);
    }
    changed
}

/// Tell both panes to re-read their sizes, from BETWEEN messages rather than from
/// inside a handler. The pool is what `process_message` gets for free: on macOS an
/// `emit` can autorelease ObjC objects on this background thread, and out here
/// there is no pool to catch them.
pub(super) fn emit_full_refresh(events: &dyn EventSink) {
    #[cfg(target_os = "macos")]
    objc2::rc::autoreleasepool(|_| emit_dir_updated(events, vec!["/".to_string()]));
    #[cfg(not(target_os = "macos"))]
    emit_dir_updated(events, vec!["/".to_string()]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the set: a burst of roots sharing one parent owes ONE
    /// walk, not one per root.
    #[test]
    fn a_burst_sharing_one_parent_queues_one_id() {
        let rollups = PendingRollups::new(Arc::new(AtomicU64::new(0)));
        for _ in 0..10_000 {
            rollups.queue(42);
        }
        assert_eq!(rollups.pending.borrow().len(), 1, "one parent, one queued roll-up");
    }

    /// Distinct parents each keep their own entry: coalescing must never lose a
    /// chain that nothing else is going to repair.
    #[test]
    fn distinct_parents_all_survive() {
        let rollups = PendingRollups::new(Arc::new(AtomicU64::new(0)));
        for id in 1..=50 {
            rollups.queue(id);
        }
        assert_eq!(rollups.pending.borrow().len(), 50);
    }
}
