//! Names on a move or copy: what a group listed with `renames` stages, and every name the
//! proposal refuses before the user would see it.

use serde_json::{Value, json};

use super::{FakeIndex, NOW, apply, local_facts, migrated_conn, volume};
use crate::agent::store::proposals::{
    AcceptanceOutcome, OpSelection, get_group, live_binding, page_ops, record_acceptance,
};
use crate::agent::tools::suggestions::input::{GroupProblem, PlanRefusal, PlannedOps, plan_sweep};
use crate::agent::tools::suggestions::problem::NameRule;
use crate::location::resolve::VolumeFacts;
use crate::mcp::resources::volumes::VolumeKind;
use cmdr_fs::validation::PlainLeafRefusal;

const DOWNLOADS: &str = "/Users/someone/Downloads";
const INVOICES: &str = "/Users/someone/Documents/invoices paid";

/// A `verb` group moving or copying `renames` (each `(file in Downloads, new name)`) into
/// the invoices folder.
fn named_call(verb: &str, renames: &[(&str, &str)]) -> Value {
    named_call_into(verb, renames, json!({ "volumeId": "root", "path": INVOICES }))
}

fn named_call_into(verb: &str, renames: &[(&str, &str)], destination: Value) -> Value {
    let renames: Vec<Value> = renames
        .iter()
        .map(|(file, name)| json!({ "path": format!("{DOWNLOADS}/{file}"), "newName": name }))
        .collect();
    json!({ "groups": [{
        "verb": verb,
        "sourceVolumeId": "root",
        "displayName": "Invoices",
        "destination": destination,
        "renames": renames,
    }] })
}

fn problem_of(call: &Value, facts: &VolumeFacts) -> GroupProblem {
    match plan_sweep(call, NOW, facts).expect_err("refused") {
        PlanRefusal::Group { problem, .. } => problem,
        other => panic!("a group refusal, got {other:?}"),
    }
}

/// The names a planned move or copy carries, in order.
fn planned_names(call: &Value) -> Vec<Option<String>> {
    let planned = plan_sweep(call, NOW, &local_facts()).expect("valid");
    let PlannedOps::Named { ops, .. } = &planned.groups[0].ops else {
        panic!("a named move or copy");
    };
    ops.iter().map(|op| op.new_name.clone()).collect()
}

#[test]
fn a_move_or_copy_lists_every_file_and_a_kept_name_draws_no_arrow() {
    for verb in ["move", "copy"] {
        let call = named_call(
            verb,
            &[
                ("Invoice-X.pdf", "2026-09-27 OpenAI invoice Rymd.pdf"),
                ("receipt.pdf", "receipt.pdf"),
            ],
        );
        assert_eq!(
            planned_names(&call),
            vec![Some("2026-09-27 OpenAI invoice Rymd.pdf".to_string()), None],
            "{verb}: a file given its own name keeps it, stored as no name"
        );
    }
}

#[test]
fn a_rename_keeps_every_name_even_one_equal_to_its_own() {
    // The rename review reads `op.destination.unwrap_or_default()`, so a NULL there would
    // rename the file to an empty name.
    let planned = plan_sweep(
        &json!({ "groups": [{
            "verb": "rename",
            "parent": DOWNLOADS,
            "sourceVolumeId": "root",
            "displayName": "x",
            "renames": [{ "path": format!("{DOWNLOADS}/a.pdf"), "newName": "a.pdf" }],
        }] }),
        NOW,
        &local_facts(),
    )
    .expect("valid");
    let PlannedOps::Rename { renames, .. } = &planned.groups[0].ops else {
        panic!("a rename");
    };
    assert_eq!(renames[0].new_name, "a.pdf");
}

#[test]
fn names_stay_off_the_verbs_that_cant_carry_them() {
    for verb in ["extract", "compress", "trash", "delete"] {
        let mut call = named_call(verb, &[("a.pdf", "b.pdf")]);
        if matches!(verb, "trash" | "delete") {
            call["groups"][0].as_object_mut().expect("group").remove("destination");
        }
        assert_eq!(
            problem_of(&call, &local_facts()),
            GroupProblem::RenamesVerbMismatch,
            "{verb}"
        );
    }
}

