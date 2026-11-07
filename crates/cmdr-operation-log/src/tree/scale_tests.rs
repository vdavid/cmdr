//! The tree's scale gates (`DETAILS.md` § "The tree view").
//!
//! - **Structural, always on**: every interactive read's query plan scans no
//!   `operation_items` or `operation_tree_dirs`, and its SQLite VM steps stay flat
//!   between a journal and one ten times bigger. Rust tests build SQLite at `-O0`, so a
//!   millisecond budget on a small twin would hide an O(op) read; a step count can't.
//! - **Timed, release only**: two 2M-row ops, each interactive read inside its budget.
//!
//! Fixtures are seeded straight into a temp journal in one transaction, and their
//! summaries built through the real aggregate (`summary::aggregate_op`), so a seed of
//! millions of rows costs seconds, not the writer's minutes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use rusqlite::Connection;

use super::TreeAxis;
use super::read::{
    FileSql, OpReader, SQL_CHILD_DIR_AT, SQL_CHILD_DIR_AT_DESC, SQL_CHILD_DIRS_FROM, SQL_CHILD_DIRS_UPTO,
    SQL_COUNT_DIRS_AFTER, SQL_COUNT_DIRS_BEFORE, SQL_DIR, SQL_DIR_NAME, SQL_ROOTS, sql_first_rows, sql_item,
};
use super::rollback_filter::{Narrowed, narrow};
use super::selection::{RollbackSelection, Selection, SelectionBase, SelectionToggle};
use super::status::BackfillRegistry;
use super::summary::{FACT_COLUMNS, aggregate_op, map_facts, read_op_tree_context};
use super::view::{OperationTreeRequest, OperationTreeView, TreeRowKey, operation_tree};
use crate::store::{
    OperationLogStore, build_tree_item_indexes, fold_name, intern_child_dir, intern_dir, open_read_connection,
    operation_log_db_path, read_rollback_rows_page,
};

/// A seeded journal.
struct Seeded {
    path: PathBuf,
    /// Interned folder ids by path.
    dirs: HashMap<String, i64>,
    _dir: tempfile::TempDir,
}

/// The shape of the two seeded ops, by size.
#[derive(Clone, Copy)]
struct Shape {
    /// The delete's one folder of direct files.
    big: u64,
    /// The delete's one folder of child folders (one file each).
    many: u64,
    /// The delete's other files, 100 per folder in a two-level tree.
    rest: u64,
    /// The copy's files, `per_folder` per created folder.
    copied: u64,
    per_folder: u64,
}

impl Shape {
    /// The structural gate's shape: the folders the reads open stay the same size,
    /// only the rest of the op grows, so an O(op) read is the only thing that can grow.
    fn gate(total: u64) -> Shape {
        Shape {
            big: 600,
            many: 150,
            rest: total - 750,
            copied: total,
            per_folder: 100,
        }
    }
}

struct Writer<'c> {
    conn: &'c Connection,
    dirs: HashMap<String, i64>,
}

impl Writer<'_> {
    fn dir(&mut self, path: &str) -> i64 {
        if let Some(&id) = self.dirs.get(path) {
            return id;
        }
        let id = intern_dir(self.conn, "v", path).expect("intern");
        self.dirs.insert(path.to_string(), id);
        id
    }
}

fn header(conn: &Connection, op_id: &str, kind: &str, state: &str) {
    conn.execute(
        "INSERT INTO operations (op_id, kind, initiator, execution_status, rollback_state, started_at, ended_at, \
         tree_summary) VALUES (?1, ?2, 'user', 'done', ?3, 1, 2, 'built')",
        rusqlite::params![op_id, kind, state],
    )
    .expect("header");
}

