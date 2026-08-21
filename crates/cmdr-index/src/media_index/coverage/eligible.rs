//! The ELIGIBLE side of coverage (the denominator): how many images each folder holds
//! that qualify for enrichment at all, per volume.
//!
//! The count per folder comes from the drive index (the same image-qualification
//! predicate the scheduler enriches by), which is an O(entries) walk — too heavy to run
//! per slider-drag frame. So the per-folder counts are cached per volume
//! ([`FolderImageCounts`]). Rather than go cold on every pass, the cache is kept warm by
//! the pass that ALREADY did the walk: a full/network pass [`replace_from_entries`] from
//! its own whole-volume walk, and a live tick [`patch_touched_dirs`] just the dirs it
//! re-walked. The rare reclaim/retro-delete prunes still [`invalidate`] (they don't have
//! a walk in hand).
//!
//! This half is walk-driven and so reaches the walk itself; the accounted numerator is
//! writer-driven and reaches nothing above it. Keeping them in separate files is what
//! keeps that a one-way dependency (see [`super`]).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use rusqlite::Connection;

use cmdr_fs::ignore_poison::IgnorePoison;

use crate::media_index::paths::parent_dir;
use crate::media_index::scheduler::enrich::{ImageEntry, WalkedDirs, for_each_qualifying_image};

use super::rollup::build_subtree_rollup;

/// The qualifying-image counts for one volume: how many images each folder holds, and
/// the volume total. Built from one index walk, cached until the next pass.
#[derive(Debug, Default, Clone)]
pub struct FolderImageCounts {
    /// `folder path → qualifying image count` for every folder with at least one.
    pub per_folder: HashMap<String, u64>,
    /// The total qualifying images across the volume.
    pub total: u64,
}

/// The process-global per-volume counts cache.
static COUNTS: LazyLock<Mutex<HashMap<String, Arc<FolderImageCounts>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// One build lock per volume, so concurrent cold callers run ONE walk between them
/// instead of each paying the full O(entries) walk (and its transient heap). Bounded by
/// the volume count, and entries are kept (a `()` mutex costs nothing) so the lock
/// identity survives an [`invalidate`] and a later rebuild.
static BUILD_LOCKS: LazyLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A shared, immutable subtree rollup (`dir → subtree total`), cached per volume.
type SharedRollup = Arc<HashMap<String, u64>>;

/// Cached subtree rollups, built on demand from a volume's `per_folder` and dropped
/// whenever `COUNTS` for the volume changes ([`invalidate_eligible_rollup`] at every
/// `COUNTS` mutation site).
static ELIGIBLE_ROLLUP: LazyLock<Mutex<HashMap<String, SharedRollup>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Get the cached folder image counts for `volume_id`, building them from the drive
/// index on first use (or after an [`invalidate`]). `None` when the volume's index
/// isn't registered (offline / never scanned) — the caller reports that volume as
/// still pending rather than counting a wrong number.
pub fn get_or_build(volume_id: &str) -> Option<Arc<FolderImageCounts>> {
    get_or_build_with(volume_id, || {
        let pool = crate::indexing::get_read_pool_for(volume_id)?;
        pool.with_conn(count_qualifying_images).ok()?.ok()
    })
}

/// [`get_or_build`] over an injectable builder: serve the cache, else run `build` ONCE
/// per volume and cache it. Split out so the tests can drive the caching and
/// deduplication behavior without a registered index read pool.
///
/// The build runs under the volume's [`BUILD_LOCKS`] entry, never under the `COUNTS` lock:
/// holding `COUNTS` across a walk that takes tens of seconds would stall every other
/// volume's cheap cached read (and the live tick's patch).
fn get_or_build_with<F>(volume_id: &str, build: F) -> Option<Arc<FolderImageCounts>>
where
    F: FnOnce() -> Option<FolderImageCounts>,
{
    if let Some(counts) = cached(volume_id) {
        return Some(counts);
    }
    let build_lock = {
        let mut locks = BUILD_LOCKS.lock_ignore_poison();
        Arc::clone(locks.entry(volume_id.to_string()).or_default())
    };
    let _building = build_lock.lock_ignore_poison();
    // Re-check under the build lock: a caller that queued behind the winner finds the
    // cache warm here and skips the walk entirely.
    if let Some(counts) = cached(volume_id) {
        return Some(counts);
    }
    let counts = Arc::new(build()?);
    COUNTS
        .lock_ignore_poison()
        .insert(volume_id.to_string(), Arc::clone(&counts));
    Some(counts)
}