#[test]
fn each_name_is_one_plain_visible_name() {
    for (name, rule) in [
        ("", PlainLeafRefusal::Empty),
        (" invoice.pdf", PlainLeafRefusal::EdgeWhitespace),
        ("invoice.pdf ", PlainLeafRefusal::EdgeWhitespace),
        ("paid/invoice.pdf", PlainLeafRefusal::Separator),
        ("..", PlainLeafRefusal::DotName),
        ("invoice\u{202E}fdp.exe", PlainLeafRefusal::Invisible('\u{202E}')),
        ("in\u{200D}voice.pdf", PlainLeafRefusal::Invisible('\u{200D}')),
        ("invoice\u{7}.pdf", PlainLeafRefusal::Control('\u{7}')),
    ] {
        for verb in ["move", "copy", "rename"] {
            let call = if verb == "rename" {
                json!({ "groups": [{
                    "verb": "rename",
                    "parent": DOWNLOADS,
                    "sourceVolumeId": "root",
                    "displayName": "x",
                    "renames": [{ "path": format!("{DOWNLOADS}/a.pdf"), "newName": name }],
                }] })
            } else {
                named_call(verb, &[("a.pdf", name)])
            };
            assert_eq!(
                problem_of(&call, &local_facts()),
                GroupProblem::NotABareName {
                    name: name.to_string(),
                    rule: NameRule::Plain(rule),
                },
                "{verb} {name:?}"
            );
        }
    }
}

#[test]
fn two_files_landing_as_one_name_refuse_even_by_case_or_unicode_form() {
    for verb in ["move", "copy"] {
        for (a, b) in [
            ("Invoice.pdf", "invoice.pdf"),
            // NFC "é" and NFD "e + combining acute": one name on APFS and on most shares.
            ("Caf\u{e9}.pdf", "Cafe\u{301}.pdf"),
        ] {
            let call = named_call(verb, &[("one.pdf", a), ("two.pdf", b)]);
            assert_eq!(
                problem_of(&call, &local_facts()),
                GroupProblem::DuplicateLeaf { name: b.to_string() },
                "{verb} {a} / {b}"
            );
        }
        // A kept name collides as readily as a chosen one.
        let call = named_call(verb, &[("one.pdf", "two.pdf"), ("two.pdf", "two.pdf")]);
        assert!(matches!(
            problem_of(&call, &local_facts()),
            GroupProblem::DuplicateLeaf { .. }
        ));
    }
}

/// Two files sharing an original name pass the shape checks, then refuse at the start of a
/// route that keys each landing on a file's own name (both ends on a local path), named apart
/// or not. The proposal asks the engine's own rule and refuses the whole call there; a route
/// that keys landings on the source path takes them.
#[test]
fn two_files_sharing_a_name_refuse_where_the_route_cant_tell_them_apart() {
    use std::sync::Arc;

    use crate::agent::tools::suggestions::input::check_own_names;
    use crate::file_system::volume::{InMemoryVolume, Volume};

    let disk: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "Root",
        "/",
        crate::volume_host::host(),
    ));
    let remote: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Remote"));
    let (first, second) = (format!("{DOWNLOADS}/a/report.pdf"), format!("{DOWNLOADS}/b/report.pdf"));
    let paths_call = |verb: &str| {
        json!({ "groups": [{
            "verb": verb,
            "sourceVolumeId": "root",
            "displayName": "Reports",
            "destination": { "volumeId": "root", "path": INVOICES },
            "paths": [first, second],
        }] })
    };
    for verb in ["move", "copy"] {
        let named = named_call(verb, &[("a/report.pdf", "one.pdf"), ("b/report.pdf", "two.pdf")]);
        for call in [paths_call(verb), named] {
            let planned = plan_sweep(&call, NOW, &local_facts()).expect("the shape is valid");
            assert_eq!(
                check_own_names(&planned, |_| Some(Arc::clone(&disk))),
                Err(PlanRefusal::Group {
                    group: 0,
                    problem: GroupProblem::SharedName {
                        name: "report.pdf".to_string(),
                        first: first.clone(),
                        second: second.clone(),
                    },
                }),
                "{verb} {call}"
            );
            assert_eq!(
                check_own_names(&planned, |_| Some(Arc::clone(&remote))),
                Ok(()),
                "{verb} {call}"
            );
        }
    }
}

#[test]
fn a_file_never_lands_on_another_source_of_its_group() {
    // `a` lands as `Old`, which is another source of the group, compared ignoring case. A
    // copy would replace it before reading it; a move turns "a to b, b to c" into a lost file
    // when the order runs wrong. (That source also sits in the destination, so the group is a
    // rename too; the overwrite is the worse news, so it's the one reported.)
    for verb in ["move", "copy"] {
        let call = json!({ "groups": [{
            "verb": verb,
            "sourceVolumeId": "root",
            "displayName": "x",
            "destination": { "volumeId": "root", "path": INVOICES },
            "renames": [
                { "path": format!("{DOWNLOADS}/a.pdf"), "newName": "Old" },
                { "path": format!("{INVOICES}/old"), "newName": "older" },
            ],
        }] });
        assert_eq!(
            problem_of(&call, &local_facts()),
            GroupProblem::ChainOverwrite {
                path: format!("{INVOICES}/Old"),
            },
            "{verb}"
        );
    }
}

