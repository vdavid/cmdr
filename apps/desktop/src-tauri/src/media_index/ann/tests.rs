//! ANN vector-search tests (plan M6). All FFI-free except usearch itself (pure
//! in-process, no models): they seed real `media.db` files through the ONE writer,
//! build/poison/corrupt the sidecar index, and assert the search behavior — the
//! rebuild triggers, the brute-force fallback, upsert/delete/rename behavior, the
//! version-mismatch path, and the over-fetch re-rank's exact ordering.

use std::path::{Path, PathBuf};

use super::cache::Route;
use super::*;
use crate::media_index::read::MediaIndex;
use crate::media_index::store::{EnrichmentState, MediaStatusRow, MediaStore, media_db_path};
use crate::media_index::vector::cache as vector_cache;
use crate::media_index::writer::{MediaWriter, UpsertAnalysis};
use crate::media_index::{coverage, predicate::MediaKind};

/// A fresh media store + writer over a scratch volume. Volume ids are unique per
/// test (the accounted cache is process-global and keyed by volume id).
fn writer(dir: &Path, volume_id: &str) -> (MediaWriter, PathBuf) {
    let db_path = media_db_path(dir, volume_id);
    MediaStore::open(&db_path).expect("open media store");
    let w = MediaWriter::spawn(&db_path, volume_id).expect("media writer");
    (w, db_path)
}

/// Seed one enriched image with a CLIP embedding through the writer.
fn seed_clip(w: &MediaWriter, path: &str, vector: Vec<f32>) {
    w.upsert(
        MediaStatusRow {
            path: path.to_string(),
            mtime: Some(1),
            size: Some(2),
            media_kind: MediaKind::Image,
            state: EnrichmentState::Done,
            engine_version: "e1".to_string(),
            clip_stamp: String::new(),
        },
        Some(UpsertAnalysis::ocr_only("t")),
    )
    .expect("seed vision row");
    w.upsert_clip(path.to_string(), "clip-v1".to_string(), Some(vector))
        .expect("seed clip embedding");
}

/// A deterministic 4-d unit-ish vector whose cosine against [`query`] strictly
/// decreases with `i` and NEVER equals the query itself (`i + 1`), so top-k
/// ordering is unambiguous and a test can insert a query-identical vector that
/// must rank strictly first.
fn vec_for(i: usize) -> Vec<f32> {
    vec![1.0, 0.02 * (i + 1) as f32, 0.0, 0.0]
}

fn query() -> Vec<f32> {
    vec![1.0, 0.0, 0.0, 0.0]
}

/// Seed `n` images `/p/000.jpg…` with decreasing-similarity vectors and flush.
fn seed_corpus(w: &MediaWriter, n: usize) {
    for i in 0..n {
        seed_clip(w, &format!("/p/{i:03}.jpg"), vec_for(i));
    }
    w.flush_blocking().expect("flush");
}

/// Route with production model id.
fn route_for(db_path: &Path, threshold: usize) -> Route {
    cache::route(db_path, AnnSpace::Clip, threshold, AnnSpace::Clip.current_model_id())
}

/// Build the index synchronously (tests don't want the background kick) and drop
/// the cached routes so the next search re-decides.
fn rebuild_now(db_path: &Path) -> u64 {
    let rows = rebuild::rebuild_blocking(db_path, AnnSpace::Clip, AnnSpace::Clip.current_model_id(), &|| false)
        .expect("rebuild");
    vector_cache::invalidate(db_path);
    rows
}

fn paths_of(hits: &[crate::media_index::read::SemanticHit]) -> Vec<String> {
    hits.iter().map(|h| h.path.clone()).collect()
}

#[test]
fn expansion_search_scales_with_corpus_size() {
    // Spike guidance: 128 at 200k; 256–512 toward 1M+.
    assert_eq!(expansion_search_for(0), 128);
    assert_eq!(expansion_search_for(200_000), 128);
    assert_eq!(expansion_search_for(300_001), 256);
    assert_eq!(expansion_search_for(700_001), 512);
    assert_eq!(expansion_search_for(2_000_000), 512);
}

