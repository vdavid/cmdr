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

use crate::indexing::aggregator::AggregationPhase;
use crate::indexing::store::{DirStatsById, EntryRow, IndexStore};
use crate::pluralize::pluralize_with;

use super::delta::{propagate_delta_by_id, propagate_recursive_has_symlinks};
use super::{AccumulatorMaps, AggregationProgressEvent, bump_generation, phase_to_str};

pub(super) fn handle_insert_entries_v2(
    conn: &rusqlite::Connection,
    entries: Vec<EntryRow>,
    accumulator: &mut AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    expected_total_entries: &AtomicU64,
    mutation_counter: &AtomicU64,
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
        Err(e) => crate::log_error!("Index writer: insert_entries_v2_batch failed: {e}"),
    }
    let elapsed = t.elapsed().as_millis();
    if elapsed > 100 {
        log::debug!(
            "Writer: insert_entries_v2_batch ({}) took {elapsed}ms",
            pluralize_with(count as u64, "entry", "entries")
        );
    }
    bump_generation(mutation_counter);
    // Emit flushing progress when we know the expected total
    let expected = expected_total_entries.load(Ordering::Relaxed);
    if expected > 0
        && let Some(app) = app_handle
    {
        let _ = AggregationProgressEvent {
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
    mutation_counter: &AtomicU64,
) {
    // Hardlink dedup: if this file has nlink > 1, check whether another entry
    // for the same inode already has non-NULL sizes. If so, override sizes to
    // None so each inode's bytes are counted exactly once.
    let should_dedup = inode.is_some() && matches!(nlink, Some(n) if n > 1) && logical_size.is_some();

    // Check if an entry already exists at (parent_id, name).
    // Auto-propagates size deltas to ancestor dir_stats on both
    // insert and update, so callers never need a separate
    // PropagateDeltaById for upserted entries.
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
                    handle_delete_subtree_by_id(conn, existing_id, mutation_counter);
                } else {
                    handle_delete_entry_by_id(conn, existing_id, mutation_counter);
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
            );
        }
        Err(e) => {
            log::warn!("Index writer: resolve_component failed for {name}: {e}");
        }
    }
    bump_generation(mutation_counter);
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
        log::warn!("Index writer: update_entry failed for id={existing_id}: {e}");
    } else if let Some(old) = old_entry {
        // Propagate size delta if anything changed
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
) {
    // Dedup: override sizes if another entry already has sizes for this inode
    let (logical_size, physical_size) =
        if should_dedup
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

    let new_entry_id = next_id.fetch_add(1, Ordering::Relaxed);
    match IndexStore::insert_entry_v2_with_id(
        conn,
        new_entry_id,
        parent_id,
        name,
        is_directory,
        is_symlink,
        logical_size,
        physical_size,
        modified_at,
        inode,
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
                    }],
                ) {
                    log::warn!("Writer: init dir_stats for new dir id={new_id} failed: {e}");
                }
                propagate_delta_by_id(conn, parent_id, 0, 0, 0, 1);
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
        Err(e) => {
            log::warn!("Index writer: insert_entry_v2 failed for {name}: {e}");
        }
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
    mutation_counter: &AtomicU64,
) {
    use crate::indexing::store::normalize_for_comparison;

    let old_entry = match IndexStore::get_entry_by_id(conn, entry_id) {
        Ok(Some(e)) => e,
        Ok(None) => {
            log::debug!(target: "indexing::writer", "MoveEntryV2: entry id={entry_id} no longer exists, skipping");
            return;
        }
        Err(e) => {
            log::warn!("Index writer: MoveEntryV2 get_entry_by_id({entry_id}) failed: {e}");
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
            if conflicting_is_dir {
                handle_delete_subtree_by_id(conn, conflicting_id, mutation_counter);
            } else {
                handle_delete_entry_by_id(conn, conflicting_id, mutation_counter);
            }
        }
        Ok(_) => {}
        Err(e) => {
            log::warn!("Index writer: MoveEntryV2 destination lookup failed for id={entry_id}: {e}");
            return;
        }
    }
    if let Err(e) = conn.execute(
        "UPDATE entries SET parent_id = ?1, name = ?2, name_folded = ?3 WHERE id = ?4",
        rusqlite::params![new_parent_id, new_name, new_name_folded, entry_id],
    ) {
        log::warn!("Index writer: MoveEntryV2 update failed for id={entry_id}: {e}");
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
        bump_generation(mutation_counter);
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

    bump_generation(mutation_counter);
}

pub(super) fn handle_delete_entry_by_id(conn: &rusqlite::Connection, entry_id: i64, mutation_counter: &AtomicU64) {
    // Read old entry before deleting to get accurate delta
    let old_entry = IndexStore::get_entry_by_id(conn, entry_id).ok().flatten();
    if let Err(e) = IndexStore::delete_entry_by_id(conn, entry_id) {
        log::warn!("Index writer: delete_entry_by_id failed for id={entry_id}: {e}");
    }
    // Auto-propagate accurate negative delta via parent_id chain
    if let Some(entry) = old_entry {
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
    }
    bump_generation(mutation_counter);
}

pub(super) fn handle_delete_subtree_by_id(conn: &rusqlite::Connection, root_id: i64, mutation_counter: &AtomicU64) {
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
        log::warn!("Index writer: delete_subtree_by_id failed for id={root_id}: {e}");
    }
    // Auto-propagate accurate negative delta via parent_id chain
    if let (Some((logical_size, physical_size, file_count, dir_count)), Some(pid)) = (totals, parent_id) {
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
    }
    bump_generation(mutation_counter);
}