/// The CACHED counts for `volume_id`, or `None` when nothing is cached yet — NEVER the
/// cold O(entries) index walk [`get_or_build`] may pay.
///
/// This is what every POLL and startup-path reader must use: the walk's transient heap is
/// gigabytes on a multi-million-entry volume, and running it from a poll is what made a
/// launch balloon to 50 GB (measured 2026-07-24). A `None` means "no honest number yet",
/// so report it as unknown; ❌ never substitute `0`. The cache is filled by the passes
/// (`replace_from_entries` / `patch_touched_dirs`) and by the user-initiated settings reads
/// that legitimately call [`get_or_build`].
pub fn cached(volume_id: &str) -> Option<Arc<FolderImageCounts>> {
    COUNTS.lock_ignore_poison().get(volume_id).map(Arc::clone)
}

/// Count a volume's qualifying images per folder WITHOUT materializing them: the
/// streaming counterpart to `build_counts(&walk_image_entries(..))`, and the only shape a
/// cold [`get_or_build`] uses.
///
/// Both go through the ONE [`for_each_qualifying_image`] walk, so the counts can't drift
/// from what a pass would enrich (pinned by the walk-parity tests in
/// `scheduler/enrich_tests.rs`). The difference is what's kept: this holds `O(folders)`
/// (one `String` key per folder that has an image, allocated on first sight), where
/// collecting holds one heap path `String` per IMAGE — gigabytes on a multi-million-entry
/// NAS index (11.3M entries, measured 2026-07-25 —
/// `docs/notes/memory-runaway-rust-heap-2026-07-25.md`).
pub(crate) fn count_qualifying_images(conn: &Connection) -> Result<FolderImageCounts, String> {
    let mut per_folder: HashMap<String, u64> = HashMap::new();
    let mut total = 0u64;
    for_each_qualifying_image(conn, &mut |image| {
        total += 1;
        // Look up before inserting so a repeat folder (the common case, since the walk
        // streams a whole dir group at a time) allocates nothing.
        match per_folder.get_mut(image.dir) {
            Some(count) => *count += 1,
            None => {
                per_folder.insert(image.dir.to_string(), 1);
            }
        }
    })?;
    Ok(FolderImageCounts { per_folder, total })
}

/// Aggregate a full qualifying-image set into per-folder counts plus the volume total. The
/// pure core the pass refills ([`replace_from_entries`]) and the live-tick patch share, so a
/// refill and a patch produce identical counts.
pub(crate) fn build_counts(entries: &[ImageEntry]) -> FolderImageCounts {
    let mut per_folder: HashMap<String, u64> = HashMap::new();
    for image in entries {
        *per_folder.entry(parent_dir(&image.path).to_string()).or_default() += 1;
    }
    FolderImageCounts {
        total: entries.len() as u64,
        per_folder,
    }
}

/// Refill a volume's cached counts DIRECTLY from a completed full/network pass's own walk,
/// replacing any previous value. The pass already ran the exact whole-volume
/// [`walk_image_entries`], so refilling from its result keeps the slider preview warm
/// instead of forcing the next preview to pay a fresh cold O(entries) walk (tens of seconds
/// on a multi-million-entry index). `entries` MUST be the pass's FULL qualifying set (the
/// unfiltered walk), never a threshold-filtered or partially-consumed subset — coverage
/// counts every qualifying image per folder, and the slider applies the threshold later.
///
/// [`walk_image_entries`]: crate::media_index::scheduler::enrich::walk_image_entries
pub(crate) fn replace_from_entries(volume_id: &str, entries: &[ImageEntry]) {
    COUNTS
        .lock_ignore_poison()
        .insert(volume_id.to_string(), Arc::new(build_counts(entries)));
    // The eligible set changed, so its cached subtree rollup is stale.
    invalidate_eligible_rollup(volume_id);
}

