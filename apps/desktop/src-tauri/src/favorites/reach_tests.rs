use super::*;
use crate::favorites::target::{DeviceBackend, FavoriteTarget, OnDisk, UnpluggedKind};
use cmdr_fs::volume::{DeviceReadiness, DeviceUnavailableReason, VolumeCapabilities};

const ALL_ON: ReachFacts = ReachFacts {
    mtp_enabled: true,
    adb_enabled: true,
};

fn volume_row(id: &str, path: &str, category: LocationCategory) -> LocationInfo {
    LocationInfo {
        id: id.to_string(),
        name: format!("{id} name"),
        path: path.to_string(),
        category,
        icon: None,
        is_ejectable: false,
        fs_type: None,
        supports_trash: false,
        mount_is_read_only: false,
        is_disk_image: false,
        is_cloud_mount: false,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        root_label: None,
        mount_account: None,
    }
}

fn with_state(state: ConnectionState, row: LocationInfo) -> LocationInfo {
    LocationInfo {
        connection_state: Some(state),
        ..row
    }
}

/// A device row the registry holds (enrichment filled its capabilities in).
fn registered(row: LocationInfo) -> LocationInfo {
    LocationInfo {
        capabilities: Some(VolumeCapabilities {
            backend_can_write: true,
            can_export: true,
            can_share_links: false,
            can_be_indexed: true,
            renames_can_copy: false,
            has_os_mount_fallback: false,
        }),
        ..row
    }
}

fn stored(id: &str, root: &str) -> FavoriteVolume {
    FavoriteVolume {
        id: id.to_string(),
        root: root.to_string(),
        name: format!("{id} stored name"),
    }
}

/// A favorite row the way discovery publishes it.
fn favorite_row(path: &str, volume: Option<FavoriteVolume>, on_disk: OnDisk) -> LocationInfo {
    LocationInfo {
        name: "Docs".to_string(),
        favorite_target: Some(FavoriteTarget::discovered(volume, on_disk)),
        ..volume_row("fav-f1", path, LocationCategory::Favorite)
    }
}

/// Runs the pass over `volumes` plus one favorite, answering the favorite row and the claims.
fn reach_of(
    favorite: LocationInfo,
    volumes: Vec<LocationInfo>,
    facts: &ReachFacts,
) -> (LocationInfo, Vec<(String, FavoriteVolume)>) {
    let mut rows = volumes;
    rows.push(favorite);
    let claims = annotate(&mut rows, facts);
    (rows.pop().expect("the favorite row"), claims)
}

fn target(row: &LocationInfo) -> &FavoriteTarget {
    row.favorite_target.as_ref().expect("a favorite row carries its target")
}

// -- a row is found --

#[test]
fn a_folder_on_a_live_local_volume_is_ready_at_its_root() {
    let (row, _) = reach_of(
        favorite_row("/Users/me/Docs", Some(stored("root", "/")), OnDisk::Yes),
        vec![volume_row("root", "/", LocationCategory::MainVolume)],
        &ALL_ON,
    );
    let target = target(&row);
    assert_eq!(target.reach, FavoriteReach::Ready);
    assert_eq!(target.volume_id.as_deref(), Some("root"));
    assert_eq!(target.volume_root.as_deref(), Some("/"));
    assert_eq!(target.volume_name.as_deref(), Some("root name"), "the row's name wins");
}

#[test]
fn a_folder_gone_from_a_live_local_volume_is_not_found() {
    let (row, _) = reach_of(
        favorite_row("/Users/me/Gone", Some(stored("root", "/")), OnDisk::No),
        vec![volume_row("root", "/", LocationCategory::MainVolume)],
        &ALL_ON,
    );
    assert_eq!(target(&row).reach, FavoriteReach::NotFound);
}

#[test]
fn a_folder_on_a_live_session_is_ready() {
    for state in [ConnectionState::Direct, ConnectionState::OsMount] {
        let share = with_state(state, volume_row("smb-n", "/Volumes/naspi", LocationCategory::Network));
        let (row, _) = reach_of(
            favorite_row(
                "/Volumes/naspi/docs",
                Some(stored("smb-n", "/Volumes/naspi")),
                OnDisk::Unchecked,
            ),
            vec![share],
            &ALL_ON,
        );
        assert_eq!(target(&row).reach, FavoriteReach::Ready, "{state:?}");
    }
}

