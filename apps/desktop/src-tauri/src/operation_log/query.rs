//! The read side of the operation log (M4): filtered/paged search, a paged
//! operation detail, and the recent-operations feed. Every function takes a
//! short-lived read-only [`Connection`](rusqlite::Connection) (the writer thread
//! owns the single write connection; reads never contend under WAL).
//!
//! ## Search is index-served and spans every `row_role`
//!
//! The product headline — "when did I delete `dog.jpg`?" — is an exact folded-name
//! lookup, not full-text (D8). A name filter joins `operation_items` to
//! `operations` and matches the indexed `source_name_folded` column, so the
//! benchmark query
//! (`source_name_folded = ? AND kind IN (delete, trash) ORDER BY ended_at DESC`)
//! is served by `operation_items_source_name` + the `operations` PK, never a full
//! table scan (asserted via `EXPLAIN QUERY PLAN` in the tests).
//!
//! Search deliberately spans ALL rows regardless of `row_role`: a trashed folder
//! records the top-level `rollback_unit` plus its subtree's `search_only` leaves
//! (D-granularity), so "when did I trash `dog.jpg`" hits even when `dog.jpg` sat
//! inside a trashed folder. An op that couldn't enumerate its subtree carries
//! `search_coverage = top_level_only` — a queryable known gap, not a silent miss.
//!
//! ## Wire types
//!
//! [`OperationRow`] (from `store`) is the summary type returned directly (it holds
//! no interned ids). [`OperationDetail`] / [`OperationItemView`] resolve the
//! interned dir prefixes of item rows to full paths so the frontend never sees a
//! `dir_id`.

use rusqlite::Connection;

use super::store::fold_name;
use super::store::{
    ITEM_COLUMNS, OPERATION_COLUMNS, OperationItemRow, OperationLogStoreError, OperationRow, join_leaf, map_item_row,
    map_operation_row, resolve_dir,
};
use super::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, OpKind, RollbackState, RowRole, SearchCoverage,
};

/// How a name filter matches the folded name column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameMatch {
    /// Exact folded-name equality (the `dog.jpg` benchmark). Index-served.
    Exact,
    /// Folded-name prefix (`report` matches `report-2026.pdf`). Served by a range
    /// scan on the same b-tree index, never `LIKE` (which wouldn't use the index).
    Prefix,
}

/// A name filter over `operation_items.source_name_folded`. The caller passes a
/// raw name; it's folded here with the same [`fold_name`] used at insert, so the
/// match key matches what's stored. Matching source names covers the product's
/// "what did I do to `<name>`" headline (a trashed subtree's leaves store their
/// own `source_name_folded`, so leaf search hits too).
#[derive(Debug, Clone)]
pub struct NameFilter {
    pub text: String,
    pub match_kind: NameMatch,
}

/// Filters for [`search_operations`]. All optional / empty ⇒ unconstrained; an
/// absent name filter means no `operation_items` join (a header-only query).
#[derive(Debug, Clone, Default)]
pub struct OperationSearchFilters {
    /// Inclusive lower bound on `started_at` (an op's occurrence time).
    pub since: Option<i64>,
    /// Inclusive upper bound on `started_at`.
    pub until: Option<i64>,
    /// A name match on the items' folded source names (spans every `row_role`).
    pub name: Option<NameFilter>,
    /// `kind IN (...)`; empty ⇒ any kind. The benchmark passes `[Delete, Trash]`.
    pub kinds: Vec<OpKind>,
    pub initiator: Option<Initiator>,
    pub execution_status: Option<ExecutionStatus>,
    pub rollback_state: Option<RollbackState>,
}

/// An operation's header plus a page of its items, with dir prefixes resolved to
/// full paths. Returned by [`get_operation`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationDetail {
    pub operation: OperationRow,
    pub items: Vec<OperationItemView>,
    /// The op's total item count (all `row_role`s), so a paged UI knows whether
    /// more items exist beyond the returned slice.
    pub total_items: u32,
}

/// One item row with its interned dir prefixes resolved to full paths and real
/// volume ids — the frontend/MCP view (never an interned `dir_id`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationItemView {
    pub seq: i64,
    pub entry_type: EntryType,
    pub row_role: RowRole,
    pub source_volume_id: String,
    pub source_path: String,
    pub dest_volume_id: Option<String>,
    pub dest_path: Option<String>,
    pub size: Option<i64>,
    pub mtime: Option<i64>,
    pub outcome: ItemOutcome,
    pub overwrote: bool,
}

