//! Suggested-ops tool tests: the input contract, the write path against a real `main.db`,
//! and the two read shapers.
//!
//! Everything here runs without a Tauri app: the handlers are thin shells over
//! [`apply_planned_sweep`], [`shape_list`], and [`shape_group`], which take a connection (or
//! plain data) so the logic is exercised directly.

use rusqlite::Connection;
use serde_json::{Value, json};

use super::group::shape_group;
use super::input::{
    GroupProblem, PlanRefusal, PlannedOps, PlannedSources, SourceShape, plan_sweep,
};
use super::list::{shape_list, to_group_summary};
use super::propose::{ApplyRefusal, GroupOutcome, apply_planned_sweep};
use crate::agent::store::proposals::{
    GroupIntent, GroupSummary, NewGroup, NewOp, NewSweep, ProposalOp, ProposalSweep, count_ops, create_group,
    create_sweep, get_group, page_ops,
};
use crate::agent::store::{MIGRATIONS, run_migrations};
use crate::agent::suggested_ops::{IndexedFile, OpSelector, SelectorIndex, SelectorRefusal};
use crate::agent::types::{OpStatus, ProposalStatus, ProposalVerb, Reversibility};

/// A fixed "now", so an age predicate lands on a number a test can name.
const NOW: i64 = 1_800_000_000;

// ── Fixtures ──────────────────────────────────────────────────────────────────

fn migrated_conn() -> Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    run_migrations(&conn, MIGRATIONS).expect("migrate");
    conn
}

/// An index that answers with a fixed set of files and counts how often it was asked, so a
/// test can pin that a selector resolves exactly once.
struct FakeIndex {
    files: Vec<IndexedFile>,
    refusal: Option<SelectorRefusal>,
    calls: std::cell::Cell<usize>,
}

impl FakeIndex {
    fn with(files: Vec<IndexedFile>) -> Self {
        FakeIndex {
            files,
            refusal: None,
            calls: std::cell::Cell::new(0),
        }
    }

    fn refusing(refusal: SelectorRefusal) -> Self {
        FakeIndex {
            files: Vec::new(),
            refusal: Some(refusal),
            calls: std::cell::Cell::new(0),
        }
    }
}

impl SelectorIndex for FakeIndex {
    fn resolve(&self, _selector: &OpSelector) -> Result<Vec<IndexedFile>, SelectorRefusal> {
        self.calls.set(self.calls.get() + 1);
        match &self.refusal {
            Some(refusal) => Err(refusal.clone()),
            None => Ok(self.files.clone()),
        }
    }
}

fn indexed(path: &str) -> IndexedFile {
    IndexedFile {
        path: path.to_string(),
        size: Some(4_096),
        modified_at: Some(NOW - 90 * 86_400),
        inode: Some(12),
    }
}

/// A trash group over two named paths: the shortest valid call.
fn trash_call() -> Value {
    json!({
        "groups": [{
            "verb": "trash",
            "sourceVolumeId": "root",
            "displayName": "Two old installers",
            "paths": ["/Users/someone/Downloads/a.dmg", "/Users/someone/Downloads/b.dmg"],
        }]
    })
}

fn selector_call() -> Value {
    json!({
        "groups": [{
            "verb": "trash",
            "selector": { "root": { "volumeId": "root", "path": "~/Downloads" }, "nameGlob": "*.dmg", "olderThanDays": 30 },
            "rationale": "They're months old.",
        }]
    })
}

fn apply(conn: &Connection, index: &dyn SelectorIndex, call: &Value) -> Result<super::propose::ProposeReport, ApplyRefusal> {
    let planned = plan_sweep(call, NOW).expect("the call is valid");
    apply_planned_sweep(conn, index, planned, None, NOW)
}

// ── The input contract ────────────────────────────────────────────────────────

