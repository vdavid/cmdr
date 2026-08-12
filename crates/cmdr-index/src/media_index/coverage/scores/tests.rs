//! The cache's contract: a read that doesn't move the store costs nothing, and every
//! way the store CAN move is reflected before the next read answers.
//!
//! The notice-folding rules ([`drain`]) are tested against a real broadcast channel
//! rather than the store, so the lag rule (the one that silently rots a cache when it
//! is wrong) is directly reachable without provoking a real recompute.

use super::*;
use crate::importance::read::{WeightsChanged, notify_recompute_completed_for_test};
use crate::importance::scorer::{FolderSignals, PathClass};
use crate::importance::store::importance_db_path;
use crate::importance::writer::{ImportanceWriter, WeightRow};

/// Write `rows` to a fresh `importance-{volume_id}.db` under a temp data dir, through
/// the real writer, and return the dir so the caller keeps it alive.
fn store_with(volume_id: &str, generation: u64, rows: &[(&str, f64)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    write_into(dir.path(), volume_id, generation, rows);
    dir
}

fn write_into(data_dir: &Path, volume_id: &str, generation: u64, rows: &[(&str, f64)]) {
    let mut signals = FolderSignals::neutral();
    signals.path_class = PathClass::UserContent;
    let signals_json = serde_json::to_string(&signals).expect("serialize signals");
    let writer = ImportanceWriter::spawn(&importance_db_path(data_dir, volume_id)).expect("spawn writer");
    writer
        .write_weights(
            generation,
            rows.iter()
                .map(|(path, score)| WeightRow {
                    path: (*path).to_string(),
                    score: *score,
                    signals_json: signals_json.clone(),
                })
                .collect(),
        )
        .expect("write");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}

fn delta(upserted: &[(&str, f64)], removed: &[&str]) -> WeightsChanged {
    WeightsChanged::Delta {
        generation: 1,
        upserted: upserted.iter().map(|(p, s)| ((*p).to_string(), *s)).collect(),
        removed: removed.iter().map(|p| (*p).to_string()).collect(),
    }
}

#[test]
fn an_empty_channel_leaves_the_cache_alone() {
    let mut rx = subscribe("drain-empty");
    assert_eq!(drain(&mut rx), Refresh::Fresh);
}

#[test]
fn a_delta_folds_into_a_patch() {
    let mut rx = subscribe("drain-delta");
    notify_recompute_completed_for_test("drain-delta", delta(&[("/a", 0.9)], &["/b"]));
    assert_eq!(
        drain(&mut rx),
        Refresh::Patch {
            upserted: vec![("/a".to_string(), 0.9)],
            removed: vec!["/b".to_string()],
        }
    );
}

#[test]
fn consecutive_deltas_fold_into_one_patch() {
    let mut rx = subscribe("drain-two-deltas");
    notify_recompute_completed_for_test("drain-two-deltas", delta(&[("/a", 0.9)], &[]));
    notify_recompute_completed_for_test("drain-two-deltas", delta(&[("/c", 0.5)], &["/b"]));
    assert_eq!(
        drain(&mut rx),
        Refresh::Patch {
            upserted: vec![("/a".to_string(), 0.9), ("/c".to_string(), 0.5)],
            removed: vec!["/b".to_string()],
        },
        "one drain applies every notice that piled up, not just the newest"
    );
}

#[test]
fn a_full_pass_wins_over_the_deltas_around_it() {
    let mut rx = subscribe("drain-reload");
    notify_recompute_completed_for_test("drain-reload", delta(&[("/before", 0.9)], &[]));
    notify_recompute_completed_for_test("drain-reload", WeightsChanged::ReloadAll { generation: 2 });
    notify_recompute_completed_for_test("drain-reload", delta(&[("/after", 0.1)], &[]));
    assert_eq!(
        drain(&mut rx),
        Refresh::Rebuild,
        "a re-read of the whole table already contains every delta around it"
    );
}

#[test]
fn a_lagged_receiver_rebuilds_instead_of_assuming_nothing_happened() {
    // Pre-fix this is the case that rots a cache silently: the channel is bounded, so
    // a receiver that falls behind is TOLD, and treating that as "no change" leaves the
    // map disagreeing with the store until the next full pass, with nothing to detect it.
    let mut rx = subscribe("drain-lagged");
    for i in 0..(NOTICE_BUFFER_HEADROOM) {
        notify_recompute_completed_for_test("drain-lagged", delta(&[(&format!("/p{i}"), 0.5)], &[]));
    }
    assert_eq!(drain(&mut rx), Refresh::Rebuild);
}

/// Comfortably past the channel's buffer, so the receiver is guaranteed to lag.
const NOTICE_BUFFER_HEADROOM: usize = 64;

#[test]
fn a_patch_applies_removals_before_upserts() {
    // A path in BOTH lists must end up at its upserted value: the store's transaction
    // cleared the subtree and rewrote it, and the rewrite is the fresher fact.
    let mut entry = CachedScores {
        all: Arc::new(HashMap::from([("/keep".to_string(), 0.4), ("/gone".to_string(), 0.7)])),
        projection: Some((0.5, Arc::new(HashMap::new()))),
        notices: subscribe("patch-order"),
    };
    patch(
        &mut entry,
        &[("/both".to_string(), 0.8)],
        &["/gone".to_string(), "/both".to_string()],
    );
    assert_eq!(entry.all.get("/keep"), Some(&0.4), "an untouched folder stays");
    assert_eq!(entry.all.get("/gone"), None, "a removed folder goes");
    assert_eq!(entry.all.get("/both"), Some(&0.8), "the upsert wins over the removal");
    assert!(
        entry.projection.is_none(),
        "the threshold projection is derived, so a patch must drop it"
    );
}

// --- The cache, end to end over a real store ---------------------------------------

#[test]
fn a_quiet_store_is_read_once_however_often_it_is_asked() {
    // The whole point of the cache. Pre-fix EVERY call re-read and re-sorted the entire
    // weights table, which is what let per-visible-range badge queries saturate the
    // blocking pool. Pointer equality is the proof no second read happened: a re-read
    // would build a new map behind a new `Arc`.
    let volume = "cache-quiet";
    clear_cache_for_test();
    let dir = store_with(volume, 1, &[("/photos", 0.9), ("/docs", 0.2)]);

    let first = importance_scores(dir.path(), volume, None).expect("scored");
    let second = importance_scores(dir.path(), volume, None).expect("scored");
    assert!(Arc::ptr_eq(&first, &second), "a quiet store is not re-read");
    assert_eq!(first.get("/photos"), Some(&0.9));
}

#[test]
fn a_delta_lands_without_going_back_to_the_store() {
    // A delta is applied in place, so the map moves without a re-read. Proven by
    // patching in a folder the STORE has never held: only the delta can put it there.
    let volume = "cache-delta";
    clear_cache_for_test();
    let dir = store_with(volume, 1, &[("/photos", 0.9)]);
    importance_scores(dir.path(), volume, None).expect("scored");

    notify_recompute_completed_for_test(volume, delta(&[("/only-in-the-delta", 0.7)], &["/photos"]));
    let patched = importance_scores(dir.path(), volume, None).expect("scored");
    assert_eq!(patched.get("/only-in-the-delta"), Some(&0.7), "the upsert landed");
    assert_eq!(patched.get("/photos"), None, "the removal landed");
}

#[test]
fn a_full_pass_makes_the_next_read_see_the_new_table() {
    let volume = "cache-reload";
    clear_cache_for_test();
    let dir = store_with(volume, 1, &[("/old", 0.9)]);
    assert_eq!(
        importance_scores(dir.path(), volume, None).expect("scored").get("/old"),
        Some(&0.9)
    );

    write_into(dir.path(), volume, 2, &[("/new", 0.4)]);
    notify_recompute_completed_for_test(volume, WeightsChanged::ReloadAll { generation: 2 });

    let reloaded = importance_scores(dir.path(), volume, None).expect("scored");
    assert_eq!(reloaded.get("/new"), Some(&0.4), "the new table is read");
    assert_eq!(reloaded.get("/old"), None, "a full pass replaces, it doesn't merge");
}

#[test]
fn an_unscored_volume_reads_none_so_the_gate_falls_back_to_overrides() {
    // Load-bearing: `None` sends the coverage gates to override-only. Caching a map for
    // an unscored volume would silently widen coverage instead.
    clear_cache_for_test();
    let dir = tempfile::tempdir().expect("temp dir");
    assert!(importance_scores(dir.path(), "never-scored", None).is_none());
}

#[test]
fn the_threshold_projection_is_memoized_and_follows_the_scores() {
    let volume = "cache-projection";
    clear_cache_for_test();
    let dir = store_with(volume, 1, &[("/high", 0.9), ("/low", 0.2)]);

    let first = importance_scores(dir.path(), volume, Some(0.5)).expect("scored");
    assert_eq!(first.len(), 1, "only the folders at or above the threshold");
    assert_eq!(first.get("/high"), Some(&0.9));

    let again = importance_scores(dir.path(), volume, Some(0.5)).expect("scored");
    assert!(Arc::ptr_eq(&first, &again), "the same threshold reuses the projection");

    let wider = importance_scores(dir.path(), volume, Some(0.1)).expect("scored");
    assert_eq!(wider.len(), 2, "a different threshold projects again");

    // A patch invalidates the projection, so a stale one can't outlive the scores it
    // was derived from.
    notify_recompute_completed_for_test(volume, delta(&[("/low", 0.8)], &[]));
    let after = importance_scores(dir.path(), volume, Some(0.5)).expect("scored");
    assert_eq!(after.len(), 2, "the rescored folder is above the threshold now");
}

#[test]
fn a_threshold_read_answers_from_a_cold_cache_too() {
    // The threshold view must never DEPEND on an entry already being there. `None` means
    // "importance never scored this volume" and sends the coverage gates to override-only,
    // so any path that reports it for a merely-absent cache entry silently narrows what
    // gets enriched. (The same reason the projection step never `?`s on the entry, which
    // it re-looks-up after releasing the lock.)
    let volume = "cache-cold-threshold";
    clear_cache_for_test();
    let dir = store_with(volume, 1, &[("/high", 0.9), ("/low", 0.1)]);

    let projected = importance_scores(dir.path(), volume, Some(0.5)).expect("a scored volume is never 'unscored'");
    assert_eq!(projected.len(), 1);
    assert_eq!(projected.get("/high"), Some(&0.9));
}

#[test]
fn a_patch_leaves_an_already_taken_snapshot_untouched() {
    // `Arc::make_mut` clones when a reader holds the handle, so a caller that took the
    // map keeps reading the scores it asked for rather than watching them mutate.
    let mut entry = CachedScores {
        all: Arc::new(HashMap::from([("/a".to_string(), 0.4)])),
        projection: None,
        notices: subscribe("patch-snapshot"),
    };
    let snapshot = Arc::clone(&entry.all);
    patch(&mut entry, &[("/b".to_string(), 0.9)], &[]);
    assert_eq!(snapshot.len(), 1, "the taken snapshot is stable");
    assert_eq!(entry.all.len(), 2, "the cache moved on");
}
