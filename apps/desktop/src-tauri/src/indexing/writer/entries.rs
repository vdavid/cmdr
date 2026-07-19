//! Entry write handlers: insert, upsert, move, delete, and truncate.
//!
//! These run on the writer thread and own the `entries` table mutations. Each
//! keeps ancestor `dir_stats` consistent by calling into `super::delta` for
//! size/count and symlink-flag propagation, and bumps the writer generation so
//! the search index can detect staleness.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Instant;

use tauri::AppHandle;
use tauri_specta::Event;

use crate::indexing::IndexFailureSignal;
use crate::indexing::aggregator::AggregationPhase;
use crate::indexing::store::{DirStatsById, EntryRow, IndexStore, IndexStoreError};
use crate::pluralize::pluralize_with;

use super::delta::{propagate_delta_by_id, propagate_min_subtree_epoch, propagate_recursive_has_symlinks};
use super::{AccumulatorMaps, AggregationProgressEvent, MutationTracker, phase_to_str};

#[allow(
    clippy::too_many_arguments,
    reason = "writer handler: ambient state + the failure signal"
)]
pub(super) fn handle_insert_entries_v2(
    conn: &rusqlite::Connection,
    entries: Vec<EntryRow>,
    accumulator: &mut AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    volume_id: &str,
    expected_total_entries: &AtomicU64,
    mutation_tracker: &MutationTracker,
    signal: &IndexFailureSignal,
) {
    let count = entries.len();
    let t = Instant::now();
    // Accumulate AFTER the DB commit succeeds. `insert_entries_v2_batch`
    // uses `INSERT OR IGNORE`, so a UNIQUE conflict on
    // `(parent_id, name_folded)` (case-sensitive volumes with `Foo.txt` and
    // `foo.txt` siblings, NFC/NFD duplicates from cross-OS sync, etc.) skips
    // just that row instead of rolling back the entire 2000-entry batch. The
    // accumulator must skip those rows too, or `compute_all_aggregates_with_maps`
    // inflates `dir_stats` with phantom bytes (the constraint comment that
    // called out "1.83 TB ghost size on a 994 GB volume" is exactly this
    // failure mode).
    //
    // A per-batch skip is logged at DEBUG only (with a sample for diagnosis): a
    // few skips per scan is expected dedup and not actionable. The accumulated
    // count is summarized once per scan at `ComputeAllAggregates`, which escalates
    // to WARN only when the skip ratio looks like a racing writer. See
    // `classify_skip_severity`.
    match IndexStore::insert_entries_v2_batch(conn, &entries) {
        Ok(inserted) => {
            let skipped_count = inserted.iter().filter(|landed| !**landed).count();
            if skipped_count == 0 {
                accumulator.accumulate(&entries);
            } else {
                accumulator.entries_skipped += skipped_count as u64;
                accumulator.accumulate(
                    entries
                        .iter()
                        .zip(inserted.iter())
                        .filter_map(|(e, landed)| if *landed { Some(e) } else { None }),
                );
                let samples: Vec<(i64, &str)> = entries
                    .iter()
                    .zip(inserted.iter())
                    .filter_map(|(e, landed)| {
                        if !*landed {
                            Some((e.parent_id, e.name.as_str()))
                        } else {
                            None
                        }
                    })
                    .take(3)
                    .collect();
                log::debug!(
                    "Index writer: {skipped_count} of {batch_size} skipped due to UNIQUE conflict on (parent_id, name_folded); sample: {samples:?}",
                    batch_size = pluralize_with(count as u64, "entry", "entries")
                );
            }
        }
        Err(e) => {
            signal.note(&e, "insert_entries_v2_batch");
        }
    }
    let elapsed = t.elapsed().as_millis();
    if elapsed > 100 {
        log::debug!(
            "Writer: insert_entries_v2_batch ({}) took {elapsed}ms",
            pluralize_with(count as u64, "entry", "entries")
        );
    }
    mutation_tracker.bump();
    // Emit flushing progress when we know the expected total
    let expected = expected_total_entries.load(Ordering::Relaxed);
    if expected > 0
        && let Some(app) = app_handle
    {
        let _ = AggregationProgressEvent {
            volume_id: volume_id.to_string(),
            phase: phase_to_str(AggregationPhase::SavingEntries).to_string(),
            current: accumulator.entries_inserted,
            total: expected,
        }
        .emit(app);
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "mirrors the DB columns for a single upsert operation"
)]
pub(super) fn handle_upsert_entry_v2(
    conn: &rusqlite::Connection,
    parent_id: i64,
    name: String,
    is_directory: bool,
    is_symlink: bool,
    logical_size: Option<u64>,
    physical_size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
    nlink: Option<u64>,
    next_id: &AtomicI64,
    mutation_tracker: &MutationTracker,
    propagate_deltas: bool,
    signal: &IndexFailureSignal,
) {
    // Hardlink dedup: if this file has nlink > 1, check whether another entry
    // for the same inode already has non-NULL sizes. If so, override sizes to
    // None so each inode's bytes are counted exactly once.
    let should_dedup = inode.is_some() && matches!(nlink, Some(n) if n > 1) && logical_size.is_some();

    // Check if an entry already exists at (parent_id, name).
    // Auto-propagates size deltas to ancestor dir_stats on both insert and update
    // (when `propagate_deltas`), so callers never need a separate
    // PropagateDeltaById for upserted entries. The full reconcile passes
    // `propagate_deltas = false` to skip the ancestor walk; its final
    // `ComputeAllAggregates` recomputes every dir's stats instead.
    match IndexStore::resolve_component(conn, parent_id, &name) {
        Ok(Some(existing_id)) => {
            // Type change (file↔dir): delete old entry and insert fresh so
            // file_count/dir_count deltas propagate correctly. An in-place update
            // would leave counts wrong because the old type's count isn't decremented.
            let old_entry = IndexStore::get_entry_by_id(conn, existing_id).ok().flatten();
            if let Some(ref old) = old_entry
                && old.is_directory != is_directory
            {
                log::debug!(
                    "Writer: UpsertEntryV2 type change for id={existing_id} \
                         (was_dir={}, now_dir={is_directory}), converting to delete+insert",
                    old.is_directory
                );
                if old.is_directory {
                    handle_delete_subtree_by_id(conn, existing_id, propagate_deltas, mutation_tracker, signal);
                } else {
                    handle_delete_entry_by_id(conn, existing_id, propagate_deltas, mutation_tracker, signal);
                }
                upsert_insert_new(
                    conn,
                    parent_id,
                    &name,
                    is_directory,
                    is_symlink,
                    logical_size,
                    physical_size,
                    modified_at,
                    inode,
                    should_dedup,
                    next_id,
                    propagate_deltas,
                    signal,
                );
                return;
            }

            upsert_update_existing(
                conn,
                existing_id,
                parent_id,
                is_directory,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
                inode,
                should_dedup,
                old_entry,
                propagate_deltas,
                signal,
            );
        }
        Ok(None) => {
            upsert_insert_new(
                conn,
                parent_id,
                &name,
                is_directory,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
                inode,
                should_dedup,
                next_id,
                propagate_deltas,
                signal,
            );
        }
        Err(e) => {
            signal.note(&e, &format!("resolve_component for {name}"));
        }
    }
    mutation_tracker.bump();
}

