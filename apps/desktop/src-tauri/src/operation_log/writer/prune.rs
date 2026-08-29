//! Retention: dropping old operations and giving the space back.
//!
//! The mechanism only; `retention.rs` decides WHEN to ask for it and with what
//! budget. All of it runs on the writer thread, which is why every step here is
//! bounded: an age-only prune reclaims one `incremental_vacuum` slice and lets
//! the periodic timer drain the rest over ticks, so a big cleanup never stops a
//! capture burst.
//!
//! A rollback pair is indivisible — an operation and the operation that undid it
//! are deleted together or not at all, so history never shows half a reversal.

use rusqlite::{Connection, OptionalExtension};

use super::super::store::OperationLogStoreError;
use super::super::types::RollbackState;
use super::PruneRequest;

/// Tiered `incremental_vacuum` caps, mirroring `indexing/writer/maintenance.rs`:
/// skip the lock below `MIN`, hold a steady cap for a modest freelist, ramp to
/// drain a real backlog. Bounded so a big prune never stops the world.
pub(super) const VACUUM_MIN_FREELIST: i64 = 1_000;
pub(super) const VACUUM_STEADY_CAP: i64 = 2_000;
pub(super) const VACUUM_BACKLOG_THRESHOLD: i64 = 20_000;
pub(super) const VACUUM_BACKLOG_CAP: i64 = 20_000;

pub(super) fn pick_vacuum_cap(freelist: i64) -> Option<i64> {
    if freelist < VACUUM_MIN_FREELIST {
        None
    } else if freelist < VACUUM_BACKLOG_THRESHOLD {
        Some(VACUUM_STEADY_CAP)
    } else {
        Some(VACUUM_BACKLOG_CAP)
    }
}

/// Prune whole operations by age and/or a size budget, GC orphaned interned dirs,
/// then reclaim freed pages. Whole-operation pruning keeps rollback pairs
/// consistent: a pruned op's dangling `rolls_back_op_id` links (in surviving ops)
/// are nulled, never left dangling. Ops a live rollback touches are never pruned
/// (see [`prunable_ops_fragment`]) so its streamed source rows can't vanish
/// mid-stream (`DETAILS.md` § "The retention race it closes").
pub(super) fn handle_prune(conn: &mut Connection, request: &PruneRequest) -> Result<(), OperationLogStoreError> {
    if let Some(max_age) = request.max_age_secs {
        let cutoff = request.now_secs.saturating_sub(max_age) as i64;
        prune_by_age(conn, cutoff)?;
    }

    if let Some(budget) = request.max_size_bytes {
        prune_by_size(conn, budget)?;
    }

    // GC the dirs the pruned ops orphaned, once, covering both passes.
    {
        let tx = conn.unchecked_transaction()?;
        gc_orphan_dirs(&tx)?;
        tx.commit()?;
    }

    // A size prune must actually return the freed pages to the OS to honor the
    // budget, so it drains the freelist fully and truncates. An age-only prune
    // just does one bounded slice (the periodic timer drains the rest over ticks),
    // keeping the writer responsive to capture.
    if request.max_size_bytes.is_some() {
        reclaim_fully(conn);
    } else if request.vacuum {
        run_bounded_vacuum(conn);
    }
    Ok(())
}

/// The SQL predicate for an op that IS safe to prune — i.e. NOT protected by an
/// in-flight rollback. It excludes any op in `rolling_back` (the original, whose
/// rows a live rollback streams across successive read connections) and the
/// `rolls_back_op_id` target of one. Interpolates the stable `rolling_back` token
/// (a compile-time constant, no injection surface); the unfinished inverse op is
/// separately protected by the `ended_at IS NOT NULL` gate every prune applies.
fn prunable_ops_fragment() -> String {
    let rolling_back = RollbackState::RollingBack.as_token();
    format!(
        "rollback_state <> '{rolling_back}' \
         AND op_id NOT IN (SELECT rolls_back_op_id FROM operations \
                           WHERE rollback_state = '{rolling_back}' AND rolls_back_op_id IS NOT NULL)"
    )
}

