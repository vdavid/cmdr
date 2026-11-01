use super::*;
use crate::network::known_shares::{AuthOptions, ConnectionMode};

const BOX: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";

fn row(address: &str, share: &str, guid: Option<&str>) -> KnownNetworkShare {
    KnownNetworkShare {
        server_name: address.to_string(),
        share_name: share.to_string(),
        protocol: "smb".to_string(),
        last_connected_at: "2026-10-26T10:00:00Z".to_string(),
        last_connection_mode: ConnectionMode::Credentials,
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: Some("david".to_string()),
        address: Some(address.to_string()),
        port: None,
        volume_id: Some(format!("smb-{address}-{share}")),
        mount_path: Some(format!("/Volumes/{share}")),
        pinned: true,
        server_guid: guid.map(str::to_string),
        server_guid_proof: GuidProof::Unauthenticated,
    }
}

/// Two rows of one share name, one without a GUID: only that one is dialed. A row
/// already holding a GUID never is, since a probe can't change it.
#[test]
fn the_probe_dials_only_rows_missing_a_guid_that_another_row_shares_a_name_with() {
    let rows = [
        row("100.127.48.122", "naspi", Some(BOX)),
        row("192.168.1.111", "NASPI", None),
        row("192.168.1.111", "media", None),
    ];

    let picked: Vec<&str> = rows_to_probe(&rows).iter().map(|r| r.share_name.as_str()).collect();

    assert_eq!(picked, ["NASPI"]);
}

#[test]
fn a_lone_share_or_one_whose_rows_all_hold_guids_is_never_probed() {
    assert!(rows_to_probe(&[row("192.168.1.111", "naspi", None)]).is_empty());
    assert!(rows_to_probe(&[row("a", "naspi", Some(BOX)), row("b", "naspi", Some(BOX))]).is_empty());
}

#[test]
fn a_zero_guid_is_probed_like_a_missing_one() {
    let rows = [
        row("a", "naspi", Some("00000000-0000-0000-0000-000000000000")),
        row("b", "naspi", Some(BOX)),
    ];
    assert_eq!(rows_to_probe(&rows).len(), 1);
}

/// ❗ **The probe never dials a host nobody connected.** A share an add named (an
/// approved Ask Cmdr suggestion, or `smb://host/share` in Add server) has no mount
/// behind it yet: it's never a target, and it doesn't make a connected twin one
/// either, since its own GUID couldn't be learned to pair them anyway.
#[test]
fn a_share_no_mount_went_through_is_never_probed_nor_counted_as_a_twin() {
    let named = KnownNetworkShare {
        address: None,
        volume_id: None,
        mount_path: None,
        pinned: false,
        ..row("nas-backup.example", "naspi", None)
    };
    let mounted = row("192.168.1.111", "naspi", None);

    assert!(rows_to_probe(std::slice::from_ref(&named)).is_empty());
    assert!(
        rows_to_probe(&[named.clone(), mounted.clone()]).is_empty(),
        "the mounted row has no connected twin"
    );
    let picked: Vec<_> = rows_to_probe(&[named, mounted, row("100.127.48.122", "naspi", Some(BOX))])
        .iter()
        .map(|r| r.address.clone())
        .collect();
    assert_eq!(picked, [Some("192.168.1.111".to_string())]);
}

/// A direct session coming up records the GUID its NEGOTIATE answered, read off
/// the session's diagnostics with no I/O. A guest session only fills, and a share
/// with no saved row keeps it in memory for the row a mount saves later.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_direct_session_records_its_server_guid() {
    let volume = cmdr_smb::volume::testing::make_docker_volume().await;
    let volume_id = "smb-integration-guid-record-test";

    record_session_guid(&volume, volume_id, "public", Some("Guest")).await;

    let live = live_share(volume_id).expect("the session's GUID is kept");
    assert_eq!(live.proof, GuidProof::Unauthenticated, "a guest session only fills");
    assert_eq!(live.share_name, "public");
    assert_eq!(live.username, None);
    let probed = cmdr_smb::connection::probe_server_guid(
        "127.0.0.1",
        cmdr_smb::volume::testing::guest_port(),
        Duration::from_secs(3),
    )
    .await
    .expect("the fixture answers");
    assert_eq!(
        Some(live.server_guid),
        ServerGuid::from_negotiated(probed),
        "the session and the probe name one server"
    );
}

