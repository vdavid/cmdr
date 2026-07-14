//! `importance.db` store tests: smoke round-trip first, then the disposable-cache
//! discipline and the as-of-generation staleness predicate.

use super::*;
use crate::importance::writer::{ImportanceWriter, SUBTREE_CLEAR_SQL, WeightRow};

/// Run `EXPLAIN QUERY PLAN` over the given SQL and return the `detail` column of
/// every step joined by newline. Binds a dummy string for each `?` placeholder (the
/// plan is structural, so the bound values don't matter), which keeps this agnostic
/// to how many parameters the statement carries.
fn explain_plan(conn: &Connection, sql: &str) -> String {
    let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain_sql).expect("prepare explain");
    let n = stmt.parameter_count();
    let dummy = "/some/folder".to_string();
    let params: Vec<&dyn rusqlite::ToSql> = (0..n).map(|_| &dummy as &dyn rusqlite::ToSql).collect();
    stmt.query_map(params.as_slice(), |row| row.get::<_, String>(3))
        .expect("explain rows")
        .map(|r| r.expect("detail"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The incremental rescore's subtree-clear DELETE MUST be index-served, not a full
/// scan of the `weights` table. This is the whole point of the folded-key column:
/// with a custom-collation PK the `LIKE`-prefix clear full-scanned ~166k rows and
/// re-ran the NFD-folding comparison on every one, pegging a CPU core. A BINARY
/// `path_folded` PK lets the equality + half-open range be served by index SEARCHes.
///
/// A full table scan shows as a bare `SCAN weights` with no `USING`; an index or PK
/// lookup shows as `SEARCH`. We reject any bare `SCAN` step. The row count doesn't
/// change the plan (it's structural), so a modest table proves it cheaply.
#[test]
fn subtree_clear_delete_is_index_served() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    let writer = ImportanceWriter::spawn(&path).expect("spawn");

    // Populate a realistically-shaped tree so the planner sees a genuine b-tree.
    let mut rows = Vec::new();
    for i in 0..2_000 {
        rows.push(WeightRow {
            path: format!("/Volumes/data/dir{}/sub{}", i % 200, i),
            score: 0.5,
            signals_json: "{}".to_string(),
        });
    }
    writer.write_weights(1, rows).expect("write");
    writer.flush_blocking().expect("flush");

    let store = ImportanceStore::open(&path).expect("open");
    let plan = explain_plan(store.read_conn(), SUBTREE_CLEAR_SQL);
    for line in plan.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("SCAN") && !trimmed.contains("USING") {
            panic!(
                "subtree-clear DELETE full-scans the weights table — offending step:\n{trimmed}\nfull plan:\n{plan}"
            );
        }
    }
    writer.shutdown();
}

/// Open a fresh store in a temp dir, returning it plus the temp dir (kept alive).
fn fresh_store() -> (ImportanceStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    let store = ImportanceStore::open(&path).expect("open importance store");
    (store, dir)
}

/// SMOKE: open a fresh DB, write one weight through the writer, read it back.
/// Everything else builds on this working.
#[test]
fn smoke_round_trips_one_weight() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");

    // Open (creates the file + schema) and confirm no weight yet.
    let store = ImportanceStore::open(&path).expect("open");
    assert_eq!(store.weight_for("/Users/me/project").expect("read"), None);
    assert_eq!(store.recompute_generation().expect("gen"), 0);

    // Write one weight at generation 1 through the writer thread.
    let writer = ImportanceWriter::spawn(&path).expect("spawn writer");
    writer
        .write_weights(
            1,
            vec![WeightRow {
                path: "/Users/me/project".to_string(),
                score: 0.82,
                signals_json: "{\"pathClass\":\"projectRoot\"}".to_string(),
            }],
        )
        .expect("write weights");
    writer.flush_blocking().expect("flush");

    // Read it back on a fresh store handle (the writer wrote to the same file).
    let store2 = ImportanceStore::open(&path).expect("reopen");
    let w = store2.weight_for("/Users/me/project").expect("read").expect("present");
    assert_eq!(w.score, 0.82);
    assert_eq!(w.as_of_generation, 1);
    assert_eq!(w.signals_json, "{\"pathClass\":\"projectRoot\"}");
    assert_eq!(store2.recompute_generation().expect("gen"), 1);
    // The store handle also observed the write.
    drop(store);
    writer.shutdown();
}