#[test]
fn below_the_threshold_search_stays_brute_force_and_builds_no_index() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-below-threshold");
    seed_corpus(&w, 3);

    let index = MediaIndex::open_at(db_path.clone());
    let hits = index.search_semantic_with_threshold(&query(), 2, 10);
    assert_eq!(paths_of(&hits), vec!["/p/000.jpg", "/p/001.jpg"], "exact results");
    assert!(
        matches!(route_for(&db_path, 10), Route::BruteForce),
        "routes brute force"
    );
    assert!(!index_path(&db_path, AnnSpace::Clip).exists(), "no index file is built");

    w.shutdown();
    coverage::invalidate_accounted("ann-below-threshold");
}

#[test]
fn at_the_threshold_a_missing_index_falls_back_exactly_until_a_rebuild_serves_ann() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-missing-index");
    seed_corpus(&w, 6);

    // No index exists yet: the search still answers, exactly (the fallback; this
    // first search also kicks the background rebuild, so no route assert here — it
    // may heal within milliseconds and that's the point).
    let index = MediaIndex::open_at(db_path.clone());
    let hits = index.search_semantic_with_threshold(&query(), 3, 4);
    assert_eq!(paths_of(&hits), vec!["/p/000.jpg", "/p/001.jpg", "/p/002.jpg"]);

    // A rebuild lands the index; the SAME query now routes ANN with the SAME results.
    rebuild_now(&db_path);
    assert!(
        matches!(route_for(&db_path, 4), Route::Ann(_)),
        "ANN serves after the rebuild"
    );
    let hits = index.search_semantic_with_threshold(&query(), 3, 4);
    assert_eq!(paths_of(&hits), vec!["/p/000.jpg", "/p/001.jpg", "/p/002.jpg"]);

    w.shutdown();
    coverage::invalidate_accounted("ann-missing-index");
}

#[test]
fn ann_results_match_brute_force_ordering_exactly() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-vs-brute");
    seed_corpus(&w, 40);
    let index = MediaIndex::open_at(db_path.clone());

    let brute = index.search_semantic_with_threshold(&query(), 10, usize::MAX);
    vector_cache::invalidate(&db_path); // drop the cached brute route before re-deciding
    rebuild_now(&db_path);
    let ann = index.search_semantic_with_threshold(&query(), 10, 1);
    assert!(
        matches!(route_for(&db_path, 1), Route::Ann(_)),
        "the second search ran ANN"
    );
    assert_eq!(
        paths_of(&ann),
        paths_of(&brute),
        "ANN + exact re-rank preserves exact ordering"
    );
    // The exact re-rank scores with the SAME `cosine_f16`, so scores agree bit-for-bit.
    for (a, b) in ann.iter().zip(brute.iter()) {
        assert_eq!(
            a.score, b.score,
            "re-ranked score equals the exact score for {}",
            a.path
        );
    }

    w.shutdown();
    coverage::invalidate_accounted("ann-vs-brute");
}

#[test]
fn re_rank_orders_by_db_truth_not_by_ann_distances() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-rerank-truth");
    // Five honest rows... and one decoy whose DB truth is orthogonal to the query.
    seed_corpus(&w, 5);
    seed_clip(&w, "/p/decoy.jpg", vec![0.0, 1.0, 0.0, 0.0]);
    w.flush_blocking().expect("flush");
    rebuild_now(&db_path);

    // Poison the INDEX: overwrite the decoy's vector with the query itself, so raw
    // ANN ranks it FIRST. The DB keeps the orthogonal truth.
    let decoy_key = {
        let conn = crate::media_index::store::open_read_connection(&db_path).expect("read conn");
        conn.query_row("SELECT id FROM media_file WHERE path = '/p/decoy.jpg'", [], |r| {
            r.get::<_, i64>(0)
        })
        .expect("decoy id") as u64
    };
    let outcome = flush_ops(
        &db_path,
        AnnSpace::Clip,
        AnnSpace::Clip.current_model_id(),
        vec![AnnOp::Upsert {
            key: decoy_key,
            vector: query(),
        }],
    );
    assert!(
        matches!(outcome, FlushOutcome::Flushed { .. }),
        "poison landed: {outcome:?}"
    );
    vector_cache::invalidate(&db_path);

    // Raw ANN would now lead with the decoy; the exact re-rank against the DB's f16
    // truth pushes it to the bottom. Pre-fix (scoring by ANN distance) this would
    // have returned the decoy first.
    let index = MediaIndex::open_at(db_path.clone());
    let hits = index.search_semantic_with_threshold(&query(), 3, 1);
    assert!(matches!(route_for(&db_path, 1), Route::Ann(_)));
    assert_eq!(
        paths_of(&hits),
        vec!["/p/000.jpg", "/p/001.jpg", "/p/002.jpg"],
        "ordering follows the stored vectors, not the index's copy"
    );

    w.shutdown();
    coverage::invalidate_accounted("ann-rerank-truth");
}

