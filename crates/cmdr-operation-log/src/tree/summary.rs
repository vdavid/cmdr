//! `operation_tree_dirs`: per (op, axis folder) subtree totals, so opening any
//! folder of any op reads its own row and its children's (`DETAILS.md` § "Tree
//! summary").
//!
//! Every column is a pure function of item-row fields, so the writer keeps the rows
//! exact by applying each batch's DELTA in the batch's own transaction ([`TreeDelta`],
//! [`apply_delta`]), and a from-scratch [`aggregate_op`] over the same rows must
//! always produce the same table. The backfill for ops journaled before the summary
//! existed is that aggregate.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, OptionalExtension};

use super::{TreeAxis, UnitRole, unit_role};
use crate::store::{OperationItemRow, OperationLogStoreError};
use crate::types::{EntryType, ItemOutcome, OpKind, RowRole, SkipReason, TreeSummaryState};

/// The subtree totals of one folder: every row placed in it or anywhere below it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SubtreeTotals {
    /// FILE rows of either role.
    pub(crate) files: i64,
    /// Their bytes, `MAX(size, 0)`. A folder unit's own `size` is an inode length,
    /// never bytes, so DIR rows add none.
    pub(crate) bytes: i64,
    /// [`UnitRole::Selectable`] rows, files and DIR units alike: the "M" of "N of M".
    pub(crate) units_selectable: i64,
    /// The FILE ones among them, which a reversal's progress bar counts.
    pub(crate) unit_files_selectable: i64,
    /// Their bytes, `MAX(size, 0)`, as `read_rollback_progress_totals` sums them.
    pub(crate) unit_bytes_selectable: i64,
    /// `rollback_unit` rows a rollback reversed, housekeeping folders included.
    pub(crate) units_rolled_back: i64,
    /// `rollback_unit` rows a rollback held back (a stored skip reason).
    pub(crate) units_held_back: i64,
    /// `rollback_unit` rows the operation itself never finished (`failed`, or
    /// `skipped` with no rollback reason).
    pub(crate) units_unfinished: i64,
    /// [`UnitRole::Housekeeping`] folders still awaiting reversal: with nothing
    /// selectable left, these are why "Roll back the rest" still has work.
    pub(crate) housekeeping_waiting: i64,
}

impl SubtreeTotals {
    fn add(&mut self, other: &SubtreeTotals, sign: i64) {
        self.files += sign * other.files;
        self.bytes += sign * other.bytes;
        self.units_selectable += sign * other.units_selectable;
        self.unit_files_selectable += sign * other.unit_files_selectable;
        self.unit_bytes_selectable += sign * other.unit_bytes_selectable;
        self.units_rolled_back += sign * other.units_rolled_back;
        self.units_held_back += sign * other.units_held_back;
        self.units_unfinished += sign * other.units_unfinished;
        self.housekeeping_waiting += sign * other.housekeeping_waiting;
    }

    fn is_zero(&self) -> bool {
        *self == SubtreeTotals::default()
    }
}

/// One `operation_tree_dirs` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeDirRow {
    /// The folder, an interned `dirs` id on the op's axis.
    pub(crate) dir_id: i64,
    /// Its parent folder (NULL for a volume root), which also has a row.
    pub(crate) parent_dir_id: Option<i64>,
    /// Its folded name, the child listing's order.
    pub(crate) name_folded: String,
    /// The `seq` of the DIR item row this folder IS (a folder unit, a created folder,
    /// a `search_only` folder), when there is one. The lowest `seq` when several are.
    pub(crate) self_seq: Option<i64>,
    /// How many child folders have a row in this op.
    pub(crate) child_dirs: i64,
    /// How many FILE rows sit directly in this folder.
    pub(crate) direct_files: i64,
    /// Everything at or below it.
    pub(crate) totals: SubtreeTotals,
}

/// The fields of one item row the summary reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowFacts {
    pub(crate) seq: i64,
    pub(crate) entry_type: EntryType,
    pub(crate) row_role: RowRole,
    pub(crate) outcome: ItemOutcome,
    pub(crate) skip_reason: Option<SkipReason>,
    pub(crate) size: Option<i64>,
    pub(crate) source_dir_id: i64,
    pub(crate) source_name: String,
    /// `(dest_dir_id, dest_name)`, present only when both are.
    pub(crate) dest: Option<(i64, String)>,
    /// A DIR row's own folder on the axis (`operation_items.self_dir_id`).
    pub(crate) self_dir_id: Option<i64>,
}

