use super::*;
use crate::volume_listing::SERVERS_VOLUME_NAME;

fn local(name: &str, id: &str) -> VolumeSummary {
    VolumeSummary {
        name: name.to_string(),
        id: id.to_string(),
        kind: VolumeKind::Local,
        filesystem: Some("apfs".to_string()),
        read_only: Some(false),
        ejectable: Some(false),
        index_status: Some("fresh"),
        connection_state: None,
        device_readiness: None,
        mount_path: Some("/".to_string()),
        space: None,
    }
}

/// An OS-mounted NFS, AFP, or FTP share is a network drive with no dedicated
/// backend, and it must read as one: pre-fix it fell through to `local`, telling
/// an agent a share that can hang for minutes was a plain disk.
#[cfg(target_os = "macos")]
#[test]
fn a_network_mount_without_a_backend_reads_as_network() {
    for fs in ["nfs", "afpfs", "ftp"] {
        assert_eq!(kind_for_location(Some(fs), false), VolumeKind::Network, "{fs}");
    }
    assert_eq!(kind_for_location(Some("apfs"), false), VolumeKind::Local);
    assert_eq!(kind_for_location(Some("smbfs"), false), VolumeKind::Smb);
    assert_eq!(kind_for_location(Some("webdav"), false), VolumeKind::Webdav);
    // ❗ Before the session arm: an S3 place has a session, and isn't SMB.
    assert_eq!(kind_for_location(Some("s3"), true), VolumeKind::S3);
}

/// ❗ **A saved share's last mount path is not where its status lives.** With
/// nothing mounted there, the path routes to `root`, and the row read `root`'s
/// `fresh`: the model trusted it and went listing a share that wasn't there.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_saved_share_never_reads_roots_freshness_off_its_dead_mount_path() {
    use cmdr_index::IndexVolumeKind;
    use cmdr_index::testing::reserve_initializing_index_for_test;

    // A live `root` index reads `fresh`, which is what leaked through.
    let _root = reserve_initializing_index_for_test(cmdr_index::ROOT_VOLUME_ID, IndexVolumeKind::Local);
    assert_eq!(
        index_status_token(&index().volume_status(cmdr_index::ROOT_VOLUME_ID)),
        "fresh",
        "precondition: root reads fresh"
    );

    let saved = crate::volume_listing::LocationInfo {
        id: "smb-192-0-2-52-445-share-ada".to_string(),
        name: "share on nas".to_string(),
        path: "/Volumes/cmdr-test-never-mounted-share".to_string(),
        category: crate::volumes::LocationCategory::Network,
        icon: None,
        is_ejectable: false,
        fs_type: Some("smbfs".to_string()),
        supports_trash: false,
        mount_is_read_only: false,
        is_disk_image: false,
        shares_disk_with_others: false,
        is_cloud_mount: false,
        connection_state: Some(cmdr_fs::volume::ConnectionState::Saved),
        pinned: Some(true),
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        root_label: None,
        mount_account: None,
    };
    let volumes = snapshot_volumes_from(std::slice::from_ref(&saved)).await;
    let _ = index().forget_volume(cmdr_index::ROOT_VOLUME_ID);

    let row = volumes.iter().find(|v| v.id == saved.id).expect("the saved row");
    assert_eq!(row.index_status, Some("off"), "its own id has no live index");
}

/// ❗ **A favorite is a folder, not a volume.** It rode `volumes:` as
/// `kind: local`, so an AI client read every favorite twice (here and in
/// `favorites:`) and the phone one as a local disk. Leaving changes nothing
/// else: every other row renders byte for byte as before.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_favorite_never_appears_in_volumes_and_every_other_row_renders_as_before() {
    let row = |id: &str, name: &str, path: &str, category| crate::volume_listing::LocationInfo {
        id: id.to_string(),
        name: name.to_string(),
        path: path.to_string(),
        category,
        icon: None,
        is_ejectable: false,
        fs_type: Some("smbfs".to_string()),
        supports_trash: false,
        mount_is_read_only: false,
        is_disk_image: false,
        shares_disk_with_others: false,
        is_cloud_mount: false,
        connection_state: Some(cmdr_fs::volume::ConnectionState::Saved),
        pinned: Some(true),
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        root_label: None,
        mount_account: None,
    };
    let share = row(
        "smb-192-0-2-53-445-share-ada",
        "share on nas",
        "/Volumes/cmdr-test-never-mounted-share",
        crate::volumes::LocationCategory::Network,
    );
    let mut phone = row(
        "fav-internal-storage",
        "Internal storage",
        "mtp://pixel/Internal storage",
        crate::volumes::LocationCategory::Favorite,
    );
    phone.fs_type = None;
    phone.connection_state = None;

    let with_favorite = snapshot_volumes_from(&[share.clone(), phone]).await;
    let without = snapshot_volumes_from(&[share]).await;

    assert!(
        with_favorite.iter().all(|v| v.id != "fav-internal-storage"),
        "the favorite isn't a volume"
    );
    assert_eq!(build_volumes_yaml(&with_favorite), build_volumes_yaml(&without));
}