#[test]
fn ghost_keys_in_the_index_drop_out_of_results() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-ghost-keys");
    seed_corpus(&w, 5);
    rebuild_now(&db_path);

    // Delete a row WITHOUT flushing the ANN ops: the index still holds its key (a
    // ghost — exactly what a crash between a delete and a flush leaves behind).
    assert_eq!(w.prune_paths(vec!["/p/001.jpg".to_string()]).expect("prune"), 1);
    let index = MediaIndex::open_at(db_path.clone());
    let hits = index.search_semantic_with_threshold(&query(), 4, 1);
    assert!(matches!(route_for(&db_path, 1), Route::Ann(_)));
    assert_eq!(
        paths_of(&hits),
        vec!["/p/000.jpg", "/p/002.jpg", "/p/003.jpg", "/p/004.jpg"],
        "the deleted row's ghost key resolves to no row and falls out"
    );

    w.shutdown();
    coverage::invalidate_accounted("ann-ghost-keys");
}

#[test]
fn writer_fed_upserts_land_on_flush_and_re_embeds_overwrite() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-writer-upserts");
    seed_corpus(&w, 4);
    rebuild_now(&db_path);
    let index = MediaIndex::open_at(db_path.clone());

    // A NEW image lands in the index via the writer's buffered op + flush.
    seed_clip(&w, "/p/new.jpg", query()); // identical to the query ⇒ must rank first
    w.flush_ann_index().expect("flush ann");
    vector_cache::invalidate(&db_path);
    let hits = index.search_semantic_with_threshold(&query(), 2, 1);
    assert!(matches!(route_for(&db_path, 1), Route::Ann(_)));
    assert_eq!(
        paths_of(&hits),
        vec!["/p/new.jpg", "/p/000.jpg"],
        "the new image is searchable"
    );

    // A re-embed overwrites the SAME key: the image drops to the bottom when its
    // vector moves away from the query.
    w.upsert_clip(
        "/p/new.jpg".to_string(),
        "clip-v2".to_string(),
        Some(vec![0.0, 1.0, 0.0, 0.0]),
    )
    .expect("re-embed");
    w.flush_ann_index().expect("flush ann");
    vector_cache::invalidate(&db_path);
    let hits = index.search_semantic_with_threshold(&query(), 2, 1);
    assert_eq!(
        paths_of(&hits),
        vec!["/p/000.jpg", "/p/001.jpg"],
        "the re-embedded vector replaced the old one under the same key"
    );

    w.shutdown();
    coverage::invalidate_accounted("ann-writer-upserts");
}

#[test]
fn gc_removes_keys_from_the_index_on_flush() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-gc-removes");
    seed_corpus(&w, 4);
    rebuild_now(&db_path);

    w.gc_paths(vec!["/p/000.jpg".to_string()]).expect("gc");
    w.flush_ann_index().expect("flush ann");
    vector_cache::invalidate(&db_path);

    let meta = read_meta(&db_path, AnnSpace::Clip, AnnSpace::Clip.current_model_id()).expect("meta");
    assert_eq!(meta.rows, 3, "the GC'd key left the index");
    let index = MediaIndex::open_at(db_path.clone());
    let hits = index.search_semantic_with_threshold(&query(), 4, 1);
    assert!(matches!(route_for(&db_path, 1), Route::Ann(_)));
    assert_eq!(paths_of(&hits), vec!["/p/001.jpg", "/p/002.jpg", "/p/003.jpg"]);

    w.shutdown();
    coverage::invalidate_accounted("ann-gc-removes");
}