pub(super) fn handle_truncate_data(
    conn: &rusqlite::Connection,
    accumulator: &mut AccumulatorMaps,
    expected_total_entries: &AtomicU64,
    next_id: &AtomicI64,
    mutation_counter: &AtomicU64,
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
            if let Err(e) = conn.execute_batch("PRAGMA incremental_vacuum;") {
                log::warn!("Writer: incremental_vacuum after truncate failed: {e}");
            }
        }
        Err(e) => log::warn!("Writer: truncate failed: {e}"),
    }
    bump_generation(mutation_counter);
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::indexing::store::ROOT_ID;
    use crate::indexing::writer::tests::{open_read, setup_db};
    use crate::indexing::writer::{IndexWriter, WriteMessage};

    // ── Integer-keyed variant tests ──────────────────────────────────

    #[test]
    fn insert_entries_v2_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "file.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1024),
            physical_size: Some(1024),
            modified_at: Some(1700000000),
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        let store = open_read(&db_path);
        let children = store.list_children(ROOT_ID).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "file.txt");
        assert_eq!(children[0].logical_size, Some(1024));
        assert_eq!(children[0].id, 10);

        writer.shutdown();
    }

    // The accumulator must only count rows that actually landed in the DB.
    // `insert_entries_v2_batch` uses `INSERT OR IGNORE`, so one duplicate in
    // a batch skips just that row and the rest insert. The accumulator maps
    // drive `compute_all_aggregates_with_maps`; counting bytes for a row that
    // lost an OR-IGNORE produces inflated dir_stats (this was one of the
    // mechanisms behind the 1.83 TB ghost size on `..` of a 994 GB volume).
    #[test]
    fn handle_insert_entries_v2_only_accumulates_rows_that_landed() {
        use std::sync::atomic::AtomicU64;

        let (db_path, _dir) = setup_db();
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Pre-seed: id=100, name="first.txt".
        let entries_first = vec![EntryRow {
            id: 100,
            parent_id: ROOT_ID,
            name: "first.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(10),
            physical_size: Some(10),
            modified_at: None,
            inode: None,
        }];
        IndexStore::insert_entries_v2_batch(&conn, &entries_first).unwrap();

        // Second batch: row 0 collides on the (parent_id, name_folded) UNIQUE
        // index (same `first.txt` under ROOT_ID). Row 1 is fresh and must land.
        let entries_dup = vec![
            EntryRow {
                id: 200,
                parent_id: ROOT_ID,
                name: "first.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(999_999),
                physical_size: Some(999_999),
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 101,
                parent_id: ROOT_ID,
                name: "second.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(20),
                physical_size: Some(20),
                modified_at: None,
                inode: None,
            },
        ];

        let mut accumulator = AccumulatorMaps::new();
        let expected = AtomicU64::new(0);
        let mutation_counter = AtomicU64::new(0);

        handle_insert_entries_v2(
            &conn,
            entries_dup,
            &mut accumulator,
            &None,
            &expected,
            &mutation_counter,
        );

        // DB has the original first.txt (id=100) and the new second.txt (id=101).
        // id=200 was the OR-IGNORE'd duplicate and must not be in the DB.
        assert_eq!(
            IndexStore::get_entry_by_id(&conn, 100).unwrap().unwrap().name,
            "first.txt"
        );
        assert_eq!(
            IndexStore::get_entry_by_id(&conn, 101).unwrap().unwrap().name,
            "second.txt"
        );
        assert!(IndexStore::get_entry_by_id(&conn, 200).unwrap().is_none());

        // Accumulator must reflect exactly one new entry (the row that landed),
        // never the 999_999-byte phantom. If a regression makes the accumulator
        // count the OR-IGNORE'd row, this assert catches it.
        assert_eq!(
            accumulator.entries_inserted, 1,
            "accumulator must count only rows that landed in the DB"
        );
        let stats = accumulator.direct_stats.get(&ROOT_ID).expect("ROOT_ID stats present");
        assert_eq!(stats.0, 20, "logical bytes must only count the landed row");
        assert_eq!(stats.1, 20, "physical bytes must only count the landed row");
        assert_eq!(stats.2, 1, "file count must only include the landed row");
    }

    #[test]
    fn upsert_entry_v2_insert_and_update() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert via UpsertEntryV2 (entry doesn't exist yet)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "new.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(256),
                physical_size: Some(256),
                modified_at: Some(1700000000),
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Update via UpsertEntryV2 (entry now exists)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "new.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(512),
                physical_size: Some(512),
                modified_at: Some(1700000001),
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let store = open_read(&db_path);
        let children = store.list_children(ROOT_ID).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "new.txt");
        assert_eq!(children[0].logical_size, Some(512), "size should be updated to 512");

        writer.shutdown();
    }

    #[test]
    fn upsert_entry_v2_initializes_dir_stats_for_new_dirs() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a new directory via UpsertEntryV2
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "newdir".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // The new directory should have a zero-valued dir_stats row
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let dir_id = IndexStore::resolve_component(&conn, ROOT_ID, "newdir")
            .unwrap()
            .expect("newdir should exist");

        let stats = IndexStore::get_dir_stats_by_id(&conn, dir_id).unwrap();
        assert!(stats.is_some(), "new dir should have dir_stats");
        let stats = stats.unwrap();
        assert_eq!(stats.recursive_logical_size, 0);
        assert_eq!(stats.recursive_file_count, 0);
        assert_eq!(stats.recursive_dir_count, 0);

        writer.shutdown();
    }

    #[test]
    fn delete_entry_by_id_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert an entry
        let entries = vec![EntryRow {
            id: 20,
            parent_id: ROOT_ID,
            name: "doomed.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(100),
            physical_size: Some(100),
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        // Delete by ID
        writer.send(WriteMessage::DeleteEntryById(20)).unwrap();
        writer.flush_blocking().unwrap();

        let store = open_read(&db_path);
        let children = store.list_children(ROOT_ID).unwrap();
        assert!(children.is_empty(), "entry should be deleted");

        writer.shutdown();
    }

    #[test]
    fn delete_subtree_by_id_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Build a tree: ROOT -> dir(10) -> file(11) + subdir(12)
        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "a".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "b.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(50),
                physical_size: Some(50),
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 12,
                parent_id: 10,
                name: "c".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        // Delete the subtree rooted at id=10
        writer.send(WriteMessage::DeleteSubtreeById(10)).unwrap();
        writer.flush_blocking().unwrap();

        let store = open_read(&db_path);
        let root_children = store.list_children(ROOT_ID).unwrap();
        assert!(root_children.is_empty(), "dir /a should be deleted");
        let a_children = store.list_children(10).unwrap();
        assert!(a_children.is_empty(), "children of /a should be deleted");

        writer.shutdown();
    }

    #[test]
    fn delete_entry_by_id_auto_propagates_delta() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a parent dir and a file
        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "p".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();

        // Pre-populate dir_stats for the parent
        writer.flush_blocking().unwrap();

        // Manually set dir_stats for parent via direct DB write (using the by-id API)
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 500,
                    recursive_physical_size: 500,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Delete the file: writer should auto-propagate (-500, -1, 0) to parent id=10
        writer.send(WriteMessage::DeleteEntryById(11)).unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(stats.recursive_logical_size, 0, "size should be 0 after file deletion");
        assert_eq!(stats.recursive_file_count, 0, "file count should be 0");

        writer.shutdown();
    }

    #[test]
    fn delete_subtree_by_id_auto_propagates_delta() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Build tree: ROOT(1) -> root_dir(10) -> sub(11) -> file.txt(12, 300 bytes)
        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "root".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "sub".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 12,
                parent_id: 11,
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(300),
                physical_size: Some(300),
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        // Pre-populate dir_stats for ancestors
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[
                    DirStatsById {
                        entry_id: ROOT_ID,
                        recursive_logical_size: 300,
                        recursive_physical_size: 300,
                        recursive_file_count: 1,
                        recursive_dir_count: 2,
                        recursive_has_symlinks: false,
                    },
                    DirStatsById {
                        entry_id: 10,
                        recursive_logical_size: 300,
                        recursive_physical_size: 300,
                        recursive_file_count: 1,
                        recursive_dir_count: 1,
                        recursive_has_symlinks: false,
                    },
                ],
            )
            .unwrap();
        }

        // Delete the /root/sub subtree (id=11)
        writer.send(WriteMessage::DeleteSubtreeById(11)).unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // root_dir(10) should have lost: size=300, files=1, dirs=1
        let root_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(root_stats.recursive_logical_size, 0);
        assert_eq!(root_stats.recursive_file_count, 0);
        assert_eq!(root_stats.recursive_dir_count, 0);

        // ROOT(1) should have lost: size=300, files=1, dirs=1
        let vol_stats = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
        assert_eq!(vol_stats.recursive_logical_size, 0);
        assert_eq!(vol_stats.recursive_file_count, 0);
        assert_eq!(vol_stats.recursive_dir_count, 1); // root_dir(10) still exists

        writer.shutdown();
    }

    #[test]
    fn delete_entry_by_id_for_nonexistent_skips_propagation() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a directory and pre-populate its dir_stats
        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "p".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 100,
                    recursive_physical_size: 100,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Delete a non-existent entry: should not propagate any delta
        writer.send(WriteMessage::DeleteEntryById(999)).unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(stats.recursive_logical_size, 100, "stats should be unchanged");
        assert_eq!(stats.recursive_file_count, 1);

        writer.shutdown();
    }

    #[test]
    fn upsert_entry_v2_auto_propagates_delta_on_insert() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a parent directory and pre-populate its dir_stats
        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "home".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 0,
                    recursive_physical_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Insert a new file via UpsertEntryV2: should auto-propagate to parent
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "doc.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: Some(1700000000),
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(stats.recursive_logical_size, 500, "parent should have file's size");
        assert_eq!(stats.recursive_file_count, 1, "parent should count the new file");
        assert_eq!(stats.recursive_dir_count, 0);

        writer.shutdown();
    }

    #[test]
    fn upsert_entry_v2_auto_propagates_delta_on_update() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert parent dir with dir_stats
        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "home".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 200,
                    recursive_physical_size: 200,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Insert a file via UpsertEntryV2
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "doc.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(200),
                physical_size: Some(200),
                modified_at: Some(1700000000),
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Update the same file with a larger size: should propagate +100 delta
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "doc.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(300),
                physical_size: Some(300),
                modified_at: Some(1700000001),
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        // Initial 200 + insert propagated 200 + update propagated +100 = 500
        assert_eq!(
            stats.recursive_logical_size, 500,
            "parent should reflect insert + update deltas"
        );
        assert_eq!(stats.recursive_file_count, 2, "file_count: 1 initial + 1 from insert");

        writer.shutdown();
    }

    #[test]
    fn upsert_entry_v2_auto_propagates_dir_count_on_new_dir() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Pre-populate root dir_stats
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: ROOT_ID,
                    recursive_logical_size: 0,
                    recursive_physical_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Insert a new directory via UpsertEntryV2
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "projects".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
        assert_eq!(stats.recursive_dir_count, 1, "root should count the new dir");
        assert_eq!(stats.recursive_file_count, 0);
        assert_eq!(stats.recursive_logical_size, 0);

        writer.shutdown();
    }

    // ── Hardlink dedup tests ────────────────────────────────────────

    #[test]
    fn hardlink_dedup_insert_primary_stores_sizes_and_inode() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "primary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let id = IndexStore::resolve_component(&conn, ROOT_ID, "primary.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(entry.logical_size, Some(1000), "primary should keep its sizes");
        assert_eq!(entry.inode, Some(100), "inode should be stored");

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_insert_secondary_gets_null_sizes() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert primary link
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "primary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Insert secondary link (same inode, different name)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let sec_id = IndexStore::resolve_component(&conn, ROOT_ID, "secondary.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, sec_id).unwrap().unwrap();
        assert_eq!(entry.logical_size, None, "secondary should have NULL sizes");
        assert_eq!(entry.physical_size, None);
        assert_eq!(entry.inode, Some(100), "inode should still be stored");

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_update_secondary_keeps_null_sizes() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert primary
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "primary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Insert secondary (gets NULL sizes via dedup)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Reconciler sends update for secondary with full sizes: dedup should fire again
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000001),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let sec_id = IndexStore::resolve_component(&conn, ROOT_ID, "secondary.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, sec_id).unwrap().unwrap();
        assert_eq!(
            entry.logical_size, None,
            "secondary sizes should stay NULL after update"
        );

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_self_healing_after_primary_deleted() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Pre-populate root dir_stats so delta propagation works
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: ROOT_ID,
                    recursive_logical_size: 0,
                    recursive_physical_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Insert primary
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "primary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Insert secondary (gets NULL sizes)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Delete primary
        let primary_id = {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::resolve_component(&conn, ROOT_ID, "primary.txt")
                .unwrap()
                .unwrap()
        };
        writer.send(WriteMessage::DeleteEntryById(primary_id)).unwrap();
        writer.flush_blocking().unwrap();

        // Reconciler sends update for secondary: nlink=1 since it's the only link now
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000001),
                inode: Some(100),
                nlink: Some(1),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let sec_id = IndexStore::resolve_component(&conn, ROOT_ID, "secondary.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, sec_id).unwrap().unwrap();
        assert_eq!(
            entry.logical_size,
            Some(1000),
            "secondary should recover sizes after primary deleted"
        );
        assert_eq!(entry.physical_size, Some(1000));

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_nlink_1_skips_dedup() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert two files with the same inode but nlink=1 (not actually hardlinked)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "file_a.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: None,
                inode: Some(200),
                nlink: Some(1),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "file_b.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: None,
                inode: Some(200),
                nlink: Some(1),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let b_id = IndexStore::resolve_component(&conn, ROOT_ID, "file_b.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, b_id).unwrap().unwrap();
        assert_eq!(entry.logical_size, Some(500), "nlink=1 should never trigger dedup");

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_no_inode_skips_dedup() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert first file with inode
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "file_a.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Insert second file with no inode (non-Unix)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "file_b.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let b_id = IndexStore::resolve_component(&conn, ROOT_ID, "file_b.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, b_id).unwrap().unwrap();
        assert_eq!(entry.logical_size, Some(500), "no inode should never trigger dedup");

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_dir_stats_only_counts_primary_size() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a parent directory and pre-populate its dir_stats
        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "mydir".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 0,
                    recursive_physical_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Insert primary hardlink into dir
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "primary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: None,
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Insert secondary hardlink into dir (same inode)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: None,
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(
            stats.recursive_logical_size, 1000,
            "dir should only count the primary's size"
        );
        assert_eq!(stats.recursive_file_count, 2, "both links count as files");

        writer.shutdown();
    }

    // ── recursive_has_symlinks tests ─────────────────────────────────

    #[test]
    fn upsert_symlink_propagates_recursive_has_symlinks_up() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Build a 2-level dir tree first (no symlinks).
        // ROOT -> outer (id=10) -> inner (id=11)
        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "outer".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "inner".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        // Confirm baseline: no symlinks anywhere
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                !IndexStore::get_dir_stats_by_id(&conn, 11)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks
            );
            assert!(
                !IndexStore::get_dir_stats_by_id(&conn, 10)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks
            );
        }

        // Add a symlink under inner via UpsertEntryV2
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 11,
                name: "link".into(),
                is_directory: false,
                is_symlink: true,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Flag should propagate up to both inner and outer
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                IndexStore::get_dir_stats_by_id(&conn, 11)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks,
                "inner should flip to true"
            );
            assert!(
                IndexStore::get_dir_stats_by_id(&conn, 10)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks,
                "outer should propagate from inner"
            );
        }

        writer.shutdown();
    }

    #[test]
    fn delete_last_symlink_clears_recursive_has_symlinks_up() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT -> outer (id=20) -> link (id=21, symlink)
        let entries = vec![
            EntryRow {
                id: 20,
                parent_id: ROOT_ID,
                name: "outer".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 21,
                parent_id: 20,
                name: "link".into(),
                is_directory: false,
                is_symlink: true,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        // Baseline: outer has the flag set
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                IndexStore::get_dir_stats_by_id(&conn, 20)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks
            );
        }

        // Delete the only symlink
        writer.send(WriteMessage::DeleteEntryById(21)).unwrap();
        writer.flush_blocking().unwrap();

        // Flag should clear up the chain
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                !IndexStore::get_dir_stats_by_id(&conn, 20)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks,
                "outer should clear after last symlink removed"
            );
        }

        writer.shutdown();
    }

    #[test]
    fn delete_subtree_with_symlinks_clears_parent_flag() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT -> top (id=30)
        //   ├── doomed (id=31) -> link (id=32, symlink)
        //   └── safe (id=33)  (no symlinks)
        let entries = vec![
            EntryRow {
                id: 30,
                parent_id: ROOT_ID,
                name: "top".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 31,
                parent_id: 30,
                name: "doomed".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 32,
                parent_id: 31,
                name: "link".into(),
                is_directory: false,
                is_symlink: true,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 33,
                parent_id: 30,
                name: "safe".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        // Baseline: top has the flag
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                IndexStore::get_dir_stats_by_id(&conn, 30)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks
            );
        }

        // Delete the doomed subtree (which contained the only symlink)
        writer.send(WriteMessage::DeleteSubtreeById(31)).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                !IndexStore::get_dir_stats_by_id(&conn, 30)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks,
                "top should clear once the subtree containing the symlink is gone"
            );
        }

        writer.shutdown();
    }

    // ── MoveEntryV2 tests ────────────────────────────────────────────

    /// Helper: insert a dir with dir_stats. Returns nothing (the caller knows the id it asked for).
    fn insert_dir_with_stats(
        writer: &IndexWriter,
        db_path: &Path,
        id: i64,
        parent_id: i64,
        name: &str,
        stats: DirStatsById,
    ) {
        writer
            .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
                id,
                parent_id,
                name: name.into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            }]))
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(db_path).unwrap();
        IndexStore::upsert_dir_stats_by_id(&conn, &[stats]).unwrap();
    }

    #[test]
    fn move_entry_v2_same_parent_preserves_dir_stats() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Parent dir + child dir with non-trivial dir_stats. The whole point
        // of MoveEntryV2 vs. delete+insert is preserving these numbers.
        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "home",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 5_000,
                recursive_physical_size: 5_000,
                recursive_file_count: 7,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            20,
            10,
            "Foo",
            DirStatsById {
                entry_id: 20,
                recursive_logical_size: 5_000,
                recursive_physical_size: 5_000,
                recursive_file_count: 7,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );

        // Same-parent rename: "Foo" → "Bar".
        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 20,
                new_parent_id: 10,
                new_name: "Bar".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(entry.name, "Bar", "name should be updated");
        assert_eq!(entry.parent_id, 10, "parent unchanged");

        let moved_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(
            moved_stats.recursive_logical_size, 5_000,
            "moved dir keeps its own stats"
        );
        assert_eq!(moved_stats.recursive_file_count, 7);

        let parent_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(
            parent_stats.recursive_logical_size, 5_000,
            "parent stats unchanged for same-parent rename"
        );
        assert_eq!(parent_stats.recursive_file_count, 7);
        assert_eq!(parent_stats.recursive_dir_count, 1);

        writer.shutdown();
    }

    /// Helper: insert a plain file row.
    fn insert_file(writer: &IndexWriter, id: i64, parent_id: i64, name: &str, size: u64) {
        writer
            .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
                id,
                parent_id,
                name: name.into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(size),
                physical_size: Some(size),
                modified_at: None,
                inode: None,
            }]))
            .unwrap();
        writer.flush_blocking().unwrap();
    }

    #[test]
    fn move_entry_v2_destination_collision_replaces_conflicting_file() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // One dir with two files. Moving "draft.txt" onto "final.txt"'s name
        // (a rename-with-overwrite, or a concurrent upsert racing ahead of the
        // move) used to fail the UNIQUE (parent_id, name_folded) constraint and
        // leave the moved entry stuck at its old name.
        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "docs",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 150,
                recursive_physical_size: 150,
                recursive_file_count: 2,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        insert_file(&writer, 20, 10, "draft.txt", 100);
        insert_file(&writer, 21, 10, "final.txt", 50);

        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 20,
                new_parent_id: 10,
                new_name: "final.txt".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let moved = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(moved.name, "final.txt", "moved entry owns the destination name");
        assert_eq!(moved.parent_id, 10);
        assert!(
            IndexStore::get_entry_by_id(&conn, 21).unwrap().is_none(),
            "conflicting entry is deleted"
        );

        // The conflicting file's contribution is subtracted from the parent.
        let parent_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(parent_stats.recursive_logical_size, 100);
        assert_eq!(parent_stats.recursive_file_count, 1);

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_destination_collision_replaces_conflicting_dir_subtree() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // A/proj (id 20, rich dir_stats) moves to B/proj, but B already has a
        // stale dir row "proj" (id 21) with a child file. The stale subtree must
        // go and the moved dir must keep its id and dir_stats.
        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "A",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 1000,
                recursive_physical_size: 1000,
                recursive_file_count: 3,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            11,
            ROOT_ID,
            "B",
            DirStatsById {
                entry_id: 11,
                recursive_logical_size: 500,
                recursive_physical_size: 500,
                recursive_file_count: 1,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            20,
            10,
            "proj",
            DirStatsById {
                entry_id: 20,
                recursive_logical_size: 1000,
                recursive_physical_size: 1000,
                recursive_file_count: 3,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            21,
            11,
            "proj",
            DirStatsById {
                entry_id: 21,
                recursive_logical_size: 500,
                recursive_physical_size: 500,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        insert_file(&writer, 22, 21, "old.txt", 500);

        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 20,
                new_parent_id: 11,
                new_name: "proj".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let moved = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(moved.parent_id, 11, "moved dir landed under B");
        assert_eq!(moved.name, "proj");
        assert!(
            IndexStore::get_entry_by_id(&conn, 21).unwrap().is_none(),
            "conflicting dir is deleted"
        );
        assert!(
            IndexStore::get_entry_by_id(&conn, 22).unwrap().is_none(),
            "conflicting dir's children are deleted"
        );

        let moved_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(
            moved_stats.recursive_logical_size, 1000,
            "moved dir keeps its own stats"
        );
        assert_eq!(moved_stats.recursive_file_count, 3);

        // A lost the moved dir's contribution entirely.
        let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(a_stats.recursive_logical_size, 0);
        assert_eq!(a_stats.recursive_file_count, 0);
        assert_eq!(a_stats.recursive_dir_count, 0);

        // B lost the stale subtree (-500, -1 file, -1 dir) and gained the moved
        // dir (+1000, +3 files, +1 dir).
        let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
        assert_eq!(b_stats.recursive_logical_size, 1000);
        assert_eq!(b_stats.recursive_file_count, 3);
        assert_eq!(b_stats.recursive_dir_count, 1);

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_cross_parent_propagates_deltas() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Two sibling dirs A and B, each with their own pre-populated stats.
        // Then a child dir D under A with non-trivial stats.
        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "A",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 1024,
                recursive_physical_size: 2048,
                recursive_file_count: 5,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            11,
            ROOT_ID,
            "B",
            DirStatsById {
                entry_id: 11,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            20,
            10,
            "D",
            DirStatsById {
                entry_id: 20,
                recursive_logical_size: 1024,
                recursive_physical_size: 2048,
                recursive_file_count: 5,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );

        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 20,
                new_parent_id: 11,
                new_name: "D".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // D itself: same dir_stats, new parent.
        let d_entry = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(d_entry.parent_id, 11);
        let d_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(d_stats.recursive_logical_size, 1024);
        assert_eq!(d_stats.recursive_file_count, 5);

        // A: lost D's contribution (size 1024, 5 files, 1 dir for D itself).
        let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(a_stats.recursive_logical_size, 0);
        assert_eq!(a_stats.recursive_physical_size, 0);
        assert_eq!(a_stats.recursive_file_count, 0);
        assert_eq!(a_stats.recursive_dir_count, 0);

        // B: gained D's contribution.
        let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
        assert_eq!(b_stats.recursive_logical_size, 1024);
        assert_eq!(b_stats.recursive_physical_size, 2048);
        assert_eq!(b_stats.recursive_file_count, 5);
        assert_eq!(b_stats.recursive_dir_count, 1);

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_file_cross_parent_propagates_deltas() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Two parent dirs, both starting with empty stats.
        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "A",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 700,
                recursive_physical_size: 700,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            11,
            ROOT_ID,
            "B",
            DirStatsById {
                entry_id: 11,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );

        // Insert a file under A (size 700, contributes 1 file).
        writer
            .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
                id: 30,
                parent_id: 10,
                name: "f.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(700),
                physical_size: Some(700),
                modified_at: Some(1700000000),
                inode: Some(99),
            }]))
            .unwrap();
        writer.flush_blocking().unwrap();

        // Move file to B.
        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 30,
                new_parent_id: 11,
                new_name: "f.txt".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(a_stats.recursive_logical_size, 0, "A loses the file's size");
        assert_eq!(a_stats.recursive_file_count, 0);

        let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
        assert_eq!(b_stats.recursive_logical_size, 700);
        assert_eq!(b_stats.recursive_file_count, 1);
        assert_eq!(b_stats.recursive_dir_count, 0, "files don't contribute to dir count");

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_no_op_when_target_matches_current() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "home",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 1024,
                recursive_physical_size: 1024,
                recursive_file_count: 3,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );

        // Capture the per-writer mutation count before the no-op. Reading the
        // global `WRITER_GENERATION` here would flake under concurrent tests,
        // since `cargo test` runs tests as threads in one process and any other
        // writer that mutates between `before` and `after` would bump it.
        let gen_before = writer.mutation_count();

        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 10,
                new_parent_id: ROOT_ID,
                new_name: "home".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(stats.recursive_logical_size, 1024, "no-op preserves stats");
        assert_eq!(stats.recursive_file_count, 3);

        // The per-writer counter should not have moved (the no-op short-circuits
        // before `bump_generation`).
        let gen_after = writer.mutation_count();
        assert_eq!(
            gen_before, gen_after,
            "no-op should not bump the writer's mutation counter"
        );

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_cross_parent_propagates_recursive_has_symlinks() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "A",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 1,
                recursive_has_symlinks: true,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            11,
            ROOT_ID,
            "B",
            DirStatsById {
                entry_id: 11,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        // The dir being moved carries the symlink flag in its own subtree.
        insert_dir_with_stats(
            &writer,
            &db_path,
            20,
            10,
            "D",
            DirStatsById {
                entry_id: 20,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: true,
            },
        );

        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 20,
                new_parent_id: 11,
                new_name: "D".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
        assert!(
            b_stats.recursive_has_symlinks,
            "new parent should pick up the symlink-bearing subtree"
        );

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_bumps_writer_generation() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "Foo",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );

        let before = writer.mutation_count();
        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 10,
                new_parent_id: ROOT_ID,
                new_name: "Bar".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();
        let after = writer.mutation_count();
        assert!(
            after > before,
            "writer's mutation counter should bump after a real move"
        );

        writer.shutdown();
    }
}
