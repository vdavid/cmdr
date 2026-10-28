//! `list_dir`'s answer from a live read: the coverage it voices, the folder sizes
//! it can't state, the entry cap, and a whole read through a real local volume.

use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::Volume;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::live::{GatePolicy, LIVE_ENTRY_CAP, LIVE_READ, build_live_list_dir};
use super::*;
use crate::location::resolve::{IndexSources, VolumeFact, VolumeFacts, resolve};
use crate::mcp::resources::volumes::VolumeKind;
use crate::test_support::TestDir;

fn entry(name: &str, is_directory: bool, size: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        modified_at: Some(1_700_000_000),
        ..FileEntry::new(name.to_string(), format!("/x/{name}"), is_directory, false)
    }
}

/// An unindexed share, as `list_volumes` would call it: `off`.
fn unindexed() -> VolumeView {
    VolumeView {
        enabled: false,
        freshness: None,
        block: VolumeBlock::new("lan".to_string(), None),
        walked_roots: Vec::new(),
        reach: IndexReach::Connected,
    }
}

fn at(path: &str) -> Location {
    Location {
        volume_id: "lan".to_string(),
        path: path.to_string(),
    }
}

fn wire(result: &ListDirResult) -> Value {
    serde_json::to_value(result).expect("serializes")
}

#[test]
fn a_live_answer_says_where_it_came_from_and_states_no_folder_size() {
    let entries = vec![
        entry("2024", true, Some(4_096)),
        entry("receipt.pdf", false, Some(52_000)),
    ];
    let result = build_live_list_dir(
        &at("/Volumes/naspi-1/papers"),
        entries,
        &unindexed(),
        &ListOptions::default(),
    );
    let wire = wire(&result);

    assert_eq!(wire["coverage"]["source"], "live");
    assert_eq!(wire["coverage"]["indexStatus"], "off");
    assert_eq!(
        wire["coverage"]["authoritative"], true,
        "a complete readdir is the folder now"
    );
    assert!(
        wire["coverage"]["note"]
            .as_str()
            .is_some_and(|n| n.contains("Folder sizes"))
    );
    // The folder's own recursive total, and every child folder's, are unknown: absent,
    // never the inode's 4 KB and never a "0 B".
    assert!(wire.get("size").is_none());
    let children = wire["children"].as_array().expect("children");
    assert_eq!(children[0]["name"], "2024");
    assert!(children[0].get("size").is_none() && children[0].get("sizeHuman").is_none());
    assert_eq!(children[1]["size"], 52_000);
    assert_eq!(wire["total"], 2);
    assert!(wire.get("remainder").is_none() && wire.get("totalIsLowerBound").is_none());
}

#[test]
fn a_folder_past_the_cap_keeps_the_first_ten_thousand_and_says_the_total_is_a_floor() {
    let entries: Vec<FileEntry> = (0..=LIVE_ENTRY_CAP)
        .map(|i| entry(&format!("f{i:05}"), false, Some(1)))
        .collect();
    let opts = ListOptions {
        sort_by: SortBy::Name,
        order: Order::Desc,
        ..ListOptions::default()
    };
    let result = build_live_list_dir(&at("/Volumes/naspi-1/scans"), entries, &unindexed(), &opts);
    let wire = wire(&result);

    assert_eq!(wire["total"], LIVE_ENTRY_CAP);
    assert_eq!(wire["totalIsLowerBound"], true);
    assert_eq!(wire["truncated"], true);
    assert_eq!(wire["coverage"]["authoritative"], false);
    // Sorted within the cap: the 10,001st entry the provider listed is gone, so the
    // name-descending page starts at the last one kept.
    assert_eq!(wire["children"][0]["name"], format!("f{:05}", LIVE_ENTRY_CAP - 1));
}

fn facts_for(root: &str) -> VolumeFacts {
    VolumeFacts {
        volumes: vec![VolumeFact {
            id: "lan".to_string(),
            name: "naspi on naspolya".to_string(),
            kind: VolumeKind::Smb,
            root: root.to_string(),
            live: true,
            connection_state: None,
            earlier_roots: Vec::new(),
            index: IndexSources::none("lan"),
            share: None,
        }],
        mount_areas: Vec::new(),
    }
}

fn open_policy() -> GatePolicy {
    GatePolicy {
        fda_pending: false,
        is_protected: |_| false,
        denial_is_privacy: |_| false,
    }
}

#[tokio::test]
async fn an_offset_past_the_cap_is_refused_before_anything_is_read() {
    let facts = facts_for("/Volumes/naspi-1");
    let resolved = resolve(
        &facts,
        &LocationInput {
            volume_id: Some("lan".to_string()),
            path: "/Volumes/naspi-1/scans".to_string(),
        },
    )
    .expect("resolves");
    let opts = ListOptions {
        offset: LIVE_ENTRY_CAP,
        ..ListOptions::default()
    };
    let problem = answer_live(LiveAsk {
        resolved: &resolved,
        facts: &facts,
        opts: &opts,
        view: &unindexed(),
        // Nothing to read with: a read reaching the volume would fail differently.
        volume: None,
        cancel: &CancellationToken::new(),
        cfg: LIVE_READ,
        policy: open_policy(),
    })
    .await
    .expect_err("refused");
    assert!(problem.message.contains("subfolder"), "the refusal names the next move");
}