#[test]
fn a_rename_touches_neither_the_index_nor_the_dirty_marker_and_hits_follow() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-rename");
    seed_corpus(&w, 3);
    rebuild_now(&db_path);
    // Land the seeding ops so the dirty marker is clear BEFORE the rename — the
    // assertion below is that the rename alone never re-creates it.
    w.flush_ann_index().expect("flush ann");
    assert!(
        !dirty_path(&db_path, AnnSpace::Clip).exists(),
        "precondition: marker clear"
    );

    // The rename is a one-row `media_file.path` update; the id (= the ANN key) is
    // stable, so no index touch is needed — pinned by the dirty marker staying
    // absent (an index-touching write would create it before committing).
    assert!(w.rename_path("/p/000.jpg", "/q/renamed.jpg").expect("rename"));
    assert!(
        !dirty_path(&db_path, AnnSpace::Clip).exists(),
        "a rename buffers no ANN op (the key is stable)"
    );

    // Hits resolve ids to the CURRENT path, so the renamed image surfaces under its
    // new name with no index change and no re-embed.
    vector_cache::invalidate(&db_path);
    let index = MediaIndex::open_at(db_path.clone());
    let hits = index.search_semantic_with_threshold(&query(), 1, 1);
    assert!(matches!(route_for(&db_path, 1), Route::Ann(_)));
    assert_eq!(paths_of(&hits), vec!["/q/renamed.jpg"], "the hit follows the rename");

    w.shutdown();
    coverage::invalidate_accounted("ann-rename");
}

#[test]
fn a_corrupt_index_file_falls_back_to_exact_results() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-corrupt");
    seed_corpus(&w, 5);
    rebuild_now(&db_path);

    // Trash the index file behind the sidecar's back (a torn disk, a bad block).
    std::fs::write(index_path(&db_path, AnnSpace::Clip), b"not a usearch file").expect("corrupt");
    vector_cache::invalidate(&db_path);

    // Search NEVER breaks: it answers exactly via the fallback (and kicks the
    // background heal, whose completion the next test pins).
    let index = MediaIndex::open_at(db_path.clone());
    let hits = index.search_semantic_with_threshold(&query(), 3, 1);
    assert_eq!(paths_of(&hits), vec!["/p/000.jpg", "/p/001.jpg", "/p/002.jpg"]);

    w.shutdown();
    coverage::invalidate_accounted("ann-corrupt");
}

#[test]
fn a_bad_index_kicks_a_background_rebuild_that_heals_it() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-background-heal");
    seed_corpus(&w, 5);
    rebuild_now(&db_path);
    std::fs::write(index_path(&db_path, AnnSpace::Clip), b"garbage").expect("corrupt");
    vector_cache::invalidate(&db_path);

    // The first search detects the corrupt index, falls back, and kicks the heal.
    let index = MediaIndex::open_at(db_path.clone());
    let hits = index.search_semantic_with_threshold(&query(), 3, 1);
    assert_eq!(paths_of(&hits), vec!["/p/000.jpg", "/p/001.jpg", "/p/002.jpg"]);

    // The background rebuild lands and the route promotes itself to ANN, with the
    // same exact results (the rebuild's completion invalidates the cached route).
    crate::test_support::wait_until(
        std::time::Duration::from_secs(10),
        "the background rebuild to heal the corrupt index",
        || {
            if matches!(route_for(&db_path, 1), Route::Ann(_)) {
                return true;
            }
            vector_cache::invalidate(&db_path); // re-decide until the new file is live
            false
        },
    );
    let hits = index.search_semantic_with_threshold(&query(), 3, 1);
    assert_eq!(paths_of(&hits), vec!["/p/000.jpg", "/p/001.jpg", "/p/002.jpg"]);

    w.shutdown();
    coverage::invalidate_accounted("ann-background-heal");
}

