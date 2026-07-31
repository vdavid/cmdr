//! The eval suite proper: hard constraints as CI-failing `#[test]`s, the pinned
//! soft-score floor, scenario-integrity guards, and the corpus round-trip +
//! auto-load. The constraint arithmetic and the anonymization contract are pinned
//! in their own sibling modules (`constraints/tests.rs`, `corpus/tests.rs`).

use super::*;
use crate::importance::scorer::Weights;

/// The pinned soft-score floor: the aggregate soft-constraint satisfaction the
/// default weights must clear. A FIXED constant, not a self-updating ratchet — a
/// change that drops quality below it fails, and when tuning IMPROVES quality this
/// number gets raised by hand (the guide walks through it). The default weights
/// currently satisfy every soft constraint (aggregate 1.0); this sits a small
/// margin below so an incidental regression is caught without flaking on
/// floating-point noise.
pub const SOFT_SCORE_FLOOR: f64 = 0.95;

/// Every scenario the harness scores: the committed synthetic ones plus any
/// labeled corpus scenarios found in the local gitignored corpus dir. With no
/// corpus present (CI, a fresh clone) this is exactly the synthetic set, so the
/// suite is fully green without any corpus files.
fn all_scenarios() -> Vec<Scenario> {
    let mut scenarios = scenarios::all();
    // The corpus dir is relative to the repo root; from this crate's manifest dir
    // (`apps/desktop/src-tauri`) the root is three levels up.
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .map(std::path::Path::to_path_buf);
    if let Some(root) = repo_root {
        scenarios.extend(corpus::load_corpus_scenarios(&corpus::corpus_dir(&root)));
    }
    scenarios
}

// ── Hard constraints: a violation fails CI ────────────────────────────────────

