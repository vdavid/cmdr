//! The over-budget-origin half of the transition suite: an origin whose subtree
//! covers most of the volume is DEMOTED (rescored alone) rather than descended.
//!
//! Split from the parent file only for length; it shares that file's harness, so the
//! differential still runs every scenario under both walks.

use super::*;

/// An origin whose subtree covers most of the volume is rescored ALONE: its own row
/// moves and every row beneath it is left exactly as it was.
///
/// This is the treadmill's remaining half (`docs/notes/importance-treadmill-2026-08-04.md`):
/// a dotfile write in `~` makes `$HOME` an origin, and `$HOME` covers 83% of the real
/// root volume. The change to the origin's own listing genuinely cannot move any
/// DESCENDANT's signals, so reading 574,007 directories to discover that is waste.
///
/// The differential is what makes the demotion safe to assert: the full walk rescores
/// the whole subtree and still leaves an identical store, because the writer skips
/// every row whose signals didn't move.
#[test]
fn an_over_budget_origin_rescores_itself_without_its_subtree() {
    let scenario = |v: &mut TestVolume| {
        two_projects(v);
        v.full_pass();
        v.set_recursive_dir_count(HOME, 1_000_000);
        v.touch("/Users/test/.zsh_history");
        v.incremental(&[HOME]);
    };

    let weights = differential(scenario);
    assert_eq!(
        parse(&weights[HOME].1).file_count,
        1,
        "the demoted origin's own listing change still lands"
    );

    let trace = scoped_trace(scenario);
    assert_eq!(trace.full_walk_passes, 0, "an over-budget origin costs no full walk");
    assert_eq!(
        trace.last_report.considered, 1,
        "a demoted origin rescores itself and nothing under it"
    );
    assert_eq!(trace.last_report.written, 1, "and writes exactly its own moved row");
}

/// A change in a subtree UNDER a demoted origin still gets rescored, because the
/// demoted origin no longer absorbs it during de-duplication.
///
/// Without this the demotion would silently drop real work: `~/.claude.json` is
/// rewritten constantly, so almost every batch carries `$HOME` alongside whatever the
/// user actually changed.
#[test]
fn a_change_under_a_demoted_origin_still_rescores_its_own_subtree() {
    let weights = differential(|v| {
        two_projects(v);
        v.full_pass();
        v.set_recursive_dir_count(HOME, 1_000_000);
        v.touch("/Users/test/.zsh_history");
        v.touch("/Users/test/projects/alpha/docs/design.md");
        v.incremental(&[HOME, "/Users/test/projects/alpha/docs"]);
    });

    assert_eq!(
        parse(&weights["/Users/test/projects/alpha/docs"].1).file_count,
        2,
        "the nested origin's own change is not lost to the demoted ancestor"
    );
}

/// A project marker appearing DIRECTLY inside a demoted origin still reaches the
/// ancestors above it: the marker guard sees the flip and takes the full walk.
///
/// The demotion's one genuine correctness hinge. A demoted origin's `has_marker_below`
/// can't be recomputed (nothing below it was read), so it comes from the stored row —
/// but its own direct children ARE read, so a `.git` landing in `~` flips the origin's
/// marker presence and escalates exactly as an unbounded origin would.
#[test]
fn a_marker_appearing_in_a_demoted_origin_takes_the_full_walk() {
    let scenario = |v: &mut TestVolume| {
        two_projects(v);
        v.full_pass();
        v.set_recursive_dir_count(HOME, 1_000_000);
        v.mkdir("/Users/test/.git");
        v.incremental(&[HOME]);
    };

    let weights = differential(scenario);
    assert!(has_marker(&weights, HOME), "the home reads as project-adjacent now");
    assert!(
        !has_marker(&weights, "/Users/test/projects"),
        "a marker rises to the ancestors, never down into the siblings below"
    );
    assert_eq!(
        full_walk_passes(scenario),
        1,
        "a marker flip at a demoted origin still costs the full walk"
    );
}

/// A demoted origin with no stored row to compare against still escalates: the
/// demotion must not turn `MarkerPresenceUnknown` into a silently wrong value.
#[test]
fn a_demoted_origin_with_no_stored_row_takes_the_full_walk() {
    assert_eq!(
        full_walk_passes(|v| {
            two_projects(v);
            // No full pass: the store holds nothing to compare the origin against.
            v.set_recursive_dir_count(HOME, 1_000_000);
            v.touch("/Users/test/.zsh_history");
            v.incremental(&[HOME]);
        }),
        1,
        "an origin the store has never seen can't rule its ancestors out"
    );
}