/// A schema-version mismatch deletes and recreates the DB fresh (disposable
/// cache, no migrations — plan Decision 2). We simulate an old DB by stamping a
/// bogus version, then reopening must wipe it.
#[test]
fn schema_mismatch_recreates_the_db() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");

    // Create a DB and write a weight.
    {
        let writer = ImportanceWriter::spawn(&path).expect("spawn");
        writer
            .write_weights(
                1,
                vec![WeightRow {
                    path: "/a".to_string(),
                    score: 0.5,
                    signals_json: "{}".to_string(),
                }],
            )
            .expect("write");
        writer.flush_blocking().expect("flush");
        writer.shutdown();
    }

    // Corrupt the stored schema version to something old.
    {
        let conn = open_write_connection(&path).expect("open");
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '0')",
            [],
        )
        .expect("stamp old version");
    }

    // Reopening must detect the mismatch and recreate fresh: the weight is gone.
    let store = ImportanceStore::open(&path).expect("reopen recreates");
    assert_eq!(
        store.weight_for("/a").expect("read"),
        None,
        "a schema mismatch must wipe the DB (disposable cache, no migration)"
    );
    assert_eq!(
        read_meta_value(store.read_conn(), "schema_version").expect("read version"),
        Some(SCHEMA_VERSION.to_string()),
        "the recreated DB carries the current schema version"
    );
}

/// The row is keyed by the folded path (`normalize_for_comparison`, the same fold
/// `platform_case` applies), so a case/normalization variant of a scored path
/// resolves to the same weight row (matching how the index keys paths).
#[test]
#[cfg(target_os = "macos")]
fn weight_lookup_is_platform_case_insensitive() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    let writer = ImportanceWriter::spawn(&path).expect("spawn");
    writer
        .write_weights(
            1,
            vec![WeightRow {
                path: "/Users/Me/Project".to_string(),
                score: 0.7,
                signals_json: "{}".to_string(),
            }],
        )
        .expect("write");
    writer.flush_blocking().expect("flush");

    let store = ImportanceStore::open(&path).expect("open");
    // A differently-cased lookup hits the same row on macOS (APFS-like folding).
    assert!(
        store.weight_for("/users/me/project").expect("read").is_some(),
        "platform_case collation must fold case on macOS"
    );
    writer.shutdown();
}

/// A row written through the INCREMENTAL path is keyed by its folded path too, so a
/// case + NFD variant of the query resolves to it — and the verbatim `path` column is
/// returned, not the folded key. Guards that the folded-key change didn't only cover
/// the full pass: `insert_rows` (shared by both) folds, and the incremental
/// subtree-clear range operates on the same folded keys.
#[test]
#[cfg(target_os = "macos")]
fn incremental_write_resolves_a_case_and_nfd_variant() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    let writer = ImportanceWriter::spawn(&path).expect("spawn");

    // Full pass at generation 1 (an unrelated row, so the store isn't empty).
    writer
        .write_weights(
            1,
            vec![WeightRow {
                path: "/Users/me/other".to_string(),
                score: 0.4,
                signals_json: "{}".to_string(),
            }],
        )
        .expect("full pass");
    writer.flush_blocking().expect("flush");

    // Incremental rescore: clear the subtree, insert a mixed-case, NFC-composed path
    // (`é` is U+00E9). No generation bump.
    let stored = "/Users/Me/Café";
    writer
        .write_weights_incremental(
            1,
            vec![WeightRow {
                path: stored.to_string(),
                score: 0.77,
                signals_json: "{}".to_string(),
            }],
            vec![stored.to_string()],
        )
        .expect("incremental");
    writer.flush_blocking().expect("flush");

    // Query with a lowercase, NFD-decomposed variant (`e` + U+0301 combining acute).
    let variant = "/users/me/cafe\u{0301}";
    let store = ImportanceStore::open(&path).expect("open");
    let w = store
        .weight_for(variant)
        .expect("read")
        .expect("a case/NFD variant resolves to the incrementally-written row");
    assert_eq!(w.score, 0.77);
    assert_eq!(w.path, stored, "the verbatim path is returned, not the folded key");
    writer.shutdown();
}