fn seed(shape: Shape) -> Seeded {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    drop(OperationLogStore::open(&path, &|_| None).expect("open store"));
    let conn = cmdr_fs::sqlite_util::open(&path).expect("write connection");
    conn.execute_batch("PRAGMA synchronous = OFF;").expect("pragma");
    let tx = conn.unchecked_transaction().expect("tx");
    let mut w = Writer {
        conn: &tx,
        dirs: HashMap::new(),
    };
    header(&tx, "del", "delete", "not_rollbackable");
    header(&tx, "copy", "copy", "rollbackable");
    {
        let mut insert = tx
            .prepare(
                "INSERT INTO operation_items (op_id, seq, entry_type, row_role, source_dir_id, source_name, \
                 source_name_folded, dest_dir_id, dest_name, dest_name_folded, size, mtime, outcome, self_dir_id) \
                 VALUES (?1, ?2, ?3, 'rollback_unit', ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, 'done', ?11)",
            )
            .expect("prepare");
        // Each op's seqs start at 0.
        let seq = std::cell::Cell::new(0i64);
        let mut row = |op: &str, entry: &str, src: i64, name: &str, dest: Option<i64>, own: Option<i64>| {
            let folded = fold_name(name);
            let (dest_name, dest_folded) = match dest {
                Some(_) => (Some(name), Some(folded.as_str())),
                None => (None, None),
            };
            insert
                .execute(rusqlite::params![
                    op,
                    seq.get(),
                    entry,
                    src,
                    name,
                    folded,
                    dest,
                    dest_name,
                    dest_folded,
                    1_000i64,
                    own
                ])
                .expect("insert");
            seq.set(seq.get() + 1);
        };
        // The delete: names hashed out of order, so only the index orders them.
        let big = w.dir("/data/big");
        for i in 0..shape.big {
            row(
                "del",
                "file",
                big,
                &format!("f{:08}", scramble(i, shape.big)),
                None,
                None,
            );
        }
        w.dir("/data/many");
        for i in 0..shape.many {
            let folder = w.dir(&format!("/data/many/m{:07}", scramble(i, shape.many)));
            row("del", "file", folder, "only", None, None);
        }
        for i in 0..shape.rest {
            let (a, b) = (i / 100 / 100, i / 100 % 100);
            let folder = w.dir(&format!("/data/deep/a{a:05}/b{b:03}"));
            row("del", "file", folder, &format!("r{:03}", i % 100), None, None);
        }
        seq.set(0);
        // The copy: one copied folder of `per_folder`-file subfolders, created dirs last.
        let folders = shape.copied.div_ceil(shape.per_folder);
        for i in 0..shape.copied {
            let sub = format!("s{:06}", i / shape.per_folder);
            let src = w.dir(&format!("/src/copy/{sub}"));
            let dest = w.dir(&format!("/dst/copy/{sub}"));
            row(
                "copy",
                "file",
                src,
                &format!("c{:04}", i % shape.per_folder),
                Some(dest),
                None,
            );
        }
        let parent = w.dir("/dst/copy");
        for i in 0..folders {
            let name = format!("s{i:06}");
            let own = intern_child_dir(&tx, parent, &name).expect("own");
            row("copy", "dir", parent, &name, Some(parent), Some(own));
        }
        let dst = w.dir("/dst");
        let own = intern_child_dir(&tx, dst, "copy").expect("own");
        row("copy", "dir", dst, "copy", Some(dst), Some(own));
    }
    let dirs = std::mem::take(&mut w.dirs);
    tx.commit().expect("commit");
    build_tree_item_indexes(&conn).expect("indexes");
    for op in ["del", "copy"] {
        let rows = aggregate_op(&conn, op).expect("aggregate");
        let mut put = conn
            .prepare(
                "INSERT INTO operation_tree_dirs (op_id, dir_id, parent_dir_id, name_folded, self_seq, child_dirs, \
                 direct_files, files, bytes, units_selectable, unit_files_selectable, unit_bytes_selectable, \
                 units_rolled_back, units_held_back, units_unfinished, housekeeping_waiting) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            )
            .expect("prepare");
        for r in rows {
            let t = r.totals;
            put.execute(rusqlite::params![
                op,
                r.dir_id,
                r.parent_dir_id,
                r.name_folded,
                r.self_seq,
                r.child_dirs,
                r.direct_files,
                t.files,
                t.bytes,
                t.units_selectable,
                t.unit_files_selectable,
                t.unit_bytes_selectable,
                t.units_rolled_back,
                t.units_held_back,
                t.units_unfinished,
                t.housekeeping_waiting
            ])
            .expect("summary row");
        }
    }
    conn.execute_batch("ANALYZE;").ok();
    Seeded { path, dirs, _dir: dir }
}

/// `i` permuted within `0..n`, so names don't arrive in order.
fn scramble(i: u64, n: u64) -> u64 {
    (i * 7_919) % n.max(1)
}

fn request(expanded: &[i64], offset: u32) -> OperationTreeRequest {
    OperationTreeRequest {
        expanded: expanded.iter().map(|&dir_id| TreeRowKey::Folder { dir_id }).collect(),
        selection: None,
        cursor: None,
        offset,
        limit: 200,
    }
}

/// One interactive read of the gate, named.
struct Read {
    name: &'static str,
    run: Box<dyn Fn(&Connection) -> u64>,
}

fn tree(conn: &Connection, op: &str, request: &OperationTreeRequest) -> OperationTreeView {
    operation_tree(conn, op, request, &BackfillRegistry::default())
        .expect("tree")
        .expect("op")
}

/// The interactive reads, over a seed of `shape`. Each returns a number its result
/// must show at any size, so the gate also proves the read did its work.
fn reads(seeded: &Seeded, shape: Shape) -> Vec<Read> {
    let big = seeded.dirs["/data/big"];
    let many = seeded.dirs["/data/many"];
    let copy = seeded.dirs["/dst/copy"];
    let (b, m) = (shape.big as u32, shape.many as u32);
    let file_toggles: Vec<SelectionToggle> = (0..3)
        .map(|i| SelectionToggle::Item { seq: i * 37, on: true })
        .collect();
    let scattered: Vec<SelectionToggle> = (0..2_000)
        .map(|i| SelectionToggle::Item {
            seq: (i * 7 + 3) % (shape.copied as i64),
            on: false,
        })
        .collect();
    vec![
        Read {
            name: "tree open",
            run: Box::new(|c| u64::from(tree(c, "del", &request(&[], 0)).total_rows)),
        },
        Read {
            name: "unfold the big folder",
            run: Box::new(move |c| tree(c, "del", &request(&[big], 0)).rows.len() as u64),
        },
        Read {
            name: "the middle of the big folder",
            run: Box::new(move |c| tree(c, "del", &request(&[big], 1 + b / 2)).rows.len() as u64),
        },
        Read {
            name: "the end of the big folder",
            run: Box::new(move |c| tree(c, "del", &request(&[big], b - 50)).rows.len() as u64),
        },
        Read {
            name: "the middle of the folder of folders",
            run: Box::new(move |c| tree(c, "del", &request(&[many], 3 + m / 2)).rows.len() as u64),
        },
        Read {
            name: "the cursor in the middle of the big folder",
            run: Box::new(move |c| {
                let mut r = request(&[big], 0);
                r.cursor = Some(TreeRowKey::Item { seq: i64::from(b / 2) });
                tree(c, "del", &r).cursor_index.map_or(0, |_| 1)
            }),
        },
        Read {
            name: "N of M with the copied folder off",
            run: Box::new(move |c| {
                let mut r = request(&[], 0);
                r.selection = Some(RollbackSelection {
                    base: SelectionBase::Everything,
                    toggles: vec![SelectionToggle::Folder {
                        dir_id: copy,
                        on: false,
                    }],
                });
                tree(c, "copy", &r).tally.selected + 1
            }),
        },
        Read {
            name: "N of M with three files on",
            run: Box::new(move |c| {
                let mut r = request(&[], 0);
                r.selection = Some(RollbackSelection {
                    base: SelectionBase::Nothing,
                    toggles: file_toggles.clone(),
                });
                tree(c, "copy", &r).tally.selected
            }),
        },
        Read {
            name: "N of M with 2,000 scattered items off",
            run: Box::new(move |c| {
                let mut r = request(&[], 0);
                r.selection = Some(RollbackSelection {
                    base: SelectionBase::Everything,
                    toggles: scattered.clone(),
                });
                let view = tree(c, "copy", &r);
                view.tally.selectable - view.tally.selected
            }),
        },
        Read {
            name: "the engine's decision for 2,000 rows",
            run: Box::new(|c| engine_decisions(c, 2_000)),
        },
    ]
}

/// The three-file selection the engine cells decide.
fn three_files() -> RollbackSelection {
    RollbackSelection {
        base: SelectionBase::Nothing,
        toggles: (0..3)
            .map(|i| SelectionToggle::Item { seq: i * 37, on: true })
            .collect(),
    }
}

/// The engine's per-row decision over the copy's first `rows` rows, with a
/// three-file selection: how many it takes.
fn engine_decisions(conn: &Connection, rows: i64) -> u64 {
    let context = read_op_tree_context(conn, "copy").expect("context").expect("op");
    let mut reader = OpReader::new(conn, "copy", context);
    let mut resolved = Selection::resolve(&mut reader, &three_files()).expect("resolve");
    let sql = format!("SELECT {FACT_COLUMNS} FROM operation_items WHERE op_id = 'copy' AND seq < ?1 ORDER BY seq");
    let mut stmt = conn.prepare(&sql).expect("prepare");
    let mut found = stmt.query([rows]).expect("query");
    let mut taken = 0;
    while let Some(row) = found.next().expect("row") {
        let facts = map_facts(row).expect("facts");
        if resolved.includes(&mut reader, &facts).expect("decide") {
            taken += 1;
        }
    }
    taken
}

/// The rollback engine's skip pass over the copy, as `execute_rollback_with_selection`
/// runs it: every row it streams (`read_rollback_rows_page`, a page at a time), each
/// through the run's filter, with a three-file selection. How many it takes.
fn engine_skip_pass(conn: &Connection) -> u64 {
    let mut filter = match narrow(conn, "copy", &three_files()).expect("narrow") {
        Narrowed::Some(filter) => *filter,
        Narrowed::Everything | Narrowed::Refused(_) => panic!("three files narrow the run"),
    };
    let mut before = i64::MAX;
    let mut taken = 0;
    loop {
        let rows = read_rollback_rows_page(conn, "copy", before, SKIP_PASS_PAGE).expect("page");
        let Some(last) = rows.last() else { break };
        before = last.seq;
        let mut page = filter.on(conn);
        for row in &rows {
            if page.admits(row).expect("decide") {
                taken += 1;
            }
        }
    }
    taken
}

/// Stream the copy's rows as [`engine_skip_pass`] does, deciding nothing: what an
/// unfiltered rollback pays before its first act.
fn stream_only(conn: &Connection) -> u64 {
    let mut before = i64::MAX;
    let mut streamed = 0;
    loop {
        let rows = read_rollback_rows_page(conn, "copy", before, SKIP_PASS_PAGE).expect("page");
        let Some(last) = rows.last() else { break };
        before = last.seq;
        streamed += rows.len() as u64;
    }
    streamed
}

/// The engine's page size (`rollback.rs::ROLLBACK_PAGE`).
const SKIP_PASS_PAGE: u32 = 512;

/// SQLite VM steps `read` takes on `conn`, counted by its progress handler.
fn vm_steps(conn: &mut Connection, read: &Read) -> (u64, u64) {
    let steps = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&steps);
    conn.progress_handler(
        1,
        Some(move || {
            counter.fetch_add(1, Ordering::Relaxed);
            false
        }),
    )
    .expect("progress handler");
    let answer = (read.run)(conn);
    conn.progress_handler(1, None::<fn() -> bool>).expect("clear handler");
    (steps.load(Ordering::Relaxed), answer)
}

