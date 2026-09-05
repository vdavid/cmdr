//! Which primitive should cover a frontier? `#[ignore]`d: this prints numbers over
//! a real tree, it doesn't assert.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use cmdr_fs::pluralize::{pluralize, pluralize_with};

use super::*;
use crate::indexing::store::ROOT_ID;

/// Which primitive should cover a frontier: the parallel walker or the serial
/// reconcile? Measured on a REAL tree rather than trusted from the in-tree
/// full-volume numbers, because a frontier is a different workload (all-new
/// ground, a bulk add, never an incremental diff).
///
/// Both are pointed at the same directory with the same fresh empty index, and
/// the time includes draining the writer, because both are writing the same rows
/// and a walk that just fills a queue faster hasn't covered anything yet. Each
/// runs after a warm-up pass over the tree, so neither pays for the other's cold
/// page cache.
///
/// Not a correctness check, so it's `#[ignore]`d. Run it in RELEASE, which is
/// where the difference is honest:
///
/// ```sh
/// CMDR_COVER_BENCH_ROOT=/Applications \
///   cargo test -p cmdr-index --release --lib -- --ignored --nocapture measure_cover_primitives
/// ```
///
/// Results and the call they back: `docs/notes/cover-walk-primitive-2026-08-05.md`.
#[test]
#[ignore = "benchmark over a real tree; run manually with --nocapture"]
fn measure_cover_primitives() {
    let root = PathBuf::from(
        std::env::var("CMDR_COVER_BENCH_ROOT").expect("set CMDR_COVER_BENCH_ROOT to a real directory to walk"),
    );
    assert!(root.is_dir(), "{} isn't a directory", root.display());

    // Emit via a stderr handle rather than `println!` (crate-banned), same as the
    // `bulk_read` bench next door.
    use std::io::Write;
    let mut out = std::io::stderr();

    // Warm the page cache so the first primitive doesn't pay for the second.
    let warm = std::time::Instant::now();
    let warmed = walkdir_count(&root);
    let _ = writeln!(
        out,
        "warm-up: {} under {} in {:?}",
        pluralize_with(warmed, "entry", "entries"),
        root.display(),
        warm.elapsed()
    );

    let parallel = measure_one(&root, Primitive::Parallel);
    let serial = measure_one(&root, Primitive::Serial);

    let _ = writeln!(out, "\n── {} ──", root.display());
    for (name, (elapsed, rows, dirs)) in [("parallel walker", parallel), ("serial reconcile", serial)] {
        let _ = writeln!(
            out,
            "{name:>17}: {elapsed:>10.2?}  {}, {} listed",
            pluralize(rows, "row"),
            pluralize(dirs, "dir"),
        );
    }
    let (p_elapsed, p_rows, _) = parallel;
    let (s_elapsed, s_rows, _) = serial;
    let _ = writeln!(
        out,
        "{:>17}: {:.1}x, and the parallel walk wrote {:.2}% of the serial walk's rows",
        "verdict",
        s_elapsed.as_secs_f64() / p_elapsed.as_secs_f64(),
        100.0 * p_rows as f64 / s_rows.max(1) as f64,
    );
}

#[derive(Clone, Copy)]
enum Primitive {
    Parallel,
    Serial,
}

/// One primitive over `root` into a fresh index: wall clock including the writer
/// drain, the row count it produced, and how many directories it marked listed.
fn measure_one(root: &Path, primitive: Primitive) -> (std::time::Duration, u64, u64) {
    let db_dir = tempfile::tempdir().expect("temp db dir");
    let db_path = db_dir.path().join("bench-index.db");
    IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");

    // Seed the chain down to the walk root, exactly as a real frontier node has it.
    {
        let conn = IndexStore::open_write_connection(&db_path).expect("write connection");
        let mut parent_id = ROOT_ID;
        for component in root.to_string_lossy().split('/').filter(|c| !c.is_empty()) {
            parent_id = match IndexStore::resolve_component(&conn, parent_id, component) {
                Ok(Some(id)) => id,
                _ => IndexStore::insert_entry_v2(&conn, parent_id, component, true, false, None, None, None, None)
                    .expect("seed"),
            };
        }
        let next_id = IndexStore::get_next_id(&conn).expect("next id");
        writer.next_id().fetch_max(next_id, Ordering::Relaxed);
    }

    let space = IndexPathSpace::root();
    let cancel = CancellationToken::new();
    let start = std::time::Instant::now();
    match primitive {
        Primitive::Parallel => {
            cover_subtree(root, &space, &writer, None, &cancel, &WalkHeartbeat::new()).expect("parallel walk");
        }
        Primitive::Serial => {
            let conn = IndexStore::open_read_connection(&db_path).expect("read connection");
            crate::indexing::reconcile::reconciler::reconcile_subtree(root, &space, &conn, &writer, &cancel, None)
                .expect("serial reconcile");
        }
    }
    writer.flush_blocking().expect("flush");
    let elapsed = start.elapsed();
    writer.shutdown();

    let conn = IndexStore::open_read_connection(&db_path).expect("read connection");
    let rows: u64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .expect("count rows");
    let dirs: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE is_directory = 1 AND listed_epoch > 0",
            [],
            |r| r.get(0),
        )
        .expect("count listed dirs");
    (elapsed, rows, dirs)
}

/// A plain recursive count, used only to warm the page cache.
fn walkdir_count(root: &Path) -> u64 {
    let mut count = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            count += 1;
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(entry.path());
            }
        }
    }
    count
}
