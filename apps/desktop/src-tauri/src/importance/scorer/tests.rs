//! Unit tests for the pure importance scorer.
//!
//! Everything here is values-in / values-out: no FFI, no DB, no clock (a fixed
//! `NOW` is passed in). Tests assert each signal's contribution DIRECTION (per the
//! plan's M1 list), the explain-sums-to-score invariant, and missing-signal
//! redistribution.

use super::*;
use crate::importance::fixtures::SyntheticHome;

/// A fixed "now" (2026-07-08T00:00:00Z-ish) so recency is deterministic.
const NOW: u64 = 1_783_900_800;
const DAY: u64 = 24 * 60 * 60;

/// Local-volume availability (all optional signals wired). M1 leaves the optional
/// signals `None`, but availability is orthogonal to value — a local volume marks
/// them available even before their sources exist.
fn all() -> SignalSet {
    SignalSet::all()
}

fn score_of(signals: &FolderSignals) -> f64 {
    score(signals, &all(), &Weights::default(), NOW).value()
}

// ── Denylist and hidden floors ───────────────────────────────────────────────

#[test]
fn node_modules_scores_near_floor() {
    // A denylisted name floors the score regardless of other signals: even a
    // recent, project-adjacent node_modules must not rank as important.
    let mut signals = FolderSignals::neutral();
    signals.name_denylisted = true;
    signals.mtime_secs = Some(NOW); // maximally recent — must not rescue it
    signals.path_class = PathClass::ProjectRoot;
    signals.has_project_marker = true;

    assert_eq!(score_of(&signals), 0.0, "denylisted folder must floor to 0.0");
}

#[test]
fn hidden_or_system_folder_floors() {
    let mut signals = FolderSignals::neutral();
    signals.hidden_or_system = true;
    signals.mtime_secs = Some(NOW);
    signals.path_class = PathClass::UserContent;

    assert_eq!(score_of(&signals), 0.0, "hidden/system folder must floor to 0.0");
}

// ── Project root scores high ─────────────────────────────────────────────────

#[test]
fn git_project_root_scores_high() {
    // An active project root: marker present, project-root path class, recent,
    // mixed source. Should land clearly in the upper half.
    let mut signals = FolderSignals::neutral();
    signals.has_project_marker = true;
    signals.path_class = PathClass::ProjectRoot;
    signals.mtime_secs = Some(NOW - DAY);
    signals.distinct_extension_count = 4;
    signals.file_count = 6;

    assert!(
        score_of(&signals) > 0.6,
        "an active project root should score high, got {}",
        score_of(&signals)
    );
}

// ── Monoculture below a mixed folder ─────────────────────────────────────────

#[test]
fn monoculture_scores_below_mixed_folder() {
    // Same everything except extension diversity: one extension over many files
    // (a logs folder) must score below a mix of kinds.
    let base = |distinct: u32, files: u32| {
        let mut s = FolderSignals::neutral();
        s.distinct_extension_count = distinct;
        s.file_count = files;
        s.mtime_secs = Some(NOW - DAY);
        s.path_class = PathClass::UserContent;
        s
    };
    let monoculture = base(1, 200);
    let mixed = base(5, 20);

    assert!(
        score_of(&monoculture) < score_of(&mixed),
        "monoculture ({}) should score below mixed ({})",
        score_of(&monoculture),
        score_of(&mixed)
    );
}

// ── Recency raises the score ─────────────────────────────────────────────────

#[test]
fn recency_raises_score() {
    let base = |mtime: u64| {
        let mut s = FolderSignals::neutral();
        s.path_class = PathClass::UserContent;
        s.distinct_extension_count = 3;
        s.file_count = 5;
        s.mtime_secs = Some(mtime);
        s
    };
    let recent = base(NOW - DAY);
    let old = base(NOW - 365 * DAY);

    assert!(
        score_of(&recent) > score_of(&old),
        "recent ({}) should score above old ({})",
        score_of(&recent),
        score_of(&old)
    );
}

#[test]
fn path_class_orders_project_over_user_over_neutral_over_system() {
    let with_class = |class: PathClass| {
        let mut s = FolderSignals::neutral();
        s.path_class = class;
        s
    };
    let project = score_of(&with_class(PathClass::ProjectRoot));
    let user = score_of(&with_class(PathClass::UserContent));
    let neutral = score_of(&with_class(PathClass::Neutral));
    let system = score_of(&with_class(PathClass::SystemOrCache));

    assert!(
        project > user && user > neutral && neutral > system,
        "path-class order broken: project={project} user={user} neutral={neutral} system={system}"
    );
}

// ── Explain sums to score ────────────────────────────────────────────────────

