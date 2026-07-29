//! The covered-count machinery behind the importance settings slider's live preview:
//! given a folder-importance threshold, how many folders and qualifying images
//! would be image-indexed on the ENABLED volumes.
//!
//! The qualifying-image count per folder comes from the drive index (the same
//! image-qualification predicate the scheduler enriches by), which is an O(entries)
//! walk — too heavy to run per slider-drag frame. So the per-folder counts are cached
//! per volume ([`FolderImageCounts`]). Rather than go cold on every pass, the cache is kept
//! warm by the pass that ALREADY did the walk: a full/network pass [`replace_from_entries`]
//! from its own whole-volume walk, and a live tick [`patch_touched_dirs`] just the dirs it
//! re-walked. The rare reclaim/retro-delete prunes still [`invalidate`] (they don't have a
//! walk in hand). The threshold is then applied cheaply: intersect the importance
//! `above_threshold` folder set with the cached counts. The importance read itself is a
//! single indexed query, so a debounced drag stays cheap.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

use rusqlite::Connection;

use crate::ignore_poison::IgnorePoison;

use super::gate::IndexScope;
use super::scheduler::enrich::{ImageEntry, for_each_qualifying_image, parent_dir};
use super::store::EnrichmentState;

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
pub(crate) fn cached(volume_id: &str) -> Option<Arc<FolderImageCounts>> {
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
/// [`walk_image_entries`]: super::scheduler::enrich::walk_image_entries
pub(crate) fn replace_from_entries(volume_id: &str, entries: &[ImageEntry]) {
    COUNTS
        .lock_ignore_poison()
        .insert(volume_id.to_string(), Arc::new(build_counts(entries)));
    // The eligible set changed, so its cached subtree rollup is stale.
    invalidate_eligible_rollup(volume_id);
}

/// The pure patch: `existing` with exactly `touched_dirs` replaced by their fresh per-tick
/// counts from `entries` (a live tick's scoped `walk_image_entries_in_dirs` result). Each
/// touched dir's cached count becomes the tick's fresh count — dropped from `per_folder`
/// when it falls to zero (the map only holds folders with ≥ 1 image) — and `total` moves by
/// the net delta. Every other folder is untouched. Pure, so the arithmetic is unit-testable.
fn patch_counts(
    existing: &FolderImageCounts,
    touched_dirs: &HashSet<String>,
    entries: &[ImageEntry],
) -> FolderImageCounts {
    // Fresh per-dir counts from the tick's scoped walk. Its entries are direct children of
    // the touched dirs, so every key here is one of `touched_dirs`; a touched dir now holding
    // no qualifying image is simply absent (its fresh count is 0).
    let mut fresh: HashMap<&str, u64> = HashMap::new();
    for image in entries {
        *fresh.entry(parent_dir(&image.path)).or_default() += 1;
    }
    let mut per_folder = existing.per_folder.clone();
    let mut delta: i64 = 0;
    for dir in touched_dirs {
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

/// Patch a volume's CACHED counts for exactly the `touched_dirs` a live tick re-walked,
/// from that tick's scoped `entries` (see [`patch_counts`]). A tick walks only the touched
/// dirs, so it can't rebuild the whole cache — it patches those dirs in place instead of
/// invalidating (a full rebuild is the O(entries) cold walk this whole cache exists to
/// avoid). A no-op when the volume has no cached counts yet: the next preview builds them.
pub(crate) fn patch_touched_dirs(volume_id: &str, touched_dirs: &HashSet<String>, entries: &[ImageEntry]) {
    let mut cache = COUNTS.lock_ignore_poison();
    let Some(existing) = cache.get(volume_id) else {
        return;
    };
    let patched = patch_counts(existing, touched_dirs, entries);
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

// ── The per-directory "accounted" aggregate (the numerator) ─────────────────
//
// The eligible counts above are the DENOMINATOR (images the drive index says qualify
// per folder). The `accounted` counts are the NUMERATOR: images whose `media_status`
// row is `done` OR `failed` (both count — a failed image can't progress, so completion
// is `accounted == eligible`, else one corrupt file would keep a folder reading
// incomplete forever).
//
// The two aggregates have DIFFERENT sources and update models, so they live in separate
// caches even though the folder-coverage command reports them together:
// - `eligible` (`COUNTS`) is rebuilt from a whole-volume index walk each pass
//   (`replace_from_entries`) — it reflects the live filesystem.
// - `accounted` (`ACCOUNTED`) is maintained INCREMENTALLY: seeded once from a
//   `media_status` scan, then bumped by the ONE writer thread per volume as rows are
//   inserted (a genuinely-new `done`/`failed`) or deleted (GC/prune/purge). It reflects
//   what enrichment has stored. Merging it into `COUNTS` would let the walk-driven
//   `replace_from_entries` wipe the incrementally-maintained accounted counts.
//
// Staleness caveat (accepted first cut): a `done` row whose file changed since indexing
// still counts as `accounted` until it's re-enriched, so a folder can briefly read
// "complete" while a changed file awaits re-work. Excluding stale rows would need a
// per-row `(mtime, size)` compare against the live index; out of scope here.

/// One volume's accounted numerator: how many enriched (`done`/`failed`) rows sit
/// DIRECTLY in each dir, plus a lazily-built subtree rollup (each dir's sum over itself
/// and all descendants), invalidated on any mutation.
#[derive(Default)]
struct VolumeAccounted {
    /// `dir → count of enriched rows whose parent dir is exactly `dir``. Only holds
    /// dirs with ≥ 1 (a dir dropping to zero is removed), mirroring `per_folder`.
    per_folder: HashMap<String, u64>,
    /// `dir → sum over `dir` and all descendant dirs`. `None` until first queried or
    /// after any mutation; rebuilt `O(dirs × depth)` on demand, never a `media_status`
    /// scan per query.
    subtree: Option<HashMap<String, u64>>,
}

/// The process-global per-volume accounted cache. Keyed by volume id like [`COUNTS`],
/// with one entry per volume that has been SEEDED this session (a missing entry = not
/// yet seeded). Mutated in place (unlike `COUNTS`'s whole-`Arc` replace) because the
/// increment/decrement deltas are `O(1)` and a whole-map clone per enriched image would
/// be `O(dirs)` per image.
static ACCOUNTED: LazyLock<Mutex<HashMap<String, VolumeAccounted>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A shared, immutable subtree rollup (`dir → subtree total`), cached per volume.
type SharedRollup = Arc<HashMap<String, u64>>;

/// Cached subtree rollups for the ELIGIBLE (`COUNTS`) side, built on demand from a
/// volume's `per_folder` and dropped whenever `COUNTS` for the volume changes
/// ([`invalidate_eligible_rollup`] at every `COUNTS` mutation site).
static ELIGIBLE_ROLLUP: LazyLock<Mutex<HashMap<String, SharedRollup>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Build a subtree rollup from a per-dir count map: every dir maps to the sum over
/// itself and all its descendant dirs. Each `(dir, count)` adds `count` to `dir` and
/// each of its ANCESTORS (so an ancestor dir holding no direct images still reports its
/// descendants' total), terminating at the root. Pure, so the arithmetic is
/// unit-testable.
fn build_subtree_rollup(per_folder: &HashMap<String, u64>) -> HashMap<String, u64> {
    let mut rollup: HashMap<String, u64> = HashMap::new();
    for (dir, &count) in per_folder {
        let mut cursor = dir.as_str();
        loop {
            *rollup.entry(cursor.to_string()).or_default() += count;
            if cursor == "/" {
                break;
            }
            cursor = parent_dir(cursor);
        }
    }
    rollup
}

/// Scan a `media.db` connection's `media_status` rows into `dir → accounted count`,
/// bucketing every `done`/`failed` row by its parent dir. Every stored row is
/// `done`/`failed` (the store persists no other state), so both count; the `state`
/// filter is explicit for robustness against a future state that shouldn't.
fn scan_accounted(conn: &Connection) -> Result<HashMap<String, u64>, super::store::MediaStoreError> {
    let mut stmt = conn.prepare("SELECT f.path, s.state FROM media_status s JOIN media_file f ON f.id = s.file_id")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let mut per_folder: HashMap<String, u64> = HashMap::new();
    for row in rows {
        let (path, state) = row?;
        if matches!(
            EnrichmentState::from_token(&state),
            EnrichmentState::Done | EnrichmentState::Failed
        ) {
            *per_folder.entry(parent_dir(&path).to_string()).or_default() += 1;
        }
    }
    Ok(per_folder)
}

/// Insert a seeded accounted entry for `volume_id`, but ONLY if none exists yet. The
/// insert-if-absent is the concurrency line: the ONE writer thread per volume seeds
/// BEFORE its first commit, so whenever a delta could exist the entry is already
/// present, and a concurrent command-side seed either wins first (a complete on-disk
/// baseline, since no writer delta can have landed) or finds the entry present and
/// discards its scan. Either way the writer's deltas compose onto exactly one baseline.
fn seed_accounted_if_absent(volume_id: &str, per_folder: HashMap<String, u64>) {
    ACCOUNTED
        .lock_ignore_poison()
        .entry(volume_id.to_string())
        .or_insert_with(|| VolumeAccounted {
            per_folder,
            subtree: None,
        });
}

/// Seed the accounted aggregate for `volume_id` from a `media.db` write connection,
/// scanning `media_status` once. Called by the writer thread as its FIRST action
/// (before it processes any write), so every later insert/delete delta composes onto a
/// correct baseline. A no-op when the volume was already seeded (by an earlier writer
/// spawn this session, or a command-side [`ensure_accounted_seeded`]).
pub(crate) fn seed_accounted_from_conn(volume_id: &str, conn: &Connection) {
    if ACCOUNTED.lock_ignore_poison().contains_key(volume_id) {
        return;
    }
    match scan_accounted(conn) {
        Ok(per_folder) => seed_accounted_if_absent(volume_id, per_folder),
        Err(e) => log::warn!(target: "media_index", "accounted seed scan failed for '{volume_id}': {e}"),
    }
}

/// Ensure the accounted aggregate for `volume_id` is seeded, scanning its `media.db`
/// read-side if it isn't yet. The folder-coverage command calls this before reading the
/// rollups, in case the volume's writer hasn't spawned this session (feature just
/// enabled, or the volume never enriched). A missing `media.db` seeds an empty map (no
/// enriched rows), never creates the file.
pub(crate) fn ensure_accounted_seeded(volume_id: &str, db_path: &Path) {
    if ACCOUNTED.lock_ignore_poison().contains_key(volume_id) {
        return;
    }
    if !db_path.exists() {
        seed_accounted_if_absent(volume_id, HashMap::new());
        return;
    }
    match super::store::open_read_connection(db_path).and_then(|conn| scan_accounted(&conn)) {
        Ok(per_folder) => seed_accounted_if_absent(volume_id, per_folder),
        Err(e) => {
            // Seed empty rather than leave it unseeded (which would rescan every call);
            // a transient read error just under-reports until the next writer reseed.
            log::warn!(target: "media_index", "accounted seed read failed for '{volume_id}': {e}");
            seed_accounted_if_absent(volume_id, HashMap::new());
        }
    }
}

/// Increment `accounted[dir]` for `volume_id` by one — a genuinely-new `done`/`failed`
/// row landed directly in `dir`. A no-op when the volume isn't seeded yet (the writer
/// seeds before any delta, so this never actually misses in production; the guard just
/// avoids inserting a partial, un-seeded entry that a later `ensure_accounted_seeded`
/// would wrongly trust). Invalidates the cached subtree rollup.
pub(crate) fn accounted_inc(volume_id: &str, dir: &str) {
    let mut cache = ACCOUNTED.lock_ignore_poison();
    if let Some(entry) = cache.get_mut(volume_id) {
        *entry.per_folder.entry(dir.to_string()).or_default() += 1;
        entry.subtree = None;
    }
}

/// Decrement `accounted[dir]` for `volume_id` by one — an enriched row under `dir` was
/// deleted (GC/prune). Saturates at zero (never negative) and drops the dir when it
/// falls to zero, mirroring `per_folder`. A no-op when the volume isn't seeded or the
/// dir isn't tracked. Invalidates the cached subtree rollup.
pub(crate) fn accounted_dec(volume_id: &str, dir: &str) {
    let mut cache = ACCOUNTED.lock_ignore_poison();
    if let Some(entry) = cache.get_mut(volume_id)
        && let Some(count) = entry.per_folder.get_mut(dir)
    {
        *count -= 1;
        if *count == 0 {
            entry.per_folder.remove(dir);
        }
        entry.subtree = None;
    }
}

/// Reset a volume's accounted counts to empty (every enriched row was dropped — the
/// disable-and-purge path). Keeps the entry present (still seeded), so later inserts
/// bump from zero. Invalidates the cached subtree rollup.
pub(crate) fn accounted_reset(volume_id: &str) {
    let mut cache = ACCOUNTED.lock_ignore_poison();
    if let Some(entry) = cache.get_mut(volume_id) {
        entry.per_folder.clear();
        entry.subtree = None;
    }
}

/// Drop a volume's accounted entry entirely (unseed it). Not used in production — the
/// aggregate is maintained for the process lifetime — but lets tests isolate from the
/// process-global cache.
#[cfg(test)]
pub(crate) fn invalidate_accounted(volume_id: &str) {
    ACCOUNTED.lock_ignore_poison().remove(volume_id);
}

/// The accounted subtree total for each of `folders` (each = the sum over the folder and
/// all its descendant dirs), built from the cached rollup. `0` for an unseeded volume or
/// a folder with no enriched rows beneath it. The rollup is built once and cached until
/// the next mutation, so a batch of visible folders shares one `O(dirs × depth)` build.
pub(crate) fn accounted_subtrees(volume_id: &str, folders: &[String]) -> Vec<u64> {
    let mut cache = ACCOUNTED.lock_ignore_poison();
    let Some(entry) = cache.get_mut(volume_id) else {
        return vec![0; folders.len()];
    };
    if entry.subtree.is_none() {
        entry.subtree = Some(build_subtree_rollup(&entry.per_folder));
    }
    let rollup = entry.subtree.as_ref().expect("subtree just built above");
    folders.iter().map(|f| rollup.get(f).copied().unwrap_or(0)).collect()
}

/// The cached ELIGIBLE subtree rollup for `volume_id`, built on demand from `COUNTS`.
/// `None` when the volume's index isn't ready (`get_or_build` returns `None`).
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
fn eligible_subtrees(volume_id: &str, folders: &[String]) -> Vec<u64> {
    match eligible_rollup(volume_id) {
        Some(rollup) => folders.iter().map(|f| rollup.get(f).copied().unwrap_or(0)).collect(),
        None => vec![0; folders.len()],
    }
}

/// Drop the cached eligible rollup for a volume (its `COUNTS` changed).
fn invalidate_eligible_rollup(volume_id: &str) {
    ELIGIBLE_ROLLUP.lock_ignore_poison().remove(volume_id);
}

/// One folder's coverage: the eligible denominator and accounted numerator, each a
/// subtree total over the folder and its descendants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FolderCoverageCounts {
    pub(crate) eligible: u64,
    pub(crate) accounted: u64,
}

/// The eligible + accounted subtree totals for `folders` on `volume_id`, one per input
/// folder in order — the folder-coverage command's core. The caller must have seeded the
/// accounted aggregate first ([`ensure_accounted_seeded`]); the eligible side builds
/// itself from the drive index on demand. Both come from cached rollups, never a
/// `media_status` scan per query.
pub(crate) fn folder_coverage(volume_id: &str, folders: &[String]) -> Vec<FolderCoverageCounts> {
    let eligible = eligible_subtrees(volume_id, folders);
    let accounted = accounted_subtrees(volume_id, folders);
    eligible
        .into_iter()
        .zip(accounted)
        .map(|(eligible, accounted)| FolderCoverageCounts { eligible, accounted })
        .collect()
}

/// The covered folder + image counts for ONE volume at `threshold`, given its cached
/// image counts and its importance folder scores. `folder_scores` is `Some(map)` of
/// `folder → score` (importance ≥ some floor); `None` means importance hasn't scored
/// this volume yet (the caller reports it pending). Pure, so the threshold arithmetic
/// is unit-testable without an index or importance DB.
pub fn covered_for_volume(
    counts: &FolderImageCounts,
    folder_scores: &HashMap<String, f64>,
    threshold: f64,
) -> (u64, u64) {
    let mut folders = 0u64;
    let mut images = 0u64;
    for (folder, score) in folder_scores {
        if *score >= threshold {
            folders += 1;
            images += counts.per_folder.get(folder).copied().unwrap_or(0);
        }
    }
    (folders, images)
}

/// The reclaim partition of a volume's STORED media rows: the set that SURVIVES the
/// current setting, and the DOOMED set a reclaim prune would delete.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StoredPartition {
    /// How many stored rows fall INSIDE current coverage (they stay).
    pub surviving: u64,
    /// The stored paths OUTSIDE current coverage — the reclaim prune's doomed set
    /// (also the per-volume `keptCount`, the same rows framed as "still searchable").
    pub doomed: Vec<String>,
}

/// Partition a volume's stored media rows into surviving vs doomed by the SAME
/// precedence enrichment uses, so the destructive reclaim selection can't drift from
/// what the pass would keep. Pure over its inputs (no DB, no importance store, no app),
/// so it's unit-testable directly.
///
/// A stored path SURVIVES when it would still be enriched at `threshold`: NOT under an
/// excluded folder (a hard privacy veto) AND (covered by an "always index" override OR
/// its parent folder scores at or above `threshold`). `folder_scores` holds every
/// scored folder (`folder → score`); a folder ABSENT from it counts as below any
/// threshold — floored junk, or a folder scored away since enrichment — so its rows are
/// DOOMED (matching the enrichment gate, which keys on score-map membership, never a
/// `>= 0.0` on a defaulted 0.0). Everything not surviving is doomed, so
/// `surviving + doomed.len()` is exactly the stored-row count: no row lands in neither
/// bucket (the partition invariant the reclaim arithmetic leans on).
///
/// `is_override` / `is_excluded` take the STORED (index-relative) path; the caller wires
/// the OS-mount mapping (identity on a local volume, mount-root join on a network one),
/// keeping this core pure and shared across both volume kinds.
pub fn partition_stored(
    stored_paths: &[String],
    folder_scores: &HashMap<String, f64>,
    threshold: f64,
    scope: IndexScope,
    is_override: &dyn Fn(&str) -> bool,
    is_excluded: &dyn Fn(&str) -> bool,
) -> StoredPartition {
    let mut surviving = 0u64;
    let mut doomed = Vec::new();
    for path in stored_paths {
        if stored_row_survives(path, folder_scores, threshold, scope, is_override, is_excluded) {
            surviving += 1;
        } else {
            doomed.push(path.clone());
        }
    }
    StoredPartition { surviving, doomed }
}

/// Whether a single stored row SURVIVES the current setting — the one canonical
/// survival rule, shared by [`partition_stored`] (which collects the doomed paths for a
/// prune) and the counts-only [`MediaScheduler::stored_coverage_counts`] (which the
/// volume-state poll calls without allocating a 200k-path list). A row survives when
/// it's NOT under an excluded folder AND (covered by an "always index" override OR, in
/// the automatic SCOPE only, its parent folder scores at or above `threshold`); the
/// score term keys on score-MAP MEMBERSHIP, so a floored folder (no row) is below any
/// threshold.
///
/// In the narrow scope the threshold term drops out entirely, matching the enrichment
/// gate ([`local_should_enrich`]) exactly — the destructive reclaim selection can never
/// propose deleting a row a pass would keep, or keep one a pass would never write.
///
/// [`MediaScheduler::stored_coverage_counts`]: crate::media_index::scheduler::MediaScheduler::stored_coverage_counts
/// [`local_should_enrich`]: crate::media_index::scheduler
pub(crate) fn stored_row_survives(
    path: &str,
    folder_scores: &HashMap<String, f64>,
    threshold: f64,
    scope: IndexScope,
    is_override: &dyn Fn(&str) -> bool,
    is_excluded: &dyn Fn(&str) -> bool,
) -> bool {
    if is_excluded(path) {
        return false;
    }
    if is_override(path) {
        return true;
    }
    scope.consults_importance() && folder_scores.get(parent_dir(path)).is_some_and(|s| *s >= threshold)
}

/// The chosen-folder counts for ONE volume: how many folders holding qualifying images
/// are covered by an "always index" override, and how many images they hold. The narrow
/// scope's counterpart to [`covered_for_volume`] — same two quantities, the other
/// coverage rule — so the settings preview and progress lines stay honest in both
/// scopes off the one cached [`FolderImageCounts`]. `is_override` takes the STORED
/// (index-relative) folder path; the caller wires the OS-mount mapping, as everywhere
/// else here.
pub fn chosen_for_volume(counts: &FolderImageCounts, is_override: &dyn Fn(&str) -> bool) -> (u64, u64) {
    let mut folders = 0u64;
    let mut images = 0u64;
    for (folder, count) in &counts.per_folder {
        if is_override(folder) {
            folders += 1;
            images += count;
        }
    }
    (folders, images)
}

/// The covered folder + image counts for one volume under the CURRENT scope: the
/// chosen folders alone, or those plus every folder at or above `threshold`. The one
/// dispatcher both the settings preview and the per-volume progress line go through, so
/// neither can drift from the enrichment gate.
pub fn covered_in_scope(
    counts: &FolderImageCounts,
    folder_scores: &HashMap<String, f64>,
    threshold: f64,
    scope: IndexScope,
    is_override: &dyn Fn(&str) -> bool,
) -> (u64, u64) {
    match scope {
        IndexScope::ChosenFolders => chosen_for_volume(counts, is_override),
        IndexScope::ByImportance => {
            // The automatic scope covers the above-threshold folders PLUS the chosen
            // ones; a chosen folder that scores below (or isn't scored at all) would
            // otherwise be missing from a count the enrichment gate does include.
            let (mut folders, mut images) = covered_for_volume(counts, folder_scores, threshold);
            for (folder, count) in &counts.per_folder {
                let scored_in = folder_scores.get(folder.as_str()).is_some_and(|s| *s >= threshold);
                if !scored_in && is_override(folder) {
                    folders += 1;
                    images += count;
                }
            }
            (folders, images)
        }
    }
}

/// Convenience: read a volume's importance folder scores as a `folder → score` map, or
/// `None` when importance never scored it (offline / fresh). Mirrors the scheduler's
/// `folder_scores`, but returns EVERY scored folder (threshold applied by
/// [`covered_for_volume`] so one read serves any slider position during a debounced
/// drag).
pub fn importance_scores(data_dir: &Path, volume_id: &str) -> Option<HashMap<String, f64>> {
    use crate::importance::{ImportanceIndex, SignalSet};
    let index = ImportanceIndex::open(data_dir, volume_id, SignalSet::all());
    if !importance_scored(&index) {
        return None;
    }
    match index.above_threshold(0.0) {
        Ok(weights) => Some(weights.into_iter().map(|w| (w.path, w.score.value())).collect()),
        Err(_) => None,
    }
}

/// Whether importance genuinely has data for this volume — the "has it scored?"
/// check both the scheduler's `folder_scores` and [`importance_scores`] gate on.
///
/// Keys on live weight rows, NOT solely the `recompute_generation` stamp: a store
/// maintained only by INCREMENTAL rescores carries hundreds of thousands of weight
/// rows but no generation (the incremental path deliberately never bumps it), and a
/// schema-recreated store starts at generation 0 until its first FULL pass stamps
/// one. Gating on the generation alone reads such a volume as "never scored" forever
/// and reports "0 covered" at every threshold, even though the weights are perfectly
/// usable (`importance/DETAILS.md` § Generation-stamp semantics). So: scored when a
/// full pass stamped a generation OR any weight row exists. Reuses the cheap
/// `scored_folder_count` probe (a `COUNT(*)`, short-circuits to 0 for a missing DB) —
/// don't add a second method.
pub(crate) fn importance_scored(index: &crate::importance::ImportanceIndex) -> bool {
    index.recompute_generation().unwrap_or(0) > 0 || index.scored_folder_count().unwrap_or(0) > 0
}

#[cfg(test)]
mod tests;
