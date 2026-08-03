//! Unit tests for the importance writer thread.
//!
//! A sibling file rather than an inline `mod tests`, matching the rest of the repo:
//! reading the writer shouldn't mean scrolling past its test suite.

use super::*;
use crate::importance::store::{ImportanceStore, importance_db_path, open_read_connection};

/// The on-disk size of the DB's `-wal` sidecar, or 0 if it's absent.
fn wal_len(db_path: &Path) -> u64 {
    std::fs::metadata(db_path.with_extension("db-wal"))
        .map(|m| m.len())
        .unwrap_or(0)
}

/// A fresh importance store + writer over a scratch volume.
fn writer(dir: &Path) -> (ImportanceWriter, PathBuf) {
    let db_path = importance_db_path(dir, "root");
    ImportanceStore::open(&db_path).expect("open store");
    let w = ImportanceWriter::spawn(&db_path).expect("spawn writer");
    (w, db_path)
}

/// Write a full pass of `n` rows through `w` and block until it commits.
fn write_pass(w: &ImportanceWriter, n: usize) {
    let rows: Vec<WeightRow> = (0..n)
        .map(|i| WeightRow {
            path: format!("/folder/{i}"),
            score: 0.5,
            signals_json: "{}".to_string(),
        })
        .collect();
    let generation = w.next_generation().expect("generation");
    w.write_weights(generation, rows).expect("write weights");
    w.flush_blocking().expect("flush");
}

/// One weight row at `path` scoring `score`, with the neutral signals blob.
fn row(path: &str, score: f64) -> WeightRow {
    WeightRow {
        path: path.to_string(),
        score,
        signals_json: "{}".to_string(),
    }
}

/// One weight row whose SIGNALS differ from [`row`]'s — what makes an incremental
/// treat it as genuinely moved. `file_count` stands in for any signal here; the
/// writer only ever compares the blob as bytes.
fn moved_row(path: &str, score: f64, file_count: u32) -> WeightRow {
    WeightRow {
        path: path.to_string(),
        score,
        signals_json: format!("{{\"fileCount\":{file_count}}}"),
    }
}

/// Sort a delta's two lists so assertions don't depend on SQLite's row order.
fn sorted(mut delta: WeightDelta) -> WeightDelta {
    delta.upserted.sort_by(|a, b| a.0.cmp(&b.0));
    delta.removed.sort();
    delta
}

/// An incremental reports exactly what a weight-map consumer has to apply: the
/// rows it wrote, and the paths that LEFT the store. The removed paths can only
/// come from here — a consumer keying on a path hash can't expand a cleared
/// subtree root into the keys to drop, because hashes carry no prefix structure.
#[test]
fn an_incremental_reports_the_rows_it_wrote_and_the_ones_it_cleared() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, _db_path) = writer(dir.path());

    // A full pass seeds a subtree of three plus an unrelated folder.
    w.write_weights(
        1,
        vec![row("/a", 0.2), row("/a/keep", 0.4), row("/a/drop", 0.6), row("/b", 0.8)],
    )
    .expect("full pass");
    w.flush_blocking().expect("flush");

    // The incremental rescores `/a`'s subtree: two of its three folders come back
    // with moved signals, so `/a/drop` is gone (deleted, renamed away, or newly
    // floored).
    let delta = sorted(
        w.write_weights_incremental(
            1,
            vec![moved_row("/a", 0.25, 1), moved_row("/a/keep", 0.45, 1)],
            vec!["/a".to_string()],
        )
        .expect("incremental")
        .delta
        .expect("a two-row pass is small enough to describe"),
    );

    assert_eq!(
        delta.upserted,
        vec![("/a".to_string(), 0.25), ("/a/keep".to_string(), 0.45)],
        "the rows the pass wrote"
    );
    assert_eq!(
        delta.removed,
        vec!["/a/drop".to_string()],
        "only the folder that left: a cleared-then-rewritten path nets down to its upsert, \
         and `/b` was never in the cleared subtree"
    );
    w.shutdown();
}

/// A row rescored to `0.0` is a REMOVAL, not an upsert. The store keeps it (it
/// isn't floored) but `for_each_nonzero_weight` skips it, and the delta describes
/// that non-zero view — which is what keeps a patched map equal to a rebuilt one.
#[test]
fn a_row_rescored_to_zero_is_reported_as_a_removal() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, _db_path) = writer(dir.path());
    w.write_weights(1, vec![row("/a", 0.5)]).expect("full pass");
    w.flush_blocking().expect("flush");

    let delta = w
        .write_weights_incremental(1, vec![moved_row("/a", 0.0, 1)], vec!["/a".to_string()])
        .expect("incremental")
        .delta
        .expect("describable");

    assert!(delta.upserted.is_empty(), "a zero score carries no ranking signal");
    assert_eq!(delta.removed, vec!["/a".to_string()], "so it leaves the non-zero set");
    w.shutdown();
}

/// A row whose SIGNALS are unchanged is not written, however much its score moved.
///
/// The equality key that stops the 60-second treadmill. A score is a function of
/// the signals AND `now_secs`, so it drifts every pass on its own; only the
/// signals say whether the folder itself moved. ❌ Don't relax this to a score
/// comparison — `docs/notes/importance-treadmill-2026-08-04.md` measured it at
/// 99.88% of rows skippable against 0.03%.
#[test]
fn a_row_whose_signals_are_unchanged_is_not_rewritten() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path());
    w.write_weights(1, vec![row("/a", 0.5), row("/a/child", 0.5)])
        .expect("full pass");
    w.flush_blocking().expect("flush");

    // Same signals, a visibly different score: the pass writes nothing.
    let write = w
        .write_weights_incremental(1, vec![row("/a", 0.9), row("/a/child", 0.1)], vec!["/a".to_string()])
        .expect("incremental");

    assert_eq!(write.count, 0, "nothing about either folder moved");
    assert_eq!(
        write.delta.expect("describable"),
        WeightDelta::default(),
        "so a weight-map consumer has nothing to apply"
    );
    let conn = open_read_connection(&db_path).expect("read conn");
    let stored: f64 = conn
        .query_row("SELECT score FROM weights WHERE path = '/a'", [], |r| r.get(0))
        .expect("row");
    assert_eq!(stored, 0.5, "the stored score keeps the `now_secs` it was written at");
    w.shutdown();
}

