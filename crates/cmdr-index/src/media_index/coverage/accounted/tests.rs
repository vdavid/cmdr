//! The accounted aggregate: seeding once, the writer's ±1 deltas, and the subtree
//! rollup they invalidate.

use super::*;

#[test]
fn accounted_seed_increment_decrement_and_subtree() {
    let vid = "coverage-test-accounted-seed";
    invalidate(vid);
    // Seed one dir with two enriched rows, then add a sibling dir via increment.
    seed_if_absent(vid, [("/a/b".to_string(), 2u64)].into_iter().collect());
    inc(vid, "/a/c");
    // The subtree of /a rolls up both dirs (2 + 1).
    assert_eq!(subtrees(vid, &["/a".to_string()]), vec![3]);
    assert_eq!(subtrees(vid, &["/a/b".to_string(), "/a/c".to_string()]), vec![2, 1]);

    // Decrement /a/b twice: it drains to zero and is dropped from the map.
    dec(vid, "/a/b");
    dec(vid, "/a/b");
    assert_eq!(subtrees(vid, &["/a/b".to_string()]), vec![0], "/a/b drained");
    assert_eq!(subtrees(vid, &["/a".to_string()]), vec![1], "only /a/c remains");

    // A decrement past zero never goes negative (a stray delete of an untracked dir).
    dec(vid, "/a/b");
    dec(vid, "/a/c");
    dec(vid, "/a/c");
    assert_eq!(subtrees(vid, &["/a".to_string()]), vec![0], "never negative");
    invalidate(vid);
}

#[test]
fn accounted_ops_on_an_unseeded_volume_are_noops() {
    // A delta before seeding must NOT insert a partial (un-seeded) entry that a later
    // `ensure_seeded` would trust as a complete baseline.
    let vid = "coverage-test-accounted-unseeded";
    invalidate(vid);
    inc(vid, "/a");
    assert!(
        !ACCOUNTED.lock_ignore_poison().contains_key(vid),
        "an increment on an unseeded volume inserts nothing"
    );
    assert_eq!(subtrees(vid, &["/a".to_string()]), vec![0]);
}

#[test]
fn seed_if_absent_never_clobbers_an_existing_entry() {
    // The insert-if-absent concurrency line: a second seed (e.g. a command scan that
    // lost the race to the writer) must not overwrite the live counts.
    let vid = "coverage-test-accounted-noclobber";
    invalidate(vid);
    seed_if_absent(vid, [("/a".to_string(), 5u64)].into_iter().collect());
    inc(vid, "/a");
    // A late, stale seed is discarded — the incremented count survives.
    seed_if_absent(vid, [("/a".to_string(), 5u64)].into_iter().collect());
    assert_eq!(subtrees(vid, &["/a".to_string()]), vec![6]);
    invalidate(vid);
}

#[test]
fn accounted_reset_empties_but_keeps_the_volume_seeded() {
    let vid = "coverage-test-accounted-reset";
    invalidate(vid);
    seed_if_absent(vid, [("/a".to_string(), 3u64)].into_iter().collect());
    reset(vid);
    assert_eq!(subtrees(vid, &["/a".to_string()]), vec![0], "emptied");
    assert!(
        ACCOUNTED.lock_ignore_poison().contains_key(vid),
        "still seeded, so a later insert bumps from zero rather than re-scanning"
    );
    inc(vid, "/a");
    assert_eq!(subtrees(vid, &["/a".to_string()]), vec![1]);
    invalidate(vid);
}