#[test]
fn a_flush_during_an_in_flight_rebuild_retains_ops_and_replays_them_after() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-rebuild-race");
    seed_corpus(&w, 5);
    rebuild_now(&db_path);
    w.flush_ann_index().expect("flush ann"); // land the seeding ops; marker clear
    assert!(
        !dirty_path(&db_path, AnnSpace::Clip).exists(),
        "precondition: marker clear"
    );

    // A rebuild is "in flight": its snapshot was taken BEFORE the row below commits.
    rebuild::test_hold_in_flight(&db_path, AnnSpace::Clip);

    // A row committed mid-rebuild, then a seam flush. The flush must RETAIN the op
    // AND the dirty marker: applying now would land on a file the install is about
    // to overwrite, and dropping would lose the row forever (the in-flight snapshot
    // predates it). Pre-fix this applied immediately: rows read 6 and the marker
    // cleared — the silent-loss shape.
    seed_clip(&w, "/p/mid.jpg", query());
    w.flush_ann_index().expect("flush ann");
    let meta = read_meta(&db_path, AnnSpace::Clip, AnnSpace::Clip.current_model_id()).expect("meta");
    assert_eq!(meta.rows, 5, "the mid-rebuild op is retained, not applied");
    assert!(
        dirty_path(&db_path, AnnSpace::Clip).exists(),
        "the dirty marker stays until the retained ops land"
    );

    // The rebuild completes; the next seam flush replays the retained op on top of
    // the installed index (idempotent by design), and the row becomes searchable.
    rebuild::test_release_in_flight(&db_path, AnnSpace::Clip);
    w.flush_ann_index().expect("flush ann");
    let meta = read_meta(&db_path, AnnSpace::Clip, AnnSpace::Clip.current_model_id()).expect("meta");
    assert_eq!(meta.rows, 6, "the retained op landed after the rebuild");
    assert!(
        !dirty_path(&db_path, AnnSpace::Clip).exists(),
        "the marker cleared with the replay"
    );
    vector_cache::invalidate(&db_path);
    let index = MediaIndex::open_at(db_path.clone());
    let hits = index.search_semantic_with_threshold(&query(), 1, 1);
    assert!(matches!(route_for(&db_path, 1), Route::Ann(_)));
    assert_eq!(
        paths_of(&hits),
        vec!["/p/mid.jpg"],
        "the mid-rebuild row is ANN-searchable"
    );

    w.shutdown();
    coverage::invalidate_accounted("ann-rebuild-race");
}

#[test]
fn a_shutdown_during_an_in_flight_rebuild_keeps_the_marker_and_the_next_spawn_wipes() {
    let dir = tempfile::tempdir().expect("temp");
    let vid = "ann-shutdown-mid-rebuild";
    let (w, db_path) = writer(dir.path(), vid);
    seed_corpus(&w, 4);
    rebuild_now(&db_path);
    w.flush_ann_index().expect("flush ann");

    // An op lands while a rebuild is in flight, then the writer shuts down. The
    // shutdown flush RETAINS (deliberately): the on-disk index may lag this op with
    // nobody left to replay it, so the dirty marker must survive the session and the
    // next spawn wipes the possibly-lagging index for a fresh rebuild.
    rebuild::test_hold_in_flight(&db_path, AnnSpace::Clip);
    seed_clip(&w, "/p/late.jpg", query());
    w.flush_blocking().expect("flush db");
    w.shutdown();
    assert!(
        dirty_path(&db_path, AnnSpace::Clip).exists(),
        "shutdown mid-rebuild keeps the marker (conservative, never silent loss)"
    );
    rebuild::test_release_in_flight(&db_path, AnnSpace::Clip);

    let (w2, _) = writer(dir.path(), vid);
    w2.flush_blocking().expect("spawn ran");
    assert!(
        !index_path(&db_path, AnnSpace::Clip).exists(),
        "the possibly-lagging index was wiped at the next spawn"
    );

    w2.shutdown();
    coverage::invalidate_accounted(vid);
}

#[test]
fn a_model_mismatch_in_the_sidecar_is_detected_and_a_rebuild_recovers() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-model-mismatch");
    seed_corpus(&w, 5);
    rebuild_now(&db_path);

    // Stamp the sidecar with a different model id (an index built from another
    // model's vectors must never be searched with this model's queries).
    let stale = AnnMeta {
        model_id: "some-older-model".to_string(),
        ..read_meta(&db_path, AnnSpace::Clip, AnnSpace::Clip.current_model_id()).expect("meta")
    };
    write_meta(&db_path, AnnSpace::Clip, &stale).expect("stamp stale");
    assert!(matches!(
        read_meta(&db_path, AnnSpace::Clip, AnnSpace::Clip.current_model_id()),
        Err(AnnError::MetaIncompatible)
    ));

    // The route refuses it and search stays exact (the kicked background heal may
    // land any moment, so no route assert — both routes answer identically)...
    vector_cache::invalidate(&db_path);
    let index = MediaIndex::open_at(db_path.clone());
    let hits = index.search_semantic_with_threshold(&query(), 2, 1);
    assert_eq!(paths_of(&hits), vec!["/p/000.jpg", "/p/001.jpg"]);

    // ...and a rebuild under the current model restores the ANN route.
    rebuild_now(&db_path);
    assert!(matches!(route_for(&db_path, 1), Route::Ann(_)));

    w.shutdown();
    coverage::invalidate_accounted("ann-model-mismatch");
}