#[tokio::test]
async fn a_live_read_lists_a_real_folder_through_its_volume() {
    let dir = TestDir::new("live-real-read");
    let folder = dir.join("papers");
    std::fs::create_dir_all(folder.join("2024")).expect("dirs");
    std::fs::write(folder.join("receipt.pdf"), b"%PDF").expect("file");
    let root = dir.to_string_lossy().into_owned();
    let facts = facts_for(&root);
    let resolved = resolve(
        &facts,
        &LocationInput {
            volume_id: Some("lan".to_string()),
            path: folder.to_string_lossy().into_owned(),
        },
    )
    .expect("resolves");
    let volume: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "naspi",
        &*dir,
        crate::volume_host::host(),
    ));
    let answer = answer_live(LiveAsk {
        resolved: &resolved,
        facts: &facts,
        opts: &ListOptions::default(),
        view: &unindexed(),
        volume: Some(volume),
        cancel: &CancellationToken::new(),
        cfg: LIVE_READ,
        policy: open_policy(),
    })
    .await
    .expect("answers");

    assert_eq!(answer["coverage"]["source"], "live");
    let names: Vec<&str> = answer["children"]
        .as_array()
        .expect("children")
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert_eq!(names, ["2024", "receipt.pdf"]);
    assert_eq!(
        answer["location"],
        json!({ "volumeId": "lan", "path": folder.to_string_lossy() })
    );
}

#[tokio::test]
async fn a_missing_folder_read_live_is_a_typed_miss_not_an_empty_listing() {
    let dir = TestDir::new("live-real-missing");
    let root = dir.to_string_lossy().into_owned();
    let facts = facts_for(&root);
    let path = format!("{root}/gone");
    let resolved = resolve(
        &facts,
        &LocationInput {
            volume_id: Some("lan".to_string()),
            path: path.clone(),
        },
    )
    .expect("resolves");
    let volume: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "naspi",
        &*dir,
        crate::volume_host::host(),
    ));
    let answer = answer_live(LiveAsk {
        resolved: &resolved,
        facts: &facts,
        opts: &ListOptions::default(),
        view: &unindexed(),
        volume: Some(volume),
        cancel: &CancellationToken::new(),
        cfg: LIVE_READ,
        policy: open_policy(),
    })
    .await
    .expect("answers");
    assert_eq!(answer["miss"], "notFound");
    assert_eq!(answer["location"], json!({ "volumeId": "lan", "path": path }));
    assert!(answer.get("children").is_none());
}

async fn dropped_read_detail(connection_state: Option<cmdr_fs::volume::ConnectionState>) -> String {
    let mut facts = facts_for("/Volumes/naspi-1");
    facts.volumes[0].connection_state = connection_state;
    let resolved = resolve(
        &facts,
        &LocationInput {
            volume_id: Some("lan".to_string()),
            path: "/Volumes/naspi-1/papers".to_string(),
        },
    )
    .expect("resolves");
    let answer = answer_live(LiveAsk {
        resolved: &resolved,
        facts: &facts,
        opts: &ListOptions::default(),
        view: &unindexed(),
        // The volume left between the resolve and the read: the dropped-session path.
        volume: None,
        cancel: &CancellationToken::new(),
        cfg: LIVE_READ,
        policy: open_policy(),
    })
    .await
    .expect("answers");
    assert_eq!(answer["miss"], "unreachable");
    answer["detail"].as_str().expect("detail").to_string()
}

/// A session Cmdr owns that drops mid-read points the model at `connect_volume`, which waits
/// for the reconnect, instead of at the user. Regression anchor: a NAS whose smb2 session had
/// gone stale answered one read with "Tell the user it may have disconnected", so the agent
/// wrote it off for the whole answer while Cmdr reconnected it nine seconds later.
#[tokio::test]
async fn a_dropped_session_points_at_connect_volume_and_an_os_mount_at_the_user() {
    use cmdr_fs::volume::ConnectionState;
    for state in [ConnectionState::Direct, ConnectionState::Disconnected] {
        let detail = dropped_read_detail(Some(state)).await;
        assert!(
            detail.contains("`connect_volume` with volumeId `lan`"),
            "{state:?}: {detail}"
        );
    }
    for state in [Some(ConnectionState::OsMount), None] {
        let detail = dropped_read_detail(state).await;
        assert!(!detail.contains("connect_volume"), "{state:?}: {detail}");
    }
}
