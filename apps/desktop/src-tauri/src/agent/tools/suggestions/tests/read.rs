//! The two read shapers, and the store's sweep reader they lean on.

use super::{NOW, migrated_conn};
use crate::agent::store::proposals::{
    GroupIntent, GroupSummary, NewGroup, NewOp, NewSweep, ProposalOp, ProposalSweep, create_group, create_sweep,
};
use crate::agent::tools::suggestions::group::shape_group;
use crate::agent::tools::suggestions::list::{shape_list, to_group_summary};
use crate::agent::types::{OpStatus, ProposalStatus, ProposalVerb, Reversibility};

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
    let detail = shape_group(
        summary(1, 10, 60_000, 60_000, None),
        vec![op(1, Some(4_096)), op(2, None)],
        100,
    );
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
