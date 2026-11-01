//! Which volume each `inspect_file` row is on, and the rows no volume holds.

use cmdr_fs::volume::ConnectionState;
use serde_json::Value;

use super::*;
use crate::location::resolve::{IndexSources, VolumeFact, VolumeFacts};
use crate::tools::read::inspect::shape_ok;
use crate::tools::resources::volumes::VolumeKind;

const TAILSCALE: &str = "smb-100-127-48-122-445-naspi-david";
const LAN: &str = "smb-192-168-1-111-445-naspi-david";

fn share(id: &str, root: &str, live: bool) -> VolumeFact {
    VolumeFact {
        id: id.to_string(),
        name: format!("naspi ({id})"),
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
        share: None,
    }
}

/// The NAS saved over Tailscale (last root `/Volumes/naspi`) and live over the LAN.
fn facts() -> VolumeFacts {
    VolumeFacts {
        volumes: vec![
            VolumeFact {
                id: "root".to_string(),
                name: "Macintosh HD".to_string(),
                kind: VolumeKind::Local,
                root: "/".to_string(),
                live: true,
                connection_state: None,
                earlier_roots: Vec::new(),
                index: IndexSources::none("root"),
                share: None,
            },
            share(TAILSCALE, "/Volumes/naspi", false),
            share(LAN, "/Volumes/naspi-1", true),
        ],
        mount_areas: vec!["/Volumes".to_string()],
    }
}

/// The volume registry's answer: the LAN share's root, else the boot disk.
fn owner(path: &str) -> Option<String> {
    Some(
        if path.starts_with("/Volumes/naspi-1/") {
            LAN
        } else {
            "root"
        }
        .to_string(),
    )
}

fn missing(path: &str) -> Option<FileRow> {
    Some(FileRow::Missing { path: path.to_string() })
}

fn wire(paths: &[&str], placed: PlacedRows) -> Value {
    let paths: Vec<String> = paths.iter().map(|p| (*p).to_string()).collect();
    serde_json::to_value(shape_ok(&paths, placed)).expect("serializes")
}

/// The transcript's guess as a file: under a dead mount point it once read
/// `missing`, as if the NAS had lost the file.
#[test]
fn a_file_under_a_dead_mount_point_is_no_volume_at_path_with_nearby_once() {
    let paths = [
        "/Volumes/naspi/papers/a.pdf",
        "/Volumes/naspi/papers/b.pdf",
        "/Users/me/gone.txt",
    ];
    let rows = paths.iter().map(|p| missing(p)).collect();
    let json = wire(&paths, place_rows(rows, Some(&facts()), &owner));

    assert_eq!(json["files"][0]["status"], "noVolumeAtPath");
    assert_eq!(json["files"][1]["status"], "noVolumeAtPath");
    assert!(json["files"][0].get("volumeId").is_none(), "no volume holds it");
    assert_eq!(json["files"][2]["status"], "missing", "a boot-disk miss stays missing");
    assert_eq!(json["files"][2]["volumeId"], "root");
    let nearby = json["nearby"].as_array().expect("nearby, once for the call");
    assert_eq!(nearby[0]["candidate"]["volumeId"], LAN);
    assert_eq!(nearby[0]["candidate"]["path"], "/Volumes/naspi-1/papers/a.pdf");
}

#[test]
fn every_row_echoes_the_volume_it_is_on() {
    let paths = ["/Volumes/naspi-1/papers/a.pdf", "/Users/me/notes"];
    let rows = vec![
        Some(FileRow::Folder {
            path: paths[0].to_string(),
        }),
        Some(FileRow::Unreachable {
            path: paths[1].to_string(),
        }),
    ];
    let json = wire(&paths, place_rows(rows, None, &owner));
    assert_eq!(json["files"][0]["volumeId"], LAN);
    assert_eq!(json["files"][0]["status"], "folder");
    assert_eq!(json["files"][1]["volumeId"], "root");
    assert!(json.get("nearby").is_none(), "nothing missed a volume");
}

/// The hot path pays nothing: with no `missing` row there are no facts to read,
/// and a `missing` row with none to classify against stays `missing`.
#[test]
fn without_facts_a_missing_row_stays_missing() {
    let paths = ["/Volumes/naspi/papers/a.pdf"];
    let json = wire(&paths, place_rows(vec![missing(paths[0])], None, &owner));
    assert_eq!(json["files"][0]["status"], "missing");
}