/// ❗ Sven's case: the share is saved but unmounted. A pick enters the saved row and the pane's own
/// connect view dials.
#[test]
fn a_folder_on_a_saved_or_dropped_place_connects() {
    for state in [
        ConnectionState::Saved,
        ConnectionState::Disconnected,
        ConnectionState::NeedsSignIn,
        ConnectionState::NeedsHostKeyApproval,
    ] {
        let share = with_state(state, volume_row("smb-n", "/Volumes/naspi", LocationCategory::Network));
        let (row, _) = reach_of(
            favorite_row(
                "/Volumes/naspi/docs",
                Some(stored("smb-n", "/Volumes/naspi")),
                OnDisk::No,
            ),
            vec![share],
            &ALL_ON,
        );
        let target = target(&row);
        assert_eq!(target.reach, FavoriteReach::Connects, "{state:?}");
        assert_eq!(target.volume_root.as_deref(), Some("/Volumes/naspi"));
    }
}

#[test]
fn a_phone_listed_but_not_dialed_or_waiting_for_allow_connects() {
    let listed = volume_row("adb-s1", "adb://s1", LocationCategory::MobileDevice);
    let waiting = registered(LocationInfo {
        device_readiness: Some(DeviceReadiness::WaitingForAuthorization),
        ..volume_row("adb-s1", "adb://s1", LocationCategory::MobileDevice)
    });
    for phone in [listed, waiting] {
        let (row, _) = reach_of(
            favorite_row(
                "adb://s1/sdcard/DCIM",
                Some(stored("adb-s1", "adb://s1")),
                OnDisk::Unchecked,
            ),
            vec![phone],
            &ALL_ON,
        );
        assert_eq!(target(&row).reach, FavoriteReach::Connects);
    }
}

#[test]
fn a_phone_that_is_listed_but_unusable_is_unplugged_with_its_reason() {
    let phone = registered(LocationInfo {
        device_readiness: Some(DeviceReadiness::Unavailable {
            reason: DeviceUnavailableReason::Offline,
        }),
        ..volume_row("adb-s1", "adb://s1", LocationCategory::MobileDevice)
    });
    let (row, _) = reach_of(
        favorite_row("adb://s1/sdcard", Some(stored("adb-s1", "adb://s1")), OnDisk::Unchecked),
        vec![phone],
        &ALL_ON,
    );
    assert_eq!(
        target(&row).reach,
        FavoriteReach::Unplugged {
            device: UnpluggedKind::Phone,
            reason: Some(DeviceUnavailableReason::Offline),
        }
    );
}

#[test]
fn a_connected_phone_is_ready() {
    let phone = registered(LocationInfo {
        device_readiness: Some(DeviceReadiness::Ready),
        ..volume_row("mtp-p1:65537", "mtp://p1/65537", LocationCategory::MobileDevice)
    });
    let (row, _) = reach_of(
        favorite_row(
            "mtp://p1/65537/DCIM",
            Some(stored("mtp-p1:65537", "mtp://p1/65537")),
            OnDisk::Unchecked,
        ),
        vec![phone],
        &ALL_ON,
    );
    assert_eq!(target(&row).reach, FavoriteReach::Ready);
}

// -- no row --

#[test]
fn an_absent_phone_is_unplugged_or_switched_off() {
    let mtp = || {
        favorite_row(
            "mtp://p1/65537/DCIM",
            Some(stored("mtp-p1:65537", "mtp://p1/65537")),
            OnDisk::Unchecked,
        )
    };
    let adb = || favorite_row("adb://s1/sdcard", Some(stored("adb-s1", "adb://s1")), OnDisk::Unchecked);
    let phone = FavoriteReach::Unplugged {
        device: UnpluggedKind::Phone,
        reason: None,
    };
    assert_eq!(target(&reach_of(mtp(), vec![], &ALL_ON).0).reach, phone);
    assert_eq!(target(&reach_of(adb(), vec![], &ALL_ON).0).reach, phone);

    let off = ReachFacts {
        mtp_enabled: false,
        adb_enabled: false,
    };
    assert_eq!(
        target(&reach_of(mtp(), vec![], &off).0).reach,
        FavoriteReach::AccessOff {
            backend: DeviceBackend::Mtp
        }
    );
    assert_eq!(
        target(&reach_of(adb(), vec![], &off).0).reach,
        FavoriteReach::AccessOff {
            backend: DeviceBackend::Adb
        }
    );
}

