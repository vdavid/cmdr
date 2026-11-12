//! Table tests for the pure resolver: one volume set, every resolution rule.

use super::*;
use crate::location::facts::RootHistory;

const TAILSCALE: &str = "smb-100-64-0-10-445-testshare-test_user_3";
const LAN: &str = "smb-192-168-50-10-445-testshare-test_user_3";

fn volume(id: &str, name: &str, kind: VolumeKind, root: &str, live: bool) -> VolumeFact {
    VolumeFact {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        root: root.to_string(),
        live,
        connection_state: match (kind, live) {
            (VolumeKind::Local, _) => None,
            (_, true) => Some(ConnectionState::OsMount),
            (_, false) => Some(ConnectionState::Saved),
        },
        earlier_roots: Vec::new(),
        index: IndexSources::none(id),
        share: None,
    }
}

/// The boot disk, the NAS saved over Tailscale at its last root `/Volumes/testshare`,
/// and the same NAS live over the LAN at `/Volumes/testshare-1`.
fn facts() -> VolumeFacts {
    VolumeFacts {
        volumes: vec![
            volume("root", "Macintosh HD", VolumeKind::Local, "/", true),
            volume(
                TAILSCALE,
                "testshare on 100.64.0.10",
                VolumeKind::Smb,
                "/Volumes/testshare",
                false,
            ),
            volume(
                LAN,
                "testshare on testnas",
                VolumeKind::Smb,
                "/Volumes/testshare-1",
                true,
            ),
        ],
        mount_areas: vec!["/Volumes".to_string()],
    }
}

fn bare(path: &str) -> LocationInput {
    LocationInput {
        volume_id: None,
        path: path.to_string(),
    }
}

fn named(id: &str, path: &str) -> LocationInput {
    LocationInput {
        volume_id: Some(id.to_string()),
        path: path.to_string(),
    }
}

#[test]
fn a_path_on_a_live_share_resolves_to_that_share() {
    let resolved = resolve(&facts(), &bare("/Volumes/testshare-1/documents")).expect("the LAN share holds it");
    assert_eq!(resolved.location.volume_id, LAN);
    assert_eq!(resolved.location.path, "/Volumes/testshare-1/documents");
    assert!(resolved.live);
    assert_eq!(resolved.rebased_from, None);
}

/// ❗ The transcript's guess: a path under the Tailscale share's DEAD mount point.
/// It once routed to `root` and read as an authoritative, fresh empty answer.
#[test]
fn a_bare_path_under_a_dead_mount_point_is_no_volume_at_path_with_candidates() {
    let miss = resolve(&facts(), &bare("/Volumes/testshare/documents")).expect_err("nothing live is mounted there");
    let LocationMiss::NoVolumeAtPath { nearby, requested } = miss else {
        panic!("expected noVolumeAtPath, got {miss:?}");
    };
    assert_eq!(requested.path, "/Volumes/testshare/documents");
    let ids: Vec<&str> = nearby.iter().map(|n| n.volume_id.as_str()).collect();
    assert_eq!(ids, [LAN, TAILSCALE], "both shares, the live one first");
    let lan = &nearby[0];
    assert_eq!(lan.candidate.volume_id, LAN);
    assert_eq!(
        lan.candidate.path, "/Volumes/testshare-1/documents",
        "the suffix rides onto its root"
    );
    assert_eq!(lan.connection_state, Some("os_mount"));
    assert_eq!(nearby[1].candidate.path, "/Volumes/testshare/documents");
    assert!(
        !nearby.iter().any(|n| n.volume_id == "root"),
        "the boot disk doesn't hold a mount area"
    );
}

#[test]
fn a_named_saved_share_resolves_not_live_on_that_share() {
    let resolved = resolve(&facts(), &named(TAILSCALE, "/Volumes/testshare/documents")).expect("named volumes resolve");
    assert_eq!(resolved.location.volume_id, TAILSCALE);
    assert_eq!(resolved.location.path, "/Volumes/testshare/documents");
    assert!(!resolved.live, "whether it can answer is the tool's call");
}

/// The earlier root comes from the facts shell's mount-event record: the share
/// arrived at `/Volumes/testshare-1`, then again at `/Volumes/testshare-2`.
#[test]
fn a_named_live_volume_whose_root_moved_rebases_the_path() {
    let history = RootHistory::default();
    history.record(LAN, "/Volumes/testshare-1");
    history.record(LAN, "/Volumes/testshare-2");
    let mut facts = facts();
    let lan = facts.volumes.iter_mut().find(|v| v.id == LAN).expect("the LAN share");
    lan.root = "/Volumes/testshare-2".to_string();
    lan.earlier_roots = history.earlier_roots(LAN, "/Volumes/testshare-2");

    let resolved = resolve(&facts, &named(LAN, "/Volumes/testshare-1/documents/invoices")).expect("rebased");
    assert_eq!(resolved.location.path, "/Volumes/testshare-2/documents/invoices");
    assert_eq!(
        resolved.rebased_from.as_deref(),
        Some("/Volumes/testshare-1/documents/invoices")
    );
}

