//! Where a proposal may point: every place resolves onto a live volume, or onto a proven,
//! signed-in sibling of the one named, or the whole call refuses in the read tools' miss shape.

use cmdr_fs::volume::ConnectionState;
use rusqlite::Connection;
use serde_json::{Value, json};

use super::{FakeIndex, NOW, apply_over, local_facts, migrated_conn, volume};
use crate::agent::store::proposals::{get_group, page_ops};
use crate::agent::suggested_ops::SelectorRefusal;
use crate::location::miss::{LocationMissResult, MissToken};
use crate::location::resolve::{VolumeFact, VolumeFacts};
use crate::location::same_share::Via;
use crate::location::share_identity::ShareIdentity;
use crate::network::server_guid::GuidProof;
use crate::tools::resources::volumes::VolumeKind;
use crate::tools::suggestions::input::{GroupProblem, PlanRefusal, PlannedOps, PlannedSources, plan_sweep};
use crate::tools::suggestions::problem::PlaceField;
use crate::tools::suggestions::propose::{ApplyRefusal, plan_refusal_content};
use crate::tools::transcript_179_tests::{INVOICES, LAN, TAILSCALE, conversation_179, conversation_179_with};

const BOX: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
const OTHER_BOX: &str = "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0";

/// An SMB share on `/Volumes/<mount>`, its GUID recorded with `proof`, opened as `user`.
fn share(id: &str, mount: &str, live: bool, guid: (&str, GuidProof), share: &str, user: &str) -> VolumeFact {
    VolumeFact {
        kind: VolumeKind::Smb,
        connection_state: Some(if live {
            ConnectionState::Direct
        } else {
            ConnectionState::Saved
        }),
        share: Some(ShareIdentity::new(Some(guid), share, Some(user))),
        ..volume(id, VolumeKind::Smb, &format!("/Volumes/{mount}"), live)
    }
}

/// The boot disk plus one NAS share reached two ways: `ts` not connected at
/// `/Volumes/naspi`, and `lan` live at `/Volumes/naspi-1` with the given proof and account.
fn nas(lan_proof: GuidProof, lan_user: &str) -> VolumeFacts {
    let mut facts = local_facts();
    facts.volumes.push(share(
        "ts",
        "naspi",
        false,
        (BOX, GuidProof::Authenticated),
        "naspi",
        "david",
    ));
    facts
        .volumes
        .push(share("lan", "naspi-1", true, (BOX, lan_proof), "naspi", lan_user));
    facts
}

/// [`nas`] plus a second share, `media`, reached the same two ways (`ts-media` dead,
/// `lan-media` live), so a group can ask for two re-addresses at once.
fn two_shares() -> VolumeFacts {
    let mut facts = nas(GuidProof::Authenticated, "david");
    for (id, mount, live) in [("ts-media", "media", false), ("lan-media", "media-1", true)] {
        facts.volumes.push(share(
            id,
            mount,
            live,
            (OTHER_BOX, GuidProof::Authenticated),
            "media",
            "david",
        ));
    }
    facts
}

/// A move from Downloads into `destination`.
fn move_into(destination: Value) -> Value {
    json!({ "groups": [{
        "verb": "move",
        "sourceVolumeId": "root",
        "displayName": "One invoice",
        "destination": destination,
        "paths": ["/Users/someone/Downloads/Invoice-X.pdf"],
    }] })
}

fn refused_at(call: &Value, facts: &VolumeFacts) -> (usize, GroupProblem) {
    match plan_sweep(call, NOW, facts).expect_err("refused") {
        PlanRefusal::Group { group, problem } => (group, problem),
        other => panic!("a group refusal, got {other:?}"),
    }
}

fn location_miss(call: &Value, facts: &VolumeFacts) -> (usize, PlaceField, LocationMissResult) {
    match refused_at(call, facts) {
        (group, GroupProblem::Location { field, miss }) => (group, field, *miss),
        other => panic!("a location miss, got {other:?}"),
    }
}

fn sweep_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM proposal_sets", [], |row| row.get(0))
        .expect("count sweeps")
}

// ── A place that can run now ──────────────────────────────────────────────────

#[test]
fn a_destination_on_a_live_volume_stages_where_it_was_named() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let facts = nas(GuidProof::Authenticated, "david");
    let named = move_into(json!({ "volumeId": "lan", "path": "/Volumes/naspi-1/papers" }));
    let report = apply_over(&conn, &index, &named, &facts).expect("staged");

    assert_eq!(report.groups[0].via, None, "nothing was re-addressed");
    let group = get_group(&conn, report.groups[0].group_id)
        .expect("read")
        .expect("exists");
    assert_eq!(group.destination.as_deref(), Some("/Volumes/naspi-1/papers"));
    assert_eq!(group.destination_volume_id.as_deref(), Some("lan"));

    // `volumeId` is optional: a bare path lands on the live volume holding it.
    let bare = move_into(json!({ "path": "/Volumes/naspi-1/papers" }));
    let report = apply_over(&conn, &index, &bare, &facts).expect("staged");
    let group = get_group(&conn, report.groups[0].group_id)
        .expect("read")
        .expect("exists");
    assert_eq!(group.destination_volume_id.as_deref(), Some("lan"));
}