/// Prune every finished, unprotected op older than `cutoff` in one transaction.
fn prune_by_age(conn: &mut Connection, cutoff: i64) -> Result<(), OperationLogStoreError> {
    let prunable = prunable_ops_fragment();
    let predicate = format!("ended_at IS NOT NULL AND ended_at < {cutoff} AND {prunable}");
    let selector = format!("SELECT op_id FROM operations WHERE {predicate}");

    let tx = conn.unchecked_transaction()?;
    // Null any SURVIVING op's rollback link that points at an op about to be
    // pruned, BEFORE the delete — otherwise the self-FK
    // (`rolls_back_op_id REFERENCES operations`) rejects deleting a referenced op.
    // A rolled-back pair whose both halves fall in the prune set deletes together;
    // a split pair leaves the survivor with a nulled link, never a dangling one.
    tx.execute(
        &format!("UPDATE operations SET rolls_back_op_id = NULL WHERE rolls_back_op_id IN ({selector})"),
        [],
    )?;
    tx.execute(&format!("DELETE FROM operation_items WHERE op_id IN ({selector})"), [])?;
    tx.execute(&format!("DELETE FROM operations WHERE {predicate}"), [])?;
    tx.commit()?;
    Ok(())
}

/// Prune the oldest whole operations until the DB's live size is within `budget`.
/// Live size is `(page_count - freelist) * page_size` — the size the file would
/// have after a full vacuum — so the loop makes progress even before pages are
/// reclaimed (each delete grows the freelist, shrinking live size). Stops when
/// under budget or nothing prunable remains (e.g. everything left is protected by
/// an in-flight rollback).
fn prune_by_size(conn: &mut Connection, budget: u64) -> Result<(), OperationLogStoreError> {
    let prunable = prunable_ops_fragment();
    let oldest_sql = format!(
        "SELECT op_id FROM operations WHERE ended_at IS NOT NULL AND {prunable} \
         ORDER BY ended_at ASC, started_at ASC, op_id ASC LIMIT 1"
    );
    loop {
        if live_size_bytes(conn)? <= budget {
            return Ok(());
        }
        let seed: Option<String> = conn
            .prepare_cached(&oldest_sql)?
            .query_row([], |row| row.get(0))
            .optional()?;
        let Some(seed) = seed else {
            // Nothing left we're allowed to prune; the vacuum still reclaims what
            // the age/earlier passes freed.
            return Ok(());
        };
        let set = rollback_pair_component(conn, &seed)?;
        let tx = conn.unchecked_transaction()?;
        delete_op_set(&tx, &set)?;
        tx.commit()?;
    }
}

/// The op plus its rollback pair partners (the op it rolls back, and any op that
/// rolls it back), so a rolled-back pair prunes together. Protected partners are
/// excluded from the delete set — [`delete_op_set`] nulls the dangling link to
/// them instead. `seed` itself is never protected (the caller selects only
/// unprotected ops).
fn rollback_pair_component(conn: &Connection, seed: &str) -> Result<Vec<String>, OperationLogStoreError> {
    let prunable = prunable_ops_fragment();
    let mut set = vec![seed.to_string()];
    let mut add = |op_id: Option<String>| {
        if let Some(id) = op_id
            && !set.contains(&id)
        {
            set.push(id);
        }
    };
    // The op this one rolls back (if any), unless it's protected.
    let target: Option<String> = conn
        .prepare_cached(&format!(
            "SELECT rolls_back_op_id FROM operations \
             WHERE op_id = ?1 AND rolls_back_op_id IS NOT NULL \
             AND rolls_back_op_id IN (SELECT op_id FROM operations WHERE {prunable})"
        ))?
        .query_row(rusqlite::params![seed], |row| row.get(0))
        .optional()?;
    add(target);
    // Ops that roll this one back, unless protected.
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT op_id FROM operations WHERE rolls_back_op_id = ?1 AND {prunable}"
    ))?;
    let inverses = stmt.query_map(rusqlite::params![seed], |row| row.get::<_, String>(0))?;
    for inv in inverses {
        add(Some(inv?));
    }
    Ok(set)
}