/// Every statement the tree reads with, under `EXPLAIN QUERY PLAN`: none scans an op's
/// rows or summary.
#[test]
fn no_interactive_read_scans_an_op() {
    let seeded = seed(Shape::gate(3_000));
    let conn = open_read_connection(&seeded.path).expect("read");
    let mut statements: Vec<String> = [
        SQL_DIR,
        SQL_ROOTS,
        SQL_CHILD_DIR_AT,
        SQL_CHILD_DIR_AT_DESC,
        SQL_CHILD_DIRS_FROM,
        SQL_CHILD_DIRS_UPTO,
        SQL_COUNT_DIRS_BEFORE,
        SQL_COUNT_DIRS_AFTER,
        SQL_DIR_NAME,
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([sql_item(), sql_first_rows()])
    .collect();
    for axis in [TreeAxis::Source, TreeAxis::Dest] {
        let files = FileSql::for_axis(axis);
        statements.extend([files.asc, files.desc, files.before, files.after]);
    }
    for sql in &statements {
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).expect("explain");
        let count = stmt.parameter_count();
        let params: Vec<i64> = (0..count).map(|_| 1).collect();
        let plan: Vec<String> = stmt
            .query_map(rusqlite::params_from_iter(params), |row| row.get::<_, String>(3))
            .expect("plan")
            .collect::<Result<_, _>>()
            .expect("plan rows");
        for line in &plan {
            // A subquery or merge co-routine is read, not scanned off a table.
            let scans_a_table = line.starts_with("SCAN")
                && !line.contains("USING")
                && !line.starts_with("SCAN (")
                && line != "SCAN CONSTANT ROW";
            let scans_a_tree_table =
                line.starts_with("SCAN operation_items") || line.starts_with("SCAN operation_tree_dirs");
            assert!(
                !scans_a_table && !scans_a_tree_table,
                "a full scan in\n{sql}\nplan: {plan:#?}"
            );
            assert!(!line.contains("TEMP B-TREE"), "a sort in\n{sql}\nplan: {plan:#?}");
        }
    }
}