/// Update an existing entry during `UpsertEntryV2`, with hardlink dedup and delta propagation.
#[allow(
    clippy::too_many_arguments,
    reason = "mirrors the DB columns for an existing-entry update"
)]
fn upsert_update_existing(
    conn: &rusqlite::Connection,
    existing_id: i64,
    parent_id: i64,
    is_directory: bool,
    is_symlink: bool,
    logical_size: Option<u64>,
    physical_size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
    should_dedup: bool,
    old_entry: Option<EntryRow>,
    propagate_deltas: bool,
    signal: &IndexFailureSignal,
) {
    // Dedup: override sizes if another entry already has sizes for this inode
    let (logical_size, physical_size) = if should_dedup
        && IndexStore::has_sized_entry_for_inode(
            conn,
            inode.expect("should_dedup is true only when inode.is_some()"),
            Some(existing_id),
        )
        .unwrap_or(false)
    {
        (None, None)
    } else {
        (logical_size, physical_size)
    };
    if let Err(e) = IndexStore::update_entry(
        conn,
        existing_id,
        is_directory,
        is_symlink,
        logical_size,
        physical_size,
        modified_at,
        inode,
    ) {
        signal.note(&e, &format!("update_entry id={existing_id}"));
    } else if let Some(old) = old_entry
        && propagate_deltas
    {
        // Propagate size delta if anything changed. Skipped under a bulk reconcile
        // (`propagate_deltas == false`): the final ComputeAllAggregates recomputes
        // ancestor stats from scratch, so this ancestor walk would be wasted work.
        let old_logical = old.logical_size.unwrap_or(0) as i64;
        let new_logical = logical_size.unwrap_or(0) as i64;
        let old_physical = old.physical_size.unwrap_or(0) as i64;
        let new_physical = physical_size.unwrap_or(0) as i64;
        let logical_delta = new_logical - old_logical;
        let physical_delta = new_physical - old_physical;
        if logical_delta != 0 || physical_delta != 0 {
            propagate_delta_by_id(conn, parent_id, logical_delta, physical_delta, 0, 0);
        }
        // Symlink state change can flip the parent's `recursive_has_symlinks`.
        if old.is_symlink != is_symlink {
            propagate_recursive_has_symlinks(conn, parent_id);
        }
    }
}