/// Delete a set of whole operations (their items too), nulling any surviving op's
/// `rolls_back_op_id` that points into the set first (the self-FK would otherwise
/// reject the delete).
fn delete_op_set(conn: &Connection, op_ids: &[String]) -> Result<(), OperationLogStoreError> {
    if op_ids.is_empty() {
        return Ok(());
    }
    let placeholders = std::iter::repeat_n("?", op_ids.len()).collect::<Vec<_>>().join(", ");
    let params: Vec<&dyn rusqlite::ToSql> = op_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    conn.execute(
        &format!(
            "UPDATE operations SET rolls_back_op_id = NULL \
             WHERE rolls_back_op_id IN ({placeholders}) AND op_id NOT IN ({placeholders})"
        ),
        [params.as_slice(), params.as_slice()].concat().as_slice(),
    )?;
    conn.execute(
        &format!("DELETE FROM operation_items WHERE op_id IN ({placeholders})"),
        params.as_slice(),
    )?;
    conn.execute(
        &format!("DELETE FROM operations WHERE op_id IN ({placeholders})"),
        params.as_slice(),
    )?;
    Ok(())
}

/// The DB's live size in bytes: `(page_count - freelist) * page_size` — what the
/// file would occupy after a full vacuum. Used as the size-budget yardstick so
/// pruning makes progress before pages are physically reclaimed.
pub(super) fn live_size_bytes(conn: &Connection) -> Result<u64, OperationLogStoreError> {
    let page_count: i64 = conn.pragma_query_value(None, "page_count", |row| row.get(0))?;
    let freelist: i64 = conn.pragma_query_value(None, "freelist_count", |row| row.get(0))?;
    let page_size: i64 = conn.pragma_query_value(None, "page_size", |row| row.get(0))?;
    Ok(((page_count - freelist).max(0) as u64) * page_size as u64)
}

/// Fully reclaim freed pages to the OS after a size prune: drain the ENTIRE
/// freelist, then TRUNCATE the WAL so the truncation reaches the physical file.
/// Unlike [`run_bounded_vacuum`] this ignores the `pick_vacuum_cap` floor — a size
/// budget can only be honored once the pages actually leave the file, however
/// small the freelist. Retention runs off the hot path, so a full drain here is
/// the point, not a stall to avoid.
fn reclaim_fully(conn: &Connection) {
    // No cap: reclaim every free page in one pass.
    if let Err(e) = crate::sqlite_util::run_incremental_vacuum(conn, None) {
        log::warn!(target: "operation_log", "incremental_vacuum failed: {e}");
    }
    // TRUNCATE so the vacuum's page-count reduction reaches the on-disk file (in
    // WAL mode it otherwise lands only in the WAL until the next checkpoint).
    if let Err(e) = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())) {
        log::warn!(target: "operation_log", "wal_checkpoint(TRUNCATE) failed: {e}");
    }
}

/// GC interned dirs no longer live: iterate leaf-up, deleting dirs referenced by
/// no item AND no child dir, until stable. This deletes exactly the complement
/// of the referenced-dirs-plus-their-ancestors closure — a referenced dir's
/// whole parent chain survives (path reconstruction walks it), and a pruned
/// dir's ancestors fall away only once nothing live remains under them.
fn gc_orphan_dirs(conn: &Connection) -> Result<(), OperationLogStoreError> {
    loop {
        let deleted = conn.execute(
            "DELETE FROM dirs
             WHERE dir_id NOT IN (SELECT source_dir_id FROM operation_items)
               AND dir_id NOT IN (SELECT dest_dir_id FROM operation_items WHERE dest_dir_id IS NOT NULL)
               AND dir_id NOT IN (SELECT parent_dir_id FROM dirs WHERE parent_dir_id IS NOT NULL)",
            [],
        )?;
        if deleted == 0 {
            break;
        }
    }
    Ok(())
}

/// Run one bounded `incremental_vacuum` slice sized to the current freelist.
fn run_bounded_vacuum(conn: &Connection) {
    let free = match conn.pragma_query_value(None, "freelist_count", |row| row.get::<_, i64>(0)) {
        Ok(n) => n,
        Err(e) => {
            log::warn!(target: "operation_log", "freelist_count query failed: {e}");
            return;
        }
    };
    let Some(cap) = pick_vacuum_cap(free) else {
        return;
    };
    if let Err(e) = crate::sqlite_util::run_incremental_vacuum(conn, Some(cap)) {
        log::warn!(target: "operation_log", "incremental_vacuum failed: {e}");
    }
}