/// Each interactive read's VM steps at 3,000 rows and at 30,000: an O(op) read grows
/// tenfold, an O(expansion + window) one stays flat.
#[test]
fn interactive_reads_stay_flat_as_the_journal_grows() {
    let small = Shape::gate(3_000);
    let large = Shape::gate(30_000);
    let (a, b) = (seed(small), seed(large));
    let mut small_conn = open_read_connection(&a.path).expect("read");
    let mut large_conn = open_read_connection(&b.path).expect("read");
    for (read_small, read_large) in reads(&a, small).iter().zip(reads(&b, large).iter()) {
        let (small_steps, small_answer) = vm_steps(&mut small_conn, read_small);
        let (large_steps, large_answer) = vm_steps(&mut large_conn, read_large);
        assert!(small_answer > 0, "{}: did no work", read_small.name);
        assert!(large_answer > 0, "{}: did no work", read_small.name);
        assert!(
            large_steps <= small_steps * 2 + 1_000,
            "{}: {small_steps} VM steps at 3,000 rows, {large_steps} at 30,000",
            read_small.name
        );
    }
}

/// The 2M scale test: two 2M-row ops, every interactive read timed against its
/// release budget.
///
/// ❗ The budgets are RELEASE numbers, and seeding 4M rows takes about 40 s (the reads
/// themselves well under a second), far past the two-second test bar, so it's ignored
/// by default and its twin below runs instead. Run it under `cargo test`, since
/// nextest's 8 s cap stops it (the pattern of `store/migration_bench.rs`):
/// `cargo test --release -p cmdr-operation-log --lib two_two_million_row_ops_answer_inside_their_budgets -- --ignored --nocapture`
#[test]
#[ignore = "release-only scale test; see its doc comment for the command"]
fn two_two_million_row_ops_answer_inside_their_budgets() {
    let shape = Shape {
        big: 1_000_000,
        many: 100_000,
        rest: 900_000,
        copied: 2_000_000,
        per_folder: 1_000,
    };
    let started = Instant::now();
    let seeded = seed(shape);
    eprintln!("seeded 4M rows in {:?}", started.elapsed());
    timed(&seeded, shape, &budgets());
    eprintln!("the whole test took {:?}", started.elapsed());
}