/// Insert a new entry during `UpsertEntryV2`, with hardlink dedup and delta propagation.
#[allow(clippy::too_many_arguments, reason = "mirrors the DB columns for a new-entry insert")]
fn upsert_insert_new(
    conn: &rusqlite::Connection,
    parent_id: i64,
    name: &str,
    is_directory: bool,
    is_symlink: bool,
    logical_size: Option<u64>,
    physical_size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
    should_dedup: bool,
    next_id: &AtomicI64,
    propagate_deltas: bool,
    signal: &IndexFailureSignal,
) {
    // Dedup: override sizes if another entry already has sizes for this inode
    let (logical_size, physical_size) = if should_dedup
        && IndexStore::has_sized_entry_for_inode(
            conn,
            inode.expect("should_dedup is true only when inode.is_some()"),
            None,
        )
        .unwrap_or(false)
    {
        (None, None)
    } else {
        (logical_size, physical_size)
    };

    match insert_with_allocated_id(
        conn,
        parent_id,
        name,
        is_directory,
        is_symlink,
        logical_size,
        physical_size,
        modified_at,
        inode,
        next_id,
    ) {
        Ok(new_id) => {
            log::trace!("Writer: UpsertEntryV2 inserted \"{name}\" (parent_id={parent_id}) → id={new_id}");
            if is_directory {
                // Initialize empty dir_stats for new directories so enrichment
                // always has a row. Child events will update it incrementally.
                if let Err(e) = IndexStore::upsert_dir_stats_by_id(
                    conn,
                    &[DirStatsById {
                        entry_id: new_id,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 0,
                        recursive_has_symlinks: false,
                        // A live-created dir is unlisted (no scanner walked its
                        // contents), so `0` is correct/honest. A later verifier
                        // scan stamps it and lifts coverage.
                        min_subtree_epoch: 0,
                    }],
                ) {
                    signal.note(&e, &format!("init dir_stats for new dir id={new_id}"));
                }
            }
            // Ancestor propagation. Skipped under a bulk reconcile
            // (`propagate_deltas == false`): the final ComputeAllAggregates
            // recomputes every dir's `dir_stats` from the entries table, so these
            // O(depth) walks per entry would be wasted work. The new-dir
            // zero-valued `dir_stats` row above is still written either way, so
            // enrichment always has a row to read during the walk.
            if propagate_deltas {
                if is_directory {
                    propagate_delta_by_id(conn, parent_id, 0, 0, 0, 1);
                    // The new dir is unlisted (`min_subtree_epoch = 0`), so a new
                    // incomplete subtree now exists: drop every ancestor's coverage
                    // to 0. A later verifier/reconcile scan stamps it and lifts
                    // coverage back. Fire from the parent so the dir's own (correct)
                    // 0 propagates up the chain.
                    propagate_min_subtree_epoch(conn, parent_id);
                } else {
                    let logical = logical_size.unwrap_or(0) as i64;
                    let physical = physical_size.unwrap_or(0) as i64;
                    propagate_delta_by_id(conn, parent_id, logical, physical, 1, 0);
                }
                // New symlink: walk the parent chain and OR in the flag.
                // We start at parent_id so the parent's stats include this symlink.
                if is_symlink {
                    propagate_recursive_has_symlinks(conn, parent_id);
                }
            }
        }
        Err(e) => {
            signal.note(&e, &format!("insert_entry_v2 for {name}"));
        }
    }
}