#[test]
fn a_format_bump_in_the_sidecar_reads_as_incompatible() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-format-bump");
    seed_corpus(&w, 5);
    rebuild_now(&db_path);

    let stale = AnnMeta {
        format: ANN_FORMAT_VERSION + 1,
        ..read_meta(&db_path, AnnSpace::Clip, AnnSpace::Clip.current_model_id()).expect("meta")
    };
    write_meta(&db_path, AnnSpace::Clip, &stale).expect("stamp future format");
    assert!(matches!(
        read_meta(&db_path, AnnSpace::Clip, AnnSpace::Clip.current_model_id()),
        Err(AnnError::MetaIncompatible)
    ));

    w.shutdown();
    coverage::invalidate_accounted("ann-format-bump");
}

#[test]
fn a_flush_over_a_stale_sidecar_deletes_the_index_for_rebuild() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-flush-stale");
    seed_corpus(&w, 5);
    rebuild_now(&db_path);
    let stale = AnnMeta {
        model_id: "some-older-model".to_string(),
        ..read_meta(&db_path, AnnSpace::Clip, AnnSpace::Clip.current_model_id()).expect("meta")
    };
    write_meta(&db_path, AnnSpace::Clip, &stale).expect("stamp stale");

    // A writer flush must not graft new-model vectors onto an old-model index: it
    // wipes the files so the query path sees a clean "missing" and rebuilds.
    seed_clip(&w, "/p/extra.jpg", query());
    w.flush_ann_index().expect("flush ann");
    assert!(
        !index_path(&db_path, AnnSpace::Clip).exists(),
        "the stale index was deleted"
    );
    assert!(
        !meta_path(&db_path, AnnSpace::Clip).exists(),
        "the stale sidecar was deleted"
    );

    w.shutdown();
    coverage::invalidate_accounted("ann-flush-stale");
}

#[test]
fn a_crashed_session_wipes_the_stale_index_at_writer_spawn() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-crash-wipe");
    seed_corpus(&w, 5);
    rebuild_now(&db_path);
    w.shutdown();

    // Simulate a crash: the dirty marker survives with nobody holding pending ops.
    mark_dirty(&db_path, AnnSpace::Clip);
    assert!(
        index_path(&db_path, AnnSpace::Clip).exists(),
        "precondition: index on disk"
    );

    // The next session's writer spawn detects it and wipes the lagging index.
    let (w2, _) = writer(dir.path(), "ann-crash-wipe");
    w2.flush_blocking().expect("flush (spawn ran)");
    assert!(
        !index_path(&db_path, AnnSpace::Clip).exists(),
        "the lagging index was wiped"
    );
    assert!(
        !dirty_path(&db_path, AnnSpace::Clip).exists(),
        "the marker went with it"
    );

    w2.shutdown();
    coverage::invalidate_accounted("ann-crash-wipe");
}

#[test]
fn a_clean_shutdown_flushes_pending_ops_and_does_not_look_like_a_crash() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-clean-shutdown");
    seed_corpus(&w, 4);
    rebuild_now(&db_path);

    // Buffer an op, then shut down WITHOUT an explicit ANN flush.
    seed_clip(&w, "/p/late.jpg", query());
    w.flush_blocking().expect("flush db");
    w.shutdown();

    // The shutdown flushed: no dirty marker, and the op landed.
    assert!(
        !dirty_path(&db_path, AnnSpace::Clip).exists(),
        "clean shutdown clears the marker"
    );
    let meta = read_meta(&db_path, AnnSpace::Clip, AnnSpace::Clip.current_model_id()).expect("meta");
    assert_eq!(meta.rows, 5, "the late op landed before the thread died");

    coverage::invalidate_accounted("ann-clean-shutdown");
}