#[test]
fn a_named_volume_with_a_path_under_none_of_its_roots_is_path_outside_volume() {
    let miss = resolve(&facts(), &named(LAN, "/Volumes/photos/2024")).expect_err("not on the share");
    assert!(
        matches!(&miss, LocationMiss::PathOutsideVolume { root, .. } if root == "/Volumes/testshare-1"),
        "{miss:?}"
    );
}

#[test]
fn a_home_relative_path_resolves_to_root() {
    let resolved = resolve(&facts(), &bare("~/Downloads")).expect("the boot disk holds home");
    assert_eq!(resolved.location.volume_id, "root");
    assert_eq!(resolved.location.path, cmdr_fs::tilde::expand_tilde("~/Downloads"));
    assert!(resolved.location.path.ends_with("/Downloads") && !resolved.location.path.starts_with('~'));
}

#[test]
fn an_unknown_volume_id_is_unknown_volume() {
    let miss = resolve(&facts(), &named("smb-nope", "/Volumes/nope")).expect_err("no such volume");
    assert!(matches!(miss, LocationMiss::UnknownVolume { .. }), "{miss:?}");
}

/// ❗ A saved share's last root is now a DIFFERENT volume's live mount. The bare
/// path is the live volume's; only the saved share's id reaches the saved share.
#[test]
fn a_reused_mount_point_resolves_bare_to_the_live_volume_and_named_to_the_saved_one() {
    let mut facts = facts();
    facts.volumes.push(volume(
        "smb-10-0-0-9-445-photos-test_user_1",
        "photos on studio",
        VolumeKind::Smb,
        "/Volumes/testshare",
        true,
    ));

    let bare = resolve(&facts, &bare("/Volumes/testshare/x")).expect("the live mount holds it");
    assert_eq!(bare.location.volume_id, "smb-10-0-0-9-445-photos-test_user_1");
    assert!(bare.live);

    let named = resolve(&facts, &named(TAILSCALE, "/Volumes/testshare/x")).expect("named volumes resolve");
    assert_eq!(named.location.volume_id, TAILSCALE);
    assert!(!named.live, "never the live volume that took its mount point");
}

#[test]
fn the_deepest_live_root_wins() {
    let mut facts = facts();
    facts.volumes.push(volume(
        "nested",
        "inner",
        VolumeKind::Local,
        "/Volumes/testshare-1/inner",
        true,
    ));
    let resolved = resolve(&facts, &bare("/Volumes/testshare-1/inner/a")).expect("resolves");
    assert_eq!(resolved.location.volume_id, "nested");
}

#[test]
fn a_mount_stem_drops_only_a_numeric_suffix() {
    assert_eq!(mount_stem("testshare-1"), "testshare");
    assert_eq!(mount_stem("testshare-12"), "testshare");
    assert_eq!(mount_stem("testshare"), "testshare");
    assert_eq!(mount_stem("my-share"), "my-share");
    assert_eq!(mount_stem("-1"), "-1");
}

#[test]
fn the_root_function_prefers_live_then_index_then_saved() {
    let all = RootSources {
        live: Some("/Volumes/testshare-1"),
        index_meta: Some("/Volumes/testshare-2"),
        saved: Some("/Volumes/testshare"),
    };
    assert_eq!(volume_root(all).as_deref(), Some("/Volumes/testshare-1"));
    assert_eq!(
        volume_root(RootSources { live: None, ..all }).as_deref(),
        Some("/Volumes/testshare-2")
    );
    assert_eq!(
        volume_root(RootSources {
            saved: Some("/Volumes/testshare"),
            ..RootSources::default()
        })
        .as_deref(),
        Some("/Volumes/testshare")
    );
    assert_eq!(volume_root(RootSources::default()), None);
}

#[test]
fn location_input_reads_the_flat_pair_and_drops_an_empty_volume_id() {
    let params = serde_json::json!({ "path": "/x", "volumeId": "" });
    assert_eq!(LocationInput::from_params(&params), Some(bare("/x")));
    let params = serde_json::json!({ "path": "/x", "volumeId": "root" });
    assert_eq!(LocationInput::from_params(&params), Some(named("root", "/x")));
    assert_eq!(LocationInput::from_params(&serde_json::json!({ "path": "" })), None);
}