/// Insert one entry under an id taken from the shared counter, healing a counter
/// that drifted behind the table.
///
/// The counter is the single allocator for `entries.id` (never `MAX(id)`, which
/// double-assigns across uncommitted inserts), but it can still fall behind the
/// table's real `MAX(id)`. The insert then hits
/// `SQLITE_CONSTRAINT_PRIMARYKEY` and, left alone, the entry is dropped from the
/// index forever AND every following insert collides the same way (one incident
/// logged ~9,600 warnings in seconds). So on that specific code we resync the
/// counter from the DB and retry once with a fresh id.
///
/// Only a PRIMARY KEY conflict heals. A `(parent_id, name_folded)` UNIQUE
/// conflict means the name is already in the table, so a retry under a fresh id
/// would insert a duplicate row; it falls through to the caller's error handling
/// (see `IndexStoreError::is_primary_key_conflict`).
#[allow(clippy::too_many_arguments, reason = "mirrors the DB columns for a new-entry insert")]
fn insert_with_allocated_id(
    conn: &rusqlite::Connection,
    parent_id: i64,
    name: &str,
    is_directory: bool,
    is_symlink: bool,
    logical_size: Option<u64>,
    physical_size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
    next_id: &AtomicI64,
) -> Result<i64, IndexStoreError> {
    let insert = |id: i64| {
        IndexStore::insert_entry_v2_with_id(
            conn,
            id,
            parent_id,
            name,
            is_directory,
            is_symlink,
            logical_size,
            physical_size,
            modified_at,
            inode,
        )
    };

    let taken_id = next_id.fetch_add(1, Ordering::Relaxed);
    match insert(taken_id) {
        Err(e) if e.is_primary_key_conflict() => {
            // One line per resync, not per row: the resync puts the counter past
            // the table's MAX, so the retried insert and every later one stop
            // colliding.
            let db_next_id = IndexStore::get_next_id(conn)?;
            let counter_before = next_id.fetch_max(db_next_id, Ordering::Relaxed);
            log::warn!(
                "Index writer: entry-ID counter drifted behind the table (id {taken_id} was already taken); \
                 resyncing {counter_before} → {} and retrying, else this entry would be dropped from the index",
                counter_before.max(db_next_id)
            );
            insert(next_id.fetch_add(1, Ordering::Relaxed))
        }
        other => other,
    }
}

