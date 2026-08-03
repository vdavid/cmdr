//! Weight-map tests: what each recompute notice means, that a delta patches the map
//! in place, and that a patched map equals the map a full reload would have built.

use super::*;
use crate::search::volumes::tests::make_importance_db;

/// A full pass replaces the whole `weights` table, so its notice says "rebuild" and
/// the reload picks up the freshly-written weights — the subscribe-don't-poll
/// contract the root importance subscriber relies on. Drives the real channel, so
/// the notify → classify → reload wiring is exercised end to end.
#[test]
fn a_full_pass_notice_makes_the_next_reload_see_new_weights() {
    use cmdr_index::importance::read::WeightsChanged;

    let dir = tempfile::tempdir().expect("temp dir");
    let vid = "smb-recompute";

    // First pass: an early weight, loaded into the snapshot.
    make_importance_db(dir.path(), vid, &[("/proj", 0.4)]);
    store_weights(vid, load_weights(dir.path(), vid));
    assert_eq!(weights_for(vid).weight_for("/proj"), 0.4);

    let mut rx = cmdr_index::importance::read::subscribe(vid);
    make_importance_db(dir.path(), vid, &[("/proj", 0.95)]);
    cmdr_index::importance::testing::notify_recompute_completed_for_test(
        vid,
        WeightsChanged::ReloadAll { generation: 2 },
    );

    let notice = rx.try_recv().expect("the notification fired");
    assert!(
        matches!(weight_refresh_for(Ok(notice)), WeightRefresh::Reload),
        "a full pass tells the consumer to rebuild"
    );
    store_weights(vid, load_weights(dir.path(), vid));
    assert_eq!(
        weights_for(vid).weight_for("/proj"),
        0.95,
        "the reload after a full pass sees the new weights"
    );
}

/// An incremental pass ships what it touched, and the map is PATCHED: upserts land,
/// removals drop, and every weight the pass didn't mention is left alone. This is the
/// whole optimization — no re-read of the store's other rows.
#[test]
fn a_delta_patches_the_map_and_leaves_untouched_weights_alone() {
    let dir = tempfile::tempdir().expect("temp dir");
    let vid = "smb-delta";
    make_importance_db(dir.path(), vid, &[("/proj", 0.4), ("/gone", 0.6), ("/elsewhere", 0.8)]);
    store_weights(vid, load_weights(dir.path(), vid));

    let upserted = [("/proj".to_string(), 0.95), ("/fresh".to_string(), 0.3)];
    let removed = ["/gone".to_string()];
    assert!(
        apply_weight_delta(vid, &upserted, &removed),
        "the volume has a map to patch"
    );

    let weights = weights_for(vid);
    assert_eq!(weights.weight_for("/proj"), 0.95, "an upsert overwrites in place");
    assert_eq!(weights.weight_for("/fresh"), 0.3, "a newly scored folder lands");
    assert_eq!(weights.weight_for("/gone"), 0.0, "a removal drops out of the map");
    assert_eq!(
        weights.weight_for("/elsewhere"),
        0.8,
        "an unmentioned weight is untouched"
    );
    assert_eq!(weights.len(), 3, "/proj, /elsewhere, /fresh");
}

/// A patched map must equal a rebuilt one — the invariant that lets the delta path
/// replace the reload at all. Stages the store's before and after states, patches the
/// before-map with the delta describing the difference, and compares it against a
/// fresh load of the after-store.
///
/// (What the WRITER reports for a real incremental is pinned crate-side, in
/// `importance::writer`'s `an_incremental_reports_the_rows_it_wrote_and_the_ones_it_cleared`;
/// this pins that applying such a report converges.)
#[test]
fn a_patched_map_matches_one_rebuilt_from_the_store() {
    let dir = tempfile::tempdir().expect("temp dir");
    let vid = "smb-converge";
    // BEFORE: a subtree of three, plus an unrelated folder.
    make_importance_db(
        dir.path(),
        vid,
        &[("/a", 0.2), ("/a/keep", 0.4), ("/a/drop", 0.6), ("/b", 0.8)],
    );
    store_weights(vid, load_weights(dir.path(), vid));

    // AFTER an incremental over `/a`: the subtree was cleared, then `/a` and `/a/keep`
    // came back rescored while `/a/drop` didn't — it was deleted or became floored.
    make_importance_db(dir.path(), vid, &[("/a", 0.25), ("/a/keep", 0.45), ("/b", 0.8)]);
    let upserted = [("/a".to_string(), 0.25), ("/a/keep".to_string(), 0.45)];
    let removed = ["/a/drop".to_string()];

    assert!(
        apply_weight_delta(vid, &upserted, &removed),
        "the volume has a map to patch"
    );
    assert_eq!(
        *weights_for(vid),
        load_weights(dir.path(), vid),
        "patching the delta lands on exactly the map a full reload would build"
    );
}

/// A delta for a volume whose map isn't loaded yet can't be applied — patching
/// nothing would leave a map holding ONLY the changed folders. The caller falls back
/// to a full load instead.
#[test]
fn a_delta_for_an_unloaded_volume_asks_for_a_full_load() {
    assert!(
        !apply_weight_delta("smb-never-loaded", &[("/proj".to_string(), 0.9)], &[]),
        "no cached map ⇒ the caller reloads rather than building a partial one"
    );
}

/// THE trap the delta channel exists to close: a consumer that falls behind must
/// RECOVER with a full reload, never treat the gap as "nothing happened". A missed
/// delta would leave the map disagreeing with the store until the next full pass,
/// with nothing to detect it.
#[test]
fn a_lagged_notice_recovers_with_a_full_reload() {
    use tokio::sync::broadcast::error::RecvError;

    assert!(
        matches!(weight_refresh_for(Err(RecvError::Lagged(3))), WeightRefresh::Reload),
        "missing notices means rebuilding, not skipping"
    );
    assert!(
        matches!(weight_refresh_for(Err(RecvError::Closed)), WeightRefresh::Stop),
        "a closed channel stops the subscriber"
    );
}
