//! Root's importance weight map: loading it, keeping it current, and handing it to
//! the ranker.
//!
//! Kept apart from the arena registry in [`super`] because the two have different
//! lifecycles: an arena is dialog-scoped (loaded on demand, dropped together on
//! idle), while root's weight map is permanently resident and refreshed by the
//! importance scheduler's recompute notices.
//!
//! **Root subscribes; every other volume takes a load-time snapshot.** A non-root
//! volume drops on idle and reloads next session, and its importance rarely
//! recomputes mid-session, so a live subscription would buy nothing.
//!
//! What the notices mean and why the channel is shaped the way it is:
//! `crates/cmdr-index/src/importance/read/DETAILS.md` § The reload contract.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use crate::ignore_poison::IgnorePoison;
use cmdr_index::ROOT_VOLUME_ID;

use super::set_data_dir;
use crate::search::ranking::ImportanceWeights;

/// Per-volume importance weight snapshots (folder path → weight), blended into
/// ranking. Kept separate from [`SEARCH_INDICES`](super::SEARCH_INDICES) so the root recompute subscriber
/// can refresh root's map live (subscribe-don't-poll) without touching the arena.
/// A missing/empty entry degrades ranking to match-quality + recency — today's
/// behavior. Held as `Arc` so a search clones a cheap handle and ranks against a
/// stable snapshot even if a reload swaps it mid-search.
static WEIGHTS: LazyLock<Mutex<HashMap<String, Arc<ImportanceWeights>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A cheap clone of a volume's importance weight snapshot, for the engine to rank
/// against. Empty when none loaded (degrades to match-quality + recency).
pub(crate) fn weights_for(volume_id: &str) -> Arc<ImportanceWeights> {
    WEIGHTS
        .lock_ignore_poison()
        .get(volume_id)
        .cloned()
        .unwrap_or_else(|| Arc::new(ImportanceWeights::empty()))
}

/// Install a freshly built map for a volume, replacing whatever it had.
pub(super) fn store_weights(volume_id: &str, weights: ImportanceWeights) {
    WEIGHTS
        .lock_ignore_poison()
        .insert(volume_id.to_string(), Arc::new(weights));
}

/// Patch a volume's cached weight map with one incremental pass's edits, in place.
/// Returns `false` when the volume has no map yet, so the caller falls back to a
/// full load rather than building a partial one out of a delta.
///
/// **O(changed), not O(all weights)** — the whole point of the delta. The trick that
/// buys it without touching the lock-free reader: [`Arc::make_mut`] mutates in place
/// when nobody else holds the `Arc` (the common case, since the map is only cloned
/// out for the duration of a search) and CLONES when a search does hold one, so that
/// reader keeps ranking against its untouched snapshot. Either way nobody can observe
/// a half-applied delta.
///
/// **Holding the [`WEIGHTS`] lock across the whole patch is what makes that sound.**
/// [`weights_for`] takes the same lock to clone the `Arc` out, so no reader can pick
/// up the handle mid-mutation; a reader that took it BEFORE has already bumped the
/// strong count, which forces the clone. ❌ Don't narrow this to "lock, take the
/// `Arc`, unlock, mutate".
///
/// Removals go FIRST, matching the order the store's transaction applied them, so a
/// path-hash collision between a removal and an upsert resolves in favor of the
/// upsert (the fresher fact).
fn apply_weight_delta(volume_id: &str, upserted: &[(String, f64)], removed: &[String]) -> bool {
    let mut weights_by_volume = WEIGHTS.lock_ignore_poison();
    let Some(entry) = weights_by_volume.get_mut(volume_id) else {
        return false;
    };
    let weights = Arc::make_mut(entry);
    for path in removed {
        weights.remove(path);
    }
    for (path, score) in upserted {
        weights.insert(path, *score);
    }
    log::debug!(
        target: "search",
        "importance weights patched for '{volume_id}': {} upserted, {} removed, {} scored folders",
        upserted.len(),
        removed.len(),
        weights.len(),
    );
    true
}

/// Load a volume's importance weights from its `importance-{volume_id}.db`. A
/// missing/empty DB yields an empty map — ranking then degrades cleanly. Runs on a
/// blocking thread (a SQLite read); never on the IPC thread.
///
/// Rows stream straight into the compact map, so the wide `path → weight` form never
/// exists: on a big volume that's the difference between a load that transiently costs
/// tens of MB and one that only ever holds what it keeps. It matters most for root,
/// whose map reloads on EVERY recompute (the subscriber below) while the old one is
/// still live.
pub(super) fn load_weights(data_dir: &Path, volume_id: &str) -> ImportanceWeights {
    use cmdr_index::importance::{ImportanceIndex, SignalSet};
    // `SignalSet::all()` matters only for `explain`, which the bulk weight read
    // ignores; it's the correct default regardless.
    let index = ImportanceIndex::open(data_dir, volume_id, SignalSet::all());
    let mut weights = ImportanceWeights::empty();
    match index.for_each_nonzero_weight(|path, score| weights.insert(path, score)) {
        Ok(()) => {
            log::debug!(target: "search", "importance weights loaded for '{volume_id}': {} scored folders", weights.len());
            weights
        }
        Err(e) => {
            log::debug!(target: "search", "importance weights not loaded for '{volume_id}': {e}");
            ImportanceWeights::empty()
        }
    }
}