/// The 2M test's twin at 20,000 rows, run every time: the same reads, answers checked,
/// no budgets (a test build's SQLite is unoptimized).
#[test]
fn the_scale_reads_answer_on_a_small_twin() {
    let shape = Shape::gate(20_000);
    let seeded = seed(shape);
    timed(&seeded, shape, &HashMap::new());
}

/// Release budgets per read, in ms: about three times what each took on an Apple
/// Silicon laptop (2026-11-07), so a busy machine stays inside them. Measured there:
/// tree open 0.2, unfold 0.6, the middle of the 1M-file folder 15, its end 0.4, the
/// middle of 100,000 child folders 2, the cursor in the 1M-file folder 29, "N of M"
/// with a folder off 0.3, with three files on 0.3, with 2,000 items off 23, the
/// engine's decision for 2,000 rows 1.2. The OFFSET inside one folder is the only
/// part that grows with a folder (half of it at most), which is why the middle of the
/// biggest folder is the budgeted worst case.
fn budgets() -> HashMap<&'static str, u64> {
    HashMap::from([
        ("tree open", 10),
        ("unfold the big folder", 10),
        ("the middle of the big folder", 50),
        ("the end of the big folder", 10),
        ("the middle of the folder of folders", 10),
        ("the cursor in the middle of the big folder", 100),
        ("N of M with the copied folder off", 10),
        ("N of M with three files on", 10),
        ("N of M with 2,000 scattered items off", 75),
        ("the engine's decision for 2,000 rows", 10),
    ])
}