#[test]
fn a_group_needs_exactly_one_source_shape() {
    // Neither: there's nothing for the user to look at.
    let refusal = plan_sweep(
        &json!({ "groups": [{ "verb": "trash", "sourceVolumeId": "root", "displayName": "x" }] }),
        NOW,
    )
    .expect_err("no sources");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::NoSources
        }
    );

    // Both: which one the user would be reviewing is a coin toss, so it's refused rather
    // than resolved by precedence.
    let refusal = plan_sweep(
        &json!({ "groups": [{
            "verb": "trash",
            "sourceVolumeId": "root",
            "displayName": "x",
            "paths": ["/a"],
            "selector": { "root": { "volumeId": "root", "path": "/" } },
        }] }),
        NOW,
    )
    .expect_err("two source shapes");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::AmbiguousSources
        }
    );
}

#[test]
fn a_verb_only_takes_the_target_its_executor_binds() {
    // Trash binds nothing at all: `trash_files_start` takes raw paths and no target.
    let refusal = plan_sweep(
        &json!({ "groups": [{
            "verb": "trash",
            "sourceVolumeId": "root",
            "displayName": "x",
            "paths": ["/a"],
            "destination": { "volumeId": "root", "path": "/tmp" },
        }] }),
        NOW,
    )
    .expect_err("trash binds no destination");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::UnboundField { field: "destination" }
        }
    );

    // Move binds one shared destination directory, and can't do without it.
    let refusal = plan_sweep(
        &json!({ "groups": [{ "verb": "move", "sourceVolumeId": "root", "displayName": "x", "paths": ["/a"] }] }),
        NOW,
    )
    .expect_err("move needs a destination");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::MissingField { field: "destination" }
        }
    );
}

#[test]
fn rename_carries_its_own_names_and_refuses_a_pattern() {
    // A selector matches files; it can't decide what they should be called.
    let refusal = plan_sweep(
        &json!({ "groups": [{
            "verb": "rename",
            "parent": "/Users/someone/Pictures",
            "renames": [{ "path": "/Users/someone/Pictures/a.jpg", "newName": "b.jpg" }],
            "selector": { "root": { "volumeId": "root", "path": "/" } },
        }] }),
        NOW,
    )
    .expect_err("a rename can't come from a pattern");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::AmbiguousSources
        }
    );

    // A rename destination is a NAME: `start_bulk_rename` refuses a row that would change
    // the parent, so a path here would be staged only to be rejected at apply.
    let refusal = plan_sweep(
        &json!({ "groups": [{
            "verb": "rename",
            "parent": "/Users/someone/Pictures",
            "sourceVolumeId": "root",
            "displayName": "x",
            "renames": [{ "path": "/Users/someone/Pictures/a.jpg", "newName": "/elsewhere/b.jpg" }],
        }] }),
        NOW,
    )
    .expect_err("a new name can't be a path");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::NotABareName {
                name: "/elsewhere/b.jpg".to_string()
            }
        }
    );

    // And renames belong to the rename verb alone.
    let refusal = plan_sweep(
        &json!({ "groups": [{
            "verb": "move",
            "sourceVolumeId": "root",
            "displayName": "x",
            "destination": { "volumeId": "root", "path": "/tmp" },
            "renames": [{ "path": "/a.jpg", "newName": "b.jpg" }],
        }] }),
        NOW,
    )
    .expect_err("only rename takes renames");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::RenamesVerbMismatch
        }
    );
}

#[test]
fn a_selector_supplies_the_volume_and_the_title_itself() {
    // Sending either alongside it would be a second, drifting answer to a settled question:
    // the pattern names the group and its root names the volume.
    for field in ["sourceVolumeId", "displayName"] {
        let mut group = json!({
            "verb": "trash",
            "selector": { "root": { "volumeId": "root", "path": "/Users/someone/Downloads" } },
        });
        group[field] = json!("something");
        let refusal = plan_sweep(&json!({ "groups": [group] }), NOW).expect_err("the selector owns it");
        assert_eq!(
            refusal,
            PlanRefusal::Group {
                group: 0,
                problem: GroupProblem::SelectorSuppliesField { field }
            }
        );
    }

    // And a hand-listed group has to supply both, since nothing else can.
    let refusal = plan_sweep(
        &json!({ "groups": [{ "verb": "trash", "paths": ["/a"], "displayName": "x" }] }),
        NOW,
    )
    .expect_err("a listed group needs a volume");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::MissingField {
                field: "sourceVolumeId"
            }
        }
    );
}

