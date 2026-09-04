//! `importance.db` store tests: smoke round-trip first, then the disposable-cache
//! discipline and the as-of-generation staleness predicate.

use super::*;
use crate::importance::writer::{ImportanceWriter, SUBTREE_READ_SQL, WeightRow};

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

/// The incremental rescore's subtree READ MUST be index-served, not a full scan of
/// the `weights` table. This is the whole point of the folded-key column: with a
/// custom-collation PK the `LIKE`-prefix subtree query full-scanned ~166k rows and
/// re-ran the NFD-folding comparison on every one, pegging a CPU core. A BINARY
/// `path_folded` PK lets the equality + half-open range be served by index SEARCHes.
/// The pass runs this range once per rescored prefix, every 60 s, so it carries the
/// same weight it did when it was a DELETE.
///
/// A full table scan shows as a bare `SCAN weights` with no `USING`; an index or PK
/// lookup shows as `SEARCH`. We reject any bare `SCAN` step. The row count doesn't
/// change the plan (it's structural), so a modest table proves it cheaply.
#[test]
fn subtree_read_is_index_served() {
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
    let plan = explain_plan(store.read_conn(), SUBTREE_READ_SQL);
    for line in plan.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("SCAN") && !trimmed.contains("USING") {
            panic!("the subtree read full-scans the weights table — offending step:\n{trimmed}\nfull plan:\n{plan}");
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
/// cache, no migrations). We simulate an old DB by stamping a
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
/// the privacy-sane shape.
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

/// The prod schema-3 upgrade ordering trap. A prod user's `importance.db`
/// arrives at the upgrade launch on the OLD schema WITH a stamped generation, so a
/// naive sweep-time READ of the generation reads "already scored" and skips the full
/// recompute — and THEN the schema recreate fires on the first write-path open,
/// wiping the generation, leaving the volume stuck at "never scored" forever.
/// `needs_full_pass` avoids the trap by binding the decision to the write-path
/// open: it forces the recreate FIRST, so the generation it reads reflects the current
/// schema and it correctly reports "needs a full pass".
#[test]
fn needs_full_pass_binds_to_the_write_path_open_not_a_read_probe() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");

    // Seed an OLD-schema store WITH a stamped generation (prod's schema-2 db).
    {
        let conn = open_write_connection(&path).expect("seed conn");
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '2')",
            [],
        )
        .expect("stamp old schema");
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, '2')",
            rusqlite::params![RECOMPUTE_GENERATION_KEY],
        )
        .expect("stamp generation");
    }

    // The trap: a READ-path generation probe still reads the OLD schema's stamped
    // generation, so a sweep-time read would decide "already scored" and skip.
    {
        let read = open_read_connection(&path).expect("read conn");
        assert_eq!(
            read_generation(&read).expect("gen"),
            2,
            "a read-path probe sees the outgoing schema's generation (the ordering trap)"
        );
    }

    // The fix: the write-path-bound probe forces the delete-and-recreate first
    // (schema 2 → 3), so it reads generation 0 and reports "needs a full pass".
    assert!(
        needs_full_pass(dir.path(), "root").expect("probe"),
        "binding to the write-path open recreates the store, so no generation remains ⇒ full pass needed"
    );

    // The store is genuinely recreated at the current schema with no generation.
    let store = ImportanceStore::open(&path).expect("reopen");
    assert_eq!(
        store.recompute_generation().expect("gen"),
        0,
        "generation wiped by the recreate"
    );
    assert_eq!(
        read_meta_value(store.read_conn(), "schema_version")
            .expect("v")
            .as_deref(),
        Some(SCHEMA_VERSION),
        "store recreated at the current schema"
    );
}

/// A store already carrying a generation AND the current scoring-policy stamp (the
/// normal case) does NOT need a full pass — so a scheduler gating on this never
/// rescores every volume on launch.
#[test]
fn needs_full_pass_is_false_for_an_already_scored_store() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    write_one_full_pass(&path);

    assert!(
        !needs_full_pass(dir.path(), "root").expect("probe"),
        "a generation-stamped store scored under the current policy is done ⇒ no full pass"
    );
}

/// Run one full pass over a store, the way the scheduler's recompute does.
fn write_one_full_pass(path: &Path) {
    let writer = ImportanceWriter::spawn(path).expect("spawn");
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
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}

