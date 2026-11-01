//! `cmdr://importance` builder tests, over a real `importance.db` written through
//! the store's own writer (no app, no index) — the M2 red→green targets.
//!
//! Split by concern: the snapshot functions (the reads) and the pure text builders
//! (the formatting an agent reads). The builders are the risky part — a wrong label
//! silently misleads an agent — so they're asserted hardest.

use std::path::Path;

use super::*;
use cmdr_index::importance::testing::{ImportanceWriter, WeightRow, importance_db_path};
use cmdr_index::importance::{FolderSignals, PathClass, SignalSet, Weights};

/// A folder's signals for a scored row that carries a real, explainable breakdown.
pub(crate) fn scored_signals(path_class: PathClass, now: u64) -> FolderSignals {
    let mut s = FolderSignals::neutral();
    s.path_class = path_class;
    s.mtime_secs = Some(now); // fresh ⇒ recency contributes
    s.distinct_extension_count = 4;
    s.file_count = 6;
    s
}

/// Write an `importance-{volume_id}.db` under `dir` with the given `(path, signals)`
/// rows at generation 1. Each row's stored score is the scorer's own output for its
/// signals, so `explain`'s recomputed breakdown matches the stored scalar.
pub(crate) fn write_db(dir: &Path, volume_id: &str, rows: &[(&str, FolderSignals)], now: u64) {
    let db_path = importance_db_path(dir, volume_id);
    let writer = ImportanceWriter::spawn(&db_path).expect("spawn writer");
    let weight_rows: Vec<WeightRow> = rows
        .iter()
        .map(|(path, signals)| {
            let score = cmdr_index::importance::score(signals, &SignalSet::all(), &Weights::default(), now);
            WeightRow {
                path: path.to_string(),
                score: score.value(),
                signals_json: serde_json::to_string(signals).expect("serialize signals"),
            }
        })
        .collect();
    writer.write_weights(1, weight_rows).expect("write");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}

// ── ?path= ───────────────────────────────────────────────────────────────────

#[test]
fn scored_path_reports_score_and_signal_breakdown() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    let path = "/Users/test/Downloads";
    write_db(
        dir.path(),
        "root",
        &[(path, scored_signals(PathClass::UserContent, now))],
        now,
    );

    let snapshot = snapshot_path(dir.path(), path, now);
    let PathImportance::Scored {
        ref weight,
        ref explanation,
        ..
    } = snapshot
    else {
        panic!("expected Scored, got {snapshot:?}");
    };
    assert!(weight.score.value() > 0.0, "a UserContent folder scores above zero");
    assert!(explanation.is_some(), "a scored path carries its explain breakdown");

    let text = build_path_text(path, &snapshot);
    assert!(text.contains(path), "names the folder");
    assert!(text.contains("scored"), "labels the status");
    assert!(text.contains(&format_score(weight.score.value())), "shows the score");
    // The breakdown lists signals by their camelCase labels.
    assert!(
        text.contains("extensionDiversity"),
        "lists a signal by its camelCase label"
    );
    assert!(text.contains("pathClass"), "lists the path-class signal");
}

#[test]
fn floored_path_reports_the_reason() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    // A scored sibling so the volume DB exists, plus a node_modules that floors.
    write_db(
        dir.path(),
        "root",
        &[("/Users/test/proj", scored_signals(PathClass::ProjectRoot, now))],
        now,
    );

    let node_modules = "/Users/test/proj/node_modules";
    let snapshot = snapshot_path(dir.path(), node_modules, now);
    assert_eq!(
        snapshot,
        PathImportance::Floored {
            reason: FloorReason::NameDenylisted
        },
        "a node_modules floors by its denylisted name"
    );

    let text = build_path_text(node_modules, &snapshot);
    assert!(text.contains("floored"), "labels it floored");
    assert!(text.contains("nameDenylisted"), "names the floor reason");
}

