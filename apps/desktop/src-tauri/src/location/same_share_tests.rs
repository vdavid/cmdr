//! The proof rules: GUID plus share name groups, GUID alone never does, and only
//! the same account, with both GUIDs recorded by a sign-in, redirects.

use super::*;
use crate::location::resolve::IndexSources;
use crate::location::share_identity::ShareIdentity;
use crate::network::server_guid::GuidProof;
use crate::tools::resources::volumes::VolumeKind;
use cmdr_fs::volume::ConnectionState;

const BOX: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
const OTHER_BOX: &str = "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0";
const ZERO: &str = "00000000-0000-0000-0000-000000000000";

fn share(id: &str, root: &str, live: bool, identity: ShareIdentity) -> VolumeFact {
    VolumeFact {
        id: id.to_string(),
        name: id.to_string(),
        kind: VolumeKind::Smb,
        root: root.to_string(),
        live,
        connection_state: Some(if live {
            ConnectionState::Direct
        } else {
            ConnectionState::Saved
        }),
        earlier_roots: Vec::new(),
        index: IndexSources::none(id),
        share: Some(identity),
    }
}

fn facts(volumes: Vec<VolumeFact>) -> VolumeFacts {
    VolumeFacts {
        volumes,
        mount_areas: vec!["/Volumes".to_string()],
    }
}

/// David's share, its GUID (if any) recorded by a signed-in session.
fn david(guid: Option<&str>, share_name: &str) -> ShareIdentity {
    ShareIdentity::new(guid.map(|g| (g, GuidProof::Authenticated)), share_name, Some("david"))
}

/// David's share, its GUID filled by the credential-free probe.
fn probed(guid: &str, share_name: &str) -> ShareIdentity {
    ShareIdentity::new(Some((guid, GuidProof::Unauthenticated)), share_name, Some("david"))
}

#[test]
fn one_guid_and_one_share_name_ignoring_case_is_one_share() {
    let facts = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share(
            "lan",
            "/Volumes/naspi-1",
            true,
            david(Some(&BOX.to_uppercase()), "NASPI"),
        ),
    ]);

    assert_eq!(facts.same_share_as("ts"), ["lan"]);
    assert_eq!(facts.same_share_as("lan"), ["ts"]);
}

/// The GUID names the BOX: two shares on one NAS are two places.
#[test]
fn one_guid_with_two_share_names_is_two_shares() {
    let facts = facts(vec![
        share("naspi", "/Volumes/naspi", true, david(Some(BOX), "naspi")),
        share("media", "/Volumes/media", true, david(Some(BOX), "media")),
    ]);

    assert!(facts.same_share_as("naspi").is_empty());
}

#[test]
fn a_missing_or_all_zero_guid_proves_nothing() {
    let facts = facts(vec![
        share("a", "/Volumes/naspi", true, david(None, "naspi")),
        share("b", "/Volumes/naspi-1", true, david(None, "naspi")),
        share("c", "/Volumes/naspi-2", true, david(Some(ZERO), "naspi")),
        share("d", "/Volumes/naspi-3", true, david(Some(ZERO), "naspi")),
    ]);

    for id in ["a", "b", "c", "d"] {
        assert!(facts.same_share_as(id).is_empty(), "{id} has no proven sibling");
    }
}

#[test]
fn two_servers_with_one_share_name_are_two_shares() {
    let facts = facts(vec![
        share("home", "/Volumes/public", true, david(Some(BOX), "public")),
        share("office", "/Volumes/public-1", true, david(Some(OTHER_BOX), "public")),
    ]);

    assert!(facts.same_share_as("home").is_empty());
}

