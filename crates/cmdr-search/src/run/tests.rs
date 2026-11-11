//! The registry, the stream's terminal facts, and the one report. The pump that
//! feeds a stream from a real walk is the host's, and so are its tests
//! (`apps/desktop/src-tauri/src/search/live/tests.rs`).

use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::*;
use crate::types::{PatternType, SearchQuery, SearchResultEntry};

// ── Fixtures ─────────────────────────────────────────────────────────

#[derive(Default)]
struct Heard {
    progress: Mutex<Vec<RunProgress>>,
    complete: Mutex<Vec<RunEnded>>,
    cancelled: Mutex<Vec<RunEnded>>,
    failed: Mutex<Vec<RunFailed>>,
}

impl RunSink for Heard {
    fn progress(&self, fact: RunProgress) {
        self.progress.lock_ignore_poison().push(fact);
    }
    fn complete(&self, fact: RunEnded) {
        self.complete.lock_ignore_poison().push(fact);
    }
    fn cancelled(&self, fact: RunEnded) {
        self.cancelled.lock_ignore_poison().push(fact);
    }
    fn failed(&self, fact: RunFailed) {
        self.failed.lock_ignore_poison().push(fact);
    }
}

fn query(stem: &str) -> SearchQuery {
    SearchQuery {
        name_pattern: Some(stem.to_string()),
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: Some(false),
        exclude_system_dirs: Some(false),
        sort_by: None,
    }
}

fn row(path: &str) -> SearchResultEntry {
    SearchResultEntry {
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        path: path.to_string(),
        parent_path: "/".to_string(),
        is_directory: false,
        size: Some(10),
        modified_at: Some(1),
        icon_id: "file".to_string(),
        entry_id: 7,
    }
}

fn unresolved(path: &str, walked: bool) -> UnresolvedScope {
    UnresolvedScope {
        path: path.to_string(),
        walked,
    }
}

// ── The report ───────────────────────────────────────────────────────

#[test]
fn a_fully_covered_run_is_the_same_report_with_nothing_walked() {
    let report =
        CoverageFacts::new(WalkEnding::NothingToWalk, CoverageKind::Covered, "root".to_string()).report(false, 3);
    assert_eq!(
        report,
        SearchRunCoverage {
            walk: WalkEnding::NothingToWalk,
            kind: CoverageKind::Covered,
            permission_denied: Vec::new(),
            off_limits: Vec::new(),
            declined: Vec::new(),
            still_covering: Vec::new(),
            unresolved_scopes: Vec::new(),
            uncovered_scopes: Vec::new(),
            abandoned_ground: false,
            abandoned_locations: 0,
            failed_roots: Vec::new(),
            capped: false,
            target_volume_id: "root".to_string(),
            hidden_by_excludes: 3,
        }
    );
}

#[test]
fn only_a_completed_walk_clears_an_unresolved_scope_it_took() {
    // The walk is the probe: a folder too new to be indexed is answered once a walk
    // has been all the way through it, and not a moment before.
    let facts = |walk| CoverageFacts {
        unresolved_scopes: vec![unresolved("/new", true), unresolved("/typo", false)],
        ..CoverageFacts::new(walk, CoverageKind::Mixed, "v".to_string())
    };
    assert_eq!(
        facts(WalkEnding::Completed).report(false, 0).unresolved_scopes,
        ["/typo"]
    );
    for short in [
        WalkEnding::Interrupted,
        WalkEnding::Cancelled,
        WalkEnding::NothingToWalk,
    ] {
        assert_eq!(
            facts(short).report(false, 0).unresolved_scopes,
            ["/new", "/typo"],
            "{short:?} proves nothing about the scope"
        );
    }
}