#[test]
fn floored_under_ancestor_reports_that_reason() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(
        dir.path(),
        "root",
        &[("/Users/test/proj", scored_signals(PathClass::ProjectRoot, now))],
        now,
    );

    let under = "/Users/test/proj/node_modules/react";
    let snapshot = snapshot_path(dir.path(), under, now);
    assert_eq!(
        snapshot,
        PathImportance::Floored {
            reason: FloorReason::UnderFlooredAncestor
        }
    );
    assert!(build_path_text(under, &snapshot).contains("underFlooredAncestor"));
}

#[test]
fn unscored_path_says_so() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(
        dir.path(),
        "root",
        &[("/Users/test/proj", scored_signals(PathClass::ProjectRoot, now))],
        now,
    );

    let ordinary = "/Users/test/some/ordinary/folder";
    let snapshot = snapshot_path(dir.path(), ordinary, now);
    assert_eq!(snapshot, PathImportance::Unscored);
    assert!(build_path_text(ordinary, &snapshot).contains("unscored"));
}

#[test]
fn path_with_no_importance_data_at_all_is_unscored_not_an_error() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir"); // no DBs written
    let snapshot = snapshot_path(dir.path(), "/Users/test/anything", now);
    assert_eq!(
        snapshot,
        PathImportance::Unscored,
        "no DB ⇒ unscored, never a read error"
    );
    // And a denylisted path still floors purely from the path, with no DB present.
    assert_eq!(
        snapshot_path(dir.path(), "/Users/test/x/node_modules", now),
        PathImportance::Floored {
            reason: FloorReason::NameDenylisted
        }
    );
}

// ── ?top= and ?threshold= ─────────────────────────────────────────────────────

#[test]
fn top_n_ranks_highest_first_across_volumes() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(
        dir.path(),
        "root",
        &[
            ("/a", low_score_signals()),
            ("/b", scored_signals(PathClass::ProjectRoot, now)),
        ],
        now,
    );
    write_db(
        dir.path(),
        "smb-share",
        &[("/mnt/c", scored_signals(PathClass::UserContent, now))],
        now,
    );

    let folders = snapshot_top(dir.path(), 2, RankedScope::default(), now).expect("top");
    assert_eq!(folders.len(), 2, "capped at n across the merged volumes");
    assert!(
        folders[0].score >= folders[1].score,
        "highest score first (got {:?})",
        folders
    );

    let text = build_ranked_text("Top 2 folders:", &folders, None);
    assert!(text.contains(&folders[0].path), "lists the top folder's path");
    assert!(text.contains(&format_score(folders[0].score)), "shows its score");
}

#[test]
fn top_n_on_a_named_volume_scopes_to_it() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(
        dir.path(),
        "root",
        &[("/a", scored_signals(PathClass::UserContent, now))],
        now,
    );
    write_db(
        dir.path(),
        "smb-share",
        &[("/mnt/c", scored_signals(PathClass::UserContent, now))],
        now,
    );

    let folders = snapshot_top(dir.path(), 10, on_volume("smb-share"), now).expect("top");
    assert_eq!(folders.len(), 1, "only the named volume's folders");
    assert_eq!(folders[0].path, "/mnt/c");
}

#[test]
fn top_n_on_an_unknown_volume_errors_honestly() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(dir.path(), "root", &[("/a", low_score_signals())], 1_000);
    let err = snapshot_top(dir.path(), 5, on_volume("nope"), 1_000).expect_err("unknown volume errors");
    assert!(err.contains("nope"), "the error names the volume: {err}");
}

