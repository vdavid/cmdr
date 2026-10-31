//! A repo's index as the hub keeps it: the one serving, the load in flight,
//! and what happens once a load lands (installed, other repos' indexes evicted
//! past the cap, an idle drop armed). DETAILS § "The index lifecycle".

use super::{RepoKey, RepoSlot, Tuning};
use crate::index_load::{Load, Waiter};
use crate::repo::{IndexedRepo, ResticError};
use crate::snapshots::Snapshot;
use cmdr_fs::volume::{TransferHold, Volume};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;

/// A repo's index: the one serving, and the load in flight (a first load, or
/// a reload beside the one serving).
// DEFAULT-OK: a repo nobody browsed into yet has no index and no load.
#[derive(Default)]
pub(crate) struct IndexSlot {
    pub(crate) ready: Option<Ready>,
    pub(crate) loading: Option<Arc<Load>>,
    /// Counts loads started, so a load's result and its idle drop know whether
    /// a newer one replaced them.
    pub(crate) generation: u64,
    /// Transfers reading from the repo right now ([`RepoSlot::hold_for_transfer`]):
    /// while any is, the index is in use, paused between files or not.
    transfer_holds: usize,
}

impl IndexSlot {
    /// The loaded index, when it reads through `parent` and holds `snapshot`.
    pub(crate) fn serving(&self, parent: &Arc<dyn Volume>, snapshot: &Snapshot) -> Option<Arc<IndexedRepo>> {
        self.ready
            .as_ref()
            .filter(|ready| ready.is_over(parent) && ready.covers.contains(&snapshot.id))
            .map(|ready| Arc::clone(&ready.repo))
    }

    /// Takes the loaded index out when it's `stale`. A reader that let go of it
    /// first (and maybe loaded the next one) leaves nothing to take.
    pub(crate) fn let_go_of(&mut self, stale: &Arc<IndexedRepo>) -> Option<Ready> {
        self.ready.take_if(|ready| Arc::ptr_eq(&ready.repo, stale))
    }

    /// A place among the waiters of the load in flight through `parent`, when
    /// there's one nobody cancelled.
    pub(crate) fn joinable(&self, parent: &Arc<dyn Volume>) -> Option<Waiter> {
        self.loading.as_ref().filter(|load| load.is_over(parent))?.join()
    }
}

/// A loaded index, the parent it reads through, and the snapshots whose trees
/// it holds for sure.
pub(crate) struct Ready {
    parent: Arc<dyn Volume>,
    pub(crate) repo: Arc<IndexedRepo>,
    covers: HashSet<String>,
    generation: u64,
}

impl Ready {
    fn is_over(&self, parent: &Arc<dyn Volume>) -> bool {
        std::ptr::addr_eq(Arc::as_ptr(&self.parent), Arc::as_ptr(parent))
    }
}

/// What a finished load needs to install its index: weak, so a load outliving
/// its hub (an app quitting) installs nothing.
pub(crate) struct Installed {
    pub(crate) slot: Weak<RepoSlot>,
    pub(crate) repos: Weak<Mutex<HashMap<RepoKey, Arc<RepoSlot>>>>,
    pub(crate) parent: Arc<dyn Volume>,
    pub(crate) covers: HashSet<String>,
    pub(crate) generation: u64,
    /// The slot's forget count when the load started: a forget since installs
    /// nothing (`RepoSlot::forgets`).
    pub(crate) forgets: u64,
    pub(crate) tuning: Tuning,
    pub(crate) runtime: Handle,
}

impl Installed {
    /// Puts a loaded index in its slot (unless a newer load already did), lets
    /// go of other repos' indexes past the cap, and arms the idle drop.
    pub(crate) fn finish(self, outcome: &Result<Arc<IndexedRepo>, ResticError>) {
        let Some(slot) = self.slot.upgrade() else { return };
        {
            let mut index = lock(&slot.index);
            if index
                .loading
                .as_ref()
                .is_some_and(|_| index.generation == self.generation)
            {
                index.loading = None;
            }
            let Ok(repo) = outcome else { return };
            if slot.forgets() != self.forgets {
                return;
            }
            if index
                .ready
                .as_ref()
                .is_some_and(|ready| ready.generation > self.generation)
            {
                return;
            }
            let replaced = index.ready.replace(Ready {
                parent: self.parent,
                repo: Arc::clone(repo),
                covers: self.covers,
                generation: self.generation,
            });
            drop(index);
            free_off_the_runtime(&self.runtime, replaced);
        }
        slot.touch_index();
        if let Some(repos) = self.repos.upgrade() {
            evict_indexes(&repos, &slot, &self.tuning, &self.runtime);
        }
        drop(self.runtime.spawn(drop_when_idle(
            Arc::downgrade(&slot),
            self.generation,
            self.tuning,
            self.runtime.clone(),
        )));
    }
}