#[test]
fn an_age_predicate_arrives_in_days_and_leaves_in_seconds() {
    // The model states whole days ago; the index compares unix seconds. Converting here is
    // what keeps the model out of epoch arithmetic it does unreliably.
    let planned = plan_sweep(&selector_call(), NOW).expect("valid");
    let PlannedOps::Sources {
        sources: PlannedSources::Selector(selector),
        shape,
    } = &planned.groups[0].ops
    else {
        panic!("a selector group");
    };
    assert_eq!(*shape, SourceShape::Trash);
    assert_eq!(selector.modified_before, Some(NOW - 30 * 86_400));
    assert_eq!(selector.modified_after, None);
    assert_eq!(selector.name_glob.as_deref(), Some("*.dmg"));
    // The tilde is expanded here, so the stored pattern is what the index actually looked
    // through.
    assert!(selector.root.path.ends_with("/Downloads"));
    assert!(!selector.root.path.starts_with('~'));
}

#[test]
fn a_window_nothing_can_satisfy_is_refused_rather_than_proposed() {
    // "Older than 30 days AND newer than 7" is empty: nothing is both. (The other way round
    // reads as "between 7 and 30 days old", which is a good window — asserted below.)
    // Staging the empty one would cost the user a review with nothing in it.
    let refusal = plan_sweep(
        &json!({ "groups": [{
            "verb": "trash",
            "selector": { "root": { "volumeId": "root", "path": "/x" }, "olderThanDays": 30, "newerThanDays": 7 },
        }] }),
        NOW,
    )
    .expect_err("empty age window");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::ImpossibleWindow
        }
    );

    // Same for a size window with its bounds the wrong way round.
    let refusal = plan_sweep(
        &json!({ "groups": [{
            "verb": "trash",
            "selector": { "root": { "volumeId": "root", "path": "/x" }, "minSizeBytes": 1000, "maxSizeBytes": 10 },
        }] }),
        NOW,
    )
    .expect_err("empty size window");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::ImpossibleWindow
        }
    );

    // The band between two ages is legitimate, and both bounds survive into the selector.
    let planned = plan_sweep(
        &json!({ "groups": [{
            "verb": "trash",
            "selector": { "root": { "volumeId": "root", "path": "/x" }, "olderThanDays": 7, "newerThanDays": 30 },
        }] }),
        NOW,
    )
    .expect("7 to 30 days old is a real window");
    let PlannedOps::Sources {
        sources: PlannedSources::Selector(selector),
        ..
    } = &planned.groups[0].ops
    else {
        panic!("a selector group");
    };
    assert_eq!(selector.modified_before, Some(NOW - 7 * 86_400));
    assert_eq!(selector.modified_after, Some(NOW - 30 * 86_400));
}

#[test]
fn a_relative_path_is_refused_because_nothing_here_has_a_working_directory() {
    let refusal = plan_sweep(
        &json!({ "groups": [{ "verb": "trash", "sourceVolumeId": "root", "displayName": "x", "paths": ["Downloads/a.dmg"] }] }),
        NOW,
    )
    .expect_err("relative path");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::RelativePath {
                path: "Downloads/a.dmg".to_string()
            }
        }
    );

    // An archive's inside is a `scheme://` path and stays legal.
    let planned = plan_sweep(
        &json!({ "groups": [{
            "verb": "extract",
            "sourceVolumeId": "archive:1",
            "displayName": "x",
            "destination": { "volumeId": "root", "path": "/tmp" },
            "paths": ["zip:///Users/someone/a.zip!/inner.txt"],
        }] }),
        NOW,
    )
    .expect("a virtual path is a real path here");
    assert_eq!(planned.groups.len(), 1);
}

#[test]
fn an_explicit_list_is_capped_and_the_refusal_names_the_way_out() {
    let paths: Vec<String> = (0..201).map(|i| format!("/Users/someone/Downloads/f-{i}.dmg")).collect();
    let refusal = plan_sweep(
        &json!({ "groups": [{ "verb": "trash", "sourceVolumeId": "root", "displayName": "x", "paths": paths }] }),
        NOW,
    )
    .expect_err("over the cap");
    assert_eq!(
        refusal,
        PlanRefusal::Group {
            group: 0,
            problem: GroupProblem::TooManyPaths { sent: 201 }
        }
    );
}