/// ❗ A model's read must never dial a host: the probe and the session recorder
/// are reachable from the network wiring only. A source scan, because nothing in
/// the type system tells a tool handler from any other caller.
#[test]
fn no_tool_handler_reaches_the_probe() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for dir in ["agent", "tools", "mcp", "location"] {
        let mut stack = vec![root.join(dir)];
        while let Some(path) = stack.pop() {
            for entry in std::fs::read_dir(&path).expect("readable source dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let text = std::fs::read_to_string(&path).expect("readable source");
                    if [
                        "probe_server_guid",
                        "schedule_probe",
                        "adopt_live_guid",
                        "record_session_guid",
                    ]
                    .iter()
                    .any(|name| text.contains(name))
                    {
                        offenders.push(path.display().to_string());
                    }
                }
            }
        }
    }
    assert!(offenders.is_empty(), "these reach the GUID probe: {offenders:?}");
}

// -- Every writer onto a saved row: the row's own account decides the proof --
//
// Each test files one saved row (signed in as `david`) in the process's share
// store under an address of its own, so tests sharing a process never meet.

/// Files a saved `david` row at `address` and answers its place.
fn saved_row(address: &str) -> String {
    let row = row(address, "naspi", None);
    let place = known_shares::place_id(&row);
    known_shares::update_known_share(row);
    place
}

fn proof_of(place: &str) -> GuidProof {
    known_shares::share_by_volume_id(place)
        .expect("the saved row")
        .server_guid_proof
}

fn guid() -> ServerGuid {
    ServerGuid::parse(BOX).expect("a GUID")
}

/// A direct session coming up: signed in as the row's account it proves the GUID;
/// as another account, or a guest, it only fills.
#[test]
fn a_session_proves_a_rows_guid_only_signed_in_as_its_account() {
    let place = saved_row("10.9.0.1");
    record_negotiated_guid(&place, "naspi", guid(), Some("dori"));
    assert_eq!(proof_of(&place), GuidProof::Unauthenticated, "another account");
    record_negotiated_guid(&place, "naspi", guid(), None);
    assert_eq!(proof_of(&place), GuidProof::Unauthenticated, "a guest");
    record_negotiated_guid(&place, "naspi", guid(), Some("david"));
    assert_eq!(proof_of(&place), GuidProof::Authenticated);
    assert_eq!(
        live_share(&place).map(|live| live.proof),
        Some(GuidProof::Authenticated),
        "the live entry is the session's own, so it vouches for itself"
    );
}

/// ❗ A row saved after its live session came up adopts the session's GUID, proven
/// only when that session signed in as the row's account.
#[test]
fn an_adopted_guid_is_proven_only_for_the_rows_own_account() {
    let place = saved_row("10.9.0.2");
    let live = |username: &str| LiveShare {
        server_guid: guid(),
        proof: GuidProof::Authenticated,
        share_name: "naspi".to_string(),
        username: Some(username.to_string()),
    };

    LIVE.lock_ignore_poison().insert(place.clone(), live("dori"));
    adopt_live_guid(&place);
    assert_eq!(proof_of(&place), GuidProof::Unauthenticated);

    LIVE.lock_ignore_poison().insert(place.clone(), live("David"));
    adopt_live_guid(&place);
    assert_eq!(proof_of(&place), GuidProof::Authenticated, "the same account, folded");
}

/// The probe signs in as nobody: it fills a missing GUID and proves nothing.
#[test]
fn a_probed_guid_is_never_proven() {
    let place = saved_row("10.9.0.3");
    assert!(record_probed_guid(&place, &guid()));
    let saved = known_shares::share_by_volume_id(&place).expect("the saved row");
    assert_eq!(saved.server_guid.as_deref(), Some(BOX));
    assert_eq!(saved.server_guid_proof, GuidProof::Unauthenticated);
}