/// A full pass stamps the scoring policy its rows were computed under, in the same
/// transaction as the generation bump. Without the stamp nothing ever re-arms: a
/// full pass runs once and an incremental only touches folders the filesystem
/// changed, so a classification fix would stay inert over the ~189,000 rows a
/// scored volume holds (observed on the local `root` volume, 2026-09-03).
#[test]
fn a_full_pass_stamps_the_scoring_policy_it_scored_under() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    write_one_full_pass(&path);

    let store = ImportanceStore::open(&path).expect("open");
    assert_eq!(
        read_meta_value(store.read_conn(), SCORING_POLICY_KEY)
            .expect("read stamp")
            .as_deref(),
        Some(crate::importance::classify::scoring_policy_fingerprint().as_str()),
        "a full pass stamps this build's scoring policy"
    );
    assert!(
        !store.predates_scoring_policy().expect("probe"),
        "so the store no longer predates the policy"
    );
}

/// A store whose rows were scored under a SUPERSEDED policy needs a full pass even
/// though it carries a generation. This is what makes a classification change
/// (a new temp root, a changed marker-promotion rule) actually reach the rows a
/// user's volume already holds.
#[test]
fn a_store_scored_under_a_superseded_policy_needs_a_full_pass() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    write_one_full_pass(&path);
    assert!(
        !needs_full_pass(dir.path(), "root").expect("probe"),
        "test setup: freshly scored, so no pass is due yet"
    );

    // Rewind the stamp to an older policy, the way a user's store looks after the
    // app upgrades into new classification rules.
    {
        let conn = open_write_connection(&path).expect("conn");
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, 'an-older-policy')",
            rusqlite::params![SCORING_POLICY_KEY],
        )
        .expect("rewind stamp");
    }

    assert!(
        needs_full_pass(dir.path(), "root").expect("probe"),
        "rows computed under a policy we no longer apply can't be trusted ⇒ full pass"
    );
}

/// A store written before the stamp existed carries none, and that counts as stale.
/// Every such store holds rows from an older policy by definition; a redundant
/// recompute costs one pass, while a skipped one leaves wrong scores in place
/// indefinitely.
#[test]
fn an_unstamped_store_needs_a_full_pass() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    write_one_full_pass(&path);

    {
        let conn = open_write_connection(&path).expect("conn");
        conn.execute("DELETE FROM meta WHERE key = ?1", rusqlite::params![SCORING_POLICY_KEY])
            .expect("drop stamp");
    }

    assert!(
        needs_full_pass(dir.path(), "root").expect("probe"),
        "an absent stamp is stale, not trusted"
    );
}

/// The rescore is forced by a STAMP rather than a [`SCHEMA_VERSION`] bump, and this
/// is why: a bump deletes the DB file, and `visits` is the one table here that isn't
/// regenerable. A superseded policy must re-arm the weights while the user's
/// navigation history survives.
#[test]
fn re_arming_the_scoring_policy_keeps_the_visit_history() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    let writer = ImportanceWriter::spawn(&path).expect("spawn");
    writer.record_visit("/Users/me/projects/thing", 1_000).expect("visit");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
    write_one_full_pass(&path);

    {
        let conn = open_write_connection(&path).expect("conn");
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, 'an-older-policy')",
            rusqlite::params![SCORING_POLICY_KEY],
        )
        .expect("rewind stamp");
    }
    assert!(needs_full_pass(dir.path(), "root").expect("probe"), "a pass is due");

    let store = ImportanceStore::open(&path).expect("reopen");
    assert_eq!(
        store.visit_for("/Users/me/projects/thing").expect("visit"),
        Some((1, 1_000)),
        "the probe re-arms the weights without touching the visit history"
    );
}

/// Read-only connections open with the smaller page cache, write connections
/// with the bigger one. `ImportanceIndex` holds thread-local read connections
/// (`../read/mod.rs`'s `READ_CONNS`), so these are the many; the writer is the
/// one. Both budgets are upper bounds drawn from the process-wide slab in
/// `cmdr_fs::sqlite_util`.
#[test]
fn read_connections_get_a_smaller_page_cache_than_write_connections() {
    use cmdr_fs::sqlite_util::{READ_PAGE_CACHE_KIB, WRITE_PAGE_CACHE_KIB, page_cache_kib};

    let dir = tempfile::tempdir().expect("temp dir");
    let path = importance_db_path(dir.path(), "root");
    let write = open_write_connection(&path).expect("write conn");
    let read = open_read_connection(&path).expect("read conn");

    assert_eq!(page_cache_kib(&write), WRITE_PAGE_CACHE_KIB);
    assert_eq!(page_cache_kib(&read), READ_PAGE_CACHE_KIB);
}