// ── A place that can't ────────────────────────────────────────────────────────

/// The whole call refuses, the group before it included, so nothing waits on the user that
/// approval would only refuse.
#[test]
fn a_place_on_a_volume_that_isnt_connected_refuses_the_whole_call() {
    let conn = migrated_conn();
    let mut facts = local_facts();
    facts.volumes.push(share(
        "ts",
        "naspi",
        false,
        (BOX, GuidProof::Authenticated),
        "naspi",
        "david",
    ));
    // An unproven volume at the second mount name: a `nearby`, never a re-address.
    facts
        .volumes
        .push(volume("stranger", VolumeKind::Smb, "/Volumes/naspi-1", true));
    let call = json!({ "groups": [
        { "verb": "trash", "sourceVolumeId": "root", "displayName": "fine", "paths": ["/Users/someone/a.dmg"] },
        {
            "verb": "move",
            "sourceVolumeId": "root",
            "displayName": "onto the sleeping share",
            "destination": { "volumeId": "ts", "path": "/Volumes/naspi/papers" },
            "paths": ["/Users/someone/b.pdf"],
        },
    ] });

    let (group, field, miss) = location_miss(&call, &facts);
    assert_eq!((group, field), (1, PlaceField::Destination));
    assert_eq!(miss.miss, MissToken::VolumeNotConnected);
    let nearby = miss.nearby.as_deref().expect("nearby");
    assert_eq!(nearby[0].candidate.volume_id, "stranger");
    assert_eq!(nearby[0].candidate.path, "/Volumes/naspi-1/papers");
    assert_eq!(miss.same_share_as.as_deref(), Some(&[][..]));

    // The wire: the read tools' miss shape, plus the refusal envelope, the group, and the field.
    let wire = plan_refusal_content(&plan_sweep(&call, NOW, &facts).expect_err("refused"));
    assert_eq!(wire["readyForReview"], json!(false));
    assert_eq!(wire["refusal"], json!("location"));
    assert_eq!(wire["group"], json!(1));
    assert_eq!(wire["field"], json!("destination"));
    assert_eq!(wire["miss"], json!("volumeNotConnected"));
    assert_eq!(
        wire["location"],
        json!({ "volumeId": "ts", "path": "/Volumes/naspi/papers" })
    );
    assert_eq!(wire["nearby"][0]["candidate"]["path"], json!("/Volumes/naspi-1/papers"));
    assert!(wire["detail"].as_str().is_some_and(|d| !d.is_empty()));

    assert_eq!(sweep_count(&conn), 0, "the good group didn't land either");
}

#[test]
fn a_hand_listed_source_on_a_volume_that_isnt_connected_refuses_too() {
    let mut facts = local_facts();
    facts.volumes.push(share(
        "ts",
        "naspi",
        false,
        (BOX, GuidProof::Authenticated),
        "naspi",
        "david",
    ));
    let call = json!({ "groups": [{
        "verb": "trash",
        "sourceVolumeId": "ts",
        "displayName": "old scans",
        "paths": ["/Volumes/naspi/scans/a.pdf"],
    }] });
    let (_, field, miss) = location_miss(&call, &facts);
    assert_eq!(field, PlaceField::Paths);
    assert_eq!(
        miss.miss,
        MissToken::VolumeNotConnected,
        "named, so it keeps `sameShareAs`"
    );
}

#[test]
fn an_unknown_volume_or_a_path_off_its_volume_refuses_with_the_resolvers_token() {
    let facts = nas(GuidProof::Authenticated, "david");

    let unknown = move_into(json!({ "volumeId": "nope", "path": "/Volumes/naspi/papers" }));
    let (_, field, miss) = location_miss(&unknown, &facts);
    assert_eq!((field, miss.miss), (PlaceField::Destination, MissToken::UnknownVolume));

    // A hallucinated volume/path pair: the source isn't under the named volume's root.
    let off = json!({ "groups": [{
        "verb": "trash",
        "sourceVolumeId": "lan",
        "displayName": "x",
        "paths": ["/Users/someone/a.dmg"],
    }] });
    let (_, field, miss) = location_miss(&off, &facts);
    assert_eq!((field, miss.miss), (PlaceField::Paths, MissToken::PathOutsideVolume));
    assert_eq!(miss.root.as_deref(), Some("/Volumes/naspi-1"));
}