/// Move an existing entry to a new `(parent_id, name)`, preserving its
/// `entry_id` and (for directories) its `dir_stats`.
///
/// Used by the live event loop's rename pre-pass: when an `item_renamed`
/// event arrives whose new path has an inode that already exists in the DB
/// at a *different* `(parent_id, name)`, we rename the row in place rather
/// than going through delete+insert (which would lose `dir_stats`).
///
/// Cross-parent moves subtract the entry's contribution from the old
/// ancestor chain and add it to the new one. Same-parent renames don't
/// change ancestor totals so no propagation runs. The OR-aggregated
/// `recursive_has_symlinks` flag is recomputed both ways for cross-parent
/// moves: the old chain may need to clear it (if this was the last
/// symlink-bearing branch), the new chain may need to set it.
pub(super) fn handle_move_entry_v2(
    conn: &rusqlite::Connection,
    entry_id: i64,
    new_parent_id: i64,
    new_name: String,
    mutation_tracker: &MutationTracker,
    signal: &IndexFailureSignal,
) {
    use crate::indexing::store::normalize_for_comparison;

    let old_entry = match IndexStore::get_entry_by_id(conn, entry_id) {
        Ok(Some(e)) => e,
        Ok(None) => {
            log::debug!(target: "indexing::writer", "MoveEntryV2: entry id={entry_id} no longer exists, skipping");
            return;
        }
        Err(e) => {
            signal.note(&e, &format!("MoveEntryV2 get_entry_by_id({entry_id})"));
            return;
        }
    };

    // Defensive no-op when the move would be a no-op anyway. Compares names
    // by their folded form so a rename that only changes case-folding
    // (e.g. NFD vs NFC on macOS) doesn't trigger spurious propagation.
    if old_entry.parent_id == new_parent_id
        && normalize_for_comparison(&old_entry.name) == normalize_for_comparison(&new_name)
    {
        log::debug!(
            target: "indexing::writer",
            "MoveEntryV2: id={entry_id} already at target (parent_id={new_parent_id}, name={new_name}), no-op",
        );
        return;
    }

    // A different entry can already occupy the destination (parent_id, name_folded): the move
    // overwrote an existing file, or a concurrent upsert raced ahead of this message. On disk
    // the moved entry owns that name now, so delete the conflicting row first (subtree-aware,
    // with delta propagation); without this the UPDATE below fails the UNIQUE constraint and
    // the moved entry stays stuck at its old location until verification heals it.
    let new_name_folded = normalize_for_comparison(&new_name);
    match IndexStore::resolve_component(conn, new_parent_id, &new_name) {
        Ok(Some(conflicting_id)) if conflicting_id != entry_id => {
            log::debug!(
                target: "indexing::writer",
                "MoveEntryV2: id={conflicting_id} already at destination (parent_id={new_parent_id}, name={new_name}), replacing it with id={entry_id}",
            );
            let conflicting_is_dir = IndexStore::get_entry_by_id(conn, conflicting_id)
                .ok()
                .flatten()
                .map(|e| e.is_directory)
                .unwrap_or(false);
            // Move is a live-only message (never part of a bulk reconcile, which
            // emits only Upsert/Delete), so its internal deletes always propagate.
            if conflicting_is_dir {
                handle_delete_subtree_by_id(conn, conflicting_id, true, mutation_tracker, signal);
            } else {
                handle_delete_entry_by_id(conn, conflicting_id, true, mutation_tracker, signal);
            }
        }
        Ok(_) => {}
        Err(e) => {
            signal.note(&e, &format!("MoveEntryV2 destination lookup for id={entry_id}"));
            return;
        }
    }
    if let Err(e) = conn.execute(
        "UPDATE entries SET parent_id = ?1, name = ?2, name_folded = ?3 WHERE id = ?4",
        rusqlite::params![new_parent_id, new_name, new_name_folded, entry_id],
    ) {
        signal.note(
            &IndexStoreError::from(e),
            &format!("MoveEntryV2 update for id={entry_id}"),
        );
        return;
    }

    log::debug!(
        target: "indexing::writer",
        "MoveEntryV2: id={entry_id} \"{}\" → \"{}\" (parent_id {} → {})",
        old_entry.name,
        new_name,
        old_entry.parent_id,
        new_parent_id,
    );

    // Same-parent rename: ancestor totals unchanged, just the row's name moved.
    if old_entry.parent_id == new_parent_id {
        mutation_tracker.bump();
        return;
    }

    // Cross-parent move: subtract from the old chain, add to the new chain.
    let (logical_delta, physical_delta, file_delta, dir_delta) = if old_entry.is_directory {
        let totals = IndexStore::get_dir_stats_by_id(conn, entry_id).ok().flatten();
        let (logical, physical, files, dirs) = match totals {
            Some(s) => (
                s.recursive_logical_size as i64,
                s.recursive_physical_size as i64,
                s.recursive_file_count as i64,
                s.recursive_dir_count as i64,
            ),
            None => (0, 0, 0, 0),
        };
        // The directory itself contributes one to the dir count of every ancestor.
        (logical, physical, files as i32, (dirs + 1) as i32)
    } else {
        (
            old_entry.logical_size.unwrap_or(0) as i64,
            old_entry.physical_size.unwrap_or(0) as i64,
            1,
            0,
        )
    };

    propagate_delta_by_id(
        conn,
        old_entry.parent_id,
        -logical_delta,
        -physical_delta,
        -file_delta,
        -dir_delta,
    );
    propagate_delta_by_id(
        conn,
        new_parent_id,
        logical_delta,
        physical_delta,
        file_delta,
        dir_delta,
    );

    // The `recursive_has_symlinks` flag may flip on either chain. The old
    // chain might lose its only symlink-bearing descendant; the new chain
    // might gain one. `propagate_recursive_has_symlinks` is monotonic on
    // additions and recomputes correctly on removals, so calling it on both
    // is safe and stops walking as soon as a value stabilizes.
    if old_entry.is_symlink {
        propagate_recursive_has_symlinks(conn, old_entry.parent_id);
        propagate_recursive_has_symlinks(conn, new_parent_id);
    } else if old_entry.is_directory {
        let had_symlinks = IndexStore::get_dir_stats_by_id(conn, entry_id)
            .ok()
            .flatten()
            .map(|s| s.recursive_has_symlinks)
            .unwrap_or(false);
        if had_symlinks {
            propagate_recursive_has_symlinks(conn, old_entry.parent_id);
            propagate_recursive_has_symlinks(conn, new_parent_id);
        }
    }

    // Coverage changes on BOTH ancestor chains when a (possibly incomplete)
    // subtree moves: the old chain may RISE (the incomplete child left), the new
    // chain may DROP (it arrived). The moved subtree's own `min_subtree_epoch` is
    // unchanged (it moved intact), so recompute only the two ancestor chains —
    // mirroring the dual-chain `recursive_has_symlinks` recompute above.
    if old_entry.is_directory {
        propagate_min_subtree_epoch(conn, old_entry.parent_id);
        propagate_min_subtree_epoch(conn, new_parent_id);
    }

    mutation_tracker.bump();
}