#[test]
fn amending_a_group_needs_the_sweep_it_belongs_to() {
    // Without the sweep id there's no way to check the group is one of this sweep's, and an
    // agent that could rewrite any group by number could rewrite one from another thread.
    let refusal = plan_sweep(
        &json!({ "groups": [{ "groupId": 7, "verb": "trash", "sourceVolumeId": "root", "displayName": "x", "paths": ["/a"] }] }),
        NOW,
    )
    .expect_err("a group id needs a sweep id");
    assert_eq!(refusal, PlanRefusal::GroupIdWithoutSweep { group: 0 });
}

#[test]
fn an_unknown_field_is_refused_rather_than_ignored() {
    // A silently dropped field is how a proposal ends up meaning something other than what
    // the model wrote.
    let refusal = plan_sweep(
        &json!({ "groups": [{ "verb": "trash", "sourceVolumeId": "root", "displayName": "x", "paths": ["/a"], "lastOpenedDays": 30 }] }),
        NOW,
    )
    .expect_err("unknown field");
    assert_eq!(refusal, PlanRefusal::Malformed);
}

// ── The write path ────────────────────────────────────────────────────────────

#[test]
fn a_named_list_becomes_a_sweep_with_one_reviewable_group() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let report = apply(&conn, &index, &trash_call()).expect("staged");

    assert!(report.ready_for_review);
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].outcome, GroupOutcome::Created);
    assert_eq!(report.groups[0].op_count, 2);
    assert_eq!(report.groups[0].verb, ProposalVerb::Trash);

    let group = get_group(&conn, report.groups[0].group_id)
        .expect("read")
        .expect("the group exists");
    assert_eq!(group.status, ProposalStatus::Pending);
    assert_eq!(group.reversible, Reversibility::RestoreMove);
    assert_eq!(group.selector, None, "a hand-listed group carries no pattern");
    assert_eq!(count_ops(&conn, group.id, Some(OpStatus::Pending)).expect("count"), 2);
    // The index was never asked: there was no pattern to resolve.
    assert_eq!(index.calls.get(), 0);
}

#[test]
fn a_selector_resolves_once_at_creation_and_freezes_what_it_found() {
    let conn = migrated_conn();
    let index = FakeIndex::with(vec![
        indexed("/Users/someone/Downloads/a.dmg"),
        indexed("/Users/someone/Downloads/b.dmg"),
    ]);
    let report = apply(&conn, &index, &selector_call()).expect("staged");

    assert_eq!(index.calls.get(), 1, "a selector is resolved exactly once");
    let group_id = report.groups[0].group_id;
    let group = get_group(&conn, group_id).expect("read").expect("exists");
    // The pattern survives as the group's display text and as stored JSON, for the dialog.
    assert!(group.display_name.ends_with("/Downloads/*.dmg"), "{}", group.display_name);
    assert!(group.selector.is_some(), "the pattern rides along for display");
    assert_eq!(group.source_volume_id, "root", "the selector's root names the volume");

    // The resolved rows carry the index's snapshot, which is what M2 checks against at
    // apply time.
    let ops = page_ops(&conn, group_id, 10, 0).expect("ops");
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].snapshot_size, Some(4_096));
    assert_eq!(ops[0].snapshot_inode, Some(12));
}

#[test]
fn a_selector_that_matches_nothing_stages_nothing_and_says_so() {
    // An empty group is a review the user can't act on, and "nothing matched" is a fact the
    // agent should relay rather than a group it should stage.
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let refusal = apply(&conn, &index, &selector_call()).expect_err("nothing matched");
    assert!(matches!(refusal, ApplyRefusal::SelectorMatchedNothing { group: 0, .. }));
    assert_eq!(sweep_count(&conn), 0, "nothing was written");
}