#[test]
fn empty_renders_flow_seq() {
    assert_eq!(build_volumes_yaml(&[]), "volumes: []\n");
}

/// ❗ **A saved server reaches `cmdr://state`.** The snapshot used to build
/// its own list from `list_locations` plus a hand-appended MTP block, so no
/// row ever carried `fs_type: "sftp"`: the `Sftp` / `Webdav` / `Adb` arms of
/// [`kind_for_location`] and every `connectionState` past `disconnected` were
/// unreachable in practice, while the resource's own docs advertised them as
/// the agent-facing contract. The compiler can't see it, because the arms are
/// constructed inside a function that IS called.
///
/// Drives the real snapshot rather than a fixture, because the fixture is
/// exactly what stayed green through the bug.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_saved_server_reaches_the_state_resource() {
    // ❗ `192.0.2.x` is reserved for documentation (RFC 5737) and routed
    // nowhere, so nothing here can dial a stranger's machine — and the
    // snapshot must not dial at all.
    let host = "192.0.2.51";
    crate::network::sftp_known_servers::remember(crate::network::sftp_known_servers::KnownSftpServer {
        host: host.to_string(),
        port: 2222,
        username: "ada".to_string(),
        display_name: format!("{host} server"),
        remote_root: "/srv/data".to_string(),
        start_folder: None,
        key_file: None,
        use_agent: false,
        auto_reconnect: true,
        pinned: true,
        last_connected_at: "2026-09-06T00:00:00Z".to_string(),
    });

    let volumes = snapshot_volumes().await;
    let id = cmdr_fs::volume::sftp_volume_id(host, 2222, "ada");
    let row = volumes
        .iter()
        .find(|v| v.id == id)
        .expect("a saved server is a volume the agent can see");

    assert_eq!(row.kind, VolumeKind::Sftp, "❌ never `local`: the fs_type names it");
    // Nothing is registered under that id, so it is the greyed row, and the
    // agent needs the word for that to route a connect at it.
    assert_eq!(row.connection_state, Some("saved"));
    assert!(build_volumes_yaml(&volumes).contains("kind: sftp"));
}

#[test]
fn local_volume_carries_every_known_field() {
    let yaml = build_volumes_yaml(&[local("Macintosh HD", "root")]);
    assert_eq!(
        yaml,
        "volumes:\n  - name: Macintosh HD\n    id: root\n    kind: local\n    mountPath: \"/\"\n    \
         filesystem: apfs\n    readOnly: false\n    ejectable: false\n    indexStatus: fresh\n"
    );
}

#[test]
fn the_mount_path_renders_verbatim() {
    // An AI client copies it into a `search` scope, so a redacted
    // `$HOME/<dir>` would be a scope that matches nothing.
    let mut v = local("Downloads", "favorite-downloads");
    v.mount_path = Some("/Users/someone/Downloads".to_string());
    let yaml = build_volumes_yaml(&[v]);
    assert!(
        yaml.contains("    mountPath: \"/Users/someone/Downloads\"\n"),
        "yaml: {yaml}"
    );
}

#[test]
fn a_watched_volume_carries_capacity_and_free_space() {
    // The pair is what makes a size answer actionable: "40 GB" reads
    // differently against 2 TB free than against 8 GB.
    let mut v = local("Macintosh HD", "root");
    v.space = Some(SpaceInfo::bounded(2_000_000_000_000, 214_300_000_000));
    let yaml = build_volumes_yaml(&[v]);
    assert!(yaml.contains("totalBytes: 2000000000000"));
    assert!(yaml.contains("availableBytes: 214300000000"));
    // Beside the bytes, not instead of them: a reader that has to state the number
    // out loud can't divide by 1,024 reliably, and one doing arithmetic still has
    // the exact value.
    assert!(yaml.contains(&format!("totalHuman: {}", format_size(2_000_000_000_000))));
    assert!(yaml.contains(&format!("availableHuman: {}", format_size(214_300_000_000))));
}

#[test]
fn an_unwatched_volume_omits_space_rather_than_reporting_zero() {
    // Nothing is polling it, so we don't know. A rendered `availableBytes: 0`
    // would read as a full disk.
    let yaml = build_volumes_yaml(&[local("Backup HD", "volumes-backup")]);
    assert!(!yaml.contains("totalBytes"));
    assert!(!yaml.contains("availableBytes"));
    assert!(!yaml.contains("totalHuman"));
    assert!(!yaml.contains("availableHuman"));
}