/// The phone is here, under another storage id: the SD card the favorite was on is swapped out.
#[test]
fn a_phone_listed_under_another_storage_has_its_storage_unplugged() {
    let other_storage = registered(volume_row(
        "mtp-p1:131073",
        "mtp://p1/131073",
        LocationCategory::MobileDevice,
    ));
    let (row, _) = reach_of(
        favorite_row(
            "mtp://p1/65537/DCIM",
            Some(stored("mtp-p1:65537", "mtp://p1/65537")),
            OnDisk::Unchecked,
        ),
        vec![other_storage],
        &ALL_ON,
    );
    assert_eq!(
        target(&row).reach,
        FavoriteReach::Unplugged {
            device: UnpluggedKind::Storage,
            reason: None,
        }
    );
}

#[test]
fn an_absent_drive_is_unplugged_and_keeps_its_stored_name() {
    for id in ["vol-t7-abc", "path-nfs-def", "cloud-dropbox"] {
        let (row, _) = reach_of(
            favorite_row("/Volumes/T7/docs", Some(stored(id, "/Volumes/T7")), OnDisk::No),
            vec![volume_row("root", "/", LocationCategory::MainVolume)],
            &ALL_ON,
        );
        let target = target(&row);
        assert_eq!(
            target.reach,
            FavoriteReach::Unplugged {
                device: UnpluggedKind::Drive,
                reason: None,
            },
            "{id}"
        );
        assert_eq!(target.volume_name, Some(format!("{id} stored name")));
        assert_eq!(target.volume_root, None, "no row, no root to enter");
    }
}

/// Forget share / Forget server removed the row: nothing can dial the id. The same place coming
/// back mints the same id and revives the favorite.
#[test]
fn a_share_or_server_nothing_saved_is_forgotten() {
    for (id, path) in [
        ("smb-n", "/Volumes/naspi/docs"),
        ("sftp-n", "sftp://ada@nas:22/srv"),
        ("webdav-n", "webdav://ada@nas:443/dav"),
        ("s3-n", "s3://key@host:443/photos"),
    ] {
        let (row, _) = reach_of(
            favorite_row(path, Some(stored(id, path)), OnDisk::Unchecked),
            vec![],
            &ALL_ON,
        );
        assert_eq!(target(&row).reach, FavoriteReach::Forgotten, "{id}");
    }
}

// -- rebase --

/// ❗ The share came back at another mount point: the favorite follows it there, and the stored
/// path's "not on disk" (it was probed at the OLD path) doesn't count against it.
#[test]
fn a_share_back_at_another_mount_point_is_rebased_onto_it() {
    let saved = with_state(
        ConnectionState::Saved,
        volume_row("smb-n", "/Volumes/naspi-1", LocationCategory::Network),
    );
    let (row, _) = reach_of(
        favorite_row(
            "/Volumes/naspi/docs",
            Some(stored("smb-n", "/Volumes/naspi")),
            OnDisk::No,
        ),
        vec![saved],
        &ALL_ON,
    );
    assert_eq!(row.path, "/Volumes/naspi-1/docs");
    assert_eq!(target(&row).reach, FavoriteReach::Connects);

    let live_drive = volume_row("vol-t7", "/Volumes/Backup", LocationCategory::AttachedVolume);
    let (row, _) = reach_of(
        favorite_row("/Volumes/T7/docs", Some(stored("vol-t7", "/Volumes/T7")), OnDisk::No),
        vec![live_drive],
        &ALL_ON,
    );
    assert_eq!(row.path, "/Volumes/Backup/docs", "a renamed drive");
    assert_eq!(target(&row).reach, FavoriteReach::Ready);
}

