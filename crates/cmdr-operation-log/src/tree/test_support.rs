//! A temp journal driven through the real writer, and readers that render an op's
//! summary rows by path, so a golden fixture names folders instead of dir ids.

use std::path::PathBuf;

use rusqlite::Connection;

use super::summary::{TreeDirRow, aggregate_op, read_tree_dirs};
use crate::store::{OperationLogStore, open_read_connection, operation_log_db_path, reconstruct_dir_path};
use crate::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, OpKind, RollbackState, RowRole, SearchCoverage, SkipReason,
};
use crate::writer::{FinalizeOperation, ItemOutcomeUpdate, JournalItem, OpenOperation, OperationLogWriter};

/// A journal in a temp dir, written through the real writer thread.
pub(super) struct Journal {
    pub(super) writer: OperationLogWriter,
    pub(super) path: PathBuf,
    _dir: tempfile::TempDir,
}

pub(super) fn journal() -> Journal {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    drop(OperationLogStore::open(&path, &|_| None).expect("open store"));
    let writer = OperationLogWriter::spawn(&path).expect("spawn writer");
    // As `OperationLog::open` does.
    writer.build_tree_indexes().expect("queue the folder indexes");
    Journal {
        writer,
        path,
        _dir: dir,
    }
}

impl Journal {
    pub(super) fn read(&self) -> Connection {
        self.writer.flush_blocking().expect("flush");
        open_read_connection(&self.path).expect("read connection")
    }

    pub(super) fn open(&self, op_id: &str, kind: OpKind) {
        self.open_with(op_id, kind, None);
    }

    pub(super) fn open_with(&self, op_id: &str, kind: OpKind, rolls_back: Option<&str>) {
        self.writer
            .open_operation(OpenOperation {
                op_id: op_id.into(),
                kind,
                initiator: Initiator::User,
                source_volume_id: Some("v".into()),
                dest_volume_id: None,
                item_count: 0,
                started_at: 1,
                rolls_back_op_id: rolls_back.map(Into::into),
                execution_status: ExecutionStatus::Running,
            })
            .expect("open");
    }

    pub(super) fn record(&self, op_id: &str, items: Vec<JournalItem>) {
        self.writer.record_items(op_id, items).expect("record");
    }

    pub(super) fn outcomes(&self, op_id: &str, updates: &[(i64, ItemOutcome, Option<SkipReason>)]) {
        let updates = updates
            .iter()
            .map(|&(seq, outcome, skip_reason)| ItemOutcomeUpdate {
                seq,
                outcome,
                skip_reason,
            })
            .collect();
        self.writer.set_item_outcomes(op_id, updates).expect("outcomes");
    }

    pub(super) fn finalize(&self, op_id: &str, state: RollbackState, ended_at: i64) {
        self.writer
            .finalize_operation(FinalizeOperation {
                op_id: op_id.into(),
                execution_status: ExecutionStatus::Done,
                rollback_state: state,
                not_rollbackable_reason: None,
                archive_subkind: None,
                search_coverage: SearchCoverage::Full,
                search_coverage_reason: None,
                ended_at,
                item_count: None,
                items_done: 0,
                bytes_total: 0,
                dev_summary: None,
            })
            .expect("finalize");
    }

    /// The op's `tree_summary` column.
    pub(super) fn summary_state(&self, op_id: &str) -> Option<String> {
        self.read()
            .query_row("SELECT tree_summary FROM operations WHERE op_id = ?1", [op_id], |row| {
                row.get(0)
            })
            .expect("op row")
    }

    /// The writer-maintained rows equal a from-scratch aggregate of the op's rows, and
    /// the op still reads built.
    pub(super) fn assert_exact(&self, op_id: &str) {
        let conn = self.read();
        assert_eq!(
            self.summary_state(op_id).as_deref(),
            Some("built"),
            "{op_id} reads built"
        );
        let kept = read_tree_dirs(&conn, op_id).expect("summary rows");
        let fresh = aggregate_op(&conn, op_id).expect("aggregate");
        assert_eq!(kept, fresh, "{op_id}: the maintained summary equals the aggregate");
    }

    /// The op's summary rows, one line per folder, by `volume:path`.
    pub(super) fn golden(&self, op_id: &str) -> Vec<String> {
        let conn = self.read();
        let mut lines: Vec<String> = read_tree_dirs(&conn, op_id)
            .expect("summary rows")
            .iter()
            .map(|row| render(&conn, row))
            .collect();
        lines.sort();
        lines
    }

    /// `volume:path` of the DIR row at `seq`'s own folder.
    pub(super) fn self_dir_path(&self, op_id: &str, seq: i64) -> String {
        let conn = self.read();
        let dir_id: i64 = conn
            .query_row(
                "SELECT self_dir_id FROM operation_items WHERE op_id = ?1 AND seq = ?2",
                rusqlite::params![op_id, seq],
                |row| row.get(0),
            )
            .expect("self_dir_id set");
        place(&conn, dir_id)
    }
}

fn place(conn: &Connection, dir_id: i64) -> String {
    let volume: String = conn
        .query_row("SELECT volume_id FROM dirs WHERE dir_id = ?1", [dir_id], |row| {
            row.get(0)
        })
        .expect("dir row");
    format!("{volume}:{}", reconstruct_dir_path(conn, dir_id).expect("path"))
}

fn render(conn: &Connection, row: &TreeDirRow) -> String {
    let t = &row.totals;
    let mut line = place(conn, row.dir_id);
    if let Some(seq) = row.self_seq {
        line.push_str(&format!(" self={seq}"));
    }
    line.push_str(&format!(
        " kids={} direct={} files={} bytes={} sel={}/{}/{}",
        row.child_dirs,
        row.direct_files,
        t.files,
        t.bytes,
        t.units_selectable,
        t.unit_files_selectable,
        t.unit_bytes_selectable
    ));
    for (label, n) in [
        ("rb", t.units_rolled_back),
        ("held", t.units_held_back),
        ("unfinished", t.units_unfinished),
        ("hk", t.housekeeping_waiting),
    ] {
        if n != 0 {
            line.push_str(&format!(" {label}={n}"));
        }
    }
    line
}

/// One row: `src` and `dest` are `(volume, parent dir, name)`.
pub(super) fn item(
    seq: i64,
    entry_type: EntryType,
    row_role: RowRole,
    src: (&str, &str, &str),
    dest: Option<(&str, &str, &str)>,
    size: Option<i64>,
) -> JournalItem {
    JournalItem {
        seq,
        entry_type,
        row_role,
        source_volume_id: src.0.into(),
        source_dir: src.1.into(),
        source_name: src.2.into(),
        dest_volume_id: dest.map(|d| d.0.into()),
        dest_dir: dest.map(|d| d.1.into()),
        dest_name: dest.map(|d| d.2.into()),
        size,
        mtime: Some(1_700_000_000),
        outcome: ItemOutcome::Done,
        overwrote: false,
        dest_identity: None,
        tags: None,
    }
}

/// A created folder as `record_created_dirs` writes it: the same place both sides.
pub(super) fn created_dir(seq: i64, place: (&str, &str, &str)) -> JournalItem {
    item(seq, EntryType::Dir, RowRole::RollbackUnit, place, Some(place), None)
}