/// ❗ Another account can see different files on the same share, so it's a
/// `sameShareAs` hint and never answers for the volume asked.
#[test]
fn the_same_share_under_two_accounts_is_grouped_but_never_redirected() {
    let facts = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share(
            "lan",
            "/Volumes/naspi-1",
            true,
            ShareIdentity::new(Some((BOX, GuidProof::Authenticated)), "naspi", Some("dori")),
        ),
    ]);
    let asked = crate::location::resolve::resolve(
        &facts,
        &crate::location::resolve::LocationInput {
            volume_id: Some("ts".to_string()),
            path: "/Volumes/naspi/papers".to_string(),
        },
    )
    .expect("resolves");

    assert_eq!(facts.same_share_as("ts"), ["lan"]);
    assert_eq!(facts.readers(&asked).len(), 1, "only the volume asked reads");
    assert_eq!(facts.preferred_member("ts").map(|v| v.id.as_str()), Some("ts"));
}

/// A not-connected miss names its proven siblings: here another account's, which
/// couldn't answer in its place.
#[test]
fn a_volume_not_connected_miss_names_the_same_share() {
    let facts = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share(
            "lan",
            "/Volumes/naspi-1",
            false,
            ShareIdentity::new(Some((BOX, GuidProof::Authenticated)), "naspi", Some("dori")),
        ),
    ]);
    let asked = crate::location::resolve::resolve(
        &facts,
        &crate::location::resolve::LocationInput {
            volume_id: Some("ts".to_string()),
            path: "/Volumes/naspi/papers".to_string(),
        },
    )
    .expect("resolves");

    let miss = crate::location::miss::volume_not_connected(&facts, &asked);

    assert_eq!(miss.same_share_as, Some(vec!["lan".to_string()]));
}

/// A guest sign-in is no account, whichever way it was spelled.
#[test]
fn guest_sessions_are_one_account() {
    let guid = Some((BOX, GuidProof::Authenticated));
    assert_eq!(
        ShareIdentity::new(guid, "public", Some("Guest")),
        ShareIdentity::new(guid, "public", None)
    );
}

/// `papers` on the volume `id`, rooted at `root`, resolved.
fn papers_on(facts: &VolumeFacts, id: &str, root: &str) -> ResolvedLocation {
    crate::location::resolve::resolve(
        facts,
        &crate::location::resolve::LocationInput {
            volume_id: Some(id.to_string()),
            path: format!("{root}/papers"),
        },
    )
    .expect("resolves")
}

/// Asserts `asked` has `sibling` as a `sameShareAs` hint and nothing more: it
/// answers its own reads, and no member stands in for it.
fn only_a_hint(facts: &VolumeFacts, asked: &str, root: &str, sibling: &str) {
    assert_eq!(facts.same_share_as(asked), [sibling], "still a hint");
    assert_eq!(
        facts.readers(&papers_on(facts, asked, root)).len(),
        1,
        "only {asked} reads"
    );
    assert_eq!(facts.preferred_member(asked).map(|v| v.id.as_str()), Some(asked));
    assert_eq!(facts.redirect_group_ids(asked), [asked]);
}

/// ❗ Whatever answers a probe of a saved address (a hotel network handing out the
/// NAS's LAN IP) must not stand in for the share, in either direction: both GUIDs
/// need a sign-in behind them.
#[test]
fn a_probe_filled_guid_on_either_member_only_ever_makes_a_hint() {
    let ts_probed = facts(vec![
        share("ts", "/Volumes/naspi", false, probed(BOX, "naspi")),
        share("lan", "/Volumes/naspi-1", true, david(Some(BOX), "naspi")),
    ]);
    only_a_hint(&ts_probed, "ts", "/Volumes/naspi", "lan");
    only_a_hint(&ts_probed, "lan", "/Volumes/naspi-1", "ts");

    let lan_probed = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share("lan", "/Volumes/naspi-1", true, probed(BOX, "naspi")),
    ]);
    only_a_hint(&lan_probed, "ts", "/Volumes/naspi", "lan");
}

/// A guest session proves nothing about the box either: two guest members of one
/// share stay a hint.
#[test]
fn a_guest_sessions_guid_only_ever_makes_a_hint() {
    let guest = || ShareIdentity::new(Some((BOX, GuidProof::Unauthenticated)), "public", None);
    let facts = facts(vec![
        share("ts", "/Volumes/public", false, guest()),
        share("lan", "/Volumes/public-1", true, guest()),
    ]);

    only_a_hint(&facts, "ts", "/Volumes/public", "lan");
}