#[test]
fn smb_volume_carries_connection_state_and_kind() {
    let smb = VolumeSummary {
        name: "naspi".to_string(),
        id: "smb-192-168-1-111-445-naspi".to_string(),
        kind: VolumeKind::Smb,
        filesystem: Some("smbfs".to_string()),
        read_only: Some(false),
        ejectable: Some(true),
        index_status: Some("stale"),
        connection_state: Some("direct"),
        device_readiness: None,
        mount_path: Some("/Volumes/naspi".to_string()),
        space: None,
    };
    let yaml = build_volumes_yaml(&[smb]);
    assert!(yaml.contains("kind: smb"));
    assert!(yaml.contains("connectionState: direct"));
    assert!(yaml.contains("indexStatus: stale"));
    assert!(yaml.contains("ejectable: true"));
}

#[test]
fn mtp_volume_omits_filesystem_and_smb_state() {
    let mtp = VolumeSummary {
        name: "Pixel 8".to_string(),
        id: "mtp-336592896:65537".to_string(),
        kind: VolumeKind::Mtp,
        filesystem: None,
        read_only: Some(true),
        ejectable: Some(true),
        index_status: Some("off"),
        connection_state: None,
        device_readiness: None,
        mount_path: None,
        space: None,
    };
    let yaml = build_volumes_yaml(&[mtp]);
    assert!(yaml.contains("kind: mtp"));
    assert!(yaml.contains("id: mtp-336592896:65537"));
    assert!(yaml.contains("readOnly: true"));
    assert!(!yaml.contains("filesystem:"));
    assert!(!yaml.contains("connectionState:"));
}

/// ❗ A phone waiting for its "Allow USB debugging?" tap IS listed, so the
/// row has to say why it can't be opened. Without `deviceReadiness` an agent
/// reads a browsable ADB row and gets a refusal it can't explain, and the
/// session field is the wrong place to say it (that would enrol a device with
/// no session in a reconnect loop).
#[test]
fn an_adb_row_says_what_the_device_is_waiting_for_without_claiming_a_session() {
    let waiting = VolumeSummary {
        name: "Pixel 8".to_string(),
        id: "adb-R58M12345".to_string(),
        kind: VolumeKind::Adb,
        filesystem: None,
        read_only: Some(false),
        ejectable: Some(true),
        index_status: None,
        connection_state: None,
        device_readiness: Some(device_readiness_token(
            cmdr_fs::volume::DeviceReadiness::WaitingForAuthorization,
        )),
        mount_path: None,
        space: None,
    };
    let yaml = build_volumes_yaml(&[waiting]);
    assert!(yaml.contains("deviceReadiness: waiting_for_authorization"));
    assert!(
        !yaml.contains("connectionState:"),
        "presence is not a session; a device with no session must not read as one"
    );
}

/// Each unavailable reason gets its own token, so a tooltip can say which.
#[test]
fn every_readiness_has_its_own_wire_word() {
    use cmdr_fs::volume::{DeviceReadiness as R, DeviceUnavailableReason as Why};
    assert_eq!(device_readiness_token(R::Ready), "ready");
    assert_eq!(
        device_readiness_token(R::WaitingForAuthorization),
        "waiting_for_authorization"
    );
    assert_eq!(
        device_readiness_token(R::Unavailable { reason: Why::Offline }),
        "unavailable_offline"
    );
    assert_eq!(
        device_readiness_token(R::Unavailable {
            reason: Why::NoPermissions
        }),
        "unavailable_no_permissions"
    );
}

#[test]
fn virtual_volume_is_name_id_kind_only() {
    let network = VolumeSummary {
        name: SERVERS_VOLUME_NAME.to_string(),
        id: "network".to_string(),
        kind: VolumeKind::Virtual,
        filesystem: None,
        read_only: None,
        ejectable: None,
        index_status: None,
        connection_state: None,
        device_readiness: None,
        mount_path: None,
        space: None,
    };
    let yaml = build_volumes_yaml(&[network]);
    assert_eq!(
        yaml,
        "volumes:\n  - name: Servers\n    id: network\n    kind: virtual\n"
    );
}

#[test]
fn mixed_fixture_keeps_every_entry_uniform_head() {
    let network = VolumeSummary {
        name: SERVERS_VOLUME_NAME.to_string(),
        id: "network".to_string(),
        kind: VolumeKind::Virtual,
        filesystem: None,
        read_only: None,
        ejectable: None,
        index_status: None,
        connection_state: None,
        device_readiness: None,
        mount_path: None,
        space: None,
    };
    let yaml = build_volumes_yaml(&[local("Macintosh HD", "root"), network]);
    // Every entry leads with name / id / kind, in order.
    let heads: Vec<&str> = yaml
        .lines()
        .filter(|l| l.trim_start().starts_with("- name:") || l.contains("id:") || l.contains("kind:"))
        .collect();
    assert_eq!(
        heads,
        vec![
            "  - name: Macintosh HD",
            "    id: root",
            "    kind: local",
            "  - name: Servers",
            "    id: network",
            "    kind: virtual",
        ]
    );
}
