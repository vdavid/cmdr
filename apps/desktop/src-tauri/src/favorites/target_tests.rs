use super::*;

fn never_asked(path: &Path) -> bool {
    panic!("discovery must not stat {path:?}")
}

#[test]
fn a_local_folder_is_stat_and_reads_yes_or_no() {
    assert_eq!(on_disk("/Users/me/Docs", Probe::Stat, |_| true), OnDisk::Yes);
    assert_eq!(on_disk("/Users/me/Gone", Probe::Stat, |_| false), OnDisk::No);
}

/// ❗ The hung-mount guard: one dead share must not cost the whole listing its 2 s budget.
#[test]
fn a_folder_on_a_network_mount_is_never_stat() {
    assert_eq!(
        on_disk("/Volumes/naspi/docs", Probe::NetworkMount, never_asked),
        OnDisk::Unchecked
    );
}

/// ❗ Even `exists()` raises a TCC popup over the onboarding modal while the FDA gate is pending.
#[test]
fn a_protected_folder_is_taken_on_trust_without_a_stat() {
    assert_eq!(
        on_disk("/Users/me/Desktop", Probe::TakenOnTrust, never_asked),
        OnDisk::Unchecked
    );
}

#[test]
fn a_scheme_path_is_never_stat() {
    for path in ["sftp://ada@nas:22/srv", "mtp://pixel/65537/DCIM", "adb://serial/sdcard"] {
        assert_eq!(on_disk(path, Probe::Stat, never_asked), OnDisk::Unchecked, "{path}");
    }
}

#[test]
fn discovery_seeds_the_target_from_the_store() {
    let stored = FavoriteVolume {
        id: "smb-naspi-0123".to_string(),
        root: "/Volumes/naspi".to_string(),
        name: "naspi on nas.local".to_string(),
    };
    let target = FavoriteTarget::discovered(Some(stored.clone()), OnDisk::No);
    assert_eq!(target.volume_id.as_deref(), Some("smb-naspi-0123"));
    assert_eq!(target.volume_name.as_deref(), Some("naspi on nas.local"));
    assert_eq!(target.reach, FavoriteReach::NotFound);
    assert_eq!(target.discovered.stored, Some(stored));
    assert_eq!(
        FavoriteTarget::discovered(None, OnDisk::Unchecked).reach,
        FavoriteReach::Ready
    );
}

/// The wire shape the frontend switches on. ❗ `discovered` never crosses.
#[test]
fn the_target_serializes_camel_case_with_a_kind_tag_and_no_discovery_facts() {
    let target = FavoriteTarget {
        reach: FavoriteReach::Unplugged {
            device: UnpluggedKind::Phone,
            reason: Some(DeviceUnavailableReason::Offline),
        },
        ..FavoriteTarget::discovered(None, OnDisk::Yes)
    };
    assert_eq!(
        serde_json::to_value(&target).expect("serialize"),
        serde_json::json!({
            "volumeId": null,
            "volumeName": null,
            "volumeRoot": null,
            "reach": { "kind": "unplugged", "device": "phone", "reason": "offline" },
        })
    );
    assert_eq!(
        serde_json::to_value(FavoriteReach::AccessOff {
            backend: DeviceBackend::Adb
        })
        .expect("serialize"),
        serde_json::json!({ "kind": "access_off", "backend": "adb" })
    );
    assert_eq!(
        serde_json::to_value(FavoriteReach::NotFound).expect("serialize"),
        serde_json::json!({ "kind": "not_found" })
    );
}