#[test]
fn threshold_caps_rows_and_flags_truncation() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    // Write more scored folders than the cap, all above the threshold.
    let paths: Vec<String> = (0..THRESHOLD_ROW_CAP + 5)
        .map(|i| format!("/Users/test/f{i:03}"))
        .collect();
    let rows: Vec<(&str, FolderSignals)> = paths
        .iter()
        .map(|p| (p.as_str(), scored_signals(PathClass::UserContent, now)))
        .collect();
    write_db(dir.path(), "root", &rows, now);

    let (folders, truncated) = snapshot_threshold(dir.path(), 0.0, RankedScope::default(), now).expect("threshold");
    assert_eq!(folders.len(), THRESHOLD_ROW_CAP, "capped at the row cap");
    assert!(truncated, "more rows existed than the cap");

    let note = "more folders match";
    let text = build_ranked_text("Folders scoring at or above 0.000:", &folders, Some(note));
    assert!(text.contains(note), "the truncation note is rendered");
}

#[test]
fn threshold_below_cap_is_not_truncated() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(
        dir.path(),
        "root",
        &[
            ("/high", scored_signals(PathClass::ProjectRoot, now)),
            ("/low", low_score_signals()),
        ],
        now,
    );
    let (folders, truncated) = snapshot_threshold(dir.path(), 0.5, RankedScope::default(), now).expect("threshold");
    assert!(!truncated, "fewer than the cap ⇒ no truncation");
    assert!(
        folders.iter().all(|f| f.score >= 0.5),
        "only folders at or above the threshold"
    );
}

fn on_volume(volume: &str) -> RankedScope<'_> {
    RankedScope {
        volume: Some(volume),
        ..RankedScope::default()
    }
}

fn ranked(rows: &[(&str, f64)]) -> Vec<RankedFolder> {
    rows.iter()
        .map(|(path, score)| RankedFolder {
            volume_id: "root".to_string(),
            path: path.to_string(),
            score: *score,
            why: String::new(),
            folded: 0,
            signals: FolderSignals::neutral(),
        })
        .collect()
}

const HOME: &str = "/Users/test";

fn paths_and_folds(folders: &[RankedFolder]) -> Vec<(&str, usize)> {
    folders.iter().map(|f| (f.path.as_str(), f.folded)).collect()
}

// ── Folding near-duplicates ───────────────────────────────────────────────────

/// The live root volume's shape (2026-10-26): a folder of projects scoring within a
/// hair of its best project, the projects' own members below them. They fold into
/// one row instead of filling the list; a descendant clearly above its parent and an
/// unrelated folder keep their own.
#[test]
fn near_duplicate_descendants_fold_into_the_folder_above() {
    let folders = collapse_descendants(
        ranked(&[
            ("/p/star", 0.995),
            ("/p/cmdr", 0.95),
            ("/p", 0.94),
            ("/p/smb2", 0.939),
            ("/p/cmdr/apps/desktop", 0.847),
            ("/docs", 0.6),
            ("/pp", 0.5),
        ]),
        None,
        HOME,
    );
    assert_eq!(
        paths_and_folds(&folders),
        vec![("/p/star", 0), ("/p", 3), ("/docs", 0), ("/pp", 0)],
        "cmdr, smb2, and cmdr's member fold into /p; /pp only shares a prefix"
    );
}

/// The folder the caller asked `under` is the one thing that doesn't absorb: its
/// children are what the caller wants listed. Their own members still fold.
#[test]
fn the_under_folder_lists_its_children() {
    let folders = collapse_descendants(
        ranked(&[
            ("/p/cmdr", 0.95),
            ("/p", 0.94),
            ("/p/smb2", 0.9),
            ("/p/cmdr/apps/desktop", 0.85),
        ]),
        Some("/p/"),
        HOME,
    );
    assert_eq!(
        paths_and_folds(&folders),
        vec![("/p/cmdr", 1), ("/p", 0), ("/p/smb2", 0)]
    );
}

/// Folding never crosses volumes: the same path on two volumes is two folders.
#[test]
fn folding_stays_on_one_volume() {
    let mut rows = ranked(&[("/share", 0.9), ("/share/photos", 0.8)]);
    rows[1].volume_id = "smb-share".to_string();
    assert_eq!(collapse_descendants(rows, None, HOME).len(), 2);
}

