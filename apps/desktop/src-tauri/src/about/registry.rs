//! Which cards are gathering, so a close stops its own and a quit stops them all.
//!
//! The frontend mints the request id, so `about_cancel` can arrive before
//! `about_start` registered anything (the card closed while the start was still in
//! flight). That cancel leaves a tombstone, and the start that follows honors it by
//! never starting.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use crate::ignore_poison::IgnorePoison;

/// How long a cancel waits for its start. A start that hasn't arrived by then
/// isn't coming (the webview reloaded, or the call failed on its way).
const TOMBSTONE_TTL: Duration = Duration::from_secs(60);

/// The most tombstones kept, so a frontend cancelling ids it never started can't
/// grow the map without bound.
const MAX_TOMBSTONES: usize = 256;

enum Entry {
    /// Gathering; cancelling trips the token. `run` tells this start from a later
    /// one under the same id.
    Live { token: CancellationToken, run: u64 },
    /// Cancelled before it started.
    Tombstone(Instant),
}

/// The cards in flight, by request id.
#[derive(Default)]
pub(crate) struct Registry {
    entries: Mutex<HashMap<String, Entry>>,
    next_run: AtomicU64,
}

/// A registered gathering: its cancel token, and the run [`Registry::finish`]
/// names.
#[derive(Debug, Clone)]
pub(crate) struct Run {
    pub(crate) token: CancellationToken,
    run: u64,
}

impl Registry {
    /// Registers `id`, or answers `None` when its cancel came first.
    pub(crate) fn register(&self, id: &str) -> Option<Run> {
        let mut entries = self.entries.lock_ignore_poison();
        if let Some(Entry::Tombstone(_)) = entries.get(id) {
            entries.remove(id);
            return None;
        }
        let run = Run {
            token: CancellationToken::new(),
            run: self.next_run.fetch_add(1, Ordering::Relaxed),
        };
        let entry = Entry::Live {
            token: run.token.clone(),
            run: run.run,
        };
        // A live id registered twice is a frontend bug; the older run stops so one
        // card never has two gatherings feeding it.
        if let Some(Entry::Live { token, .. }) = entries.insert(id.to_string(), entry) {
            token.cancel();
        }
        Some(run)
    }

    /// Cancels `id`: trips a live one, or leaves a tombstone for a start still on
    /// its way. Idempotent.
    pub(crate) fn cancel(&self, id: &str) {
        let mut entries = self.entries.lock_ignore_poison();
        match entries.get(id) {
            Some(Entry::Live { token, .. }) => {
                token.cancel();
                entries.remove(id);
            }
            Some(Entry::Tombstone(_)) => {}
            None => {
                prune_tombstones(&mut entries);
                entries.insert(id.to_string(), Entry::Tombstone(Instant::now()));
            }
        }
    }

    /// Forgets `run` once its gathering is done. A newer run under the same id,
    /// or a tombstone, stays.
    pub(crate) fn finish(&self, id: &str, run: &Run) {
        let mut entries = self.entries.lock_ignore_poison();
        if matches!(entries.get(id), Some(Entry::Live { run: live, .. }) if *live == run.run) {
            entries.remove(id);
        }
    }

    /// Cancels every card: the app is quitting. Returns how many were live.
    pub(crate) fn cancel_all(&self) -> usize {
        let drained: Vec<Entry> = self
            .entries
            .lock_ignore_poison()
            .drain()
            .map(|(_, entry)| entry)
            .collect();
        let mut live = 0;
        for entry in drained {
            if let Entry::Live { token, .. } = entry {
                token.cancel();
                live += 1;
            }
        }
        live
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.lock_ignore_poison().len()
    }
}

/// Drops tombstones past their wait, then the oldest until there's room for one
/// more.
fn prune_tombstones(entries: &mut HashMap<String, Entry>) {
    let now = Instant::now();
    entries.retain(|_, entry| match entry {
        Entry::Tombstone(at) => now.duration_since(*at) < TOMBSTONE_TTL,
        Entry::Live { .. } => true,
    });
    let mut tombstones: Vec<(String, Instant)> = entries
        .iter()
        .filter_map(|(id, entry)| match entry {
            Entry::Tombstone(at) => Some((id.clone(), *at)),
            Entry::Live { .. } => None,
        })
        .collect();
    if tombstones.len() >= MAX_TOMBSTONES {
        tombstones.sort_by_key(|(_, at)| *at);
        let excess = tombstones.len() + 1 - MAX_TOMBSTONES;
        for (id, _) in tombstones.into_iter().take(excess) {
            entries.remove(&id);
        }
    }
}

/// The app's one registry.
pub(crate) static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::default);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancel_before_its_start_leaves_a_tombstone_the_start_honors() {
        let registry = Registry::default();
        registry.cancel("early");
        assert!(registry.register("early").is_none(), "the card closed first");
        assert_eq!(registry.len(), 0, "the tombstone is spent");
        assert!(registry.register("early").is_some(), "a later start under the id runs");
    }

    #[test]
    fn a_cancel_trips_the_live_run_and_forgets_it() {
        let registry = Registry::default();
        let run = registry.register("card").expect("nothing cancelled it");
        registry.cancel("card");
        assert!(run.token.is_cancelled());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn finishing_an_older_run_leaves_the_newer_one_registered() {
        let registry = Registry::default();
        let older = registry.register("card").expect("first start");
        let newer = registry.register("card").expect("second start");
        assert!(older.token.is_cancelled(), "one card has one gathering");
        registry.finish("card", &older);
        assert_eq!(registry.len(), 1);
        registry.finish("card", &newer);
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn cancel_all_trips_every_live_run_and_counts_them() {
        let registry = Registry::default();
        let a = registry.register("a").expect("starts");
        let b = registry.register("b").expect("starts");
        registry.cancel("never-started");
        assert_eq!(registry.cancel_all(), 2);
        assert!(a.token.is_cancelled() && b.token.is_cancelled());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn tombstones_stay_bounded() {
        let registry = Registry::default();
        for i in 0..(MAX_TOMBSTONES * 2) {
            registry.cancel(&format!("id-{i}"));
        }
        assert!(registry.len() <= MAX_TOMBSTONES);
    }
}