#[test]
fn ground_given_up_on_counts_whether_this_walk_or_an_earlier_one_gave_up() {
    let durable = CoverageFacts {
        unreadable: UnreadableGround {
            abandoned: vec!["/mnt/a/x".to_string(), "/mnt/a/y".to_string(), "/mnt/b".to_string()],
            ..UnreadableGround::default()
        },
        ..CoverageFacts::new(WalkEnding::Completed, CoverageKind::Live, "v".to_string())
    }
    .report(false, 0);
    assert!(
        durable.abandoned_ground,
        "an earlier walk's give-up is why this one never went there"
    );
    assert_eq!(
        durable.abandoned_locations, 2,
        "places, grouped by parent, never raw folders"
    );

    let this_walk = CoverageFacts {
        walk_abandoned_ground: true,
        ..CoverageFacts::new(WalkEnding::Completed, CoverageKind::Live, "v".to_string())
    }
    .report(false, 0);
    assert!(this_walk.abandoned_ground);
    assert_eq!(
        this_walk.abandoned_locations, 0,
        "a give-up with no recorded path has no place to count"
    );
}

#[test]
fn the_unreadable_lists_reach_the_report_apart() {
    let report = CoverageFacts {
        unreadable: UnreadableGround {
            permission_denied: vec!["/denied".to_string()],
            off_limits: vec!["/root-only".to_string()],
            declined: vec!["/snapshots".to_string()],
            abandoned: Vec::new(),
        },
        still_covering: vec!["/other-walk".to_string()],
        failed_roots: vec!["/gone".to_string()],
        uncovered_scopes: vec!["/server".to_string()],
        ..CoverageFacts::new(WalkEnding::Completed, CoverageKind::Mixed, "v".to_string())
    }
    .report(true, 0);
    assert_eq!(report.permission_denied, ["/denied"]);
    assert_eq!(report.off_limits, ["/root-only"]);
    assert_eq!(report.declined, ["/snapshots"]);
    assert_eq!(report.still_covering, ["/other-walk"]);
    assert_eq!(report.failed_roots, ["/gone"]);
    assert_eq!(report.uncovered_scopes, ["/server"]);
    assert!(report.capped);
}

// ── The stream's last word ───────────────────────────────────────────

#[test]
fn a_cancelled_run_ends_as_cancelled_whatever_its_facts_say() {
    let run = LiveRun::detached("stopped", "v");
    run.cancel_token().cancel();
    let heard = Heard::default();
    ResultStream::new(&run, &heard, &query("report")).finish(CoverageFacts::new(
        WalkEnding::Completed,
        CoverageKind::Live,
        "v".to_string(),
    ));
    assert!(heard.complete.lock_ignore_poison().is_empty());
    let cancelled = heard.cancelled.lock_ignore_poison();
    assert_eq!(cancelled.len(), 1);
    assert_eq!(cancelled[0].coverage.walk, WalkEnding::Cancelled);
}

#[test]
fn the_report_carries_the_streams_cap_and_both_halves_exclusions() {
    let run = LiveRun::detached("capped", "v");
    let heard = Heard::default();
    let mut q = query("report");
    q.limit = 1;
    let mut stream = ResultStream::new(&run, &heard, &q);
    stream.add_indexed(vec![row("/a/report.pdf"), row("/b/report.pdf")], 2, 4);
    stream.note_excluded();
    stream.finish(CoverageFacts::new(
        WalkEnding::NothingToWalk,
        CoverageKind::Covered,
        "v".to_string(),
    ));
    let complete = heard.complete.lock_ignore_poison();
    assert_eq!(complete[0].match_count, 2);
    assert!(complete[0].coverage.capped);
    assert_eq!(complete[0].coverage.hidden_by_excludes, 5);
    // A run that never walked signs off reading the index, never walking.
    assert_eq!(
        heard.progress.lock_ignore_poison().last().map(|fact| fact.phase),
        Some(SearchPhase::ReadingIndex)
    );
}

#[test]
fn a_run_that_cannot_run_says_so_unless_nobody_is_listening() {
    // The refusal path is the one that tells someone WHY they got nothing ("add a
    // filename pattern" for a query too broad to walk with). It has to reach a live
    // run, and it has to stay quiet for a superseded one, whose id the frontend
    // dropped when it asked the next question.
    let q = query("report");
    let heard = Heard::default();
    let listening = LiveRun::detached("failing", "v");
    ResultStream::new(&listening, &heard, &q).fail(SearchRunError::Query, "Query too broad.".to_string());
    let failed = heard.failed.lock_ignore_poison();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].error, SearchRunError::Query);
    assert_eq!(failed[0].message, "Query too broad.");

    let gone = Heard::default();
    let superseded = LiveRun::detached("failing-superseded", "v");
    superseded.supersede();
    ResultStream::new(&superseded, &gone, &q).fail(SearchRunError::Query, "Query too broad.".to_string());
    assert!(gone.failed.lock_ignore_poison().is_empty());
}

