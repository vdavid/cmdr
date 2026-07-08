//! `importance.db` store tests: smoke round-trip first, then the disposable-cache
//! discipline and the as-of-generation staleness predicate.

use super::*;
use crate::importance::writer::{ImportanceWriter, WeightRow};

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

/// The path key uses `platform_case`, so a case/normalization variant of a scored
/// path resolves to the same weight row (matching how the index keys paths).
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

/// THE AS-OF-GENERATION STALENESS PREDICATE (plan M2 TDD target). A weight is
/// stale relative to the current recompute generation when the generation it was
/// stamped at is older than the store's current generation. This is the predicate
/// a consumer uses to caveat "this weight is from an older pass".
#[test]
fn as_of_generation_marks_a_weight_stale_after_a_newer_pass() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    let writer = ImportanceWriter::spawn(&path).expect("spawn");

    // Pass 1 writes /old at generation 1.
    writer
        .write_weights(
            1,
            vec![WeightRow {
                path: "/old".to_string(),
                score: 0.3,
                signals_json: "{}".to_string(),
            }],
        )
        .expect("write pass 1");
    writer.flush_blocking().expect("flush");

    // Pass 2 bumps the generation to 2 and writes /fresh, leaving /old untouched.
    writer
        .write_weights(
            2,
            vec![WeightRow {
                path: "/fresh".to_string(),
                score: 0.9,
                signals_json: "{}".to_string(),
            }],
        )
        .expect("write pass 2");
    writer.flush_blocking().expect("flush");

    let store = ImportanceStore::open(&path).expect("open");
    let current = store.recompute_generation().expect("gen");
    assert_eq!(current, 2, "two passes ⇒ generation 2");

    let old = store.weight_for("/old").expect("read").expect("present");
    let fresh = store.weight_for("/fresh").expect("read").expect("present");

    // The staleness predicate: stamped-generation < current-generation ⇒ stale.
    assert!(
        old.as_of_generation < current,
        "/old was written at gen {} but current is {current} ⇒ stale",
        old.as_of_generation
    );
    assert_eq!(
        fresh.as_of_generation, current,
        "/fresh was written at the current generation ⇒ not stale"
    );
    writer.shutdown();
}

/// A repeated write to the same path OVERWRITES (upsert on the path PK), keeping
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
