use cmdr_fs::volume::ConnectionState;

use super::*;
use crate::file_system::volume::SpaceInfo;
use crate::location::resolve::{IndexSources, VolumeFact};

const TAILSCALE: &str = "smb-100-127-48-122-445-naspi-david";
const LAN: &str = "smb-192-168-1-111-445-naspi-david";

fn summary(
    id: &str,
    kind: VolumeKind,
    index_status: Option<&'static str>,
    session: Option<&'static str>,
) -> VolumeSummary {
    VolumeSummary {
        name: id.to_string(),
        id: id.to_string(),
        kind,
        filesystem: None,
        read_only: None,
        ejectable: None,
        index_status,
        connection_state: session,
        device_readiness: None,
        mount_path: Some(format!("/Volumes/{id}")),
        space: None,
    }
}

fn fact(id: &str, kind: VolumeKind, root: &str, live: bool) -> VolumeFact {
    VolumeFact {
        id: id.to_string(),
        name: id.to_string(),
        kind,
        root: root.to_string(),
        live,
        connection_state: (!live).then_some(ConnectionState::Saved),
        earlier_roots: Vec::new(),
        index: IndexSources::none(id),
        share: None,
    }
}

fn facts(volumes: Vec<VolumeFact>) -> VolumeFacts {
    VolumeFacts {
        volumes,
        mount_areas: vec!["/Volumes".to_string()],
    }
}

fn favorite(name: &str, path: &str) -> FavoriteSummary {
    FavoriteSummary {
        id: name.to_lowercase(),
        name: name.to_string(),
        path: path.to_string(),
        volume_id: None,
        volume_name: None,
        reach: None,
    }
}

fn map(summaries: &[VolumeSummary], facts: &VolumeFacts) -> VolumeMap {
    to_volume_map(summaries, &[], facts, &[])
}

fn wire<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("serializes")
}

#[test]
fn tokens_pass_through_verbatim_including_off_and_disconnected() {
    // The honesty tokens must survive the mapping unchanged: a stale, disconnected
    // SMB share and an unindexed (`off`) local disk both read honestly.
    let out = map(
        &[
            summary("nas", VolumeKind::Smb, Some("stale"), Some("disconnected")),
            summary("scratch", VolumeKind::Local, Some("off"), None),
        ],
        &facts(vec![]),
    )
    .volumes;
    assert_eq!(out[0].kind, "smb");
    assert_eq!(out[0].index_status.as_deref(), Some("stale"));
    assert_eq!(out[0].connection_state.as_deref(), Some("disconnected"));
    assert_eq!(out[1].kind, "local");
    assert_eq!(out[1].index_status.as_deref(), Some("off"));
    assert_eq!(out[1].connection_state, None);
}

#[test]
fn space_carries_a_human_form_beside_the_bytes() {
    // The agent can't divide by 1,024 reliably, so "how full is it" has to arrive
    // already spelled out; the raw bytes stay for anything that needs arithmetic.
    let mut v = summary("root", VolumeKind::Local, Some("fresh"), None);
    v.space = Some(SpaceInfo::bounded(2_000_000_000_000, 214_300_000_000));
    let out = map(&[v], &facts(vec![])).volumes;
    assert_eq!(out[0].total_bytes, Some(2_000_000_000_000));
    assert_eq!(out[0].total_human, Some(format_size(2_000_000_000_000)));
    assert_eq!(out[0].available_human, Some(format_size(214_300_000_000)));
}

#[test]
fn an_unwatched_volume_omits_the_human_form_too() {
    // Present exactly when the byte counterparts are: "0 B free" would read as a
    // full disk.
    let out = map(
        &[summary("backup", VolumeKind::Local, Some("off"), None)],
        &facts(vec![]),
    )
    .volumes;
    assert_eq!(out[0].total_bytes, None);
    assert_eq!(out[0].total_human, None);
    assert_eq!(out[0].available_human, None);
}