/// When folding eats the whole first window, the read widens until it fills: the live
/// root volume had 153 of its top 160 rows under one folder of projects.
#[test]
fn a_window_folded_away_widens_until_it_fills() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    let members: Vec<String> = (0..40).map(|i| format!("/Users/test/p/repo{i:02}")).collect();
    let mut rows: Vec<(&str, FolderSignals)> = members
        .iter()
        .map(|p| (p.as_str(), scored_signals(PathClass::ProjectRoot, now)))
        .collect();
    rows.push(("/Users/test/p", scored_signals(PathClass::ProjectRoot, now)));
    rows.push(("/Users/test/Documents", low_score_signals()));
    write_db(dir.path(), "root", &rows, now);

    let folders = snapshot_top(dir.path(), 2, RankedScope::default(), now).expect("top");
    assert_eq!(
        paths_and_folds(&folders),
        vec![("/Users/test/p", 40), ("/Users/test/Documents", 0)]
    );
    assert!(!folders[0].why.is_empty(), "a returned row says why");
}

/// `$HOME`, its ancestors, and a volume root never absorb: on the live root volume
/// (2026-10-26) `~` at 0.50 swallowed `Documents` at 0.51 and fifty more, leaving one
/// generic row where the user files things.
#[test]
fn home_and_its_ancestors_never_absorb() {
    let folders = collapse_descendants(
        ranked(&[
            ("/Users/test/Documents", 0.51),
            ("/Users/test", 0.50),
            ("/Users/test/Documents/tax", 0.45),
            ("/Users", 0.42),
            ("/", 0.3),
            ("/Users/test/Music", 0.3),
        ]),
        None,
        HOME,
    );
    assert_eq!(
        paths_and_folds(&folders),
        vec![
            ("/Users/test/Documents", 1),
            ("/Users/test", 0),
            ("/Users", 0),
            ("/", 0),
            ("/Users/test/Music", 0)
        ]
    );
}

#[test]
fn under_reads_only_that_subtree_through_the_snapshot() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(
        dir.path(),
        "root",
        &[
            ("/Users/test/Documents", scored_signals(PathClass::UserContent, now)),
            ("/Users/test/Documents/tax", scored_signals(PathClass::UserContent, now)),
            ("/Users/test/projects", scored_signals(PathClass::ProjectRoot, now)),
        ],
        now,
    );
    let scope = RankedScope {
        volume: Some("root"),
        under: Some("/Users/test/Documents"),
        among: None,
    };
    let folders = snapshot_top(dir.path(), 10, scope, now).expect("top");
    let mut paths: Vec<&str> = folders.iter().map(|f| f.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(paths, vec!["/Users/test/Documents", "/Users/test/Documents/tax"]);

    let text = build_importance_resource(dir.path(), Some("top=5&under=/Users/test/Documents"), now);
    assert!(!text.contains("/Users/test/projects"), "outside `under`: {text}");
    assert!(
        text.contains("under '/Users/test/Documents'"),
        "the header names the scope: {text}"
    );
}

/// `why` names the strongest signals in a few words each, strongest first, and
/// drops the ones that say nothing (visibility, a neutral path class).
#[test]
fn why_names_the_strongest_signals_tersely() {
    let now = 1_000_000;
    let mut signals = scored_signals(PathClass::ProjectRoot, now);
    signals.marker_depth = Some(0);
    signals.visit_count = Some(6);
    let explanation = explain(&signals, &SignalSet::all(), &Weights::default(), now);
    let why = why_text(&signals, &explanation, now);
    assert!(why.starts_with("projectRoot"), "the path class leads: {why}");
    assert!(why.contains("changed today"), "{why}");
    assert_eq!(why.split(", ").count(), 3, "three reasons at most: {why}");
    assert!(!why.contains("visib"), "{why}");

    let quiet = why_text(
        &low_score_signals(),
        &explain(&low_score_signals(), &SignalSet::all(), &Weights::default(), now),
        now,
    );
    assert!(!quiet.contains("neutral"), "a neutral class isn't a reason: {quiet}");
}

