use super::holders::{HolderKind, HolderScan, VolumeHolder};
use super::*;

#[test]
fn device_volume_routes_to_its_provider() {
    // The id is handed over whole: the provider owns its parse (MTP splits
    // `{device_id}:{storage_id}` on the LAST colon, so a serial with `:` in
    // it survives).
    let ctx = EjectContext {
        volume_id: "mtp-AA:BB:CC:65537",
        is_ejectable: false,
        is_smb: false,
        is_remote_session: false,
        device_provider: Some("mtp"),
    };
    assert_eq!(
        decide_eject_action(&ctx).unwrap(),
        EjectAction::DeviceDisconnect {
            provider: "mtp",
            volume_id: "mtp-AA:BB:CC:65537".to_string()
        }
    );
}

#[test]
fn device_provider_wins_over_every_other_flag() {
    let ctx = EjectContext {
        volume_id: "adb-serial",
        is_ejectable: true,
        is_smb: true,
        is_remote_session: false,
        device_provider: Some("adb"),
    };
    assert!(matches!(
        decide_eject_action(&ctx).unwrap(),
        EjectAction::DeviceDisconnect { provider: "adb", .. }
    ));
}

#[test]
fn smb_volume_routes_to_unmount() {
    let ctx = EjectContext {
        volume_id: "smb-testnas-445-public",
        is_ejectable: false,
        is_smb: true,
        is_remote_session: false,
        device_provider: None,
    };
    assert_eq!(decide_eject_action(&ctx).unwrap(), EjectAction::DiskutilUnmount);
}

#[test]
fn ejectable_disk_routes_to_eject() {
    let ctx = EjectContext {
        volume_id: "volumes-usb-drive",
        is_ejectable: true,
        is_smb: false,
        is_remote_session: false,
        device_provider: None,
    };
    assert_eq!(decide_eject_action(&ctx).unwrap(), EjectAction::DiskutilEject);
}

/// A saved SFTP place nobody is connected to, the way an unpinned-but-remembered
/// server sits in the switcher. Registers nothing: that IS the state under test.
async fn with_a_saved_server<F, Fut, T>(host: &str, body: F) -> T
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = T>,
{
    use crate::network::sftp_known_servers::{self, KnownSftpServer};

    let (port, username) = (22, "test_user_3");
    sftp_known_servers::remember(KnownSftpServer {
        host: host.to_string(),
        port,
        username: username.to_string(),
        display_name: String::new(),
        remote_root: "/".to_string(),
        start_folder: None,
        key_file: None,
        use_agent: false,
        auto_reconnect: false,
        pinned: true,
        last_connected_at: "2026-09-17T00:00:00Z".to_string(),
    });
    let answer = body(cmdr_fs::volume::sftp_volume_id(host, port, username)).await;
    // The store is process-global, so this cell cleans up after itself.
    sftp_known_servers::forget(host, port, username);
    answer
}

#[tokio::test]
async fn a_saved_server_nobody_is_connected_to_is_told_so_in_words_about_a_server() {
    // Pre-fix this answered `VolumeNotFound`, whose copy is about a DRIVE that
    // isn't connected any more: the wrong noun and the wrong verb in front of
    // someone asking about a server.
    let result = with_a_saved_server("eject-cell-saved.invalid", |volume_id| async move {
        eject(&volume_id).await
    })
    .await;
    assert!(
        matches!(result, Err(EjectError::RemoteNotConnected { .. })),
        "got {result:?}"
    );
}

#[tokio::test]
async fn an_id_the_app_knows_nothing_about_is_still_a_missing_volume() {
    // The other half of the same branch: `RemoteNotConnected` is earned by being a
    // KNOWN place, ❌ never by the id's shape.
    let result = eject("sftp-nothing-has-ever-been-saved-here").await;
    assert!(
        matches!(result, Err(EjectError::VolumeNotFound { .. })),
        "got {result:?}"
    );
}

#[test]
fn remote_session_routes_to_a_disconnect() {
    // Pre-fix this answered `NotEjectable`, which is how an open SFTP place came to
    // be offered an Eject that could only turn it down (`ERR-P7F5Q`).
    let ctx = EjectContext {
        volume_id: "sftp-testnas-22-test_user_3",
        is_ejectable: false,
        is_smb: false,
        is_remote_session: true,
        device_provider: None,
    };
    assert_eq!(
        decide_eject_action(&ctx).unwrap(),
        EjectAction::RemoteDisconnect {
            volume_id: "sftp-testnas-22-test_user_3".to_string(),
        }
    );
}

