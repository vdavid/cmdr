//! Constraint-evaluation arithmetic (TDD target): ranking order + tie handling,
//! decile boundaries, top-N and above semantics, and the satisfied-fraction score.
//! These are the load-bearing semantics the whole harness rests on, so they're
//! pinned directly (no scorer, no fixtures — pure `(path, score)` in, outcome out).

use super::*;

/// A ranking of `n` folders named `f0..fn` with descending scores, so `f0` is the
/// most important and `f{n-1}` the least. Handy for decile/rank assertions.
fn descending(n: usize) -> Ranking {
    Ranking::from_scores((0..n).map(|i| (format!("f{i}"), (n - i) as f64)))
}

#[test]
fn ranking_sorts_by_score_desc_then_path_asc() {
    // Two folders tie on score; the tie breaks by path ASC (the read API's order),
    // never by input order — so `alpha` outranks `beta` regardless of insertion.
    let r = Ranking::from_scores([
        ("beta".to_string(), 0.5),
        ("alpha".to_string(), 0.5),
        ("top".to_string(), 0.9),
    ]);
    let order: Vec<&str> = r.ordered().iter().map(|f| f.path.as_str()).collect();
    assert_eq!(order, ["top", "alpha", "beta"], "score desc, then path asc for ties");
    assert_eq!(r.rank_of("top"), Some(0));
    assert_eq!(r.rank_of("alpha"), Some(1));
    assert_eq!(r.rank_of("beta"), Some(2));
    assert_eq!(r.rank_of("missing"), None);
}

#[test]
fn decile_boundaries_are_inclusive_and_clamped() {
    // 10 items: rank 0 → decile 1, rank 9 → decile 10 (clamped, never 11).
    assert_eq!(decile(0, 10), 1);
    assert_eq!(decile(9, 10), 10);
    // The tenth's boundary: rank 1 of 10 is still decile 2 (10*1/10 + 1 = 2).
    assert_eq!(decile(1, 10), 2);
    // A single item is in decile 1 (the whole list is the top tenth degenerate).
    assert_eq!(decile(0, 1), 1);
    // The last item of any full list lands in decile 10, not out of range.
    assert_eq!(decile(19, 20), 10);
    assert_eq!(decile(99, 100), 10);
    // Defensive: an empty set can't rank anything, so decile is 1, never a panic.
    assert_eq!(decile(0, 0), 1);
}

#[test]
fn decile_of_reads_from_the_ranking() {
    let r = descending(10);
    assert_eq!(r.decile_of("f0"), Some(1), "the top folder is decile 1");
    assert_eq!(r.decile_of("f9"), Some(10), "the bottom folder is decile 10");
    assert_eq!(r.decile_of("nope"), None);
}

#[test]
fn above_constraint_respects_strict_ordering() {
    let r = descending(3); // f0 > f1 > f2
    assert_eq!(
        Constraint::Above {
            above: "f0".to_string(),
            below: "f2".to_string(),
        }
        .evaluate(&r),
        ConstraintOutcome::Satisfied
    );
    // The reverse is violated.
    assert!(matches!(
        Constraint::Above {
            above: "f2".to_string(),
            below: "f0".to_string(),
        }
        .evaluate(&r),
        ConstraintOutcome::Violated(_)
    ));
    // A folder is NOT strictly above itself (equal rank ⇒ violated).
    assert!(matches!(
        Constraint::Above {
            above: "f1".to_string(),
            below: "f1".to_string(),
        }
        .evaluate(&r),
        ConstraintOutcome::Violated(_)
    ));
    // A missing folder is Unknown, not a silent pass.
    assert!(matches!(
        Constraint::Above {
            above: "f0".to_string(),
            below: "ghost".to_string(),
        }
        .evaluate(&r),
        ConstraintOutcome::Unknown(_)
    ));
}

#[test]
fn top_n_is_zero_based_and_exclusive_of_n() {
    let r = descending(10);
    // f0 is rank 0, in the top 1. f1 is rank 1, NOT in the top 1 but in the top 2.
    assert_eq!(
        Constraint::TopN {
            path: "f0".to_string(),
            n: 1
        }
        .evaluate(&r),
        ConstraintOutcome::Satisfied
    );
    assert!(matches!(
        Constraint::TopN {
            path: "f1".to_string(),
            n: 1
        }
        .evaluate(&r),
        ConstraintOutcome::Violated(_)
    ));
    assert_eq!(
        Constraint::TopN {
            path: "f1".to_string(),
            n: 2
        }
        .evaluate(&r),
        ConstraintOutcome::Satisfied
    );
}

#[test]
fn bottom_decile_and_decile_at_most() {
    let r = descending(20); // f0 best … f19 worst
    // f19 is the last of 20 ⇒ decile 10 ⇒ bottom decile satisfied.
    assert_eq!(
        Constraint::BottomDecile {
            path: "f19".to_string()
        }
        .evaluate(&r),
        ConstraintOutcome::Satisfied
    );
    // f0 is decile 1, so bottom-decile is violated for it.
    assert!(matches!(
        Constraint::BottomDecile { path: "f0".to_string() }.evaluate(&r),
        ConstraintOutcome::Violated(_)
    ));
    // DecileAtMost: f0 (decile 1) satisfies "decile 2 or better".
    assert_eq!(
        Constraint::DecileAtMost {
            path: "f0".to_string(),
            at_most: 2
        }
        .evaluate(&r),
        ConstraintOutcome::Satisfied
    );
}

#[test]
fn score_at_most_pins_the_floor() {
    // A floored folder scores exactly 0.0; ScoreAtMost pins that (ordering alone
    // can't, since many folders may tie at 0.0).
    let r = Ranking::from_scores([("cache".to_string(), 0.0), ("docs".to_string(), 0.7)]);
    assert_eq!(
        Constraint::ScoreAtMost {
            path: "cache".to_string(),
            max: 0.0
        }
        .evaluate(&r),
        ConstraintOutcome::Satisfied
    );
    assert!(matches!(
        Constraint::ScoreAtMost {
            path: "docs".to_string(),
            max: 0.0
        }
        .evaluate(&r),
        ConstraintOutcome::Violated(_)
    ));
}

#[test]
fn satisfied_fraction_counts_only_satisfied() {
    let r = descending(4); // f0 > f1 > f2 > f3
    let constraints = vec![
        // satisfied
        Constraint::Above {
            above: "f0".to_string(),
            below: "f3".to_string(),
        },
        // violated
        Constraint::Above {
            above: "f3".to_string(),
            below: "f0".to_string(),
        },
        // unknown (missing folder) — counts as NOT satisfied
        Constraint::TopN {
            path: "ghost".to_string(),
            n: 1,
        },
        // satisfied
        Constraint::TopN {
            path: "f0".to_string(),
            n: 2,
        },
    ];
    assert_eq!(
        satisfied_fraction(&constraints, &r),
        0.5,
        "2 of 4 satisfied ⇒ 0.5 (unknown and violated both count against)"
    );
    // An empty set is vacuously perfect (nothing to violate).
    assert_eq!(satisfied_fraction(&[], &r), 1.0);
}