#[test]
fn an_unreadable_index_refuses_differently_from_an_empty_one() {
    // "I can't see that drive" and "nothing matched" read the same as an empty list and
    // mean opposite things, so they stay two typed refusals.
    let conn = migrated_conn();
    let index = FakeIndex::refusing(SelectorRefusal::NotIndexed {
        volume_id: "nas".to_string(),
    });
    let refusal = apply(&conn, &index, &selector_call()).expect_err("no index");
    assert!(matches!(
        refusal,
        ApplyRefusal::Selector {
            group: 0,
            refusal: SelectorRefusal::NotIndexed { .. }
        }
    ));
    assert_eq!(sweep_count(&conn), 0);
}

#[test]
fn one_bad_group_stages_none_of_them() {
    // A half-applied sweep leaves the user reading a mix of what the agent meant and what
    // it managed, so the check runs over every group before the first write.
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let call = json!({
        "groups": [
            { "verb": "trash", "sourceVolumeId": "root", "displayName": "fine", "paths": ["/Users/someone/a.dmg"] },
            { "verb": "trash", "selector": { "root": { "volumeId": "root", "path": "/Users/someone/Downloads" } } },
        ]
    });
    let refusal = apply(&conn, &index, &call).expect_err("the second group matched nothing");
    assert!(matches!(refusal, ApplyRefusal::SelectorMatchedNothing { group: 1, .. }));
    assert_eq!(sweep_count(&conn), 0, "the good group didn't land either");
}

#[test]
fn re_proposing_replaces_a_pending_groups_ops_without_minting_a_new_group() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let first = apply(&conn, &index, &trash_call()).expect("staged");
    let group_id = first.groups[0].group_id;

    let amendment = json!({
        "sweepId": first.sweep_id,
        "groups": [{
            "groupId": group_id,
            "verb": "trash",
            "sourceVolumeId": "root",
            "displayName": "One old installer",
            "paths": ["/Users/someone/Downloads/a.dmg"],
        }]
    });
    let second = apply(&conn, &index, &amendment).expect("amended");
    assert_eq!(second.sweep_id, first.sweep_id, "the amendment stays in its sweep");
    assert_eq!(second.groups[0].group_id, group_id, "same group, new contents");
    assert_eq!(second.groups[0].outcome, GroupOutcome::Amended);
    assert_eq!(count_ops(&conn, group_id, None).expect("count"), 1);
    assert_eq!(sweep_count(&conn), 1);
}

#[test]
fn an_answered_group_is_the_users_and_cant_be_rewritten() {
    // `pending` is the only mutable status. An agent that could still rewrite an approved
    // group could rewrite what the user already said yes to.
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let first = apply(&conn, &index, &trash_call()).expect("staged");
    let group_id = first.groups[0].group_id;
    conn.execute(
        "UPDATE proposals SET status = ?2 WHERE id = ?1",
        rusqlite::params![group_id, ProposalStatus::Approved.as_token()],
    )
    .expect("approve it behind the tool's back");

    let amendment = json!({
        "sweepId": first.sweep_id,
        "groups": [{
            "groupId": group_id,
            "verb": "trash",
            "sourceVolumeId": "root",
            "displayName": "Sneaky rewrite",
            "paths": ["/Users/someone/Downloads/c.dmg"],
        }]
    });
    let refusal = apply(&conn, &index, &amendment).expect_err("approved is frozen");
    assert!(matches!(
        refusal,
        ApplyRefusal::GroupNotPending {
            group: 0,
            status: ProposalStatus::Approved,
            ..
        }
    ));
    // The ops the user approved are untouched.
    let ops = page_ops(&conn, group_id, 10, 0).expect("ops");
    assert_eq!(ops.len(), 2);
}

#[test]
fn a_group_from_another_sweep_cant_be_amended_through_this_one() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let first = apply(&conn, &index, &trash_call()).expect("staged");
    let other = apply(&conn, &index, &trash_call()).expect("a second sweep");

    let amendment = json!({
        "sweepId": first.sweep_id,
        "groups": [{
            "groupId": other.groups[0].group_id,
            "verb": "trash",
            "sourceVolumeId": "root",
            "displayName": "x",
            "paths": ["/Users/someone/Downloads/a.dmg"],
        }]
    });
    let refusal = apply(&conn, &index, &amendment).expect_err("wrong sweep");
    assert!(matches!(refusal, ApplyRefusal::GroupNotInSweep { group: 0, .. }));
}