/// Start the root recompute subscriber that keeps root's importance weight map
/// fresh, and record the app data dir for the search commands + MCP.
///
/// Subscribes to root's recompute-completed channel (subscribe-don't-poll), loads the
/// map once up front, then keeps it current the cheap way: a FULL pass replaces the
/// whole table so it reloads wholesale, while an incremental pass ships the rows it
/// touched and the map is patched in O(changed). Non-root volumes take a load-time
/// weight snapshot instead (they drop on idle and reload next session; their
/// importance rarely recomputes mid-session). Called once from app setup.
///
/// **A lagged receiver reloads.** The channel is bounded, so a consumer that falls
/// behind is told it missed notices rather than silently skipping a delta — and a
/// skipped delta would leave the map wrong until the next full pass with nothing to
/// detect it. That's why `Lagged` shares the `ReloadAll` arm rather than being
/// ignored.
///
/// **Subscribing BEFORE the first load is deliberate**, so a pass finishing during
/// that load can't slip through the gap. The cost is that a notice already buffered
/// when the load finishes gets applied on top of a snapshot that may already include
/// it; re-applying is idempotent, and notices arrive in order, so the map converges
/// to the newest fact either way.
pub(crate) fn start_importance_weight_subscriber(data_dir: PathBuf) {
    set_data_dir(data_dir.clone());
    let mut rx = cmdr_index::importance::read::subscribe(ROOT_VOLUME_ID);
    tauri::async_runtime::spawn(async move {
        let reload = {
            let dir = data_dir.clone();
            move || store_weights(ROOT_VOLUME_ID, load_weights(&dir, ROOT_VOLUME_ID))
        };
        let r = reload.clone();
        let _ = tauri::async_runtime::spawn_blocking(r).await;
        loop {
            let needs_reload = match weight_refresh_for(rx.recv().await) {
                // A delta is pure in-memory work (microseconds), so it stays on this
                // task rather than paying for a blocking hop. A volume with no map
                // yet has nothing to patch, so it falls back to the full load.
                WeightRefresh::Patch { upserted, removed } => !apply_weight_delta(ROOT_VOLUME_ID, &upserted, &removed),
                WeightRefresh::Reload => true,
                WeightRefresh::Stop => break,
            };
            if needs_reload {
                let r = reload.clone();
                let _ = tauri::async_runtime::spawn_blocking(r).await;
            }
        }
    });
}

/// What the subscriber does with one notice.
#[derive(Debug)]
enum WeightRefresh {
    /// Patch the cached map with these edits.
    Patch {
        upserted: Arc<[(String, f64)]>,
        removed: Arc<[String]>,
    },
    /// Rebuild the map from the store.
    Reload,
    /// The channel is gone; stop listening.
    Stop,
}

/// Decide what one recompute notice means for the cached weight map.
///
/// **A LAGGED receiver reloads.** That's the load-bearing rule: the channel is
/// bounded, so a consumer that falls behind is told it missed notices rather than
/// silently skipping a delta — and a skipped delta would leave the map disagreeing
/// with the store until the next full pass, with nothing to detect it. ❌ Never treat
/// a lag as "nothing happened".
///
/// Split out of the subscriber's task so that rule is directly testable without a
/// running app, a runtime, or a real recompute.
fn weight_refresh_for(
    notice: Result<cmdr_index::importance::read::WeightsChanged, tokio::sync::broadcast::error::RecvError>,
) -> WeightRefresh {
    use cmdr_index::importance::read::WeightsChanged;
    use tokio::sync::broadcast::error::RecvError;

    match notice {
        Ok(WeightsChanged::Delta { upserted, removed, .. }) => WeightRefresh::Patch { upserted, removed },
        Ok(WeightsChanged::ReloadAll { .. }) => WeightRefresh::Reload,
        Err(RecvError::Lagged(missed)) => {
            log::debug!(target: "search", "importance weight notices missed ({missed}), reloading root's map");
            WeightRefresh::Reload
        }
        // The senders are process-global, so this never fires in practice.
        Err(RecvError::Closed) => WeightRefresh::Stop,
    }
}

#[cfg(test)]
mod tests;