#[test]
fn a_remote_session_that_the_os_calls_ejectable_still_disconnects() {
    // A server is asked about BEFORE the mount questions: `is_ejectable` is an
    // answer about a mount, and wording a session drop as an eject is the bug this
    // ordering exists to prevent.
    let ctx = EjectContext {
        volume_id: "webdav-example-443-test_user_3",
        is_ejectable: true,
        is_smb: true,
        is_remote_session: true,
        device_provider: None,
    };
    assert!(matches!(
        decide_eject_action(&ctx).unwrap(),
        EjectAction::RemoteDisconnect { .. }
    ));
}

#[test]
fn a_device_provider_still_wins_over_a_remote_session() {
    // The provider lookup is live state, so an id it claims is authoritative.
    let ctx = EjectContext {
        volume_id: "mtp-phone",
        is_ejectable: false,
        is_smb: false,
        is_remote_session: true,
        device_provider: Some("mtp"),
    };
    assert!(matches!(
        decide_eject_action(&ctx).unwrap(),
        EjectAction::DeviceDisconnect { provider: "mtp", .. }
    ));
}

#[test]
fn non_ejectable_local_volume_errors() {
    let ctx = EjectContext {
        volume_id: "root",
        is_ejectable: false,
        is_smb: false,
        is_remote_session: false,
        device_provider: None,
    };
    assert_eq!(
        decide_eject_action(&ctx).unwrap_err(),
        EjectDecisionError::NotEjectable {
            volume_id: "root".to_string(),
        }
    );
}

#[tokio::test]
async fn eject_stops_the_index_before_the_unmount() {
    use cmdr_index::IndexVolumeKind;
    use cmdr_index::testing::{is_index_active, reserve_initializing_index_for_test};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    // A LocalExternal drive with a live index is ejected: the index MUST be
    // stopped (its FSEvents watcher + SQLite handles released) BEFORE the unmount
    // runs — the wedge-safe ordering. Pre-fix this would have passed wrongly: the
    // eject path never touched indexing, so a live index survived into the unmount.
    // This drives the REAL
    // `stop_indexing` through the ordering seam with a fake unmount that records
    // whether the index was still active when it ran.
    let vid = "volumes-cmdr-test-eject-stop-order";
    let _tmp = reserve_initializing_index_for_test(vid, IndexVolumeKind::LocalExternal);
    assert!(is_index_active(vid), "precondition: the index is active");

    let active_when_unmount_ran = Arc::new(AtomicBool::new(true));
    let observed = Arc::clone(&active_when_unmount_ran);
    let vid_for_unmount = vid.to_string();

    let result = stop_index_then_unmount(
        vid,
        stop_index_blocking(vid.to_string(), stop_removable_index),
        || async move {
            // Record the index state at the exact moment the unmount would run.
            observed.store(is_index_active(&vid_for_unmount), Ordering::SeqCst);
            Ok(())
        },
    )
    .await;

    assert!(result.is_ok(), "the ordering seam must propagate the unmount result");
    assert!(
        !active_when_unmount_ran.load(Ordering::SeqCst),
        "the index must be stopped BEFORE the unmount runs"
    );
    assert!(!is_index_active(vid), "the index instance is gone after eject");
}