/// A server's paths are absolute in the server's own namespace, so an edited remote root doesn't
/// move them, and a favorite now outside the root is not found.
#[test]
fn a_server_path_is_never_rebased_and_outside_its_root_is_not_found() {
    let server = with_state(
        ConnectionState::Saved,
        volume_row("sftp-n", "sftp://ada@nas:22/data", LocationCategory::Network),
    );
    let (inside, _) = reach_of(
        favorite_row(
            "sftp://ada@nas:22/data/x",
            Some(stored("sftp-n", "sftp://ada@nas:22/srv")),
            OnDisk::Unchecked,
        ),
        vec![server.clone()],
        &ALL_ON,
    );
    assert_eq!(inside.path, "sftp://ada@nas:22/data/x");
    assert_eq!(target(&inside).reach, FavoriteReach::Connects);

    let (outside, _) = reach_of(
        favorite_row(
            "sftp://ada@nas:22/srv/x",
            Some(stored("sftp-n", "sftp://ada@nas:22/srv")),
            OnDisk::Unchecked,
        ),
        vec![server],
        &ALL_ON,
    );
    assert_eq!(outside.path, "sftp://ada@nas:22/srv/x");
    assert_eq!(target(&outside).reach, FavoriteReach::NotFound);
}

// -- claiming legacy entries --

#[test]
fn a_legacy_favorite_under_a_saved_share_is_claimed_by_the_share() {
    let saved = with_state(
        ConnectionState::Saved,
        volume_row("smb-n", "/Volumes/naspi", LocationCategory::Network),
    );
    let (row, claims) = reach_of(
        favorite_row("/Volumes/naspi/docs", None, OnDisk::No),
        vec![volume_row("root", "/", LocationCategory::MainVolume), saved],
        &ALL_ON,
    );
    assert_eq!(target(&row).reach, FavoriteReach::Connects);
    assert_eq!(target(&row).volume_id.as_deref(), Some("smb-n"));
    assert_eq!(
        claims,
        vec![(
            "f1".to_string(),
            FavoriteVolume {
                id: "smb-n".to_string(),
                root: "/Volumes/naspi".to_string(),
                name: "smb-n name".to_string(),
            }
        )]
    );
}

/// ❗ Claim only from evidence: an unmounted share nobody saved would otherwise be claimed by the
/// boot volume, written down, and read "not found" forever after the share comes back.
#[test]
fn a_legacy_favorite_with_no_evidence_is_not_claimed_by_the_boot_volume() {
    for on_disk in [OnDisk::Unchecked, OnDisk::No] {
        let (row, claims) = reach_of(
            favorite_row("/Volumes/naspi/docs", None, on_disk),
            vec![volume_row("root", "/", LocationCategory::MainVolume)],
            &ALL_ON,
        );
        assert!(claims.is_empty(), "{on_disk:?}");
        assert_eq!(target(&row).reach, FavoriteReach::NotFound);
        assert_eq!(target(&row).volume_id, None);
    }
}

#[test]
fn a_legacy_favorite_seen_on_the_boot_disk_is_claimed_by_it() {
    let (row, claims) = reach_of(
        favorite_row("/Users/x/Docs", None, OnDisk::Yes),
        vec![
            volume_row("root", "/", LocationCategory::MainVolume),
            volume_row("vol-t7", "/Volumes/T7", LocationCategory::AttachedVolume),
        ],
        &ALL_ON,
    );
    assert_eq!(target(&row).reach, FavoriteReach::Ready);
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].1.id, "root");
}

/// The most specific root wins, and a favorite never claims another favorite.
#[test]
fn a_claim_takes_the_deepest_volume_and_never_a_favorite() {
    let other_favorite = LocationInfo {
        favorite_target: Some(FavoriteTarget::discovered(
            Some(stored("vol-t7", "/Volumes/T7")),
            OnDisk::Yes,
        )),
        ..volume_row("fav-other", "/Volumes/T7/docs", LocationCategory::Favorite)
    };
    let (_, claims) = reach_of(
        favorite_row("/Volumes/T7/docs/inner", None, OnDisk::Yes),
        vec![
            volume_row("root", "/", LocationCategory::MainVolume),
            volume_row("vol-t7", "/Volumes/T7", LocationCategory::AttachedVolume),
            other_favorite,
        ],
        &ALL_ON,
    );
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].1.id, "vol-t7");
}

#[test]
fn a_volume_row_is_left_alone() {
    let mut rows = vec![volume_row("root", "/", LocationCategory::MainVolume)];
    assert!(annotate(&mut rows, &ALL_ON).is_empty());
    assert!(rows[0].favorite_target.is_none());
}