fn view_from_row(conn: &Connection, item: &OperationItemRow) -> Result<OperationItemView, OperationLogStoreError> {
    let (source_volume_id, source_dir) = resolve_dir(conn, item.source_dir_id)?;
    let source_path = join_leaf(&source_dir, &item.source_name).to_string_lossy().into_owned();
    let (dest_volume_id, dest_path) = match (item.dest_dir_id, &item.dest_name) {
        (Some(dir_id), Some(name)) => {
            let (vol, dir) = resolve_dir(conn, dir_id)?;
            (Some(vol), Some(join_leaf(&dir, name).to_string_lossy().into_owned()))
        }
        _ => (None, None),
    };
    Ok(OperationItemView {
        seq: item.seq,
        entry_type: item.entry_type,
        row_role: item.row_role,
        source_volume_id,
        source_path,
        dest_volume_id,
        dest_path,
        size: item.size,
        mtime: item.mtime,
        outcome: item.outcome,
        overwrote: item.overwrote,
    })
}

/// The most recent operations by start time (newest first), paged. This is the
/// alpha UI's feed ("last 50 + load 50 more" = `limit = 50` with a growing
/// `offset`). Includes unfinished ops.
pub fn recent_operations(
    conn: &Connection,
    limit: u32,
    offset: u32,
) -> Result<Vec<OperationRow>, OperationLogStoreError> {
    let sql =
        format!("SELECT {OPERATION_COLUMNS} FROM operations ORDER BY started_at DESC, op_id DESC LIMIT ?1 OFFSET ?2");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![limit, offset])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_operation_row(row)?);
    }
    Ok(out)
}

/// One operation's header plus a page of its items (seq ascending, for grouped
/// display), dir prefixes resolved. `None` if the op is absent.
pub fn get_operation(
    conn: &Connection,
    op_id: &str,
    item_limit: u32,
    item_offset: u32,
) -> Result<Option<OperationDetail>, OperationLogStoreError> {
    let op_sql = format!("SELECT {OPERATION_COLUMNS} FROM operations WHERE op_id = ?1");
    let operation = {
        let mut stmt = conn.prepare_cached(&op_sql)?;
        let mut rows = stmt.query(rusqlite::params![op_id])?;
        match rows.next()? {
            Some(row) => map_operation_row(row)?,
            None => return Ok(None),
        }
    };

    let total_items: u32 = conn.query_row(
        "SELECT COUNT(*) FROM operation_items WHERE op_id = ?1",
        rusqlite::params![op_id],
        |row| row.get::<_, i64>(0).map(|n| n as u32),
    )?;

    let item_sql =
        format!("SELECT {ITEM_COLUMNS} FROM operation_items WHERE op_id = ?1 ORDER BY seq ASC LIMIT ?2 OFFSET ?3");
    let rows = {
        let mut stmt = conn.prepare_cached(&item_sql)?;
        let mut q = stmt.query(rusqlite::params![op_id, item_limit, item_offset])?;
        let mut collected = Vec::new();
        while let Some(row) = q.next()? {
            collected.push(map_item_row(row)?);
        }
        collected
    };
    // Resolve dirs after draining the query so the cached statement's borrow is
    // released (the read connection reuses one cache).
    let items = rows.iter().map(|r| view_from_row(conn, r)).collect::<Result<_, _>>()?;

    Ok(Some(OperationDetail {
        operation,
        items,
        total_items,
    }))
}

/// Search operations by the composed filters, newest-ended first, paged. A name
/// filter joins `operation_items` (spanning every `row_role`) and matches the
/// indexed folded source name; without one the query is header-only over
/// `operations`. Results are distinct operations even when several items of one
/// op match the name.
pub fn search_operations(
    conn: &Connection,
    filters: &OperationSearchFilters,
    limit: u32,
    offset: u32,
) -> Result<Vec<OperationRow>, OperationLogStoreError> {
    let (sql, params) = build_search_query(filters, limit, offset);
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(param_refs.as_slice())?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_operation_row(row)?);
    }
    Ok(out)
}