#[test]
fn a_new_group_can_join_an_existing_sweep() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let first = apply(&conn, &index, &trash_call()).expect("staged");

    let addition = json!({
        "sweepId": first.sweep_id,
        "groups": [{
            "verb": "move",
            "sourceVolumeId": "root",
            "displayName": "Screenshots to Pictures",
            "destination": { "volumeId": "root", "path": "/Users/someone/Pictures" },
            "paths": ["/Users/someone/Downloads/shot.png"],
        }]
    });
    let second = apply(&conn, &index, &addition).expect("added");
    assert_eq!(second.sweep_id, first.sweep_id);
    assert_eq!(second.groups[0].outcome, GroupOutcome::Created);
    assert_eq!(sweep_count(&conn), 1, "no second sweep was opened");
}

#[test]
fn an_unknown_sweep_is_refused_before_anything_is_written() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let mut call = trash_call();
    call["sweepId"] = json!(4_242);
    let refusal = apply(&conn, &index, &call).expect_err("no such sweep");
    assert!(matches!(refusal, ApplyRefusal::UnknownSweep { sweep_id: 4_242 }));
    assert_eq!(sweep_count(&conn), 0);
}

fn sweep_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM proposal_sets", [], |row| row.get(0))
        .expect("count sweeps")
}

// ── The read shapers ──────────────────────────────────────────────────────────

fn summary(group_id: i64, set_id: i64, live: u64, total: u64, selector: Option<&str>) -> GroupSummary {
    GroupSummary {
        group: crate::agent::store::proposals::ProposalGroup {
            id: group_id,
            set_id,
            seq: 0,
            verb: ProposalVerb::Trash,
            status: ProposalStatus::Pending,
            source_volume_id: "root".to_string(),
            destination: None,
            destination_volume_id: None,
            reversible: Reversibility::RestoreMove,
            display_name: format!("group {group_id}"),
            rationale: None,
            selector: selector.map(str::to_string),
            created_at: NOW,
            decided_at: None,
        },
        live_op_count: live,
        total_op_count: total,
    }
}

fn sweep(id: i64) -> ProposalSweep {
    ProposalSweep {
        id,
        conversation_id: None,
        created_at: NOW,
        created_by_model: None,
        rationale: Some(format!("sweep {id}")),
    }
}

#[test]
fn the_list_nests_groups_under_their_sweeps_and_counts_without_op_rows() {
    let groups = vec![
        to_group_summary(summary(1, 10, 3, 4, None)),
        to_group_summary(summary(2, 10, 5, 5, Some("{}"))),
        to_group_summary(summary(3, 11, 1, 1, None)),
    ];
    let result = shape_list("pending", groups, 3, false, &[sweep(10), sweep(11)]);

    assert_eq!(result.sweeps.len(), 2);
    assert_eq!(result.sweeps[0].sweep_id, 10);
    assert_eq!(result.sweeps[0].groups.len(), 2, "both groups of the first sweep");
    assert_eq!(result.sweeps[1].groups.len(), 1);
    assert_eq!(result.returned, 3);
    assert_eq!(result.total, 3);
    assert!(!result.truncated);

    // A deselected op keeps its row, so the two counts differ and both are reported.
    assert_eq!(result.sweeps[0].groups[0].op_count, 3);
    assert_eq!(result.sweeps[0].groups[0].excluded_op_count, 1);
    assert!(result.sweeps[0].groups[1].from_selector, "a pattern produced it");
    assert!(!result.sweeps[0].groups[0].from_selector);
}

#[test]
fn a_cut_list_reports_what_it_left_out() {
    // The model has to be able to say "3 of 40", so the denominator survives the cut.
    let result = shape_list(
        "pending",
        vec![to_group_summary(summary(1, 10, 1, 1, None))],
        40,
        true,
        &[sweep(10)],
    );
    assert_eq!(result.returned, 1);
    assert_eq!(result.total, 40);
    assert!(result.truncated);
    assert_eq!(result.status, "pending");
}