// ── no query (overview) ────────────────────────────────────────────────────────

#[test]
fn overview_lists_each_scored_volume() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(
        dir.path(),
        "root",
        &[
            ("/a", scored_signals(PathClass::UserContent, now)),
            ("/b", low_score_signals()),
        ],
        now,
    );

    let overview = snapshot_overview(dir.path());
    assert_eq!(overview.len(), 1, "one scored volume");
    assert_eq!(overview[0].volume_id, "root");
    assert_eq!(overview[0].folder_count, 2, "both stored folders counted");
    assert_eq!(overview[0].generation, 1, "generation 1 after one pass");

    let text = build_overview_text(&overview);
    assert!(text.contains("root"), "names the volume");
    assert!(text.contains("?path="), "teaches the query syntax");
    assert!(text.contains("?top="), "teaches the top syntax");
    assert!(text.contains("?threshold="), "teaches the threshold syntax");
}

#[test]
fn overview_with_no_data_is_honest_and_still_teaches_syntax() {
    let dir = tempfile::tempdir().expect("temp dir"); // no DBs
    let overview = snapshot_overview(dir.path());
    assert!(overview.is_empty(), "no scored volumes");
    let text = build_overview_text(&overview);
    assert!(
        text.contains("?path="),
        "still teaches the syntax on a blind first read"
    );
    assert!(
        text.to_lowercase().contains("no importance data"),
        "says there's no data yet: {text}"
    );
}

// ── dispatch + tilde ───────────────────────────────────────────────────────────

#[test]
fn dispatch_routes_each_mode() {
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(
        dir.path(),
        "root",
        &[("/Users/test/proj", scored_signals(PathClass::ProjectRoot, now))],
        now,
    );

    // No query ⇒ overview.
    assert!(build_importance_resource(dir.path(), None, now).contains("?path="));
    // ?path= ⇒ path block.
    let scored = build_importance_resource(dir.path(), Some("path=/Users/test/proj"), now);
    assert!(scored.contains("scored"), "path mode: {scored}");
    // ?top= ⇒ ranked.
    assert!(build_importance_resource(dir.path(), Some("top=5"), now).contains("/Users/test/proj"));
    // A malformed number is a helpful message, not a panic.
    assert!(build_importance_resource(dir.path(), Some("top=abc"), now).contains("top"));
}

#[test]
fn tilde_in_a_path_expands_to_home() {
    // Seeds the row from the REAL home rather than setting `HOME` to a fake one.
    // ❌ Don't reintroduce `env::set_var("HOME", ...)`: the environment is
    // process-wide, so under a plain `cargo test` (threads, one process) it
    // poisons every concurrently-running test that resolves a home path —
    // `favorites::store`'s defaults and `go_to_path` among them — and mutating
    // the environment while other threads read it is UB besides. Reading the
    // real home costs nothing and tests the same expansion.
    let home = dirs::home_dir().expect("a home directory");
    let downloads = home.join("Downloads");
    let now = 1_000_000;
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(
        dir.path(),
        "root",
        &[(
            downloads.to_str().expect("home path is UTF-8"),
            scored_signals(PathClass::UserContent, now),
        )],
        now,
    );

    let snapshot = snapshot_path(dir.path(), "~/Downloads", now);
    assert!(
        matches!(snapshot, PathImportance::Scored { .. }),
        "~/Downloads resolves to the scored {}, got {snapshot:?}",
        downloads.display()
    );
}

/// A folder that scores low but non-floored (neutral class, stale, monoculture-ish).
pub(crate) fn low_score_signals() -> FolderSignals {
    let mut s = FolderSignals::neutral();
    s.path_class = PathClass::Neutral;
    s.mtime_secs = Some(0); // ancient ⇒ recency ~0
    s.distinct_extension_count = 1;
    s.file_count = 1;
    s
}
