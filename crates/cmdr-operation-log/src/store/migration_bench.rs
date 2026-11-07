//! What the v6 step costs a big journal at launch, measured on a seeded v5 file.
//!
//! The app opens the journal synchronously in `setup()`, so a slow migration step is
//! a slow launch. This bench seeds 4,000,000 item rows straight into a v5 file, then
//! times the open that migrates it (exactly the app's `open_write_connection`), and
//! reports the WAL growth during the step and the file's permanent growth. Numbers:
//! `DETAILS.md` § "Tree summary". Run it in release (debug builds SQLite at `-O0`):
//!
//! `cargo nextest run --release -p cmdr-operation-log --run-ignored ignored-only -E 'test(bench_v6_migration_on_a_4m_row_journal)' --no-capture`

use std::time::Instant;

use super::migrations::{MIGRATIONS, run_migrations};
use super::open_write_connection;

const OPS: usize = 2_000;
const ROWS_PER_OP: usize = 2_000;
const DIRS_PER_OP: i64 = 100;

fn file_len(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

#[test]
#[ignore = "a 4M-row bench that runs for a minute; see the module docs for the command"]
#[allow(clippy::print_stdout, reason = "a benchmark prints its measurements")]
fn bench_v6_migration_on_a_4m_row_journal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("operation-log.db");
    let wal = dir.path().join("operation-log.db-wal");

    // Seed a v5 file the way production lays it out: same pragmas, the v1–v5 ladder.
    let seed_started = Instant::now();
    {
        let conn = cmdr_fs::sqlite_util::open(&db).expect("open");
        conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .expect("pragmas");
        run_migrations(&conn, &MIGRATIONS[..5]).expect("migrate to v5");
        let tx = conn.unchecked_transaction().expect("tx");
        {
            let mut dir_stmt = tx
                .prepare("INSERT INTO dirs (dir_id, volume_id, parent_dir_id, name, name_folded) VALUES (?1, 'root', ?2, ?3, ?3)")
                .expect("prepare dirs");
            dir_stmt.execute(rusqlite::params![1, None::<i64>, ""]).expect("root");
            let mut op_stmt = tx
                .prepare(
                    "INSERT INTO operations (op_id, kind, initiator, execution_status, rollback_state, started_at, ended_at) \
                     VALUES (?1, 'copy', 'user', 'done', 'rollbackable', ?2, ?2)",
                )
                .expect("prepare ops");
            let mut item_stmt = tx
                .prepare(
                    "INSERT INTO operation_items (op_id, seq, entry_type, row_role, source_dir_id, source_name, \
                     source_name_folded, dest_dir_id, dest_name, dest_name_folded, size, mtime, outcome) \
                     VALUES (?1, ?2, 'file', 'rollback_unit', ?3, ?4, ?5, ?6, ?4, ?5, 123456, 1700000000, 'done')",
                )
                .expect("prepare items");
            let mut next_dir: i64 = 2;
            for op in 0..OPS {
                let op_id = format!("0190a0b0-0000-7000-8000-{op:012}");
                op_stmt.execute(rusqlite::params![op_id, op as i64]).expect("op");
                let first_dir = next_dir;
                for d in 0..DIRS_PER_OP {
                    let name = format!("folder-{op}-{d}");
                    dir_stmt.execute(rusqlite::params![next_dir, 1, name]).expect("dir");
                    next_dir += 1;
                }
                for seq in 0..ROWS_PER_OP {
                    let name = format!("IMG_{seq:05}.jpg");
                    let folded = name.to_lowercase();
                    let source_dir = first_dir + (seq as i64 % (DIRS_PER_OP / 2));
                    // Half the rows carry a dest, so the partial dest index is half full.
                    let dest_dir =
                        (seq % 2 == 0).then_some(first_dir + DIRS_PER_OP / 2 + (seq as i64 % (DIRS_PER_OP / 2)));
                    item_stmt
                        .execute(rusqlite::params![op_id, seq as i64, source_dir, name, folded, dest_dir])
                        .expect("item");
                }
            }
        }
        tx.commit().expect("commit");
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .expect("checkpoint");
    }
    let seeded = file_len(&db);
    println!(
        "seeded {} rows in {:.1?}: v5 file {:.1} MB",
        OPS * ROWS_PER_OP,
        seed_started.elapsed(),
        seeded as f64 / 1e6
    );

    // The launch: the app's own open, which runs the pending v6 step.
    let started = Instant::now();
    let conn = open_write_connection(&db).expect("migrating open");
    let launch = started.elapsed();
    let wal_during = file_len(&wal);
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .expect("checkpoint");
    drop(conn);
    let migrated = file_len(&db);
    let growth = migrated.saturating_sub(seeded);
    println!(
        "v6 open: {launch:.2?}; WAL after the step {:.1} MB; file {:.1} MB → {:.1} MB (+{:.1} MB, {:.1} MB per million rows, {:.0} bytes/row)",
        wal_during as f64 / 1e6,
        seeded as f64 / 1e6,
        migrated as f64 / 1e6,
        growth as f64 / 1e6,
        growth as f64 / 1e6 / ((OPS * ROWS_PER_OP) as f64 / 1e6),
        growth as f64 / (OPS * ROWS_PER_OP) as f64,
    );
}