fn op(id: i64, size: Option<u64>) -> ProposalOp {
    ProposalOp {
        id,
        group_id: 1,
        seq: id,
        source_path: format!("/Users/someone/Downloads/f-{id}.dmg"),
        destination: None,
        status: OpStatus::Pending,
        snapshot_size: size,
        snapshot_mtime: Some(NOW - 86_400),
        snapshot_inode: None,
    }
}

#[test]
fn a_group_page_reports_its_place_in_the_whole_group() {
    // 60 000 ops is a legitimate group: a page has to say where it sits, or the model
    // reports a slice as the whole thing.
    let detail = shape_group(summary(1, 10, 60_000, 60_000, None), vec![op(1, Some(4_096)), op(2, None)], 100);
    assert!(detail.found);
    assert_eq!(detail.offset, 100);
    assert_eq!(detail.returned, 2);
    assert_eq!(detail.total, 60_000);
    assert!(detail.truncated, "102 of 60,000 is not the whole group");
    assert_eq!(detail.group.op_count, 60_000);
}

#[test]
fn a_whole_group_in_one_page_is_not_truncated() {
    let detail = shape_group(summary(1, 10, 2, 2, None), vec![op(1, Some(10)), op(2, Some(20))], 0);
    assert!(!detail.truncated);
    assert_eq!(detail.returned, 2);
    assert_eq!(detail.total, 2);
}

#[test]
fn every_op_number_arrives_spoken_and_an_unknown_size_stays_silent() {
    let detail = shape_group(summary(1, 10, 2, 2, None), vec![op(1, Some(4_096)), op(2, None)], 0);
    assert_eq!(detail.ops[0].snapshot_size, Some(4_096));
    assert_eq!(
        detail.ops[0].snapshot_size_human.as_deref(),
        Some(crate::search::format_size(4_096)).as_deref()
    );
    assert!(detail.ops[0].snapshot_modified_human.is_some());
    // No size means NO string: a "0 B" would read as an empty file rather than an unknown
    // one.
    assert_eq!(detail.ops[1].snapshot_size, None);
    assert_eq!(detail.ops[1].snapshot_size_human, None);
}

#[test]
fn the_op_shape_the_model_reads_names_its_numbers_as_snapshots() {
    // The fields are what the index knew at creation, not what the file is now, and the
    // wire names have to say so or the model relays a stale size as current.
    let detail = shape_group(summary(1, 10, 1, 1, None), vec![op(1, Some(4_096))], 0);
    let json = serde_json::to_value(&detail).expect("serializes");
    assert_eq!(json["ops"][0]["snapshotSize"], 4_096);
    assert_eq!(json["ops"][0]["opId"], 1);
    assert_eq!(json["group"]["verb"], "trash");
    assert_eq!(json["group"]["reversible"], "restoreMove");
    assert!(json["ops"][0].get("newName").is_none(), "only a rename has one");
}

// ── The store fixtures this module leans on ───────────────────────────────────

#[test]
fn the_stores_sweep_reader_answers_for_a_sweep_and_for_no_sweep() {
    let conn = migrated_conn();
    let set_id = create_sweep(
        &conn,
        &NewSweep {
            conversation_id: None,
            created_by_model: Some("test-model".to_string()),
            rationale: Some("because".to_string()),
        },
        NOW,
    )
    .expect("sweep");
    create_group(
        &conn,
        set_id,
        &NewGroup {
            intent: GroupIntent::Trash {
                sources: vec![NewOp {
                    source_path: "/Users/someone/a.dmg".to_string(),
                    snapshot: None,
                }],
            },
            source_volume_id: "root".to_string(),
            display_name: "x".to_string(),
            rationale: None,
            selector: None,
        },
        NOW,
    )
    .expect("group");

    let read = crate::agent::store::proposals::get_sweep(&conn, set_id)
        .expect("read")
        .expect("exists");
    assert_eq!(read.id, set_id);
    assert_eq!(read.rationale.as_deref(), Some("because"));
    assert_eq!(read.created_by_model.as_deref(), Some("test-model"));
    assert!(
        crate::agent::store::proposals::get_sweep(&conn, set_id + 999)
            .expect("read")
            .is_none()
    );
}