#[test]
fn explain_contributions_sum_to_score_unfloored() {
    let mut signals = FolderSignals::neutral();
    signals.has_project_marker = true;
    signals.path_class = PathClass::UserContent;
    signals.mtime_secs = Some(NOW - 5 * DAY);
    signals.distinct_extension_count = 3;
    signals.file_count = 8;

    let explanation = explain(&signals, &all(), &Weights::default(), NOW);
    let sum: f64 = explanation.contributions.iter().map(|c| c.contribution).sum();

    assert!(!explanation.floored, "this fixture should not be floored");
    assert!(
        (sum - explanation.score.value()).abs() < 1e-9,
        "contributions sum {sum} != score {}",
        explanation.score.value()
    );
    // Every contribution equals weight * raw.
    for c in &explanation.contributions {
        assert!(
            (c.contribution - c.weight * c.raw).abs() < 1e-12,
            "{:?}: contribution {} != weight {} * raw {}",
            c.signal,
            c.contribution,
            c.weight,
            c.raw
        );
    }
}

#[test]
fn explain_marks_floored_when_denylisted() {
    let mut signals = FolderSignals::neutral();
    signals.name_denylisted = true;
    let explanation = explain(&signals, &all(), &Weights::default(), NOW);
    assert!(explanation.floored);
    assert_eq!(explanation.score.value(), 0.0);
}

#[test]
fn explain_covers_every_weighted_signal() {
    // COVERAGE assertion (testing.md anti-pattern §"no-op fixture"): the breakdown
    // must include every weighted signal, not silently drop one.
    let explanation = explain(&FolderSignals::neutral(), &all(), &Weights::default(), NOW);
    let kinds: Vec<SignalKind> = explanation.contributions.iter().map(|c| c.signal).collect();
    for expected in SignalKind::ALL {
        assert!(kinds.contains(&expected), "explain missing signal {expected:?}");
    }
    assert_eq!(
        kinds.len(),
        SignalKind::ALL.len(),
        "explain has duplicate/extra signals"
    );
}

// ── Missing-signal redistribution ────────────────────────────────────────────

#[test]
fn missing_optional_signal_redistributes_not_penalizes() {
    // An SMB folder (no Spotlight, no visit store) with the SAME listing signals as
    // a local folder must score the SAME on those listing signals: the weight of
    // the unavailable optional signals is redistributed across the available ones,
    // never left as dead weight that drags the score down.
    let mut signals = FolderSignals::neutral();
    signals.path_class = PathClass::UserContent;
    signals.distinct_extension_count = 4;
    signals.file_count = 6;
    signals.mtime_secs = Some(NOW - DAY);
    // Both local and SMB leave the optional VALUES None in M1; only availability
    // differs.

    let local = score(&signals, &SignalSet::all(), &Weights::default(), NOW).value();
    let smb = score(&signals, &SignalSet::listing_only(), &Weights::default(), NOW).value();

    // On a local volume the optional signals are available but None-valued, so they
    // contribute 0 and drag the reachable maximum down. On SMB their weight
    // redistributes to the present signals, so the SMB score is HIGHER for the same
    // listing signals — redistribution, not fabrication.
    assert!(
        smb > local,
        "SMB redistribution should lift the score above local-with-unavailable-but-zero: smb={smb} local={local}"
    );
}

#[test]
fn redistribution_preserves_total_weight() {
    // The effective weights over the AVAILABLE signals must still sum to the same
    // total as the full weight set (the redistribution conserves mass).
    let weights = Weights::default();
    let full_total = weights.additive_total();

    // Max out every available signal so raw == 1.0 for each; the sum of
    // contributions then equals the sum of effective weights.
    let mut signals = FolderSignals::neutral();
    signals.path_class = PathClass::ProjectRoot;
    signals.has_project_marker = true;
    signals.mtime_secs = Some(NOW);
    signals.distinct_extension_count = 10;
    signals.file_count = 10;
    signals.visit_count = Some(1_000);
    signals.last_used_secs = Some(NOW);

    let explanation = explain(&signals, &SignalSet::listing_only(), &weights, NOW);
    let effective_total: f64 = explanation.contributions.iter().map(|c| c.weight).sum();

    assert!(
        (effective_total - full_total).abs() < 1e-9,
        "redistributed weights sum {effective_total} != full total {full_total}"
    );
}

// ── Fixture generator shape ──────────────────────────────────────────────────

#[test]
fn fixture_generator_builds_expected_tree() {
    let home = SyntheticHome::canonical(NOW);
    let volume = home.volume();

    // The tree has the folders the plan names.
    for expected in [
        "/Users/test/Downloads",
        "/Users/test/projects/webapp",
        "/Users/test/projects/webapp/node_modules",
        "/Users/test/projects/webapp/.git",
        "/Users/test/logs",
        "/Users/test/Documents/invoices",
        "/Users/test/Library/Caches",
    ] {
        let signals = home.signals_for(expected);
        // Every named folder resolves to real signals (non-panicking, classified).
        assert!(
            signals.file_count > 0 || signals.path_class != PathClass::Neutral || signals.name_denylisted,
            "folder {expected} produced empty signals"
        );
    }

    // The volume is a usable InMemoryVolume with the entries in it.
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let downloads = rt.block_on(async {
        crate::file_system::volume::Volume::list_directory(&volume, std::path::Path::new("/Users/test/Downloads"), None)
            .await
            .unwrap()
    });
    assert_eq!(downloads.len(), 4, "Downloads should list its four mixed files");
}