#[test]
fn a_flush_with_no_index_drops_ops_and_clears_the_marker() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-no-index-flush");
    seed_corpus(&w, 2); // below any real threshold; no index file exists

    assert!(
        dirty_path(&db_path, AnnSpace::Clip).exists(),
        "ops buffered ⇒ marker on disk"
    );
    w.flush_ann_index().expect("flush ann");
    assert!(
        !dirty_path(&db_path, AnnSpace::Clip).exists(),
        "flush cleared the marker"
    );
    assert!(
        !index_path(&db_path, AnnSpace::Clip).exists(),
        "no index materializes from a delta alone"
    );

    w.shutdown();
    coverage::invalidate_accounted("ann-no-index-flush");
}

#[test]
fn prune_all_clip_deletes_the_index_files() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-prune-all-clip");
    seed_corpus(&w, 5);
    rebuild_now(&db_path);
    assert!(index_path(&db_path, AnnSpace::Clip).exists());

    assert_eq!(w.prune_all_clip().expect("prune all clip"), 5);
    assert!(
        !index_path(&db_path, AnnSpace::Clip).exists(),
        "no CLIP rows ⇒ no CLIP index"
    );
    assert!(!meta_path(&db_path, AnnSpace::Clip).exists());
    assert!(!dirty_path(&db_path, AnnSpace::Clip).exists());

    w.shutdown();
    coverage::invalidate_accounted("ann-prune-all-clip");
}

#[test]
fn a_schema_recreate_takes_the_index_files_with_it() {
    let dir = tempfile::tempdir().expect("temp");
    let (w, db_path) = writer(dir.path(), "ann-schema-wipe");
    seed_corpus(&w, 5);
    rebuild_now(&db_path);
    w.shutdown();
    assert!(index_path(&db_path, AnnSpace::Clip).exists());

    // Force a schema mismatch, then re-open: the disposable-cache recreate must
    // take the derivative index along (a fresh DB searched through an old index
    // would resurrect deleted rows).
    {
        let conn = rusqlite::Connection::open(&db_path).expect("raw conn");
        conn.execute("UPDATE meta SET value = '0' WHERE key = 'schema_version'", [])
            .expect("age the schema");
    }
    MediaStore::open(&db_path).expect("recreate");
    assert!(
        !index_path(&db_path, AnnSpace::Clip).exists(),
        "the schema wipe removed the index"
    );
    assert!(!meta_path(&db_path, AnnSpace::Clip).exists(), "…and the sidecar");

    coverage::invalidate_accounted("ann-schema-wipe");
}

// ── Real-corpus recall + latency harness (plan M6 verification) ──────────────