pub(super) fn handle_delete_entry_by_id(
    conn: &rusqlite::Connection,
    entry_id: i64,
    propagate_deltas: bool,
    mutation_tracker: &MutationTracker,
    signal: &IndexFailureSignal,
) {
    // Read old entry before deleting to get accurate delta
    let old_entry = IndexStore::get_entry_by_id(conn, entry_id).ok().flatten();
    if let Err(e) = IndexStore::delete_entry_by_id(conn, entry_id) {
        signal.note(&e, &format!("delete_entry_by_id id={entry_id}"));
    }
    // Auto-propagate accurate negative delta via parent_id chain. Skipped under a
    // bulk reconcile (`propagate_deltas == false`): the final ComputeAllAggregates
    // recomputes ancestor stats from the entries table, so this walk is wasted.
    if let Some(entry) = old_entry
        && propagate_deltas
    {
        let (logical_delta, physical_delta, file_delta, dir_delta) = if entry.is_directory {
            (0i64, 0i64, 0i32, -1i32)
        } else {
            (
                -(entry.logical_size.unwrap_or(0) as i64),
                -(entry.physical_size.unwrap_or(0) as i64),
                -1,
                0,
            )
        };
        propagate_delta_by_id(
            conn,
            entry.parent_id,
            logical_delta,
            physical_delta,
            file_delta,
            dir_delta,
        );
        // If we just deleted a symlink, the parent's `recursive_has_symlinks`
        // may flip back to false (and propagate further up).
        if entry.is_symlink {
            propagate_recursive_has_symlinks(conn, entry.parent_id);
        }
        // Removing a directory can RAISE the parent's coverage (its incomplete
        // child is gone). Deleting a file never changes coverage. Fire only for
        // a directory removal.
        if entry.is_directory {
            propagate_min_subtree_epoch(conn, entry.parent_id);
        }
    }
    mutation_tracker.bump();
}

