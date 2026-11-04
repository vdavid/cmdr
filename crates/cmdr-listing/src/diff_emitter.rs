//! Coalesces transport, never coordinate spaces or committed revisions.
//! Producers stamp and enqueue each transition under the listing-cache write lock.
//! Lock order is cache then queue; draining the queue never reads the cache.
//!
//! That order is a type, not a rule: the queue's only writer is [`Publish`],
//! which [`CacheWrite`] hands out borrowed from the cache's write guard. So a
//! batch can't be enqueued, and a listing's queue can't be dropped, by anyone
//! not holding the cache write lock at that moment.

use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock, RwLockWriteGuard};
use std::time::Duration;

use crate::rt::Runtime;

use crate::DirectoryDiffBatch;
use crate::host::ListingEvents;
use crate::listing_id::ListingId;
use cmdr_fs::ignore_poison::{IgnorePoison, RwLockIgnorePoison};

const FLUSH_WINDOW_MS: u64 = 50;

// DEFAULT-OK: an empty queue has no flush scheduled.
#[derive(Default)]
struct PendingDiff {
    batches: Vec<DirectoryDiffBatch>,
    flush_scheduled: bool,
}

/// One engine's committed diff batches, per listing, waiting for their flush.
pub(crate) struct DiffQueue {
    pending: Mutex<HashMap<String, PendingDiff>>,
    /// Where a flush reports.
    events: Arc<dyn ListingEvents>,
    /// Where the flush timer runs.
    runtime: Runtime,
}

impl DiffQueue {
    pub(crate) fn new(events: Arc<dyn ListingEvents>, runtime: Runtime) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            events,
            runtime,
        }
    }

    /// Takes an already stamped transition while its producer holds the cache
    /// write lock: private, so only [`Publish`] reaches it.
    fn enqueue(self: &Arc<Self>, listing_id: &str, batch: DirectoryDiffBatch) {
        // Projection changes can advance a revision with no rows affected (a scratch
        // dotfile while hidden files are off). Clients still need that revision link.
        let needs_schedule = {
            let mut pending = self.pending.lock_ignore_poison();
            let entry = pending.entry(listing_id.to_string()).or_default();
            entry.batches.push(batch);
            if entry.flush_scheduled {
                false
            } else {
                entry.flush_scheduled = true;
                true
            }
        };
        if needs_schedule {
            let lid = listing_id.to_string();
            // A strong handle: the task ends after one flush window.
            let queue = Arc::clone(self);
            self.runtime.spawn(async move {
                crate::rt::time::sleep(Duration::from_millis(FLUSH_WINDOW_MS)).await;
                queue.flush(&lid);
            });
        }
    }

    /// Under the cache write lock, when replacing a row space or ending a
    /// listing: private, so only [`Publish`] and [`CacheWrite::remove`] reach it.
    fn drop_pending(&self, listing_id: &str) {
        self.pending.lock_ignore_poison().remove(listing_id);
    }

    pub(crate) fn take_pending(&self, listing_id: &str) -> Vec<DirectoryDiffBatch> {
        let mut pending = self.pending.lock_ignore_poison();
        let Some(entry) = pending.get_mut(listing_id) else {
            return Vec::new();
        };
        entry.flush_scheduled = false;
        std::mem::take(&mut entry.batches)
    }

    pub(crate) fn flush(&self, listing_id: &str) {
        let batches = self.take_pending(listing_id);
        if batches.is_empty() {
            return;
        }
        self.events.diff(&ListingId::new(listing_id), batches);
    }

    #[cfg(feature = "playwright-e2e")]
    pub(crate) fn flush_all_pending(&self) {
        let ids: Vec<String> = self.pending.lock_ignore_poison().keys().cloned().collect();
        for id in ids {
            self.flush(&id);
        }
    }

    /// Marks `listing_id`'s flush as already scheduled, so a test reads its
    /// queue without racing the flush timer.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn hold(&self, listing_id: &str) {
        self.pending
            .lock_ignore_poison()
            .entry(listing_id.to_string())
            .or_default()
            .flush_scheduled = true;
    }

    /// Every batch `listing_id` has waiting, oldest first.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn pending_batches(&self, listing_id: &str) -> Vec<DirectoryDiffBatch> {
        self.pending
            .lock_ignore_poison()
            .get(listing_id)
            .map(|e| e.batches.clone())
            .unwrap_or_default()
    }
}

/// The listing cache under its write lock, and the only way to [`Publish`].
///
/// Generic over what the cache holds so this module needn't name a listing.
pub(crate) struct CacheWrite<'a, V> {
    cache: RwLockWriteGuard<'a, HashMap<String, V>>,
    diffs: &'a Arc<DiffQueue>,
}

impl<'a, V> CacheWrite<'a, V> {
    /// Takes `cache`'s write lock, with the right to publish to `diffs`. The
    /// cache first, then (inside each publish) the queue: the one lock order.
    pub(crate) fn lock(cache: &'a RwLock<HashMap<String, V>>, diffs: &'a Arc<DiffQueue>) -> Self {
        Self {
            cache: cache.write_ignore_poison(),
            diffs,
        }
    }

    /// The listing under `listing_id` to change, and the right to publish what
    /// changed, which lives no longer than this lock.
    pub(crate) fn get_mut(&mut self, listing_id: &str) -> Option<(&mut V, Publish<'_>)> {
        let diffs = self.diffs;
        self.cache
            .get_mut(listing_id)
            .map(|listing| (listing, Publish { diffs }))
    }

    /// Removes a listing and drops its pending batches, in one acquisition.
    pub(crate) fn remove(&mut self, listing_id: &str) -> Option<V> {
        let removed = self.cache.remove(listing_id);
        self.diffs.drop_pending(listing_id);
        removed
    }

    /// The right to publish for a listing this lock holds no entry for, so a
    /// test can drive the queue under the same lock production does.
    #[cfg(test)]
    pub(crate) fn publish(&self) -> Publish<'_> {
        Publish { diffs: self.diffs }
    }
}

impl<V> Deref for CacheWrite<'_, V> {
    type Target = HashMap<String, V>;

    fn deref(&self) -> &Self::Target {
        &self.cache
    }
}

/// The right to publish a listing's committed transitions, borrowed from a
/// [`CacheWrite`]: holding one proves the cache write lock is held.
pub(crate) struct Publish<'a> {
    diffs: &'a Arc<DiffQueue>,
}

impl Publish<'_> {
    /// Queues a stamped transition for the next coalesced flush.
    pub(crate) fn enqueue(&self, listing_id: &str, batch: DirectoryDiffBatch) {
        self.diffs.enqueue(listing_id, batch);
    }

    /// Drops `listing_id`'s pending batches: its row space was just replaced,
    /// so they speak a space the pane no longer has.
    pub(crate) fn drop_pending(&self, listing_id: &str) {
        self.diffs.drop_pending(listing_id);
    }
}