/// ❗ The row that sent the model to `/Volumes/naspi/...` carried `mountPath`, the
/// saved share's last mount point, read as "where it is". `root` is the prefix its
/// locations start with, and `connectionState` says it isn't connected.
#[test]
fn a_saved_share_carries_its_root_and_no_mount_path() {
    let row = summary(TAILSCALE, VolumeKind::Smb, Some("off"), Some("saved"));
    let json = wire(
        &map(
            &[row],
            &facts(vec![fact(TAILSCALE, VolumeKind::Smb, "/Volumes/naspi", false)]),
        )
        .volumes[0],
    );
    assert_eq!(json["root"], "/Volumes/naspi");
    assert_eq!(json["connectionState"], "saved");
    assert!(
        json.get("mountPath").is_none(),
        "mountPath is cmdr://state's, not the agent's"
    );
}

/// The NAS is answerable while it's asleep, and says how old the answer is: the
/// same `stale` + `asOf` `list_dir` voices for an offline read.
#[test]
fn a_saved_share_with_an_index_on_disk_reads_stale_as_of_its_last_scan() {
    let mut tailscale = fact(TAILSCALE, VolumeKind::Smb, "/Volumes/naspi", false);
    tailscale.index.stored = true;
    tailscale.index.stored_as_of = Some(1_761_000_000); // 2025-10-20T22:40Z
    let row = summary(TAILSCALE, VolumeKind::Smb, Some("off"), Some("saved"));

    let json = wire(&map(&[row], &facts(vec![tailscale])).volumes[0]);
    assert_eq!(json["indexStatus"], "stale");
    assert_eq!(json["asOf"], "2025-10-20");
}

/// With no index on disk, nothing changes: `off`, no date to be as of.
#[test]
fn a_saved_share_with_no_index_stays_off() {
    let row = summary(TAILSCALE, VolumeKind::Smb, Some("off"), Some("saved"));
    let json = wire(
        &map(
            &[row],
            &facts(vec![fact(TAILSCALE, VolumeKind::Smb, "/Volumes/naspi", false)]),
        )
        .volumes[0],
    );
    assert_eq!(json["indexStatus"], "off");
    assert!(json.get("asOf").is_none());
}

/// A live volume's status is its own index's, even with an old database beside
/// it: `list_dir` reads it live, so an offline date would describe nothing.
#[test]
fn a_live_volume_never_borrows_an_offline_date() {
    let mut lan = fact(LAN, VolumeKind::Smb, "/Volumes/naspi-1", true);
    lan.index.stored_as_of = Some(1_761_000_000);
    let row = summary(LAN, VolumeKind::Smb, Some("off"), Some("direct"));
    let json = wire(&map(&[row], &facts(vec![lan])).volumes[0]);
    assert_eq!(json["indexStatus"], "off");
    assert!(json.get("asOf").is_none());
}

#[test]
fn a_server_row_has_its_scheme_root() {
    let root = cmdr_fs::volume::sftp_app_root("nas.local", 22, "ada");
    let id = cmdr_fs::volume::sftp_volume_id("nas.local", 22, "ada");
    let mut row = summary(&id, VolumeKind::Sftp, Some("off"), Some("saved"));
    row.mount_path = None;
    let json = wire(&map(&[row], &facts(vec![fact(&id, VolumeKind::Sftp, &root, false)])).volumes[0]);
    assert_eq!(json["root"], root.as_str());
}

/// The hub row is a navigation target with no locations, so it has no root to
/// offer, and an absent field says so.
#[test]
fn a_row_the_resolver_doesnt_know_has_no_root() {
    let mut hub = summary("network", VolumeKind::Virtual, None, None);
    hub.mount_path = None;
    assert!(wire(&map(&[hub], &facts(vec![])).volumes[0]).get("root").is_none());
}

/// ❗ The phone favorite (`mtp://pixel/Internal storage`) rode `volumes` as a
/// local disk. A favorite is a folder, so it's listed as one, and a favorite
/// nothing connected holds says so instead of handing over a location that would
/// miss.
#[test]
fn an_unreachable_favorite_keeps_its_raw_path_and_no_location() {
    let out = to_volume_map(
        &[],
        &[favorite("Internal storage", "mtp://pixel/Internal storage")],
        &facts(vec![fact("root", VolumeKind::Local, "/", true)]),
        &[],
    );
    let json = wire(&out.favorites[0]);
    assert_eq!(json["path"], "mtp://pixel/Internal storage");
    assert!(json.get("location").is_none());
    assert_eq!(json["reachable"], false);
    assert!(out.volumes.is_empty(), "a favorite is never a volume row");
}