/// Skipping the unchanged rows must not spare a STALE one: the removal and the
/// insert are two halves of ONE decision over the rescored subtree.
///
/// The way this optimization could lose data is by deleting a row the insert then
/// skips, or by keeping one the pass no longer scores. Both siblings sit in the
/// same subtree, and exactly one of them has to go.
#[test]
fn an_unchanged_row_survives_a_pass_that_removes_its_sibling() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path());
    w.write_weights(1, vec![row("/a", 0.5), row("/a/keep", 0.4), row("/a/drop", 0.6)])
        .expect("full pass");
    w.flush_blocking().expect("flush");

    // `/a/drop` is no longer scored; the other two come back byte-identical.
    let write = w
        .write_weights_incremental(1, vec![row("/a", 0.5), row("/a/keep", 0.4)], vec!["/a".to_string()])
        .expect("incremental");

    assert_eq!(write.count, 0, "the two survivors were already stored");
    let delta = write.delta.expect("describable");
    assert!(delta.upserted.is_empty(), "nothing was rewritten");
    assert_eq!(delta.removed, vec!["/a/drop".to_string()], "only the stale row left");

    let conn = open_read_connection(&db_path).expect("read conn");
    let paths: Vec<String> = conn
        .prepare("SELECT path FROM weights ORDER BY path")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect();
    assert_eq!(paths, vec!["/a".to_string(), "/a/keep".to_string()]);
    w.shutdown();
}

/// An incremental that touched nothing still reports an empty delta rather than
/// asking for a reload — a pass a minute must not cost a full weight-map rebuild.
#[test]
fn a_pass_that_changed_nothing_reports_an_empty_delta() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, _db_path) = writer(dir.path());
    let delta = w
        .write_weights_incremental(1, Vec::new(), vec!["/nothing/here".to_string()])
        .expect("incremental")
        .delta
        .expect("describable");
    assert_eq!(delta, WeightDelta::default());
    w.shutdown();
}

/// Past `MAX_DELTA_ROWS` the pass stops describing itself: shipping that many
/// paths approaches the cost of streaming the table back, which is what the delta
/// exists to avoid. The consumer reloads instead.
#[test]
fn a_pass_too_big_to_describe_asks_for_a_reload() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, _db_path) = writer(dir.path());
    let rows: Vec<WeightRow> = (0..=MAX_DELTA_ROWS).map(|i| row(&format!("/big/{i}"), 0.5)).collect();
    assert!(
        w.write_weights_incremental(1, rows, Vec::new())
            .expect("incremental")
            .delta
            .is_none(),
        "past the cap the consumer is told to reload"
    );
    w.shutdown();
}

/// The clear side is capped too: a pass that only DELETES can be just as wide as
/// one that writes, and it's the side a consumer can't reconstruct.
#[test]
fn a_clear_too_big_to_describe_asks_for_a_reload() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, _db_path) = writer(dir.path());
    let rows: Vec<WeightRow> = (0..=MAX_DELTA_ROWS).map(|i| row(&format!("/big/{i}"), 0.5)).collect();
    w.write_weights(1, rows).expect("full pass");
    w.flush_blocking().expect("flush");

    assert!(
        w.write_weights_incremental(1, Vec::new(), vec!["/big".to_string()])
            .expect("incremental")
            .delta
            .is_none(),
        "clearing more than the cap tells the consumer to reload"
    );
    w.shutdown();
}

#[test]
fn checkpoint_truncates_the_wal_at_rest() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path());

    // A committed full pass leaves frames in the WAL; passive autocheckpoint never
    // truncates the file, so it sits non-empty on disk.
    write_pass(&w, 500);
    assert!(wal_len(&db_path) > 0, "the WAL holds frames before the checkpoint");

    // The checkpoint hook truncates it to zero (no reader is blocking).
    w.checkpoint_wal().expect("checkpoint");
    assert_eq!(wal_len(&db_path), 0, "the checkpoint truncated the WAL to zero at rest");

    w.shutdown();
}

#[test]
fn checkpoint_tolerates_a_blocking_reader_without_erroring() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path());
    write_pass(&w, 50);

    // Pin an old read snapshot: an open read transaction holds a WAL read mark, so a
    // later TRUNCATE can't reclaim the frames past it.
    let reader = open_read_connection(&db_path).expect("reader");
    reader.execute_batch("BEGIN").expect("begin read txn");
    let _pinned: i64 = reader
        .query_row("SELECT COUNT(*) FROM weights", [], |r| r.get(0))
        .expect("pin snapshot");

    // Advance the WAL past the reader's snapshot, then checkpoint. The truncate is
    // blocked, but the hook must NOT surface an error (it degrades to PASSIVE).
    write_pass(&w, 50);
    w.checkpoint_wal()
        .expect("checkpoint tolerates the reader without erroring");

    reader.execute_batch("END").ok();

    // The writer keeps working after a blocked checkpoint (the recompute path is intact).
    write_pass(&w, 10);
    w.shutdown();
}
