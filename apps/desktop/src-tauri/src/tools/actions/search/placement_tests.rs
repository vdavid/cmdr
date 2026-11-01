//! Table tests for where a `search` call covers.

use cmdr_fs::volume::ConnectionState;
use serde_json::Value;

use super::*;
use crate::location::resolve::{IndexSources, VolumeFact};
use crate::tools::resources::volumes::VolumeKind;

const TAILSCALE: &str = "smb-100-127-48-122-445-naspi-david";
const LAN: &str = "smb-192-168-1-111-445-naspi-david";
const ATTIC: &str = "smb-attic-445-old-david";
const DRIVE: &str = "cloud-google-drive";
const DRIVE_ROOT: &str = "/Users/me/Library/CloudStorage/GoogleDrive";

fn volume(id: &str, root: &str, live: bool, index: IndexSources) -> VolumeFact {
    VolumeFact {
        id: id.to_string(),
        name: format!("{id} name"),
        kind: if id.starts_with("smb-") {
            VolumeKind::Smb
        } else {
            VolumeKind::Local
        },
        root: root.to_string(),
        live,
        connection_state: (!live).then_some(ConnectionState::Saved),
        earlier_roots: Vec::new(),
        index,
        share: None,
    }
}

/// The boot disk; the NAS saved over Tailscale with its index on disk; the same
/// NAS live over the LAN with no index; a saved share with no index at all; and a
/// cloud drive whose rows live in `root`'s index.
fn facts() -> VolumeFacts {
    let root_index = || IndexSources {
        live: true,
        stored: true,
        ..IndexSources::none("root")
    };
    VolumeFacts {
        volumes: vec![
            volume("root", "/", true, root_index()),
            volume(
                TAILSCALE,
                "/Volumes/naspi",
                false,
                IndexSources {
                    stored: true,
                    stored_as_of: Some(1_790_000_000),
                    ..IndexSources::none(TAILSCALE)
                },
            ),
            volume(LAN, "/Volumes/naspi-1", true, IndexSources::none(LAN)),
            volume(ATTIC, "/Volumes/attic", false, IndexSources::none(ATTIC)),
            volume(DRIVE, DRIVE_ROOT, true, root_index()),
        ],
        mount_areas: vec!["/Volumes".to_string()],
    }
}

fn scope(paths: &[&str]) -> Option<Vec<String>> {
    Some(paths.iter().map(|p| (*p).to_string()).collect())
}

fn wire(miss: Box<LocationMissResult>) -> Value {
    serde_json::to_value(miss).expect("serializes")
}

#[test]
fn a_volume_id_with_no_scope_covers_that_volumes_root() {
    let placed = place(&facts(), Some(LAN), None).expect("a live share");
    assert_eq!(placed.include_paths, scope(&["/Volumes/naspi-1"]));
    assert_eq!(placed.on_index.as_deref(), Some(LAN));
    assert_eq!(placed.offline, None);
}

/// The boot disk whole is the unscoped search it always was, so an unindexed boot
/// disk still isn't reported as a gap nobody asked about.
#[test]
fn the_boot_disk_by_id_with_no_scope_is_the_unscoped_search() {
    let placed = place(&facts(), Some("root"), None).expect("the boot disk");
    assert_eq!(placed.include_paths, None);
    assert_eq!(placed.on_index.as_deref(), Some("root"));
}

/// A cloud drive is its own volume, but `root`'s index walks it: the run reads that
/// index, scoped to the drive's root.
#[test]
fn a_cloud_drive_by_id_searches_its_root_on_the_index_that_holds_it() {
    let placed = place(&facts(), Some(DRIVE), None).expect("a live cloud drive");
    assert_eq!(placed.include_paths, scope(&[DRIVE_ROOT]));
    assert_eq!(placed.on_index.as_deref(), Some("root"));
}

#[test]
fn a_share_that_isnt_connected_answers_from_its_index_on_disk() {
    let placed = place(&facts(), Some(TAILSCALE), scope(&["/Volumes/naspi/papers"])).expect("indexed earlier");
    assert_eq!(placed.include_paths, scope(&["/Volumes/naspi/papers"]));
    assert_eq!(placed.on_index.as_deref(), Some(TAILSCALE));
    let offline = placed.offline.expect("says it's offline");
    assert_eq!(offline.as_of, Some(1_790_000_000));
    assert!(offline_note(&offline).contains("isn't connected"));
}

#[test]
fn a_share_that_isnt_connected_with_no_index_is_volume_not_connected() {
    let miss = place(&facts(), Some(ATTIC), None).expect_err("nothing can answer for it");
    let wire = wire(miss);
    assert_eq!(wire["miss"], "volumeNotConnected");
    assert_eq!(wire["location"]["volumeId"], ATTIC);
}

/// The transcript's guess, as a search scope: a dead mount point under
/// `/Volumes` once routed to the boot disk and walked a folder that isn't the NAS.
#[test]
fn a_bare_scope_under_a_dead_mount_is_no_volume_at_path() {
    let miss = place(&facts(), None, scope(&["/Volumes/naspi/papers"])).expect_err("nothing is mounted there");
    let wire = wire(miss);
    assert_eq!(wire["miss"], "noVolumeAtPath");
    assert_eq!(wire["nearby"][0]["candidate"]["volumeId"], LAN);
    assert_eq!(wire["nearby"][0]["candidate"]["path"], "/Volumes/naspi-1/papers");
}

/// Only a dead mount point is a miss: every other bare scope routes as it did, so a
/// phone's or a server's index still answers by its path, connected or not.
#[test]
fn other_bare_scopes_route_as_before() {
    for paths in [
        scope(&["/Volumes/naspi-1/papers", "/Volumes/naspi-1/photos"]),
        scope(&["/Users/me/projects"]),
        scope(&["mtp://pixel/65537/DCIM"]),
        None,
    ] {
        let placed = place(&facts(), None, paths.clone()).expect("routes by the scope");
        assert_eq!(
            placed,
            Placement {
                include_paths: paths,
                on_index: None,
                offline: None
            }
        );
    }
}

#[test]
fn a_scope_off_the_named_volume_is_path_outside_volume() {
    let miss = place(&facts(), Some(LAN), scope(&["/Volumes/naspi-1/papers", "/Users/me"]))
        .expect_err("the second path isn't on the share");
    let wire = wire(miss);
    assert_eq!(wire["miss"], "pathOutsideVolume");
    assert_eq!(wire["root"], "/Volumes/naspi-1");
}

#[test]
fn an_unknown_volume_id_is_unknown_volume() {
    let miss = place(&facts(), Some("smb-nowhere"), None).expect_err("no such volume");
    assert_eq!(wire(miss)["miss"], "unknownVolume");
}