/// Every hard constraint in every scenario must hold under the default weights.
/// These are the ordering facts that must ALWAYS be true (a `node_modules` scores
/// 0, a project root outranks its logs). A violation here is a real regression, so
/// it fails the build. The failure message names the scenario, the constraint, and
/// why it broke.
#[test]
fn all_hard_constraints_hold_under_default_weights() {
    let weights = Weights::default();
    let mut failures = Vec::new();
    for scenario in all_scenarios() {
        let ranking = rank_scenario(&scenario, &weights);
        for constraint in &scenario.hard {
            match constraint.evaluate(&ranking) {
                ConstraintOutcome::Satisfied => {}
                ConstraintOutcome::Violated(why) | ConstraintOutcome::Unknown(why) => {
                    failures.push(format!("[{}] {why}", scenario.name));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "hard constraints violated:\n  {}",
        failures.join("\n  ")
    );
}

// ── Soft-score floor: quality must not regress below the pinned floor ──────────

/// The aggregate soft-constraint satisfaction under the default weights must clear
/// the pinned floor. This is the regression guard the tuning loop leans on: turn a
/// weight knob, run this, and a drop below the floor fails. When a knob turn
/// IMPROVES the aggregate, raise [`SOFT_SCORE_FLOOR`] to lock the gain in.
#[test]
fn soft_score_clears_the_pinned_floor() {
    let weights = Weights::default();
    let scenarios = all_scenarios();
    let aggregate = aggregate_score(&scenarios, &weights);
    assert!(
        aggregate >= SOFT_SCORE_FLOOR,
        "aggregate soft score {aggregate:.4} dropped below the pinned floor {SOFT_SCORE_FLOOR:.4} — either a weight \
         change regressed ranking quality (fix it), or the scenarios changed (re-pin the floor consciously)"
    );
}

/// Each synthetic scenario, on its own, clears the floor too — so a single
/// scenario regressing can't hide behind a healthy average. Corpus scenarios are
/// excluded here (an unlabeled dump has no soft constraints and scores a vacuous
/// 1.0; a labeled one is David's personal ground truth, guarded by the aggregate).
#[test]
fn each_synthetic_scenario_clears_the_floor() {
    let weights = Weights::default();
    for scenario in scenarios::all() {
        let score = score_scenario(&scenario, &weights);
        assert!(
            score >= SOFT_SCORE_FLOOR,
            "scenario '{}' soft score {score:.4} is below the floor {SOFT_SCORE_FLOOR:.4}",
            scenario.name
        );
    }
}

// ── Scenario integrity: expectations must name real folders ───────────────────

/// Every path a constraint names must exist in its scenario's folders. A mistyped
/// path would otherwise evaluate to `Unknown` and silently count as a violation
/// (dragging the soft score, or failing a hard test with a confusing message); this
/// catches the typo directly, at authoring time.
#[test]
fn all_constraints_reference_existing_folders() {
    for scenario in scenarios::all() {
        let paths: std::collections::HashSet<&str> = scenario.folders.iter().map(|f| f.path.as_str()).collect();
        let check = |constraint: &Constraint, tier: &str| {
            for named in constraint_paths(constraint) {
                assert!(
                    paths.contains(named.as_str()),
                    "scenario '{}' {tier} constraint names a folder not in the scenario: {named}",
                    scenario.name
                );
            }
        };
        for c in &scenario.hard {
            check(c, "hard");
        }
        for c in &scenario.soft {
            check(c, "soft");
        }
    }
}

/// The folder paths a constraint references (for the integrity check).
fn constraint_paths(constraint: &Constraint) -> Vec<String> {
    match constraint {
        Constraint::Above { above, below } => vec![above.clone(), below.clone()],
        Constraint::TopN { path, .. }
        | Constraint::BottomDecile { path }
        | Constraint::DecileAtMost { path, .. }
        | Constraint::ScoreAtMost { path, .. } => vec![path.clone()],
    }
}

/// Every scenario has at least one hard and one soft constraint — an unconstrained
/// scenario measures nothing.
#[test]
fn synthetic_scenarios_carry_expectations() {
    for scenario in scenarios::all() {
        assert!(
            !scenario.hard.is_empty(),
            "scenario '{}' has no hard constraints",
            scenario.name
        );
        assert!(
            !scenario.soft.is_empty(),
            "scenario '{}' has no soft constraints",
            scenario.name
        );
    }
}

// ── The fitness function is pure and deterministic ────────────────────────────

/// [`score_scenario`] is a pure function of `(scenario, weights)`: same inputs,
/// same output, every time — the property a grid-search / hill-climb tuner relies
/// on. (No clock: the scenario carries its own `now`.)
#[test]
fn score_scenario_is_deterministic() {
    let weights = Weights::default();
    for scenario in scenarios::all() {
        let a = score_scenario(&scenario, &weights);
        let b = score_scenario(&scenario, &weights);
        assert_eq!(a, b, "scenario '{}' scored differently on a re-run", scenario.name);
    }
}

// ── Corpus format round-trip + labeled auto-load ──────────────────────────────

/// A scenario survives a JSON round-trip unchanged — the committed + dumped file
/// format. Pins that the serde shape (including the `#[serde(tag = "kind")]`
/// constraints and the camelCase signal fields) is stable.
#[test]
fn scenario_json_roundtrips() {
    for scenario in scenarios::all() {
        let json = scenario.to_json().expect("serialize");
        let back = Scenario::from_json(&json).expect("deserialize");
        assert_eq!(scenario, back, "scenario '{}' didn't round-trip", scenario.name);
    }
}

/// The labeled corpus auto-load turns a dump + labels into a scenario with
/// personalized soft constraints, over a temp dir (no real corpus needed). This is
/// the end-to-end proof of the "snapshot → label → the harness scores your ground
/// truth" loop the guide describes.
#[test]
fn corpus_load_applies_labels_as_soft_constraints() {
    use crate::importance::evals::corpus::{LabeledFolder, LabelsTemplate};

    let dir = tempfile::tempdir().expect("temp dir");

    // A tiny anonymized dump: three folders, no expectations (a fresh snapshot).
    let dump = Scenario {
        name: "corpus-test".to_string(),
        description: "test dump".to_string(),
        availability: Availability::Local,
        now_secs: 1_000_000_000,
        folders: vec![
            ScenarioFolder {
                path: "/home/dir-aaaa".to_string(),
                signals: important_signals(),
            },
            ScenarioFolder {
                path: "/home/dir-bbbb".to_string(),
                signals: neutral_signals(),
            },
            ScenarioFolder {
                path: "/home/dir-cccc".to_string(),
                signals: neutral_signals(),
            },
        ],
        hard: Vec::new(),
        soft: Vec::new(),
    };
    std::fs::write(
        dir.path().join("corpus-test.scenario.json"),
        dump.to_json().expect("serialize dump"),
    )
    .expect("write dump");

    // David labels the first folder important.
    let labels = LabelsTemplate {
        note: String::new(),
        important: vec![LabeledFolder {
            path: "/home/dir-aaaa".to_string(),
            importance: 1,
        }],
    };
    std::fs::write(
        dir.path().join("corpus-test.labels.json"),
        serde_json::to_string_pretty(&labels).expect("serialize labels"),
    )
    .expect("write labels");

    let loaded = corpus::load_corpus_scenarios(dir.path());
    assert_eq!(loaded.len(), 1, "one corpus scenario loaded");
    let scenario = &loaded[0];
    assert!(
        !scenario.soft.is_empty(),
        "the label produced a soft constraint (personalized ground truth)"
    );
    // The important folder has genuinely-important signals, so it should satisfy
    // its generated top-N constraint under the default weights.
    let weights = Weights::default();
    assert_eq!(
        score_scenario(scenario, &weights),
        1.0,
        "the labeled important folder ranks where its signals put it"
    );
}

/// A dump with no labels (no labels file, or an empty `important` list) is SKIPPED
/// entirely — it measures nothing, and a real dump can be hundreds of MB, so the
/// loader must not parse it just to add zero constraints. This is what keeps the
/// suite fast even as David's unlabeled dumps accumulate locally, and green in CI
/// (which has no corpus at all).
#[test]
fn corpus_load_skips_unlabeled_dumps() {
    let dir = tempfile::tempdir().expect("temp dir");
    let dump = Scenario {
        name: "unlabeled".to_string(),
        description: "test".to_string(),
        availability: Availability::Local,
        now_secs: 1_000_000_000,
        folders: vec![ScenarioFolder {
            path: "/home/dir-1234".to_string(),
            signals: neutral_signals(),
        }],
        hard: Vec::new(),
        soft: Vec::new(),
    };
    std::fs::write(
        dir.path().join("unlabeled.scenario.json"),
        dump.to_json().expect("serialize"),
    )
    .expect("write");

    // No labels file at all ⇒ skipped.
    assert!(
        corpus::load_corpus_scenarios(dir.path()).is_empty(),
        "an unlabeled dump is skipped"
    );

    // An empty labels file ⇒ still skipped.
    use crate::importance::evals::corpus::LabelsTemplate;
    std::fs::write(
        dir.path().join("unlabeled.labels.json"),
        LabelsTemplate::default().to_json().expect("serialize labels"),
    )
    .expect("write labels");
    assert!(
        corpus::load_corpus_scenarios(dir.path()).is_empty(),
        "an empty-label dump is also skipped"
    );
}

/// A missing corpus dir loads nothing (the committed-suite-is-green guarantee).
#[test]
fn corpus_load_missing_dir_is_empty() {
    let missing = std::path::Path::new("/nonexistent/corpus/dir/that/does/not/exist");
    assert!(corpus::load_corpus_scenarios(missing).is_empty());
}

// ── Signal helpers for the corpus tests ───────────────────────────────────────

fn neutral_signals() -> crate::importance::scorer::FolderSignals {
    crate::importance::scorer::FolderSignals::neutral()
}

/// A signal vector a healthy user-content folder would carry (mixed, recent,
/// visited) — so a labeled-important test folder actually ranks high.
fn important_signals() -> crate::importance::scorer::FolderSignals {
    use crate::importance::scorer::{FolderSignals, PathClass};
    FolderSignals {
        distinct_extension_count: 4,
        file_count: 5,
        mtime_secs: Some(1_000_000_000),
        path_class: PathClass::UserContent,
        visit_count: Some(10),
        last_used_secs: Some(1_000_000_000),
        ..FolderSignals::neutral()
    }
}