/// What deciding every row may add to streaming them. Through the engine's own page
/// read (`read_rollback_rows_page`, every column, 512 a page), the copy's 2M rows
/// streamed in 2.62 s and the three-file skip pass took 2.83 s (release, Apple Silicon
/// laptop, 2026-11-08): the decision adds 8%, the stream is the cost, and an
/// unfiltered rollback pays that stream too.
const DECISION_OVERHEAD: f64 = 0.25;

fn timed(seeded: &Seeded, shape: Shape, budgets: &HashMap<&'static str, u64>) {
    let conn = open_read_connection(&seeded.path).expect("read");
    let mut over = Vec::new();
    for read in reads(seeded, shape) {
        // Once to warm the page cache as an open dialog would, once timed.
        (read.run)(&conn);
        let started = Instant::now();
        let answer = (read.run)(&conn);
        let took = started.elapsed();
        eprintln!("{:>45}: {:>8.2?} (answer {answer})", read.name, took);
        assert!(answer > 0, "{}: did no work", read.name);
        if let Some(&ms) = budgets.get(read.name)
            && took > Duration::from_millis(ms)
        {
            over.push(format!("{}: {took:?} > {ms} ms", read.name));
        }
    }
    // The skip pass streams every row the way a rollback does today, so it's budgeted
    // against streaming alone: the decision may add a quarter on top.
    let started = Instant::now();
    let streamed = stream_only(&conn);
    let stream_took = started.elapsed();
    eprintln!(
        "{:>45}: {stream_took:>8.2?} ({streamed} rows)", // allowed-pluralize-noun: a 2M-row bench line
        "streaming every row, no decision"
    );
    let started = Instant::now();
    let taken = engine_skip_pass(&conn);
    let took = started.elapsed();
    eprintln!(
        "{:>45}: {took:>8.2?} (took {taken})",
        "the engine's skip pass over every row"
    );
    assert_eq!(taken, 3, "exactly the three selected files");
    let allowed = stream_took.mul_f64(1.0 + DECISION_OVERHEAD) + Duration::from_millis(20);
    if !budgets.is_empty() && took > allowed {
        over.push(format!("skip pass: {took:?} > {allowed:?}"));
    }
    assert!(over.is_empty(), "over budget: {over:#?}");
}
