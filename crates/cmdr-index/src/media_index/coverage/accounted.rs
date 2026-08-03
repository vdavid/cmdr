//! The ACCOUNTED side of coverage (the numerator): how many images the media index
//! actually has a stored result for, per directory, per volume.
//!
//! A row counts once its `media_status` state is `done` OR `failed` (both count — a
//! failed image can't progress, so completion is `accounted == eligible`, else one
//! corrupt file would keep a folder reading incomplete forever).
//!
//! Maintained INCREMENTALLY: seeded once from a `media_status` scan, then bumped by the
//! ONE writer thread per volume as rows are inserted (a genuinely-new `done`/`failed`)
//! or deleted (GC/prune/purge). It reflects what enrichment has STORED, where the
//! eligible denominator reflects the live filesystem. That's why the two live in
//! separate caches and separate files: merging them would let the walk-driven
//! `replace_from_entries` wipe these incrementally-maintained counts, and would put the
//! writer and the walk back in one dependency cycle.
//!
//! Staleness caveat (accepted first cut): a `done` row whose file changed since indexing
//! still counts as accounted until it's re-enriched, so a folder can briefly read
//! "complete" while a changed file awaits re-work. Excluding stale rows would need a
//! per-row `(mtime, size)` compare against the live index; out of scope here.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use rusqlite::Connection;

use cmdr_fs::ignore_poison::IgnorePoison;

use crate::media_index::paths::parent_dir;
use crate::media_index::store::EnrichmentState;

use super::rollup::build_subtree_rollup;

/// One volume's accounted numerator: how many enriched (`done`/`failed`) rows sit
/// DIRECTLY in each dir, plus a lazily-built subtree rollup (each dir's sum over itself
/// and all descendants), invalidated on any mutation.
#[derive(Default)]
struct VolumeAccounted {
    /// `dir → count of enriched rows whose parent dir is exactly `dir``. Only holds
    /// dirs with ≥ 1 (a dir dropping to zero is removed), mirroring the eligible
    /// `per_folder`.
    per_folder: HashMap<String, u64>,
    /// `dir → sum over `dir` and all descendant dirs`. `None` until first queried or
    /// after any mutation; rebuilt `O(dirs × depth)` on demand, never a `media_status`
    /// scan per query.
    subtree: Option<HashMap<String, u64>>,
}

/// The process-global per-volume accounted cache. Keyed by volume id like the eligible
/// counts, with one entry per volume that has been SEEDED this session (a missing entry
/// = not yet seeded). Mutated in place (unlike the eligible side's whole-`Arc` replace)
/// because the increment/decrement deltas are `O(1)` and a whole-map clone per enriched
/// image would be `O(dirs)` per image.
static ACCOUNTED: LazyLock<Mutex<HashMap<String, VolumeAccounted>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Scan a `media.db` connection's `media_status` rows into `dir → accounted count`,
/// bucketing every `done`/`failed` row by its parent dir. Every stored row is
/// `done`/`failed` (the store persists no other state), so both count; the `state`
/// filter is explicit for robustness against a future state that shouldn't.
fn scan(conn: &Connection) -> Result<HashMap<String, u64>, crate::media_index::store::MediaStoreError> {
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
fn seed_if_absent(volume_id: &str, per_folder: HashMap<String, u64>) {
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
/// spawn this session, or a command-side [`ensure_seeded`]).
pub(crate) fn seed_from_conn(volume_id: &str, conn: &Connection) {
    if ACCOUNTED.lock_ignore_poison().contains_key(volume_id) {
        return;
    }
    match scan(conn) {
        Ok(per_folder) => seed_if_absent(volume_id, per_folder),
        Err(e) => log::warn!(target: "media_index", "accounted seed scan failed for '{volume_id}': {e}"),
    }
}

/// Ensure the accounted aggregate for `volume_id` is seeded, scanning its `media.db`
/// read-side if it isn't yet. The folder-coverage command calls this before reading the
/// rollups, in case the volume's writer hasn't spawned this session (feature just
/// enabled, or the volume never enriched). A missing `media.db` seeds an empty map (no
/// enriched rows), never creates the file.
pub(crate) fn ensure_seeded(volume_id: &str, db_path: &Path) {
    if ACCOUNTED.lock_ignore_poison().contains_key(volume_id) {
        return;
    }
    if !db_path.exists() {
        seed_if_absent(volume_id, HashMap::new());
        return;
    }
    match crate::media_index::store::open_read_connection(db_path).and_then(|conn| scan(&conn)) {
        Ok(per_folder) => seed_if_absent(volume_id, per_folder),
        Err(e) => {
            // Seed empty rather than leave it unseeded (which would rescan every call);
            // a transient read error just under-reports until the next writer reseed.
            log::warn!(target: "media_index", "accounted seed read failed for '{volume_id}': {e}");
            seed_if_absent(volume_id, HashMap::new());
        }
    }
}

/// Increment `accounted[dir]` for `volume_id` by one — a genuinely-new `done`/`failed`
/// row landed directly in `dir`. A no-op when the volume isn't seeded yet (the writer
/// seeds before any delta, so this never actually misses in production; the guard just
/// avoids inserting a partial, un-seeded entry that a later [`ensure_seeded`]
/// would wrongly trust). Invalidates the cached subtree rollup.
pub(crate) fn inc(volume_id: &str, dir: &str) {
    let mut cache = ACCOUNTED.lock_ignore_poison();
    if let Some(entry) = cache.get_mut(volume_id) {
        *entry.per_folder.entry(dir.to_string()).or_default() += 1;
        entry.subtree = None;
    }
}

/// Decrement `accounted[dir]` for `volume_id` by one — an enriched row under `dir` was
/// deleted (GC/prune). Saturates at zero (never negative) and drops the dir when it
/// falls to zero, mirroring the eligible `per_folder`. A no-op when the volume isn't
/// seeded or the dir isn't tracked. Invalidates the cached subtree rollup.
pub(crate) fn dec(volume_id: &str, dir: &str) {
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
pub(crate) fn reset(volume_id: &str) {
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
pub(crate) fn invalidate(volume_id: &str) {
    ACCOUNTED.lock_ignore_poison().remove(volume_id);
}

/// The accounted subtree total for each of `folders` (each = the sum over the folder and
/// all its descendant dirs), built from the cached rollup. `0` for an unseeded volume or
/// a folder with no enriched rows beneath it. The rollup is built once and cached until
/// the next mutation, so a batch of visible folders shares one `O(dirs × depth)` build.
pub(crate) fn subtrees(volume_id: &str, folders: &[String]) -> Vec<u64> {
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

#[cfg(test)]
mod tests;