#[test]
fn naming_a_file_into_its_own_folder_is_a_rename() {
    // Any one source already in the destination makes it one, and the sentence says to split.
    let call = json!({ "groups": [{
        "verb": "move",
        "sourceVolumeId": "root",
        "displayName": "x",
        "destination": { "volumeId": "root", "path": INVOICES },
        "renames": [
            { "path": format!("{DOWNLOADS}/a.pdf"), "newName": "2026 a.pdf" },
            { "path": format!("{INVOICES}/b.pdf"), "newName": "2026 b.pdf" },
        ],
    }] });
    assert_eq!(problem_of(&call, &local_facts()), GroupProblem::UseRename);

    // A group whose names all match the files' own is a plain move, as if sent as `paths`.
    let call = json!({ "groups": [{
        "verb": "move",
        "sourceVolumeId": "root",
        "displayName": "x",
        "destination": { "volumeId": "root", "path": INVOICES },
        "renames": [{ "path": format!("{INVOICES}/b.pdf"), "newName": "b.pdf" }],
    }] });
    plan_sweep(&call, NOW, &local_facts()).expect("no name, nothing to refuse");
}

#[test]
fn a_phone_takes_shorter_names_and_none_of_fats_reserved_characters() {
    let mut facts = local_facts();
    facts
        .volumes
        .push(volume("mtp-pixel", VolumeKind::Mtp, "mtp://pixel/", true));
    let onto_phone = |name: &str| {
        named_call_into(
            "copy",
            &[("a.pdf", name)],
            json!({ "volumeId": "mtp-pixel", "path": "mtp://pixel/Documents" }),
        )
    };

    // At `mtp-rs`'s limit it lands; one unit past it (a fine Mac name) is what the device
    // layer would refuse after approval, so the proposal refuses it first.
    let at_limit = format!("{}.pdf", "a".repeat(cmdr_mtp::MAX_NAME_UNITS - ".pdf".len()));
    plan_sweep(&onto_phone(&at_limit), NOW, &facts).expect("a name at the limit lands on a phone");
    let long = format!("{}.pdf", "a".repeat(cmdr_mtp::MAX_NAME_UNITS + 1 - ".pdf".len()));
    assert_eq!(long.encode_utf16().count(), 255, "one past what a PTP string holds");
    assert_eq!(
        problem_of(&onto_phone(&long), &facts),
        GroupProblem::NotABareName {
            name: long.clone(),
            rule: NameRule::PhoneTooLong,
        }
    );
    assert_eq!(
        problem_of(&onto_phone("12:30 invoice.pdf"), &facts),
        GroupProblem::NotABareName {
            name: "12:30 invoice.pdf".to_string(),
            rule: NameRule::PhoneReserved(':'),
        }
    );
    plan_sweep(&onto_phone("12-30 invoice.pdf"), NOW, &facts).expect("a plain name lands on a phone");

    // The same names onto the Mac's own disk are fine.
    plan_sweep(&named_call("copy", &[("a.pdf", &long)]), NOW, &local_facts()).expect("a Mac takes it");
    plan_sweep(
        &named_call("copy", &[("a.pdf", "12:30 invoice.pdf")]),
        NOW,
        &local_facts(),
    )
    .expect("a Mac takes a colon");
}

#[test]
fn the_names_are_stored_per_op_and_bound_by_the_claim() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let first = apply(
        &conn,
        &index,
        &named_call(
            "move",
            &[("Invoice-X.pdf", "2026 OpenAI.pdf"), ("receipt.pdf", "receipt.pdf")],
        ),
    )
    .expect("staged");
    let group_id = first.groups[0].group_id;
    let ops = page_ops(&conn, group_id, 10, 0).expect("ops");
    let stored: Vec<Option<&str>> = ops.iter().map(|op| op.destination.as_deref()).collect();
    assert_eq!(stored, vec![Some("2026 OpenAI.pdf"), None]);
    let shown_revision = get_group(&conn, group_id).expect("read").expect("exists").revision;
    let before = live_binding(&conn, group_id).expect("binding");

    // An amendment that only changes a name is a different group to approve.
    let mut amended = named_call(
        "move",
        &[
            ("Invoice-X.pdf", "2026 OpenAI Rymd.pdf"),
            ("receipt.pdf", "receipt.pdf"),
        ],
    );
    amended["sweepId"] = json!(first.sweep_id);
    amended["groups"][0]["groupId"] = json!(group_id);
    apply(&conn, &index, &amended).expect("amended");
    assert_ne!(live_binding(&conn, group_id).expect("binding").digest, before.digest);
    assert!(matches!(
        record_acceptance(&conn, group_id, shown_revision, &OpSelection::all_except(&[]), NOW).expect("preflight"),
        AcceptanceOutcome::RevisionChanged { .. }
    ));
}
