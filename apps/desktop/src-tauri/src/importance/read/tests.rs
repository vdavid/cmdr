//! `ImportanceIndex` read-API tests: ordering + threshold-edge correctness,
//! `explain` round-tripping the stored signals, and the recompute subscription.
//!
//! All over a populated `importance.db` written through the real writer (no FFI,
//! no index) — the M3 TDD targets.

use super::*;
use crate::importance::scorer::{FolderSignals, PathClass, SignalSet};
use crate::importance::writer::{ImportanceWriter, WeightRow};

/// A serialized signal vector for a folder with a given path class, so a stored
/// row carries a real `FolderSignals` `explain` can re-score.
fn signals_json(path_class: PathClass, mtime: Option<u64>) -> String {
    let mut s = FolderSignals::neutral();
    s.path_class = path_class;
    s.mtime_secs = mtime;
    s.distinct_extension_count = 3;
    s.file_count = 4;
    serde_json::to_string(&s).expect("serialize signals")
}

/// Populate a fresh `importance.db` with the given `(path, score, path_class)`
/// rows at generation 1, returning an `ImportanceIndex` over it plus the temp dir.
fn populated_index(rows: &[(&str, f64, PathClass)]) -> (ImportanceIndex, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = importance_db_path(dir.path(), "root");
    let writer = ImportanceWriter::spawn(&db_path).expect("spawn writer");
    let weight_rows: Vec<WeightRow> = rows
        .iter()
        .map(|(path, score, class)| WeightRow {
            path: path.to_string(),
            score: *score,
            signals_json: signals_json(*class, Some(1_000)),
        })
        .collect();
    writer.write_weights(1, weight_rows).expect("write");
    writer.flush_blocking().expect("flush");
    writer.shutdown();

    let index = ImportanceIndex::open(dir.path(), "root", SignalSet::all());
    (index, dir)
}

/// SMOKE: a written weight reads back with its scalar, signals, and as-of
/// generation. Everything else builds on this working.
#[test]
fn smoke_weight_for_reads_back_a_written_row() {
    let (index, _dir) = populated_index(&[("/Users/me/proj", 0.8, PathClass::ProjectRoot)]);
    let w = index.weight_for("/Users/me/proj").expect("read").expect("present");
    assert_eq!(w.score.value(), 0.8);
    assert_eq!(w.as_of_generation, 1);
    assert_eq!(w.signals.path_class, PathClass::ProjectRoot);
    assert_eq!(
        index.weight_for("/Users/me/nope").expect("read"),
        None,
        "an unscored path reads None"
    );
}

/// `top_n` returns the highest-scoring folders first, capped at `n`. THE M3
/// ordering target.
#[test]
fn top_n_returns_highest_scores_first_capped() {
    let (index, _dir) = populated_index(&[
        ("/a", 0.10, PathClass::Neutral),
        ("/b", 0.90, PathClass::ProjectRoot),
        ("/c", 0.50, PathClass::UserContent),
        ("/d", 0.70, PathClass::UserContent),
    ]);
    let top2 = index.top_n(2).expect("top_n");
    let paths: Vec<&str> = top2.iter().map(|w| w.path.as_str()).collect();
    assert_eq!(paths, vec!["/b", "/d"], "the two highest scores, highest first");
    assert_eq!(top2.len(), 2, "capped at n");
}

/// `top_n` with `n` larger than the row count returns all rows (no panic, no
/// padding), still ordered.
#[test]
fn top_n_larger_than_rows_returns_all_ordered() {
    let (index, _dir) = populated_index(&[("/a", 0.2, PathClass::Neutral), ("/b", 0.8, PathClass::ProjectRoot)]);
    let all = index.top_n(100).expect("top_n");
    let paths: Vec<&str> = all.iter().map(|w| w.path.as_str()).collect();
    assert_eq!(paths, vec!["/b", "/a"]);
}

/// `above_threshold` is INCLUSIVE at the bound and excludes below it. THE M3
/// threshold-edge target: a folder scoring exactly `threshold` is returned; one a
/// hair below is not.
#[test]
fn above_threshold_is_inclusive_at_the_edge() {
    let (index, _dir) = populated_index(&[
        ("/exactly", 0.50, PathClass::Neutral),
        ("/above", 0.51, PathClass::UserContent),
        ("/below", 0.49, PathClass::Neutral),
    ]);
    let hits = index.above_threshold(0.50).expect("above_threshold");
    let paths: Vec<&str> = hits.iter().map(|w| w.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["/above", "/exactly"],
        "inclusive at 0.50: /exactly is in, /below is out, ordered by score desc"
    );
}