impl RowFacts {
    /// `record_created_dirs` writes a created folder as both source and dest (the
    /// same marker the rollback reads, `rollback.rs::is_created_in_place`). One
    /// interned dir id is one `(volume, path)`, so equal ids mean equal places.
    pub(crate) fn created_in_place(&self) -> bool {
        self.dest
            .as_ref()
            .is_some_and(|(dir, name)| *dir == self.source_dir_id && *name == self.source_name)
    }

    /// The row's `(parent dir, name)` on `axis`.
    pub(crate) fn axis_side(&self, axis: TreeAxis) -> (i64, &str) {
        axis.side(
            (self.source_dir_id, &self.source_name),
            self.dest.as_ref().map(|(dir, name)| (*dir, name.as_str())),
        )
    }

    /// The folder this row counts in: a DIR row in its OWN folder (so unchecking that
    /// folder subtracts exactly the unit), any other row in its axis parent. A DIR row
    /// whose own folder isn't interned yet (written before the summary existed) counts
    /// in its parent until the backfill fills it.
    pub(crate) fn placement(&self, axis: TreeAxis) -> i64 {
        match (self.entry_type, self.self_dir_id) {
            (EntryType::Dir, Some(own)) => own,
            _ => self.axis_side(axis).0,
        }
    }

    pub(crate) fn role(&self, kind: OpKind) -> UnitRole {
        unit_role(
            kind,
            self.row_role,
            self.entry_type,
            self.created_in_place(),
            self.outcome,
            self.skip_reason,
        )
    }

    fn contribution(&self, kind: OpKind) -> SubtreeTotals {
        let mut t = SubtreeTotals::default();
        let is_file = self.entry_type == EntryType::File;
        let bytes = self.size.unwrap_or(0).max(0);
        if is_file {
            t.files = 1;
            t.bytes = bytes;
        }
        match self.role(kind) {
            UnitRole::Selectable => {
                t.units_selectable = 1;
                if is_file {
                    t.unit_files_selectable = 1;
                    t.unit_bytes_selectable = bytes;
                }
            }
            UnitRole::Housekeeping => t.housekeeping_waiting = 1,
            UnitRole::Contents | UnitRole::Settled | UnitRole::NotReversible => {}
        }
        if self.row_role == RowRole::RollbackUnit {
            match (self.outcome, self.skip_reason) {
                (ItemOutcome::RolledBack, _) => t.units_rolled_back = 1,
                (ItemOutcome::Skipped, Some(_)) => t.units_held_back = 1,
                (ItemOutcome::Failed, _) | (ItemOutcome::Skipped, None) => t.units_unfinished = 1,
                (ItemOutcome::Done, _) => {}
            }
        }
        t
    }
}

impl From<&OperationItemRow> for RowFacts {
    /// The facts of a row the rollback engine streamed, for its per-row selection
    /// decision (`selection.rs::Selection::includes`).
    fn from(row: &OperationItemRow) -> Self {
        RowFacts {
            seq: row.seq,
            entry_type: row.entry_type,
            row_role: row.row_role,
            outcome: row.outcome,
            skip_reason: row.rollback_skip_reason,
            size: row.size,
            source_dir_id: row.source_dir_id,
            source_name: row.source_name.clone(),
            dest: match (row.dest_dir_id, &row.dest_name) {
                (Some(dir), Some(name)) => Some((dir, name.clone())),
                _ => None,
            },
            self_dir_id: row.self_dir_id,
        }
    }
}

/// The item columns [`map_facts`] reads.
pub(crate) const FACT_COLUMNS: &str = "seq, entry_type, row_role, outcome, rollback_skip_reason, size, \
     source_dir_id, source_name, dest_dir_id, dest_name, self_dir_id";

fn token<T>(
    value: String,
    parse: impl Fn(&str) -> Option<T>,
    column: &'static str,
) -> Result<T, OperationLogStoreError> {
    parse(&value).ok_or(OperationLogStoreError::Decode { column, value })
}