/// A FULL PASS REPLACES THE WHOLE TABLE at its new generation. A folder scored in
/// an earlier pass but not the later one leaves NO stale row (the compaction never
/// keeps a row a fresh pass wouldn't write — a folder that became floored or
/// vanished from the index), and every surviving row carries the current
/// generation (the honest as-of marker). This is stronger than the old
/// upsert-and-leave-stale semantics.
#[test]
fn a_full_pass_replaces_the_table_and_restamps_the_generation() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    let writer = ImportanceWriter::spawn(&path).expect("spawn");

    // Pass 1 writes /old and /keep at generation 1.
    writer
        .write_weights(
            1,
            vec![
                WeightRow {
                    path: "/old".to_string(),
                    score: 0.3,
                    signals_json: "{}".to_string(),
                },
                WeightRow {
                    path: "/keep".to_string(),
                    score: 0.5,
                    signals_json: "{}".to_string(),
                },
            ],
        )
        .expect("write pass 1");
    writer.flush_blocking().expect("flush");

    // Pass 2 bumps to generation 2 and rewrites /keep + /fresh — but NOT /old (it
    // floored or vanished). The full pass replaces the table, so /old is gone.
    writer
        .write_weights(
            2,
            vec![
                WeightRow {
                    path: "/keep".to_string(),
                    score: 0.6,
                    signals_json: "{}".to_string(),
                },
                WeightRow {
                    path: "/fresh".to_string(),
                    score: 0.9,
                    signals_json: "{}".to_string(),
                },
            ],
        )
        .expect("write pass 2");
    writer.flush_blocking().expect("flush");

    let store = ImportanceStore::open(&path).expect("open");
    let current = store.recompute_generation().expect("gen");
    assert_eq!(current, 2, "two passes ⇒ generation 2");

    assert_eq!(
        store.weight_for("/old").expect("read"),
        None,
        "a folder dropped from the second full pass leaves no stale row (the table is replaced)"
    );
    let keep = store.weight_for("/keep").expect("read").expect("present");
    let fresh = store.weight_for("/fresh").expect("read").expect("present");
    assert_eq!(keep.score, 0.6, "a rewritten folder carries the new pass's score");
    assert_eq!(keep.as_of_generation, current, "and the current generation");
    assert_eq!(
        fresh.as_of_generation, current,
        "a newly-scored folder is at the current generation"
    );
    writer.shutdown();
}

/// A repeated write to the same path OVERWRITES (upsert on the folded-path PK), keeping
/// the latest score and generation. A recompute pass rewrites every folder, so an
/// upsert is the correct semantics (no duplicate rows, no stale leftover).
#[test]
fn writing_the_same_path_upserts() {
    let (store, _dir) = fresh_store();
    let path = store.db_path().to_path_buf();
    let writer = ImportanceWriter::spawn(&path).expect("spawn");

    writer
        .write_weights(
            1,
            vec![WeightRow {
                path: "/p".to_string(),
                score: 0.1,
                signals_json: "{\"v\":1}".to_string(),
            }],
        )
        .expect("write");
    writer.flush_blocking().expect("flush");
    writer
        .write_weights(
            2,
            vec![WeightRow {
                path: "/p".to_string(),
                score: 0.9,
                signals_json: "{\"v\":2}".to_string(),
            }],
        )
        .expect("rewrite");
    writer.flush_blocking().expect("flush");

    let store2 = ImportanceStore::open(&path).expect("open");
    let w = store2.weight_for("/p").expect("read").expect("present");
    assert_eq!(w.score, 0.9, "the later write wins");
    assert_eq!(w.as_of_generation, 2);
    assert_eq!(w.signals_json, "{\"v\":2}");
    writer.shutdown();
}

/// Purging a volume drops every weight and visit row but keeps the schema (the
/// file stays; only the data goes). Used when a consumer forgets a volume.
#[test]
fn purge_clears_weights_and_visits() {
    let (store, _dir) = fresh_store();
    let path = store.db_path().to_path_buf();
    let writer = ImportanceWriter::spawn(&path).expect("spawn");
    writer
        .write_weights(
            1,
            vec![WeightRow {
                path: "/p".to_string(),
                score: 0.5,
                signals_json: "{}".to_string(),
            }],
        )
        .expect("write");
    writer.record_visit("/p", 100).expect("visit");
    writer.flush_blocking().expect("flush");

    writer.purge_volume().expect("purge");
    writer.flush_blocking().expect("flush");

    let store2 = ImportanceStore::open(&path).expect("open");
    assert_eq!(store2.weight_for("/p").expect("read"), None, "weights gone after purge");
    assert_eq!(store2.visit_for("/p").expect("read"), None, "visits gone after purge");
    writer.shutdown();
}

/// A visit accumulates: the first `record_visit` creates the row at count 1, a
/// second bumps to 2 and advances the timestamp. Counts and timestamps only —
/// the privacy-sane shape (plan Decision 3).
#[test]
fn record_visit_accumulates_count_and_recency() {
    let (store, _dir) = fresh_store();
    let path = store.db_path().to_path_buf();
    let writer = ImportanceWriter::spawn(&path).expect("spawn");

    writer.record_visit("/Users/me/docs", 1000).expect("visit 1");
    writer.flush_blocking().expect("flush");
    writer.record_visit("/Users/me/docs", 2000).expect("visit 2");
    writer.flush_blocking().expect("flush");

    let store2 = ImportanceStore::open(&path).expect("open");
    let (count, last) = store2.visit_for("/Users/me/docs").expect("read").expect("present");
    assert_eq!(count, 2, "two visits ⇒ count 2");
    assert_eq!(last, 2000, "last-visit timestamp advances to the newer visit");
    writer.shutdown();
}