/// Build the parameterized search SQL + its bound params. Split out from
/// [`search_operations`] so a test can wrap it in `EXPLAIN QUERY PLAN` and assert
/// the benchmark stays index-served (no full table scan).
fn build_search_query(
    filters: &OperationSearchFilters,
    limit: u32,
    offset: u32,
) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut where_clauses: Vec<String> = Vec::new();

    // Name predicate on the joined items (only present when a name filter is set).
    let joins_items = filters.name.is_some();
    if let Some(name) = &filters.name {
        let folded = fold_name(&name.text);
        match name.match_kind {
            NameMatch::Exact => {
                where_clauses.push(format!("i.source_name_folded = ?{}", params.len() + 1));
                params.push(Box::new(folded));
            }
            NameMatch::Prefix => {
                where_clauses.push(format!("i.source_name_folded >= ?{}", params.len() + 1));
                params.push(Box::new(folded.clone()));
                if let Some(upper) = prefix_upper_bound(&folded) {
                    where_clauses.push(format!("i.source_name_folded < ?{}", params.len() + 1));
                    params.push(Box::new(upper));
                }
            }
        }
    }

    if let Some(since) = filters.since {
        where_clauses.push(format!("o.started_at >= ?{}", params.len() + 1));
        params.push(Box::new(since));
    }
    if let Some(until) = filters.until {
        where_clauses.push(format!("o.started_at <= ?{}", params.len() + 1));
        params.push(Box::new(until));
    }
    if !filters.kinds.is_empty() {
        let placeholders: Vec<String> = filters
            .kinds
            .iter()
            .map(|kind| {
                params.push(Box::new(kind.as_token().to_string()));
                format!("?{}", params.len())
            })
            .collect();
        where_clauses.push(format!("o.kind IN ({})", placeholders.join(", ")));
    }
    if let Some(initiator) = filters.initiator {
        where_clauses.push(format!("o.initiator = ?{}", params.len() + 1));
        params.push(Box::new(initiator.as_token().to_string()));
    }
    if let Some(status) = filters.execution_status {
        where_clauses.push(format!("o.execution_status = ?{}", params.len() + 1));
        params.push(Box::new(status.as_token().to_string()));
    }
    if let Some(state) = filters.rollback_state {
        where_clauses.push(format!("o.rollback_state = ?{}", params.len() + 1));
        params.push(Box::new(state.as_token().to_string()));
    }

    // Prefix the shared column list with the `o` alias so the join can't make
    // `op_id` ambiguous; the column ORDER is unchanged, so `map_operation_row`'s
    // positional reads still line up.
    let select_cols = OPERATION_COLUMNS
        .split(',')
        .map(|c| format!("o.{}", c.trim()))
        .collect::<Vec<_>>()
        .join(", ");
    let from = if joins_items {
        "FROM operations o JOIN operation_items i ON i.op_id = o.op_id"
    } else {
        "FROM operations o"
    };
    let distinct = if joins_items { "DISTINCT " } else { "" };
    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };
    params.push(Box::new(limit));
    let limit_ph = params.len();
    params.push(Box::new(offset));
    let offset_ph = params.len();

    let sql = format!(
        "SELECT {distinct}{select_cols} {from} {where_sql} \
         ORDER BY o.ended_at DESC, o.op_id DESC LIMIT ?{limit_ph} OFFSET ?{offset_ph}"
    );
    (sql, params)
}

/// The exclusive upper bound of a prefix range: the smallest string strictly
/// greater than every string starting with `prefix`. Increment the last char
/// (skipping the UTF-16 surrogate hole so the result stays a valid `char`); if the
/// last char is already the max, drop it and carry to the previous. `None` only
/// for a prefix of all-max chars (never a real filename), meaning "no upper
/// bound" (match everything `>= prefix`).
fn prefix_upper_bound(prefix: &str) -> Option<String> {
    let mut chars: Vec<char> = prefix.chars().collect();
    while let Some(last) = chars.pop() {
        if let Some(next) = next_char(last) {
            chars.push(next);
            return Some(chars.into_iter().collect());
        }
        // `last` was char::MAX: carry to the previous char.
    }
    None
}

/// The next scalar Unicode value after `c`, skipping the surrogate range
/// (`0xD800..=0xDFFF`, which isn't a valid `char`). `None` at `char::MAX`.
fn next_char(c: char) -> Option<char> {
    let next = c as u32 + 1;
    let next = if (0xD800..=0xDFFF).contains(&next) {
        0xE000
    } else {
        next
    };
    char::from_u32(next)
}

/// Whether an operation's search coverage is complete — a convenience for
/// consumers that want to flag a known gap (`top_level_only`) without matching on
/// the enum. Kept here so the flag's meaning lives beside the search API.
pub fn coverage_is_complete(op: &OperationRow) -> bool {
    op.search_coverage == SearchCoverage::Full
}

#[cfg(test)]
mod tests;