/// An index still letting go of the drive when its stop's wait ran out must
/// never meet the unmount: a watcher alive at unmount is what wedges FSKit.
#[tokio::test]
async fn an_index_still_letting_go_of_the_drive_never_meets_the_unmount() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let vid = "volumes-cmdr-test-eject-still-releasing";
    let unmount_ran = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&unmount_ran);

    let result = stop_index_then_unmount(
        vid,
        stop_index_blocking(vid.to_string(), |_: &str| cmdr_index::RemovableStop::StillReleasing),
        move || async move {
            observed.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .await;

    assert!(
        matches!(
            result,
            Err(EjectError::NotResponding {
                step: EjectStep::IndexStop
            })
        ),
        "got {result:?}"
    );
    assert!(
        !unmount_ran.load(Ordering::SeqCst),
        "a drive its index hasn't let go of must stay mounted"
    );
}

/// A stop that panicked says nothing about whether the index let go, so it
/// must not be read as "done".
#[tokio::test]
async fn an_index_stop_that_panicked_never_meets_the_unmount() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let vid = "volumes-cmdr-test-eject-stop-panicked";
    let unmount_ran = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&unmount_ran);

    let result = stop_index_then_unmount(
        vid,
        stop_index_blocking(vid.to_string(), |_: &str| -> cmdr_index::RemovableStop {
            panic!("simulated panic inside the index stop")
        }),
        move || async move {
            observed.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .await;

    assert!(matches!(result, Err(EjectError::Unexpected { .. })), "got {result:?}");
    assert!(
        !unmount_ran.load(Ordering::SeqCst),
        "an index stop nobody knows the end of must not reach the unmount"
    );
}

#[test]
fn smb_wins_over_ejectable_flag() {
    // Belt-and-braces: if anything ever sets is_ejectable on an SMB
    // volume, the SMB branch should still win so we run `unmount` (no
    // hardware to power down) instead of `eject`.
    let ctx = EjectContext {
        volume_id: "smb-foo",
        is_ejectable: true,
        is_smb: true,
        is_remote_session: false,
        device_provider: None,
    };
    assert_eq!(decide_eject_action(&ctx).unwrap(), EjectAction::DiskutilUnmount);
}

#[test]
fn a_drive_whose_root_left_the_mount_table_is_already_ejected() {
    // Its eject landed a moment ago and the row still lingers in the
    // switcher: a click there must not reach `resolve_is_ejectable`, which
    // answers "not ejectable" for a path that's gone.
    let drive = cmdr_fs::volume::local_volume_id(Some("5C1A2D4E-0000-4000-8000-00000000BEEF"), "/Volumes/Backup");
    assert!(is_already_unmounted(&drive, || false));

    let share = cmdr_fs::volume::smb_volume_id("testnas", 445, "public");
    assert!(is_already_unmounted(&share, || false));
}

#[test]
fn a_drive_still_in_the_mount_table_is_not_already_ejected() {
    let drive = cmdr_fs::volume::local_volume_id(None, "/Volumes/NO NAME");
    assert!(!is_already_unmounted(&drive, || true));
}

#[test]
fn a_cloud_drive_is_never_already_unmounted_and_never_reads_the_mount_table() {
    // A cloud drive's root is a plain folder that was never in the mount
    // table, so "not listed" says nothing about it: it must keep answering
    // `NotEjectable` instead of a false success.
    assert!(!is_already_unmounted("cloud-dropbox", || panic!(
        "a root that was never a mount must not read the mount table"
    )));
}
// ── The wire type the frontend matches on ─────────────────────────

/// The wire shape the frontend matches on. Internally tagged, camelCase, with
/// the fields intact — the same contract `MutationError` keeps.
#[test]
fn eject_error_crosses_the_wire_as_a_tagged_value() {
    let json = serde_json::to_value(EjectError::VolumeNotFound {
        volume_id: "volumes-usb".to_string(),
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "type": "volumeNotFound", "volumeId": "volumes-usb" })
    );

    // ❗ `holders` is its own tagged value, so the frontend can tell "the scan found
    // nobody" from "nobody could say", which ❌ an empty list alone never could.
    let json = serde_json::to_value(EjectError::UnmountRefused {
        holders: HolderScan::Complete {
            named: vec![VolumeHolder {
                pid: 1234,
                name: "mds".to_string(),
                bundle_id: None,
                kind: HolderKind::Unclassified,
            }],
        },
        detail: "in use by process 1234 (mds)".to_string(),
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "type": "unmountRefused",
            "holders": {
                "type": "complete",
                "named": [{ "pid": 1234, "name": "mds", "bundleId": null, "kind": "unclassified" }],
            },
            "detail": "in use by process 1234 (mds)",
        })
    );

    assert_eq!(
        serde_json::to_value(EjectError::UnmountRefused {
            holders: HolderScan::not_scanned(),
            detail: "diskutil eject: Unmount was dissented".to_string(),
        })
        .unwrap()["holders"],
        serde_json::json!({ "type": "incomplete", "named": [] }),
        "a refusal nobody could scan is typed as such, ❌ never as an empty holder list"
    );

    assert_eq!(
        serde_json::to_value(EjectError::TimedOut).unwrap(),
        serde_json::json!({ "type": "timedOut" })
    );

    assert_eq!(
        serde_json::to_value(EjectError::NotResponding {
            step: EjectStep::IndexStop
        })
        .unwrap(),
        serde_json::json!({ "type": "notResponding", "step": "indexStop" })
    );
}

/// A decision refusal keeps its own identity instead of collapsing into a
/// generic "couldn't".
#[test]
fn a_decision_refusal_keeps_its_variant() {
    let err: EjectError = EjectDecisionError::NotEjectable {
        volume_id: "root".to_string(),
    }
    .into();
    assert!(matches!(err, EjectError::NotEjectable { ref volume_id } if volume_id == "root"));

    let json = serde_json::to_value(EjectError::DeviceDisconnectRefused {
        provider: "mtp".to_string(),
        detail: "PTP CloseSession timed out".to_string(),
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "type": "deviceDisconnectRefused", "provider": "mtp", "detail": "PTP CloseSession timed out" })
    );
}