// ── The registry ─────────────────────────────────────────────────────

#[test]
fn cancelling_everything_stops_every_run_in_flight() {
    // What a quitting app calls. It reaches an agent's run too: nobody's search
    // outlives the process.
    let runs = SearchRuns::new();
    let one = runs.register("run-all-1", "volume-a", RunOrigin::Dialog { order: 5 });
    let two = runs.register("run-all-2", "volume-b", RunOrigin::Agent);
    runs.cancel_all();
    assert!(one.is_cancelled() && two.is_cancelled());
}

#[test]
fn cancelling_a_run_nobody_registered_says_so_rather_than_pretending() {
    let runs = SearchRuns::new();
    assert!(!runs.cancel("a-run-that-never-was"));
    let run = runs.register("run-cancel-one", "volume-c", RunOrigin::Dialog { order: 6 });
    assert!(runs.cancel("run-cancel-one"));
    assert!(run.is_cancelled());
    runs.deregister("run-cancel-one");
    assert!(!runs.cancel("run-cancel-one"), "a deregistered run is gone");
}

#[test]
fn an_agent_run_and_the_dialog_stay_out_of_each_others_way() {
    // Superseding exists because ONE dialog asks one question at a time. An agent's
    // call is its own asker with its own caller waiting, so neither run may silence
    // the other: a person typing mid-agent-search would otherwise watch their
    // results stop arriving, and an agent's answer would come back empty because
    // somebody touched the keyboard.
    let runs = SearchRuns::new();
    let dialog = runs.register("run-dialog", "volume-d", RunOrigin::Dialog { order: 7 });
    let agent = runs.register("run-agent", "volume-d", RunOrigin::Agent);
    assert!(dialog.wants_events(), "an agent's search doesn't supersede the dialog");
    assert!(agent.wants_events());

    // And the dialog asking its next question supersedes only its own.
    let refined = runs.register("run-dialog-2", "volume-d", RunOrigin::Dialog { order: 8 });
    assert!(!dialog.wants_events(), "the dialog's own previous run is superseded");
    assert!(
        !dialog.is_cancelled(),
        "superseding is not cancelling: its walk carries on"
    );
    assert!(agent.wants_events(), "the agent's is not");
    assert!(refined.wants_events());
}

#[test]
fn a_dialog_run_that_registers_after_a_newer_one_is_the_one_silenced() {
    // Two starts race from the frontend to here, and nothing orders their arrival:
    // each builds its query and installs its listeners first, and commands run
    // concurrently. If arrival decided, an earlier question reaching the registry
    // last would silence the later one the dialog is watching, and the dialog would
    // wait on a run that never reports.
    let runs = SearchRuns::new();
    let later = runs.register("run-order-later", "volume-f", RunOrigin::Dialog { order: 20 });
    let earlier = runs.register("run-order-earlier", "volume-f", RunOrigin::Dialog { order: 10 });
    assert!(later.wants_events(), "the question asked later keeps reporting");
    assert!(
        !earlier.wants_events(),
        "the earlier question, arriving late, starts out superseded"
    );
}

#[test]
fn closing_the_dialog_leaves_an_agents_search_and_the_kept_run_going() {
    // `release_search_index` means "nobody is watching the dialog any more", which
    // says nothing about an agent's call still waiting on its answer, nor about the
    // run "Open in pane" handed to a pane.
    let runs = SearchRuns::new();
    let dialog = runs.register("run-close-dialog", "volume-e", RunOrigin::Dialog { order: 9 });
    let kept = runs.register("run-close-kept", "volume-e", RunOrigin::Dialog { order: 10 });
    let agent = runs.register("run-close-agent", "volume-e", RunOrigin::Agent);
    runs.cancel_dialog_runs_except(Some("run-close-kept"));
    assert!(dialog.is_cancelled());
    assert!(!kept.is_cancelled(), "the handed-off run keeps walking");
    assert!(
        !agent.is_cancelled(),
        "the agent's run is none of the dialog's business"
    );
}