/// Map a row selected with [`FACT_COLUMNS`].
pub(crate) fn map_facts(row: &rusqlite::Row<'_>) -> Result<RowFacts, OperationLogStoreError> {
    let dest = match (row.get::<_, Option<i64>>(8)?, row.get::<_, Option<String>>(9)?) {
        (Some(dir), Some(name)) => Some((dir, name)),
        _ => None,
    };
    Ok(RowFacts {
        seq: row.get(0)?,
        entry_type: token(row.get(1)?, EntryType::from_token, "entry_type")?,
        row_role: token(row.get(2)?, RowRole::from_token, "row_role")?,
        outcome: token(row.get(3)?, ItemOutcome::from_token, "outcome")?,
        skip_reason: match row.get::<_, Option<String>>(4)? {
            Some(value) => Some(token(value, SkipReason::from_token, "rollback_skip_reason")?),
            None => None,
        },
        size: row.get(5)?,
        source_dir_id: row.get(6)?,
        source_name: row.get(7)?,
        dest,
        self_dir_id: row.get(10)?,
    })
}

/// What the summary needs to know about an op: its kind, its axis, and whether its
/// summary is built (an unbuilt one gets no maintenance until it's rebuilt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpTreeContext {
    pub(crate) kind: OpKind,
    pub(crate) axis: TreeAxis,
    pub(crate) built: bool,
}