/// The pure patch: `existing` with exactly the WALKED dirs replaced by their fresh
/// per-tick counts from `entries` (a live tick's scoped `walk_image_entries_in_dirs`
/// result). Each walked dir's cached count becomes the tick's fresh count — dropped from
/// `per_folder` when it falls to zero (the map only holds folders with ≥ 1 image) — and
/// `total` moves by the net delta. Every other folder is untouched. Pure, so the
/// arithmetic is unit-testable.
fn patch_counts(existing: &FolderImageCounts, walked: WalkedDirs<'_>, entries: &[ImageEntry]) -> FolderImageCounts {
    // Fresh per-dir counts from the tick's scoped walk. Its entries are direct children of
    // the walked dirs, so every key here is one of them; a walked dir now holding no
    // qualifying image is simply absent (its fresh count is 0).
    let mut fresh: HashMap<&str, u64> = HashMap::new();
    for image in entries {
        *fresh.entry(parent_dir(&image.path)).or_default() += 1;
    }
    let mut per_folder = existing.per_folder.clone();
    let mut delta: i64 = 0;
    for dir in walked.iter() {
        let old = per_folder.get(dir.as_str()).copied().unwrap_or(0);
        let new = fresh.get(dir.as_str()).copied().unwrap_or(0);
        delta += new as i64 - old as i64;
        if new == 0 {
            per_folder.remove(dir.as_str());
        } else {
            per_folder.insert(dir.clone(), new);
        }
    }
    FolderImageCounts {
        per_folder,
        total: (existing.total as i64 + delta).max(0) as u64,
    }
}

/// Patch a volume's CACHED counts for exactly the dirs a live tick re-walked, from that
/// tick's scoped `entries` (see [`patch_counts`]). A tick walks only some dirs, so it
/// can't rebuild the whole cache — it patches those in place instead of invalidating (a
/// full rebuild is the O(entries) cold walk this whole cache exists to avoid). A no-op
/// when the volume has no cached counts yet: the next preview builds them.
///
/// Each dir named by `walked` is REPLACED by its count in `entries`, so naming a dir the
/// walk skipped would replace its count with zero and silently lose images that are still
/// on disk. That's why the dirs arrive as a [`WalkedDirs`] token the walk itself mints,
/// rather than as a set a caller chooses.
///
/// The flip side: a dir the tick's coverage filter dropped keeps whatever count the last
/// whole-volume walk gave it, until a full pass refills the cache.
pub(crate) fn patch_touched_dirs(volume_id: &str, walked: WalkedDirs<'_>, entries: &[ImageEntry]) {
    let mut cache = COUNTS.lock_ignore_poison();
    let Some(existing) = cache.get(volume_id) else {
        return;
    };
    let patched = patch_counts(existing, walked, entries);
    cache.insert(volume_id.to_string(), Arc::new(patched));
    drop(cache);
    // The eligible set moved for the touched dirs, so its cached rollup is stale.
    invalidate_eligible_rollup(volume_id);
}

/// Drop a volume's cached counts. Used by the rare reclaim / retro-delete prunes, which
/// don't have a fresh walk in hand to refill from; the background passes keep the cache warm
/// instead ([`replace_from_entries`] / [`patch_touched_dirs`]). The next preview rebuilds.
pub fn invalidate(volume_id: &str) {
    COUNTS.lock_ignore_poison().remove(volume_id);
    invalidate_eligible_rollup(volume_id);
}

/// The cached subtree rollup for `volume_id`, built on demand from `COUNTS`. `None` when
/// the volume's index isn't ready ([`get_or_build`] returns `None`).
fn eligible_rollup(volume_id: &str) -> Option<SharedRollup> {
    if let Some(rollup) = ELIGIBLE_ROLLUP.lock_ignore_poison().get(volume_id) {
        return Some(Arc::clone(rollup));
    }
    let counts = get_or_build(volume_id)?;
    let rollup = Arc::new(build_subtree_rollup(&counts.per_folder));
    ELIGIBLE_ROLLUP
        .lock_ignore_poison()
        .insert(volume_id.to_string(), Arc::clone(&rollup));
    Some(rollup)
}

/// The eligible subtree total for each of `folders`. `0` for a folder with no qualifying
/// images beneath it, or for every folder when the index isn't ready.
pub(super) fn eligible_subtrees(volume_id: &str, folders: &[String]) -> Vec<u64> {
    match eligible_rollup(volume_id) {
        Some(rollup) => folders.iter().map(|f| rollup.get(f).copied().unwrap_or(0)).collect(),
        None => vec![0; folders.len()],
    }
}

/// Drop the cached rollup for a volume (its `COUNTS` changed).
fn invalidate_eligible_rollup(volume_id: &str) {
    ELIGIBLE_ROLLUP.lock_ignore_poison().remove(volume_id);
}

#[cfg(test)]
mod tests;