/// `list_dir`'s last tier reads the freshest index a not-connected member left on
/// disk. A probe-only member's index, however fresh, is never that pick.
#[test]
fn a_probe_only_member_is_never_the_offline_index_pick() {
    let mut lan = share("lan", "/Volumes/naspi-1", false, david(Some(BOX), "naspi"));
    lan.index.stored = true;
    lan.index.stored_as_of = Some(50);
    let mut ts = share("ts", "/Volumes/naspi", false, probed(BOX, "naspi"));
    ts.index.stored = true;
    ts.index.stored_as_of = Some(100);
    let facts = facts(vec![lan, ts]);

    let readers = facts.readers(&papers_on(&facts, "lan", "/Volumes/naspi-1"));

    assert_eq!(
        readers
            .iter()
            .map(|r| r.location.volume_id.as_str())
            .collect::<Vec<_>>(),
        ["lan"]
    );
    assert_eq!(facts.preferred_member("lan").map(|v| v.id.as_str()), Some("lan"));
}

/// Each GUID keeps the proof it was learned with on its way into the facts: a
/// saved row's own, else the live session's along with its GUID.
#[test]
fn a_shares_identity_carries_the_proof_its_guid_was_recorded_with() {
    use crate::location::facts::identity_of;
    use crate::network::known_shares::{AuthOptions, ConnectionMode, KnownNetworkShare};
    use crate::network::server_guid::ServerGuid;
    use crate::network::smb_server_guid::LiveShare;

    let row = |guid: Option<&str>, proof| KnownNetworkShare {
        server_name: "naspolya".to_string(),
        share_name: "naspi".to_string(),
        protocol: "smb".to_string(),
        last_connected_at: "2026-10-26T00:00:00Z".to_string(),
        last_connection_mode: ConnectionMode::Credentials,
        last_known_auth_options: AuthOptions::CredentialsOnly,
        username: Some("david".to_string()),
        address: None,
        port: None,
        volume_id: None,
        mount_path: None,
        pinned: true,
        server_guid: guid.map(str::to_string),
        server_guid_proof: proof,
    };
    let live = |proof| LiveShare {
        server_guid: ServerGuid::parse(BOX).expect("a GUID"),
        proof,
        share_name: "naspi".to_string(),
        username: Some("david".to_string()),
    };

    assert_eq!(
        identity_of(Some(&row(Some(BOX), GuidProof::Unauthenticated)), None),
        Some(probed(BOX, "naspi"))
    );
    assert_eq!(
        identity_of(Some(&row(Some(BOX), GuidProof::Authenticated)), None),
        Some(david(Some(BOX), "naspi"))
    );
    assert_eq!(
        identity_of(
            Some(&row(None, GuidProof::Unauthenticated)),
            Some(&live(GuidProof::Authenticated))
        ),
        Some(david(Some(BOX), "naspi")),
        "a row with no GUID borrows the session's, proof and all"
    );
    assert_eq!(
        identity_of(None, Some(&live(GuidProof::Unauthenticated))),
        Some(probed(BOX, "naspi"))
    );
}

#[test]
fn a_read_is_re_addressed_onto_each_same_account_member_with_via() {
    let facts = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share("lan", "/Volumes/naspi-1", true, david(Some(BOX), "naspi")),
    ]);
    let asked = crate::location::resolve::resolve(
        &facts,
        &crate::location::resolve::LocationInput {
            volume_id: Some("ts".to_string()),
            path: "/Volumes/naspi/papers/invoices paid".to_string(),
        },
    )
    .expect("resolves");

    let readers = facts.readers(&asked);

    assert_eq!(readers[0], asked, "the volume asked comes first");
    assert_eq!(readers[1].location.volume_id, "lan");
    assert_eq!(readers[1].location.path, "/Volumes/naspi-1/papers/invoices paid");
    assert_eq!(readers[1].via.as_deref(), Some("ts"));
    assert!(readers[1].live);
}