#[test]
fn a_reachable_favorite_carries_its_location() {
    let out = to_volume_map(
        &[],
        &[favorite("Downloads", "/Users/ada/Downloads")],
        &facts(vec![fact("root", VolumeKind::Local, "/", true)]),
        &[],
    );
    let json = wire(&out.favorites[0]);
    assert_eq!(json["location"]["volumeId"], "root");
    assert_eq!(json["location"]["path"], "/Users/ada/Downloads");
    assert!(
        json.get("reachable").is_none(),
        "reachable is said only when it's false"
    );
}

/// Rows of one share sit together, most recently connected first, so the
/// address that worked last is the first one a model meets.
#[test]
fn rows_of_one_share_sit_together_most_recently_connected_first() {
    let rows = [
        summary(TAILSCALE, VolumeKind::Smb, Some("off"), Some("saved")),
        summary("root", VolumeKind::Local, Some("fresh"), None),
        summary(LAN, VolumeKind::Smb, Some("off"), Some("direct")),
        summary("smb-other", VolumeKind::Smb, Some("off"), Some("saved")),
    ];
    let share = |id: &str, name: &str, at: &str| ShareRecency {
        volume_id: id.to_string(),
        share_name: name.to_string(),
        last_connected_at: at.to_string(),
    };
    let shares = [
        share(TAILSCALE, "naspi", "2026-10-20T08:00:00Z"),
        share(LAN, "NASPI", "2026-10-25T19:00:00Z"),
        share("smb-other", "photos", "2026-10-26T07:00:00Z"),
    ];
    let ids: Vec<String> = to_volume_map(&rows, &[], &facts(vec![]), &shares)
        .volumes
        .into_iter()
        .map(|v| v.id)
        .collect();
    assert_eq!(ids, [LAN, TAILSCALE, "root", "smb-other"]);
}

#[test]
fn serializes_camel_case_and_omits_absent_fields() {
    let json = wire(
        &map(
            &[summary("root", VolumeKind::Local, Some("fresh"), None)],
            &facts(vec![]),
        )
        .volumes[0],
    );
    assert_eq!(json["indexStatus"], "fresh");
    assert_eq!(json["kind"], "local");
    // Absent optionals don't clutter the payload the model reads.
    assert!(json.get("connectionState").is_none());
    assert!(json.get("filesystem").is_none());
    assert!(json.get("asOf").is_none());
}

/// One NAS by two addresses, proven by its server GUID and share name, says so on
/// both rows; a row nothing proves a sibling for carries no `sameShareAs`.
#[test]
fn rows_of_one_proven_share_name_each_other() {
    use crate::location::share_identity::ShareIdentity;
    let identity = || {
        Some(ShareIdentity::new(
            Some((
                "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
                crate::network::server_guid::GuidProof::Authenticated,
            )),
            "naspi",
            None,
        ))
    };
    let mut ts = fact(TAILSCALE, VolumeKind::Smb, "/Volumes/naspi", false);
    ts.share = identity();
    let mut lan = fact(LAN, VolumeKind::Smb, "/Volumes/naspi-1", true);
    lan.share = identity();
    let out = wire(&map(
        &[
            summary(LAN, VolumeKind::Smb, Some("off"), Some("direct")),
            summary(TAILSCALE, VolumeKind::Smb, Some("off"), Some("saved")),
            summary("root", VolumeKind::Local, Some("fresh"), None),
        ],
        &facts(vec![lan, ts, fact("root", VolumeKind::Local, "/", true)]),
    ));

    assert_eq!(out["volumes"][0]["sameShareAs"], serde_json::json!([TAILSCALE]));
    assert_eq!(out["volumes"][1]["sameShareAs"], serde_json::json!([LAN]));
    assert!(out["volumes"][2].get("sameShareAs").is_none());
}