/// Recall@10 + latency on REAL CLIP embeddings — the plan's "verify recall on real
/// embeddings, not just the spike's synthetic ones". `#[ignore]`d measurement, not a
/// regression test — run it by hand against a COPY of a real `media.db`:
///
/// ```sh
/// CMDR_ANN_EVAL_DB=/path/to/media-root-copy.db cargo test -p cmdr --release --lib \
///   media_index::ann::tests::real_corpus_recall_and_latency -- --ignored --nocapture
/// ```
///
/// Method: every stored embedding takes a turn as the query (capped at 500,
/// stride-sampled), leave-self-out. Ground truth is the exact brute-force top-10;
/// the ANN answer is the production path (over-fetch + exact re-rank), plus a raw
/// HNSW top-10 (no re-rank) to show what the re-rank buys.
#[test]
#[ignore = "measurement harness; run by hand against a real media.db copy (see doc comment)"]
#[allow(
    clippy::print_stderr,
    reason = "an ignored measurement harness prints its table to stderr for `--nocapture`; it never runs in the app or CI"
)]
fn real_corpus_recall_and_latency() {
    use std::time::Instant;
    let db = std::env::var("CMDR_ANN_EVAL_DB").expect("set CMDR_ANN_EVAL_DB to a media.db COPY");
    let db_path = PathBuf::from(db);
    assert!(db_path.exists(), "media.db copy not found at {}", db_path.display());

    let conn = crate::media_index::store::open_read_connection(&db_path).expect("open");
    let rows: Vec<(i64, Vec<f32>)> = {
        let mut out = Vec::new();
        crate::media_index::store::for_each_embedding_with_id::<MediaStoreError>(
            &conn,
            AnnSpace::Clip.table(),
            |id, v| {
                out.push((id, v.iter().map(|x| x.to_f32()).collect()));
                Ok(())
            },
        )
        .expect("read embeddings");
        out
    };
    let n = rows.len();
    assert!(
        n >= 100,
        "need a real corpus (found {})",
        crate::pluralize::pluralize(n as u64, "embedding")
    );
    eprintln!("\n=== M6 ANN real-corpus verification ===");
    eprintln!("corpus: {n} CLIP embeddings from {}", db_path.display());

    // Build the index through the production rebuild path.
    delete_index_files(&db_path, AnnSpace::Clip);
    let t = Instant::now();
    let built = rebuild::rebuild_blocking(&db_path, AnnSpace::Clip, AnnSpace::Clip.current_model_id(), &|| false)
        .expect("rebuild");
    eprintln!(
        "rebuild: {} in {:.2?}",
        crate::pluralize::pluralize(built, "vector"),
        t.elapsed()
    );
    vector_cache::invalidate(&db_path);

    let index = MediaIndex::open_at(db_path.clone());
    let stride = (n / 500).max(1);
    let queries: Vec<&(i64, Vec<f32>)> = rows.iter().step_by(stride).take(500).collect();

    // Ground truth + brute latency (leave-self-out via k+1 then drop self).
    let mut brute_lat = Vec::new();
    let mut truth: Vec<Vec<String>> = Vec::new();
    for (_, q) in &queries {
        let t = Instant::now();
        let hits = index.search_semantic_with_threshold(q, 11, usize::MAX);
        brute_lat.push(t.elapsed().as_secs_f64() * 1e3);
        truth.push(paths_of(&hits).into_iter().skip(1).take(10).collect());
    }

    // Production ANN (over-fetch + exact re-rank) + latency.
    vector_cache::invalidate(&db_path);
    assert!(matches!(route_for(&db_path, 1), Route::Ann(_)), "ANN route is live");
    let mut ann_lat = Vec::new();
    let mut rerank_recall_sum = 0.0f64;
    for ((_, q), truth10) in queries.iter().zip(&truth) {
        let t = Instant::now();
        let hits = index.search_semantic_with_threshold(q, 11, 1);
        ann_lat.push(t.elapsed().as_secs_f64() * 1e3);
        let got: Vec<String> = paths_of(&hits).into_iter().skip(1).take(10).collect();
        let overlap = got.iter().filter(|p| truth10.contains(p)).count();
        rerank_recall_sum += overlap as f64 / truth10.len().max(1) as f64;
    }

    // Raw HNSW top-10 (no re-rank), for the re-rank's contribution.
    let Route::Ann(handle) = route_for(&db_path, 1) else {
        panic!("ann route expected")
    };
    let id_to_path: HashMap<u64, String> = {
        let mut m = HashMap::new();
        let mut stmt = conn.prepare("SELECT id, path FROM media_file").expect("stmt");
        let rs = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?)))
            .expect("query");
        for row in rs {
            let (id, path) = row.expect("row");
            m.insert(id, path);
        }
        m
    };
    let mut raw_recall_sum = 0.0f64;
    for ((_, q), truth10) in queries.iter().zip(&truth) {
        let m = handle.index.search(q, 11).expect("raw search");
        let got: Vec<&String> = m
            .keys
            .iter()
            .filter_map(|k| id_to_path.get(k))
            .skip(1)
            .take(10)
            .collect();
        let overlap = got.iter().filter(|p| truth10.contains(**p)).count();
        raw_recall_sum += overlap as f64 / truth10.len().max(1) as f64;
    }

    let pct = |lat: &mut Vec<f64>, p: f64| -> f64 {
        lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
        lat[((lat.len() as f64 - 1.0) * p) as usize]
    };
    let nq = queries.len();
    eprintln!("queries: {nq} (stride {stride}), leave-self-out top-10");
    eprintln!(
        "brute-force exact: p50 {:.2} ms / p95 {:.2} ms (recall 1.0 by definition)",
        pct(&mut brute_lat.clone(), 0.5),
        pct(&mut brute_lat.clone(), 0.95)
    );
    eprintln!(
        "ANN + exact re-rank: p50 {:.2} ms / p95 {:.2} ms, recall@10 {:.4}",
        pct(&mut ann_lat.clone(), 0.5),
        pct(&mut ann_lat.clone(), 0.95),
        rerank_recall_sum / nq as f64
    );
    eprintln!("raw HNSW (no re-rank): recall@10 {:.4}", raw_recall_sum / nq as f64);

    delete_index_files(&db_path, AnnSpace::Clip);
}