#[test]
fn the_preferred_member_is_live_then_indexed_then_freshest_on_disk() {
    let mut ts = share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi"));
    ts.index.stored = true;
    ts.index.stored_as_of = Some(100);
    let mut old = share("old", "/Volumes/naspi-old", false, david(Some(BOX), "naspi"));
    old.index.stored = true;
    old.index.stored_as_of = Some(50);
    let lan = share("lan", "/Volumes/naspi-1", true, david(Some(BOX), "naspi"));

    let offline = facts(vec![old.clone(), ts.clone()]);
    assert_eq!(offline.preferred_member("old").map(|v| v.id.as_str()), Some("ts"));

    let with_lan = facts(vec![ts, old, lan]);
    assert_eq!(with_lan.preferred_member("ts").map(|v| v.id.as_str()), Some("lan"));
    assert_eq!(with_lan.preferred_member("lan").map(|v| v.id.as_str()), Some("lan"));
}

/// When each test member was last connected, in seconds: `lan` an hour after `ts`.
fn recency(id: &str) -> Option<i64> {
    match id {
        "ts" => Some(1_000),
        "lan" => Some(4_600),
        _ => None,
    }
}

fn target<'a>(facts: &'a VolumeFacts, id: &str) -> Option<&'a str> {
    facts.connect_target(id, recency).map(|v| v.id.as_str())
}

/// A connect asked of a member whose same-account sibling is serving answers
/// with that sibling: nothing needs dialing.
#[test]
fn a_connect_on_a_member_with_a_serving_sibling_lands_on_the_sibling() {
    let facts = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share("lan", "/Volumes/naspi-1", true, david(Some(BOX), "naspi")),
    ]);

    assert_eq!(target(&facts, "ts"), Some("lan"));
    assert_eq!(target(&facts, "lan"), Some("lan"));
}

/// With no member serving, the dial goes to whichever was connected last,
/// whichever the model named.
#[test]
fn with_no_member_serving_the_most_recently_connected_one_is_dialed() {
    let facts = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share("lan", "/Volumes/naspi-1", false, david(Some(BOX), "naspi")),
    ]);

    assert_eq!(target(&facts, "ts"), Some("lan"));
    assert_eq!(target(&facts, "lan"), Some("lan"));
    assert_eq!(
        facts.connect_target("ts", |_| None).map(|v| v.id.as_str()),
        Some("ts"),
        "with no recency to go by, the volume named"
    );
}

/// A registered member whose session dropped isn't serving: the pick falls to
/// recency, and the standing of whichever wins decides the move.
#[test]
fn a_registered_member_that_dropped_is_not_serving() {
    let mut lan = share("lan", "/Volumes/naspi-1", true, david(Some(BOX), "naspi"));
    lan.connection_state = Some(ConnectionState::Disconnected);
    let facts = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        lan,
    ]);

    assert_eq!(
        facts
            .connect_target("ts", |id| (id == "ts").then_some(9_999))
            .map(|v| v.id.as_str()),
        Some("ts")
    );
}

/// Another account's member, or one only a probe vouches for, is never the one a
/// connect lands on or dials, however live or recent.
#[test]
fn a_connect_never_lands_on_another_account_or_an_unproven_member() {
    let other_account = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share(
            "lan",
            "/Volumes/naspi-1",
            true,
            ShareIdentity::new(Some((BOX, GuidProof::Authenticated)), "naspi", Some("dori")),
        ),
    ]);
    assert_eq!(target(&other_account, "ts"), Some("ts"));

    let probe_only = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share("lan", "/Volumes/naspi-1", false, probed(BOX, "naspi")),
    ]);
    assert_eq!(target(&probe_only, "ts"), Some("ts"));
}

#[test]
fn a_connect_on_an_unknown_volume_has_no_target() {
    assert_eq!(target(&facts(Vec::new()), "nope"), None);
}