/// `explain` recomputes the per-signal breakdown from the STORED signals via the
/// pure scorer — the same formula the score came from — and the breakdown sums to
/// the score. THE M3 explain round-trip target.
#[test]
fn explain_round_trips_the_stored_signals_and_sums_to_score() {
    // Store a folder's real signals at a known score, then explain it.
    let now = 2_000;
    let mut signals = FolderSignals::neutral();
    signals.path_class = PathClass::UserContent;
    signals.mtime_secs = Some(now); // fresh ⇒ recency 1.0
    signals.distinct_extension_count = 5;
    signals.file_count = 5;

    let expected = crate::importance::score(&signals, &SignalSet::all(), &Weights::default(), now);

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = importance_db_path(dir.path(), "root");
    let writer = ImportanceWriter::spawn(&db_path).expect("spawn");
    writer
        .write_weights(
            1,
            vec![WeightRow {
                path: "/Users/me/Documents/work".to_string(),
                score: expected.value(),
                signals_json: serde_json::to_string(&signals).expect("serialize"),
            }],
        )
        .expect("write");
    writer.flush_blocking().expect("flush");
    writer.shutdown();

    let index = ImportanceIndex::open(dir.path(), "root", SignalSet::all());
    let explanation = index
        .explain("/Users/me/Documents/work", now)
        .expect("explain")
        .expect("present");

    // The breakdown's score matches what the scorer computes for the stored
    // signals (no drift between the stored scalar and the re-scored breakdown).
    assert_eq!(
        explanation.score.value(),
        expected.value(),
        "explain re-scores the stored signals with the one formula"
    );
    // The additive contributions sum (clamped) to the score for an unfloored folder.
    let sum: f64 = explanation.contributions.iter().map(|c| c.contribution).sum();
    assert!(
        (sum.clamp(0.0, 1.0) - explanation.score.value()).abs() < 1e-9,
        "the explain breakdown sums to the score (sum {sum}, score {})",
        explanation.score.value()
    );
    assert!(!explanation.floored, "a UserContent folder isn't floored");
}

/// `all_nonzero_weights` returns the bulk path→score map the search ranker loads,
/// OMITTING zero-scored (floored) folders so the map holds only ranking signal.
#[test]
fn all_nonzero_weights_omits_zero_scores() {
    let (index, _dir) = populated_index(&[
        ("/Users/me/Documents", 0.72, PathClass::UserContent),
        ("/Users/me/proj", 0.88, PathClass::ProjectRoot),
        // A floored folder (node_modules subtree, cache, etc.) scores exactly 0.0.
        ("/Users/me/proj/node_modules", 0.0, PathClass::SystemOrCache),
    ]);
    let map = index.all_nonzero_weights().expect("bulk read");
    assert_eq!(map.len(), 2, "the two non-zero folders, the floored one omitted");
    assert_eq!(map.get("/Users/me/Documents").copied(), Some(0.72));
    assert_eq!(map.get("/Users/me/proj").copied(), Some(0.88));
    assert_eq!(
        map.get("/Users/me/proj/node_modules"),
        None,
        "a 0.0-scored folder is omitted (its lookup defaults to 0.0 anyway)"
    );
}

/// An `all_nonzero_weights` on a never-scored volume (no `importance.db`) is an
/// empty map, not an error — the search degradation contract's data source.
#[test]
fn all_nonzero_weights_missing_db_is_empty() {
    let dir = tempfile::tempdir().expect("temp dir");
    let index = ImportanceIndex::open(dir.path(), "never-scored", SignalSet::all());
    let map = index.all_nonzero_weights().expect("bulk read on missing db is Ok");
    assert!(map.is_empty(), "no db ⇒ empty weight map");
}

/// `signals_for` hands back the stored raw vector for a re-weighting consumer.
#[test]
fn signals_for_returns_the_stored_vector() {
    let (index, _dir) = populated_index(&[("/p", 0.6, PathClass::ProjectRoot)]);
    let s = index.signals_for("/p").expect("read").expect("present");
    assert_eq!(s.path_class, PathClass::ProjectRoot);
    assert_eq!(index.signals_for("/missing").expect("read"), None, "unscored ⇒ None");
}

/// A weight from an older pass is stale relative to the store's current
/// generation; the read API surfaces both so a consumer can caveat.
#[test]
fn staleness_is_visible_via_as_of_generation() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = importance_db_path(dir.path(), "root");
    let writer = ImportanceWriter::spawn(&db_path).expect("spawn");
    // Full pass 1 writes /old at gen 1.
    writer
        .write_weights(
            1,
            vec![WeightRow {
                path: "/old".into(),
                score: 0.3,
                signals_json: "{}".into(),
            }],
        )
        .expect("write 1");
    // Full pass 2 bumps to gen 2, leaving /old at gen 1.
    writer
        .write_weights(
            2,
            vec![WeightRow {
                path: "/new".into(),
                score: 0.9,
                signals_json: "{}".into(),
            }],
        )
        .expect("write 2");
    writer.flush_blocking().expect("flush");
    writer.shutdown();

    let index = ImportanceIndex::open(dir.path(), "root", SignalSet::all());
    let current = index.recompute_generation().expect("gen");
    assert_eq!(current, 2);
    let old = index.weight_for("/old").expect("read").expect("present");
    assert!(old.as_of_generation < current, "/old is stale (gen 1 < current 2)");
}

/// THE M3 subscription target: the recompute subscription fires exactly once per
/// recompute-completed notification, carrying the finished generation. A late
/// subscriber sees the retained last generation; a subsequent notify bumps it once.
#[test]
fn subscription_fires_once_per_recompute() {
    let vid = "sub-once-test";
    // A late subscriber first: no recompute yet ⇒ retained 0.
    let mut rx = subscribe(vid);
    assert_eq!(*rx.borrow_and_update(), 0, "no recompute completed yet");

    // One recompute completes at generation 5.
    notify_recompute_completed(vid, 5);
    assert!(
        rx.has_changed().expect("sender alive"),
        "the subscription observed the completion"
    );
    assert_eq!(*rx.borrow_and_update(), 5, "carries the finished generation");

    // No further notification ⇒ no further change (fires once, not repeatedly).
    assert!(
        !rx.has_changed().expect("sender alive"),
        "the subscription doesn't re-fire without a new recompute"
    );

    // A second recompute fires it again, exactly once.
    notify_recompute_completed(vid, 6);
    assert!(rx.has_changed().expect("sender alive"));
    assert_eq!(*rx.borrow_and_update(), 6);
}
