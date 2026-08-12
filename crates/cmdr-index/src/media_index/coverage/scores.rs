//! The cached importance folder scores the coverage gates read, and the
//! threshold projection of them the enrichment gate checks membership against.
//!
//! **Why a cache at all.** Reading a volume's scores is `above_threshold(0.0)`: an
//! ordered read of EVERY scored folder, which SQLite runs as an external merge sort
//! (a measured 368,043 scored folders on one root). That is fine once per enrichment
//! pass and ruinous per UI query — and the file-status badge asks per visible range,
//! per pane, on every listing swap and enrichment tick. Uncached, those queries piled
//! up on the blocking pool until it hit its 512-thread cap, and every OTHER
//! `spawn_blocking` in the app (directory listings, the volume list) starved behind
//! them.
//!
//! **Why a subscription and not a generation stamp.** An INCREMENTAL rescore writes
//! rows at the CURRENT generation without bumping it (`importance/writer.rs`), so a
//! generation-keyed cache would serve stale scores until the next full pass. The
//! recompute bus is the store's own answer to this: [`WeightsChanged`] carries either
//! a patch or "rebuild", and a receiver that falls behind is TOLD rather than silently
//! skipped. The reload contract is documented in
//! `crates/cmdr-index/src/importance/read/DETAILS.md` § The reload contract.
//!
//! **Why draining is pull-based.** The one other caching consumer (search's
//! `weights.rs`) owns a background task because its map must be fresh for a query
//! that never asks for it. Here every read goes through [`importance_scores`], so
//! draining the receiver at the top of a read is the same freshness with no task, no
//! lifecycle, and no per-volume spawn from inside a blocking closure. A notice that
//! arrives while nobody is asking simply waits in the channel.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::TryRecvError;

use crate::importance::read::{WeightsChanged, subscribe};
use cmdr_fs::ignore_poison::IgnorePoison;

/// One volume's cached scores, plus the subscription that keeps them honest.
struct CachedScores {
    /// Every scored folder's `path → score`, exactly what a fresh
    /// `above_threshold(0.0)` would build. Held as an `Arc` so a reader clones a
    /// handle rather than a map that costs tens of MB.
    all: Arc<HashMap<String, f64>>,
    /// The last threshold projection built from `all`, memoized because the gate asks
    /// for the same threshold on every call and rebuilding it copies the whole map.
    /// Dropped whenever `all` is rebuilt or patched.
    projection: Option<(f64, Arc<HashMap<String, f64>>)>,
    /// Recompute notices for this volume. Subscribed BEFORE the first read, so a pass
    /// that finishes during that read lands in the channel instead of being missed.
    notices: Receiver<WeightsChanged>,
}

/// Per-volume score caches. Entries live for the process, like the recompute bus
/// itself: an unmounted volume's entry costs one map and stays correct if it returns.
static CACHE: LazyLock<Mutex<HashMap<String, CachedScores>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// What draining the notices told us to do with a cached entry.
#[derive(Debug, PartialEq)]
enum Refresh {
    /// Nothing changed; the cached map is current.
    Fresh,
    /// Apply these edits in place — `O(changed)`, not `O(all scores)`.
    Patch {
        upserted: Vec<(String, f64)>,
        removed: Vec<String>,
    },
    /// Re-read the whole table: a full pass replaced it, or we fell behind and can't
    /// know what we missed.
    Rebuild,
}

/// Fold every notice waiting on `notices` into ONE decision, without blocking.
///
/// **A lag rebuilds.** The channel is bounded, so a receiver that falls behind is
/// told it missed notices rather than handed a hole — and a skipped delta would leave
/// the map disagreeing with the store with nothing to detect it until the next full
/// pass. ❌ Never treat a lag as "nothing happened".
///
/// A rebuild absorbs every notice after it: once we're re-reading the whole table,
/// the deltas we'd apply on top are already in what we'll read. Deltas before a
/// rebuild are dropped for the same reason. A closed channel means the senders are
/// gone (they're process-global, so this never fires in practice); the cached map is
/// then as good as it will ever get, so it reads `Fresh`.
fn drain(notices: &mut Receiver<WeightsChanged>) -> Refresh {
    let mut refresh = Refresh::Fresh;
    loop {
        match notices.try_recv() {
            Ok(WeightsChanged::Delta { upserted, removed, .. }) => {
                if let Refresh::Rebuild = refresh {
                    continue;
                }
                let (mut ups, mut rems) = match refresh {
                    Refresh::Patch { upserted, removed } => (upserted, removed),
                    _ => (Vec::new(), Vec::new()),
                };
                ups.extend(upserted.iter().cloned());
                rems.extend(removed.iter().cloned());
                refresh = Refresh::Patch {
                    upserted: ups,
                    removed: rems,
                };
            }
            Ok(WeightsChanged::ReloadAll { .. }) => refresh = Refresh::Rebuild,
            Err(TryRecvError::Lagged(missed)) => {
                log::debug!(target: "media_index", "importance score notices missed ({missed}), rebuilding the coverage score cache");
                refresh = Refresh::Rebuild;
            }
            Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => return refresh,
        }
    }
}

/// Apply one drained delta to a cached map in place.
///
/// Removals go FIRST, matching the order the store's transaction applied them and
/// the search cache's rule, so an upsert of a path that also appears in `removed`
/// resolves in favor of the upsert (the fresher fact). [`Arc::make_mut`] mutates in
/// place when no reader holds the handle and clones when one does, so a reader that
/// already took a snapshot keeps reading it untouched.
fn patch(entry: &mut CachedScores, upserted: &[(String, f64)], removed: &[String]) {
    let all = Arc::make_mut(&mut entry.all);
    for path in removed {
        all.remove(path);
    }
    for (path, score) in upserted {
        all.insert(path.clone(), *score);
    }
    entry.projection = None;
}