#[test]
fn a_bare_path_on_a_dead_mount_point_refuses_as_no_volume_there() {
    let facts = nas(GuidProof::Authenticated, "david");
    let call = move_into(json!({ "path": "/Volumes/naspi/papers" }));
    let (_, field, miss) = location_miss(&call, &facts);
    assert_eq!((field, miss.miss), (PlaceField::Destination, MissToken::NoVolumeAtPath));
}

// ── Re-addressing onto a proven sibling ───────────────────────────────────────

/// The stored group is what runs, so it's stored on the live sibling: its id, its rebased
/// path, and one `via` naming the volume asked.
#[test]
fn a_dead_volume_with_a_signed_in_sibling_is_stored_on_the_sibling() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let facts = nas(GuidProof::Authenticated, "david");
    let call = move_into(json!({ "volumeId": "ts", "path": "/Volumes/naspi/papers" }));
    let report = apply_over(&conn, &index, &call, &facts).expect("re-addressed");

    assert_eq!(
        report.groups[0].via,
        Some(Via {
            volume_id: "ts".to_string()
        })
    );
    let wire = serde_json::to_value(&report).expect("serializes");
    assert_eq!(wire["groups"][0]["via"], json!({ "volumeId": "ts" }));
    let group = get_group(&conn, report.groups[0].group_id)
        .expect("read")
        .expect("exists");
    assert_eq!(group.destination.as_deref(), Some("/Volumes/naspi-1/papers"));
    assert_eq!(group.destination_volume_id.as_deref(), Some("lan"));
}

#[test]
fn hand_listed_sources_re_address_with_their_volume() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let facts = nas(GuidProof::Authenticated, "david");
    let call = json!({ "groups": [{
        "verb": "move",
        "sourceVolumeId": "ts",
        "displayName": "scans home",
        "destination": { "path": "/Users/someone/Documents" },
        "paths": ["/Volumes/naspi/scans/a.pdf", "/Volumes/naspi/scans/b.pdf"],
    }] });
    let report = apply_over(&conn, &index, &call, &facts).expect("re-addressed");

    assert_eq!(report.groups[0].via.as_ref().map(|v| v.volume_id.as_str()), Some("ts"));
    let group = get_group(&conn, report.groups[0].group_id)
        .expect("read")
        .expect("exists");
    assert_eq!(group.source_volume_id, "lan");
    let ops = page_ops(&conn, group.id, 10, 0).expect("ops");
    let paths: Vec<_> = ops.iter().map(|op| op.source_path.as_str()).collect();
    assert_eq!(paths, ["/Volumes/naspi-1/scans/a.pdf", "/Volumes/naspi-1/scans/b.pdf"]);
}

/// A GUID the credential-free probe filled only says what answered a saved address, and
/// another account can see other files: either way the sibling is a hint, never a target.
#[test]
fn an_unproven_or_other_account_sibling_is_only_a_hint() {
    for (proof, user) in [
        (GuidProof::Unauthenticated, "david"),
        (GuidProof::Authenticated, "dori"),
    ] {
        let facts = nas(proof, user);
        let call = move_into(json!({ "volumeId": "ts", "path": "/Volumes/naspi/papers" }));
        let (_, field, miss) = location_miss(&call, &facts);
        assert_eq!(
            (field, miss.miss),
            (PlaceField::Destination, MissToken::VolumeNotConnected)
        );
        assert_eq!(
            miss.same_share_as.as_deref(),
            Some(&["lan".to_string()][..]),
            "{proof:?} {user}"
        );
        let nearby = miss.nearby.as_deref().expect("nearby");
        assert_eq!(nearby[0].candidate.volume_id, "lan");
        assert_eq!(nearby[0].candidate.path, "/Volumes/naspi-1/papers");
    }
}

/// One `via` per group: re-addressing onto two members, or re-addressing one place while
/// naming the live member directly for another, is a group nothing can describe honestly.
#[test]
fn a_group_never_re_addresses_onto_two_members() {
    let facts = two_shares();
    let onto_two = json!({ "groups": [{
        "verb": "copy",
        "sourceVolumeId": "ts-media",
        "displayName": "x",
        "destination": { "volumeId": "ts", "path": "/Volumes/naspi/papers" },
        "paths": ["/Volumes/media/a.mov"],
    }] });
    assert_eq!(refused_at(&onto_two, &facts), (0, GroupProblem::MixedReaddress));

    let half_and_half = json!({ "groups": [{
        "verb": "move",
        "sourceVolumeId": "lan",
        "displayName": "x",
        "destination": { "volumeId": "ts", "path": "/Volumes/naspi/papers" },
        "paths": ["/Volumes/naspi-1/inbox/a.pdf"],
    }] });
    assert_eq!(refused_at(&half_and_half, &facts), (0, GroupProblem::MixedReaddress));

    let wire = plan_refusal_content(&plan_sweep(&half_and_half, NOW, &facts).expect_err("refused"));
    assert_eq!(wire["refusal"], json!("mixedReaddress"));
}

