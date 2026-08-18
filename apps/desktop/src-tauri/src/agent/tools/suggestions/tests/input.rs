//! The input contract: what `propose_suggestions` accepts, and every way a call is refused
//! before a single row is written.

use serde_json::json;

use super::{NOW, selector_call};
use crate::agent::tools::suggestions::input::{
    GroupProblem, PlanRefusal, PlannedOps, PlannedSources, SourceShape, plan_sweep,
};

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
    let paths: Vec<String> = (0..201)
        .map(|i| format!("/Users/someone/Downloads/f-{i}.dmg"))
        .collect();
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