/// Read every scored folder for `volume_id` straight from the store, bypassing the
/// cache. `None` when importance has NEVER scored the volume (fresh, offline, or
/// importance disabled) — the load-bearing signal that sends the coverage gates to
/// override-only rather than to "cover everything".
fn read_all(data_dir: &Path, volume_id: &str) -> Option<HashMap<String, f64>> {
    use crate::importance::{ImportanceIndex, SignalSet};
    let index = ImportanceIndex::open(data_dir, volume_id, SignalSet::all());
    if !index.is_scored() {
        return None;
    }
    match index.above_threshold(0.0) {
        Ok(weights) => Some(weights.into_iter().map(|w| (w.path, w.score.value())).collect()),
        Err(e) => {
            log::debug!(target: "media_index", "importance scores unreadable for '{volume_id}': {e}");
            None
        }
    }
}

/// A volume's importance folder scores as a `folder → score` map, or `None` when
/// importance never scored it (fresh / offline / disabled).
///
/// Returns EVERY scored folder, with no threshold applied, so ONE read serves any
/// slider position during a debounced drag. A gate that only wants the folders at or
/// above one threshold takes [`importance_scores_above`] instead of filtering this
/// itself — the filter copies the whole map, which is the cost the cache exists to
/// avoid.
///
/// Cheap after the first call per volume: a fresh read happens only when the store
/// says its weights moved. See this module's header for why that signal is the
/// recompute subscription rather than the generation stamp.
pub fn importance_scores(data_dir: &Path, volume_id: &str) -> Option<Arc<HashMap<String, f64>>> {
    let mut cache = CACHE.lock_ignore_poison();
    // Taking the entry OUT hands us its receiver to carry into the rebuild below. ❌
    // Don't `resubscribe()` there instead: a fresh receiver starts at the channel's
    // tail, so a notice sent between the drain and the resubscribe would be skipped,
    // and this receiver is already positioned exactly after the last notice we read.
    if let Some(mut entry) = cache.remove(volume_id) {
        match drain(&mut entry.notices) {
            Refresh::Fresh => {}
            Refresh::Patch { upserted, removed } => patch(&mut entry, &upserted, &removed),
            Refresh::Rebuild => {
                // Read with the subscription still LIVE, so a pass that commits
                // during the read waits in the channel rather than falling into the
                // gap. The cost is re-applying a notice the read already reflects,
                // which is idempotent.
                match read_all(data_dir, volume_id) {
                    Some(all) => {
                        entry.all = Arc::new(all);
                        entry.projection = None;
                    }
                    // The store went unreadable (purged, or the volume unmounted
                    // mid-rebuild). Dropping the entry here is deliberate: serving a
                    // map the store no longer backs would outlive the data, and the
                    // next call re-reads (and re-subscribes) from scratch.
                    None => return None,
                }
            }
        }
        let all = Arc::clone(&entry.all);
        cache.insert(volume_id.to_string(), entry);
        return Some(all);
    }
    // First read for this volume: subscribe BEFORE reading, same gap-free reason.
    let notices = subscribe(volume_id);
    let all = Arc::new(read_all(data_dir, volume_id)?);
    cache.insert(
        volume_id.to_string(),
        CachedScores {
            all: Arc::clone(&all),
            projection: None,
            notices,
        },
    );
    Some(all)
}

/// The folders scoring at or above `threshold`, as the enrichment coverage gate reads
/// them: [`local_should_enrich`](crate::media_index::scheduler::local_should_enrich)
/// keys on score-map MEMBERSHIP, so the threshold has to be baked into the map rather
/// than checked at lookup. `None` for an unscored volume, exactly as
/// [`importance_scores`].
///
/// Memoized per volume for the LAST threshold asked, which is all a gate needs: it
/// reads the one live setting, so it hits the memo on every call, while a slider drag
/// (which walks thresholds) goes through [`importance_scores`] and never builds these
/// at all.
pub fn importance_scores_above(data_dir: &Path, volume_id: &str, threshold: f64) -> Option<Arc<HashMap<String, f64>>> {
    // Refresh (and cache) the full map first, so the projection below is built from
    // scores that are current, and the drained notices can't be lost.
    let all = importance_scores(data_dir, volume_id)?;

    let mut cache = CACHE.lock_ignore_poison();
    let entry = cache.get_mut(volume_id)?;
    if let Some((cached_threshold, projection)) = &entry.projection
        && cached_threshold.to_bits() == threshold.to_bits()
    {
        return Some(Arc::clone(projection));
    }
    let projection: Arc<HashMap<String, f64>> = Arc::new(
        all.iter()
            .filter(|(_, score)| **score >= threshold)
            .map(|(path, score)| (path.clone(), *score))
            .collect(),
    );
    entry.projection = Some((threshold, Arc::clone(&projection)));
    Some(projection)
}

/// Drop every cached entry. Test-only: the cache is keyed by volume id, and a test
/// that reuses one across two different data dirs would otherwise read the first
/// one's scores.
#[cfg(any(test, feature = "testing"))]
pub fn clear_cache_for_test() {
    CACHE.lock_ignore_poison().clear();
}

#[cfg(test)]
mod tests;