pub(super) fn handle_delete_subtree_by_id(
    conn: &rusqlite::Connection,
    root_id: i64,
    propagate_deltas: bool,
    mutation_tracker: &MutationTracker,
    signal: &IndexFailureSignal,
) {
    // Read subtree totals before deleting to get accurate delta
    let totals = IndexStore::get_subtree_totals_by_id(conn, root_id).ok();
    let parent_id = IndexStore::get_parent_id(conn, root_id).ok().flatten();
    // Did the subtree contain any symlinks? Read the root's stored flag before
    // deletion (covers descendants), and also check any direct symlink children.
    let subtree_had_symlinks = {
        let from_root = IndexStore::get_dir_stats_by_id(conn, root_id)
            .ok()
            .flatten()
            .map(|s| s.recursive_has_symlinks)
            .unwrap_or(false);
        if from_root {
            true
        } else {
            // The root itself might be a symlink (rare), or a child might be one
            // without dir_stats covering it. Check directly.
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM entries WHERE id = ?1 AND is_symlink = 1)",
                rusqlite::params![root_id],
                |row| row.get::<_, i32>(0).map(|n| n != 0),
            )
            .unwrap_or(false)
        }
    };
    if let Err(e) = IndexStore::delete_subtree_by_id(conn, root_id) {
        signal.note(&e, &format!("delete_subtree_by_id id={root_id}"));
    }
    // Auto-propagate accurate negative delta via parent_id chain. Skipped under a
    // bulk reconcile (`propagate_deltas == false`): the final ComputeAllAggregates
    // recomputes ancestor stats from the entries table, so this walk is wasted.
    if let (Some((logical_size, physical_size, file_count, dir_count)), Some(pid)) = (totals, parent_id)
        && propagate_deltas
    {
        propagate_delta_by_id(
            conn,
            pid,
            -(logical_size as i64),
            -(physical_size as i64),
            -(file_count as i32),
            -(dir_count as i32),
        );
        // If the deleted subtree contained any symlinks, the parent's
        // `recursive_has_symlinks` may flip, so recompute up the chain.
        if subtree_had_symlinks {
            propagate_recursive_has_symlinks(conn, pid);
        }
        // The removed subtree may have been incomplete (`min_subtree_epoch = 0`);
        // its removal can RAISE the parent's coverage, so recompute up the chain.
        propagate_min_subtree_epoch(conn, pid);
    }
    mutation_tracker.bump();
}

pub(super) fn handle_truncate_data(
    conn: &rusqlite::Connection,
    accumulator: &mut AccumulatorMaps,
    expected_total_entries: &AtomicU64,
    next_id: &AtomicI64,
    mutation_tracker: &MutationTracker,
    signal: &IndexFailureSignal,
) {
    accumulator.clear();
    expected_total_entries.store(0, Ordering::Relaxed);
    let t = Instant::now();
    match conn.execute_batch(
        "DELETE FROM dir_stats; DELETE FROM entries; INSERT OR IGNORE INTO entries (id, parent_id, name, is_directory, is_symlink) VALUES (1, 0, '', 1, 0);",
    ) {
        Ok(()) => {
            // Root sentinel is id=1, so next assignable ID is 2
            next_id.store(2, Ordering::Relaxed);
            log::info!(
                "Writer: truncated entries + dir_stats ({}ms)",
                t.elapsed().as_millis(),
            );
            // Reclaim free pages from the truncation
            if let Err(e) = crate::sqlite_util::run_incremental_vacuum(conn, None) {
                log::warn!("Writer: incremental_vacuum after truncate failed: {e}");
            }
        }
        Err(e) => {
            signal.note(&IndexStoreError::from(e), "truncate");
        }
    }
    mutation_tracker.bump();
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