#[test]
fn fixture_ranking_matches_expected_importance_order() {
    // The end-to-end M1 assertion: scoring the canonical home ranks the folders the
    // way a human would. node_modules and .git near the floor; the project root and
    // Downloads high; the monoculture logs folder below the mixed Documents tree.
    let home = SyntheticHome::canonical(NOW);
    // The fixture owns its clock; score against the same "now" it built mtimes from.
    let now = home.now_secs;
    let s = |p: &str| score(&home.signals_for(p), &SignalSet::all(), &Weights::default(), now).value();

    let downloads = s("/Users/test/Downloads");
    let webapp = s("/Users/test/projects/webapp");
    let node_modules = s("/Users/test/projects/webapp/node_modules");
    let git = s("/Users/test/projects/webapp/.git");
    let logs = s("/Users/test/logs");
    let invoices = s("/Users/test/Documents/invoices");
    let caches = s("/Users/test/Library/Caches");

    // Floored machine output.
    assert_eq!(node_modules, 0.0, "node_modules should floor");
    assert_eq!(git, 0.0, ".git should floor");

    // Caches is system/cache: should be low (not necessarily floored, but bottom).
    assert!(caches < 0.2, "Library/Caches should score low, got {caches}");

    // The monoculture logs folder scores below the mixed Documents/invoices.
    assert!(
        logs < invoices,
        "monoculture logs ({logs}) should rank below mixed invoices ({invoices})"
    );

    // The active project root and Downloads are the top of the tree.
    assert!(
        webapp > invoices,
        "project root ({webapp}) should outrank invoices ({invoices})"
    );
    assert!(downloads > logs, "Downloads ({downloads}) should outrank logs ({logs})");
}

// ── Serde round-trip (load-bearing for M2 persistence) ───────────────────────

#[test]
fn folder_signals_serde_roundtrips() {
    // M2 persists `FolderSignals` as the raw signal vector (plan Decision 2). If
    // its serde shape drifts, stored vectors become unreadable, so pin it here.
    let mut signals = FolderSignals::neutral();
    signals.name_denylisted = true;
    signals.distinct_extension_count = 7;
    signals.file_count = 42;
    signals.mtime_secs = Some(1_700_000_000);
    signals.has_project_marker = true;
    signals.path_class = PathClass::ProjectRoot;
    signals.visit_count = Some(3);
    signals.last_used_secs = Some(1_699_000_000);

    let json = serde_json::to_string(&signals).expect("serialize FolderSignals");
    // camelCase on the wire (matches the house convention).
    assert!(json.contains("nameDenylisted"), "expected camelCase keys, got {json}");
    assert!(json.contains("distinctExtensionCount"));

    let back: FolderSignals = serde_json::from_str(&json).expect("deserialize FolderSignals");
    assert_eq!(back, signals, "FolderSignals must round-trip through serde");
}

// ── Property: score is always a valid, finite [0,1] ──────────────────────────

proptest::proptest! {
    #[test]
    fn score_is_always_finite_and_in_range(
        name_denylisted in proptest::bool::ANY,
        hidden_or_system in proptest::bool::ANY,
        distinct in 0u32..500,
        files in 0u32..5000,
        mtime in proptest::option::of(0u64..2_000_000_000),
        has_marker in proptest::bool::ANY,
        path_class_idx in 0usize..4,
        visit in proptest::option::of(0u32..10_000),
        last_used in proptest::option::of(0u64..2_000_000_000),
        visit_available in proptest::bool::ANY,
        last_used_available in proptest::bool::ANY,
        now in 0u64..2_000_000_000,
    ) {
        let path_class = [PathClass::UserContent, PathClass::ProjectRoot, PathClass::SystemOrCache, PathClass::Neutral][path_class_idx];
        let signals = FolderSignals {
            name_denylisted,
            hidden_or_system,
            distinct_extension_count: distinct,
            file_count: files,
            mtime_secs: mtime,
            has_project_marker: has_marker,
            path_class,
            visit_count: visit,
            last_used_secs: last_used,
        };
        let available = SignalSet { visit_available, last_used_available };
        let value = score(&signals, &available, &Weights::default(), now).value();
        proptest::prop_assert!(value.is_finite(), "score not finite: {value}");
        proptest::prop_assert!((0.0..=1.0).contains(&value), "score out of range: {value}");

        // Explain must agree with score and stay consistent.
        let explanation = explain(&signals, &available, &Weights::default(), now);
        proptest::prop_assert_eq!(explanation.score.value(), value);
        if !explanation.floored {
            let sum: f64 = explanation.contributions.iter().map(|c| c.contribution).sum::<f64>().clamp(0.0, 1.0);
            proptest::prop_assert!((sum - value).abs() < 1e-9, "unfloored sum {} != score {}", sum, value);
        }
    }
}