impl RepoSlot {
    /// Keeps this repo's index loaded for as long as the answer lives: a
    /// transfer reading from the repo takes it when it starts running and drops
    /// it when it settles, so neither the idle drop nor another repo's load
    /// takes the index while a copy is paused between files (it holds no read
    /// stream then). Holds the slot, never the index itself, so an index not
    /// loaded yet is kept once it is. Releasing counts as a use, so the idle
    /// drop's clock starts when the transfer ends.
    pub(crate) fn hold_for_transfer(self: &Arc<Self>) -> TransferHold {
        lock(&self.index).transfer_holds += 1;
        TransferHold::new(HeldForTransfer(Arc::clone(self)))
    }

    /// Whether a transfer reading from the repo holds it right now: `latest`
    /// stays put while one does (`LatestPin::rebind`).
    pub(crate) fn is_held_for_transfer(&self) -> bool {
        lock(&self.index).transfer_holds > 0
    }
}

/// One transfer's hold on a repo's index; dropping it lets go.
struct HeldForTransfer(Arc<RepoSlot>);

impl Drop for HeldForTransfer {
    fn drop(&mut self) {
        let mut index = lock(&self.0.index);
        index.transfer_holds = index.transfer_holds.saturating_sub(1);
        drop(index);
        self.0.touch_index();
    }
}

/// Lets go of the least recently used indexes past [`Tuning::max_indexed`],
/// keeping `keep`'s. ❌ Never one in use (an open read stream, a listing, a
/// scan between two listings, a transfer's hold): a copy out of a repo a third browsed repo
/// evicted would pay a re-open plus a full reload mid-copy. With every one in
/// use, the cap is overshot. The password, the opened repo, and the
/// snapshot list stay.
fn evict_indexes(
    repos: &Mutex<HashMap<RepoKey, Arc<RepoSlot>>>,
    keep: &Arc<RepoSlot>,
    tuning: &Tuning,
    runtime: &Handle,
) {
    let slots: Vec<Arc<RepoSlot>> = lock(repos).values().cloned().collect();
    let mut others: Vec<(Instant, Arc<RepoSlot>)> = slots
        .into_iter()
        .filter(|slot| !Arc::ptr_eq(slot, keep) && lock(&slot.index).ready.is_some())
        .map(|slot| (slot.index_last_used(), slot))
        .collect();
    let mut over = (others.len() + 1).saturating_sub(tuning.max_indexed);
    others.sort_by_key(|(used, _)| *used);
    for (_, slot) in &others {
        if over == 0 {
            return;
        }
        let mut index = lock(&slot.index);
        if index.transfer_holds == 0
            && index
                .ready
                .as_ref()
                .is_some_and(|ready| !in_use(slot, ready, tuning.lease_grace))
        {
            let gone = index.ready.take();
            drop(index);
            free_off_the_runtime(runtime, gone);
            over -= 1;
        }
    }
}

/// Drops a slot's index once nothing has used it for [`Tuning::index_idle_drop`]
/// and nothing holds it. Ends when a newer load replaced it.
async fn drop_when_idle(slot: Weak<RepoSlot>, generation: u64, tuning: Tuning, runtime: Handle) {
    let mut wait = tuning.index_idle_drop;
    loop {
        tokio::time::sleep(wait).await;
        let Some(slot) = slot.upgrade() else { return };
        let mut index = lock(&slot.index);
        let Some(ready) = index.ready.as_ref().filter(|ready| ready.generation == generation) else {
            return;
        };
        let idle_for = slot.index_last_used().elapsed();
        if idle_for < tuning.index_idle_drop {
            wait = tuning.index_idle_drop - idle_for;
            continue;
        }
        if Arc::strong_count(&ready.repo) > 1 || index.transfer_holds > 0 {
            // A read stream or a transfer still holds it, however idle the
            // panes are.
            wait = tuning.index_idle_drop;
            continue;
        }
        let gone = index.ready.take();
        drop(index);
        free_off_the_runtime(&runtime, gone);
        return;
    }
}

/// Frees a loaded index on the blocking pool, never under the slot's lock or on
/// an async worker: the last `Arc` of a 2.4M-blob index plus its 32 MB blob
/// cache takes a while to deallocate, and every listing of the repo would wait
/// on the lock meanwhile.
pub(crate) fn free_off_the_runtime(runtime: &Handle, ready: Option<Ready>) {
    if let Some(ready) = ready {
        drop(runtime.spawn_blocking(move || drop(ready)));
    }
}

/// Whether a loaded index is in use: something holds it outside the slot (an
/// open read stream, a listing in flight), or something used the repo a moment
/// ago (the gap between a scan's listings, or a copy's files).
fn in_use(slot: &RepoSlot, ready: &Ready, grace: Duration) -> bool {
    Arc::strong_count(&ready.repo) > 1 || slot.index_last_used().elapsed() < grace
}

/// Locks a std mutex, taking the data over from a thread that panicked holding
/// it: every value here stays whole between statements.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