/// A selector re-addressed onto a sibling reads THAT sibling's index; with none there, the
/// selector's own refusal stands (the model then lists paths it read live).
#[test]
fn a_selector_root_re_addresses_and_reads_the_siblings_index() {
    let conn = migrated_conn();
    let facts = nas(GuidProof::Authenticated, "david");
    let index = FakeIndex::refusing(SelectorRefusal::NotIndexed {
        volume_id: "lan".to_string(),
    });
    let call = json!({ "groups": [{
        "verb": "trash",
        "selector": { "root": { "volumeId": "ts", "path": "/Volumes/naspi/Downloads" }, "nameGlob": "*.dmg" },
    }] });
    let planned = plan_sweep(&call, NOW, &facts).expect("placed");
    assert_eq!(planned.groups[0].via.as_ref().map(|v| v.volume_id.as_str()), Some("ts"));
    let PlannedOps::Sources {
        sources: PlannedSources::Selector(selector),
        ..
    } = &planned.groups[0].ops
    else {
        panic!("a selector group");
    };
    assert_eq!(selector.root.volume_id, "lan");
    assert_eq!(selector.root.path, "/Volumes/naspi-1/Downloads");

    let refusal = super::super::propose::apply_planned_sweep(&conn, &index, planned, None, NOW)
        .expect_err("no index on the sibling");
    assert!(matches!(
        refusal,
        ApplyRefusal::Selector {
            group: 0,
            refusal: SelectorRefusal::NotIndexed { ref volume_id }
        } if volume_id == "lan"
    ));
}

/// An amendment is a fresh proposal of the group, so it resolves again.
#[test]
fn an_amendment_re_resolves_its_places() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let live = nas(GuidProof::Authenticated, "david");
    let first = apply_over(
        &conn,
        &index,
        &move_into(json!({ "volumeId": "lan", "path": "/Volumes/naspi-1/papers" })),
        &live,
    )
    .expect("staged");

    let mut amendment = move_into(json!({ "volumeId": "ts", "path": "/Volumes/naspi/papers" }));
    amendment["sweepId"] = json!(first.sweep_id);
    amendment["groups"][0]["groupId"] = json!(first.groups[0].group_id);
    let mut gone = local_facts();
    gone.volumes.push(share(
        "ts",
        "naspi",
        false,
        (BOX, GuidProof::Authenticated),
        "naspi",
        "david",
    ));
    let (_, _, miss) = location_miss(&amendment, &gone);
    assert_eq!(miss.miss, MissToken::VolumeNotConnected);

    let amended = apply_over(&conn, &index, &amendment, &live).expect("amended");
    assert_eq!(amended.groups[0].via.as_ref().map(|v| v.volume_id.as_str()), Some("ts"));
}

// ── Conversation 179 ──────────────────────────────────────────────────────────

/// The prod situation: an invoice filed into `invoices paid` naming the Tailscale share,
/// which isn't connected, while its signed-in LAN sibling is. It lands on the LAN share.
/// With the LAN share gone too, the call refuses whole and names both ways forward.
#[test]
fn conversation_179_files_onto_the_live_lan_share_or_refuses_whole() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let call = move_into(json!({ "volumeId": TAILSCALE, "path": format!("/Volumes/naspi/{INVOICES}") }));

    let report = apply_over(&conn, &index, &call, &conversation_179()).expect("re-addressed");
    assert_eq!(
        report.groups[0].via.as_ref().map(|v| v.volume_id.as_str()),
        Some(TAILSCALE)
    );
    let group = get_group(&conn, report.groups[0].group_id)
        .expect("read")
        .expect("exists");
    assert_eq!(group.destination_volume_id.as_deref(), Some(LAN));
    assert_eq!(
        group.destination.as_deref(),
        Some(format!("/Volumes/naspi-1/{INVOICES}").as_str())
    );

    let both_gone = conversation_179_with("/Volumes/naspi-1", ConnectionState::Saved);
    let (_, field, miss) = location_miss(&call, &both_gone);
    assert_eq!(
        (field, miss.miss),
        (PlaceField::Destination, MissToken::VolumeNotConnected)
    );
    assert_eq!(miss.same_share_as.as_deref(), Some(&[LAN.to_string()][..]));
    let nearby = miss.nearby.as_deref().expect("nearby");
    assert_eq!(nearby[0].candidate.volume_id, LAN);
    assert_eq!(nearby[0].candidate.path, format!("/Volumes/naspi-1/{INVOICES}"));
}