/// Read [`OpTreeContext`] for `op_id`, `None` for an op with no header.
pub(crate) fn read_op_tree_context(
    conn: &Connection,
    op_id: &str,
) -> Result<Option<OpTreeContext>, OperationLogStoreError> {
    let row = conn
        .prepare_cached("SELECT kind, rolls_back_op_id IS NOT NULL, tree_summary FROM operations WHERE op_id = ?1")?
        .query_row(rusqlite::params![op_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .optional()?;
    let Some((kind, is_inverse, summary)) = row else {
        return Ok(None);
    };
    let kind = token(kind, OpKind::from_token, "kind")?;
    let built = match summary {
        Some(value) => token(value, TreeSummaryState::from_token, "tree_summary")? == TreeSummaryState::Built,
        None => false,
    };
    Ok(Some(OpTreeContext {
        kind,
        axis: TreeAxis::of(kind, is_inverse),
        built,
    }))
}

/// A change to an op's summary, accumulated per folder in memory: a batch of inserted
/// rows (each added), or a batch of outcome changes (each old row taken away, its new
/// self added).
#[derive(Debug, Default)]
pub(crate) struct TreeDelta {
    dirs: HashMap<i64, DirDelta>,
}

#[derive(Debug, Default)]
struct DirDelta {
    direct_files: i64,
    self_seq: Option<i64>,
    own: SubtreeTotals,
}

/// A folder after [`TreeDelta::roll_up`]: its own changes plus everything below.
struct Placed {
    parent_dir_id: Option<i64>,
    name_folded: String,
    self_seq: Option<i64>,
    direct_files: i64,
    totals: SubtreeTotals,
}

impl TreeDelta {
    /// Add (`sign = 1`) or take away (`sign = -1`) one row.
    pub(crate) fn add_row(&mut self, kind: OpKind, axis: TreeAxis, facts: &RowFacts, sign: i64) {
        let entry = self.dirs.entry(facts.placement(axis)).or_default();
        entry.own.add(&facts.contribution(kind), sign);
        match facts.entry_type {
            EntryType::File => entry.direct_files += sign,
            EntryType::Dir if sign > 0 && facts.self_dir_id.is_some() => {
                entry.self_seq = Some(entry.self_seq.map_or(facts.seq, |seq| seq.min(facts.seq)));
            }
            EntryType::Dir => {}
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.dirs.is_empty()
    }

    /// Fold each folder's own change into every ancestor's subtree totals, up to the
    /// volume root. Every ancestor gets an entry even when its totals don't move, so a
    /// new folder's whole chain exists. Each distinct dir's parent is read once.
    fn roll_up(self, conn: &Connection) -> Result<BTreeMap<i64, Placed>, OperationLogStoreError> {
        let mut dirs: HashMap<i64, (Option<i64>, String)> = HashMap::new();
        let mut lookup = |dir_id: i64| -> Result<(Option<i64>, String), OperationLogStoreError> {
            if let Some(known) = dirs.get(&dir_id) {
                return Ok(known.clone());
            }
            let read = conn
                .prepare_cached("SELECT parent_dir_id, name_folded FROM dirs WHERE dir_id = ?1")?
                .query_row(rusqlite::params![dir_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            dirs.insert(dir_id, read.clone());
            Ok(read)
        };
        let mut placed: BTreeMap<i64, Placed> = BTreeMap::new();
        for (dir_id, delta) in self.dirs {
            let mut current = Some(dir_id);
            let mut is_own = true;
            while let Some(id) = current {
                let (parent_dir_id, name_folded) = lookup(id)?;
                let node = placed.entry(id).or_insert_with(|| Placed {
                    parent_dir_id,
                    name_folded,
                    self_seq: None,
                    direct_files: 0,
                    totals: SubtreeTotals::default(),
                });
                node.totals.add(&delta.own, 1);
                if is_own {
                    node.direct_files += delta.direct_files;
                    node.self_seq = min_seq(node.self_seq, delta.self_seq);
                    is_own = false;
                }
                current = parent_dir_id;
            }
        }
        Ok(placed)
    }
}

fn min_seq(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Apply `delta` to `op_id`'s summary rows. Runs on the writer's connection, inside
/// the transaction that wrote the rows the delta describes. A folder row created here
/// adds one to its parent's `child_dirs`.
pub(crate) fn apply_delta(conn: &Connection, op_id: &str, delta: TreeDelta) -> Result<(), OperationLogStoreError> {
    let placed = delta.roll_up(conn)?;
    let mut new_children: HashMap<i64, i64> = HashMap::new();
    {
        let mut create = conn.prepare_cached(
            "INSERT OR IGNORE INTO operation_tree_dirs (op_id, dir_id, parent_dir_id, name_folded) \
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (dir_id, node) in &placed {
            let created = create.execute(rusqlite::params![op_id, dir_id, node.parent_dir_id, node.name_folded])?;
            if created == 1
                && let Some(parent) = node.parent_dir_id
            {
                *new_children.entry(parent).or_default() += 1;
            }
        }
    }
    let mut update = conn.prepare_cached(
        "UPDATE operation_tree_dirs SET \
            self_seq = COALESCE(MIN(self_seq, ?3), self_seq, ?3), \
            child_dirs = child_dirs + ?4, \
            direct_files = direct_files + ?5, \
            files = files + ?6, \
            bytes = bytes + ?7, \
            units_selectable = units_selectable + ?8, \
            unit_files_selectable = unit_files_selectable + ?9, \
            unit_bytes_selectable = unit_bytes_selectable + ?10, \
            units_rolled_back = units_rolled_back + ?11, \
            units_held_back = units_held_back + ?12, \
            units_unfinished = units_unfinished + ?13, \
            housekeeping_waiting = housekeeping_waiting + ?14 \
         WHERE op_id = ?1 AND dir_id = ?2",
    )?;
    for (dir_id, node) in &placed {
        let child_dirs = new_children.get(dir_id).copied().unwrap_or(0);
        if node.self_seq.is_none() && child_dirs == 0 && node.direct_files == 0 && node.totals.is_zero() {
            continue;
        }
        let t = &node.totals;
        update.execute(rusqlite::params![
            op_id,
            dir_id,
            node.self_seq,
            child_dirs,
            node.direct_files,
            t.files,
            t.bytes,
            t.units_selectable,
            t.unit_files_selectable,
            t.unit_bytes_selectable,
            t.units_rolled_back,
            t.units_held_back,
            t.units_unfinished,
            t.housekeeping_waiting,
        ])?;
    }
    Ok(())
}

/// Mark `op_id`'s summary unbuilt: the next open of its tree rebuilds it exactly from
/// its rows, and until then the writer maintains nothing for it.
pub(crate) fn mark_unbuilt(conn: &Connection, op_id: &str) -> Result<(), OperationLogStoreError> {
    conn.prepare_cached("UPDATE operations SET tree_summary = NULL WHERE op_id = ?1")?
        .execute(rusqlite::params![op_id])?;
    Ok(())
}

/// How many rows [`aggregate_op`] and a backfill read per query.
pub(crate) const AGGREGATE_CHUNK: i64 = 4_096;

/// The summary `op_id`'s rows add up to, from scratch: what the writer-maintained
/// rows must always equal, and what a backfill writes. Reads the rows in `seq`
/// chunks, so it never holds the op in memory, only its folders.
#[cfg(test)]
pub(crate) fn aggregate_op(conn: &Connection, op_id: &str) -> Result<Vec<TreeDirRow>, OperationLogStoreError> {
    let Some(context) = read_op_tree_context(conn, op_id)? else {
        return Ok(Vec::new());
    };
    let mut delta = TreeDelta::default();
    let mut after = i64::MIN;
    loop {
        let chunk = read_facts_after(conn, op_id, after, AGGREGATE_CHUNK)?;
        let Some(last) = chunk.last() else { break };
        after = last.seq;
        for facts in &chunk {
            delta.add_row(context.kind, context.axis, facts, 1);
        }
    }
    delta.into_rows(conn)
}

/// Up to `limit` of `op_id`'s rows with a `seq` past `after`, in `seq` order (served by
/// the `(op_id, seq)` index). Capture hands out one `seq` per row, so `seq` alone
/// pages without a gap.
pub(crate) fn read_facts_after(
    conn: &Connection,
    op_id: &str,
    after: i64,
    limit: i64,
) -> Result<Vec<RowFacts>, OperationLogStoreError> {
    let sql =
        format!("SELECT {FACT_COLUMNS} FROM operation_items WHERE op_id = ?1 AND seq > ?2 ORDER BY seq ASC LIMIT ?3");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![op_id, after, limit])?;
    let mut chunk = Vec::new();
    while let Some(row) = rows.next()? {
        chunk.push(map_facts(row)?);
    }
    Ok(chunk)
}

impl TreeDelta {
    /// The summary rows a delta of WHOLE rows adds up to (every row added once, none
    /// taken away): each folder with its ancestors, `child_dirs` counted among them.
    pub(crate) fn into_rows(self, conn: &Connection) -> Result<Vec<TreeDirRow>, OperationLogStoreError> {
        let placed = self.roll_up(conn)?;
        let mut child_dirs: HashMap<i64, i64> = HashMap::new();
        for node in placed.values() {
            if let Some(parent) = node.parent_dir_id {
                *child_dirs.entry(parent).or_default() += 1;
            }
        }
        Ok(placed
            .into_iter()
            .map(|(dir_id, node)| TreeDirRow {
                dir_id,
                parent_dir_id: node.parent_dir_id,
                name_folded: node.name_folded,
                self_seq: node.self_seq,
                child_dirs: child_dirs.get(&dir_id).copied().unwrap_or(0),
                direct_files: node.direct_files,
                totals: node.totals,
            })
            .collect())
    }
}

/// Every summary row of `op_id`, by `dir_id`.
#[cfg_attr(not(test), expect(dead_code, reason = "the tree view is its first non-test caller"))]
pub(crate) fn read_tree_dirs(conn: &Connection, op_id: &str) -> Result<Vec<TreeDirRow>, OperationLogStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT dir_id, parent_dir_id, name_folded, self_seq, child_dirs, direct_files, files, bytes, \
            units_selectable, unit_files_selectable, unit_bytes_selectable, units_rolled_back, \
            units_held_back, units_unfinished, housekeeping_waiting \
         FROM operation_tree_dirs WHERE op_id = ?1 ORDER BY dir_id",
    )?;
    let rows = stmt.query_map(rusqlite::params![op_id], map_tree_dir)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Maps one `operation_tree_dirs` row selected as its 15 columns in declaration order
/// (`dir_id` … `housekeeping_waiting`), the shape every summary read selects.
pub(crate) fn map_tree_dir(row: &rusqlite::Row<'_>) -> rusqlite::Result<TreeDirRow> {
    Ok(TreeDirRow {
        dir_id: row.get(0)?,
        parent_dir_id: row.get(1)?,
        name_folded: row.get(2)?,
        self_seq: row.get(3)?,
        child_dirs: row.get(4)?,
        direct_files: row.get(5)?,
        totals: SubtreeTotals {
            files: row.get(6)?,
            bytes: row.get(7)?,
            units_selectable: row.get(8)?,
            unit_files_selectable: row.get(9)?,
            unit_bytes_selectable: row.get(10)?,
            units_rolled_back: row.get(11)?,
            units_held_back: row.get(12)?,
            units_unfinished: row.get(13)?,
            housekeeping_waiting: row.get(14)?,
        },
    })
}

/// Whether a BUILT summary shows that a rollback already reversed or held back any
/// of `op_id`'s rows, read off its volume-root rows (a handful). `None` when the
/// summary isn't built, so the caller falls back to reading the rows.
pub(crate) fn summary_shows_rollback_outcome(
    conn: &Connection,
    op_id: &str,
) -> Result<Option<bool>, OperationLogStoreError> {
    match read_op_tree_context(conn, op_id)? {
        Some(context) if context.built => {}
        _ => return Ok(None),
    }
    let touched: i64 = conn
        .prepare_cached(
            "SELECT COALESCE(SUM(units_rolled_back + units_held_back), 0) FROM operation_tree_dirs \
             WHERE op_id = ?1 AND parent_dir_id IS NULL",
        )?
        .query_row(rusqlite::params![op_id], |row| row.get(0))?;
    Ok(Some(touched > 0))
}
