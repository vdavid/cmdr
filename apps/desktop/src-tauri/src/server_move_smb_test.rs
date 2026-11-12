//! An SMB host moving to a new address: what moves at Save, what refuses, and the
//! first mount there completing each share's pending move.
//!
//! ❗ **This suite owns `198.18.0.x`** (RFC 2544, routed nowhere), beside
//! `server_moves_test.rs`'s `203.0.113.x`: the stores are process-global, so a shared
//! address is two cells racing one entry.

use std::sync::Arc;

use super::*;
use crate::favorites::store as favorites;
use crate::network::known_shares::{AuthOptions, ConnectionMode};
use crate::operations_host::app_engine;
use cmdr_operations::testing::QueuedOperationFixture;

/// A share Cmdr mounted from `address:port`, filed under its discovery name.
fn mounted_row(address: &str, port: u16, share: &str, volume_id: &str) -> KnownNetworkShare {
    KnownNetworkShare {
        server_name: crate::network::manual_servers::discovery_name(address, port),
        share_name: share.to_string(),
        protocol: "smb".to_string(),
        last_connected_at: "2026-10-01T00:00:00Z".to_string(),
        last_connection_mode: ConnectionMode::Credentials,
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: Some("test_user_1".to_string()),
        address: Some(address.to_string()),
        port: (port != 445).then_some(port),
        volume_id: Some(volume_id.to_string()),
        mount_path: Some(format!("/Volumes/{share}")),
        pinned: true,
        server_guid: None,
        server_guid_proof: crate::network::server_guid::GuidProof::Unauthenticated,
    }
}

/// Files `row` and answers the host as the listing would hand it over.
fn saved_host(row: KnownNetworkShare) -> SmbHost {
    let address = row.address.clone().expect("a mounted row");
    let port = row.port.unwrap_or(445);
    assert!(known_shares::remember_share(row).is_none());
    let rows = known_shares::saved_shares()
        .into_iter()
        .filter(|saved| saved.address.as_deref() == Some(address.as_str()))
        .collect();
    SmbHost {
        server: SmbServer::new(&address, port),
        rows,
    }
}

fn no_manual_entry() -> Result<(), String> {
    Ok(())
}

/// ❗ **Save moves the rows and the passwords, and KEEPS the share's id, pending**: a
/// favorite on it still names the old id, which now reaches the share at its new address.
#[tokio::test]
async fn moving_an_smb_host_takes_its_shares_and_passwords_and_keeps_their_ids() {
    let _secrets = crate::test_support::isolate_secrets();
    let (old, new) = ("198.18.0.10", "198.18.0.11");
    let id = "smb-198-18-0-10-move-test";
    let host = saved_host(mounted_row(old, 11480, "move-test", id));
    let (from_name, to_name) = (smb_server(old, 11480), smb_server(new, 11480));
    keychain::save_credentials(&from_name, None, "test_user_1", "pa55").expect("the test store accepts");
    keychain::save_credentials(&from_name, Some("move-test"), "test_user_1", "share-pa55")
        .expect("the test store accepts");

    let outcome = move_host(host, SmbServer::new(new, 11480), &[], no_manual_entry).await;

    assert_eq!(outcome, SavedServerOutcome::Saved);
    let row = known_shares::share_by_volume_id(id).expect("the share keeps its id until a mount");
    assert_eq!(row.address.as_deref(), Some(new));
    assert_eq!(row.server_name, to_name);
    assert_eq!(
        keychain::get_credentials(&to_name, None)
            .map(|c| c.password)
            .ok()
            .as_deref(),
        Some("pa55")
    );
    assert_eq!(
        keychain::get_credentials(&to_name, Some("move-test"))
            .map(|c| c.password)
            .ok()
            .as_deref(),
        Some("share-pa55")
    );
    assert!(
        matches!(
            keychain::get_credentials(&from_name, None),
            Err(KeychainError::NotFound(_))
        ),
        "nothing left under the old address"
    );
    let announced = volume_broadcast::server_moves_from("/Volumes/move-test");
    let redial = announced.last().expect("a pane on the share hears to dial it again");
    assert_eq!(redial.places[0].old_volume_id, id);
    assert_eq!(redial.places[0].new_volume_id, id, "nothing re-keys before the mount");
    assert_eq!(redial.places[0].connection_state, None);
}

/// ❗ **The first mount at the new address completes the move**: favorites re-key to the
/// id it reported, and the panes follow to a LIVE row, so they don't dial it again.
#[tokio::test]
async fn the_first_mount_at_the_new_address_re_keys_favorites_and_panes() {
    let (old_id, new_id) = ("smb-198-18-0-20-complete", "smb-198-18-0-21-complete");
    favorites::add(
        "/Volumes/complete-test/docs",
        Some("Docs".to_string()),
        FavoriteVolume {
            id: old_id.to_string(),
            root: "/Volumes/complete-test".to_string(),
            name: "complete-test on 198.18.0.20".to_string(),
        },
    );

    complete(CompletedMove {
        old_volume_id: old_id.to_string(),
        old_mount_path: Some("/Volumes/complete-test".to_string()),
        new_volume_id: new_id.to_string(),
        new_mount_path: "/Volumes/complete-test-1".to_string(),
        share_name: "complete-test".to_string(),
        server_name: "198.18.0.21".to_string(),
    })
    .await;

    let favorite = favorites::list()
        .into_iter()
        .find(|f| f.name == "Docs" && f.path.starts_with("/Volumes/complete-test"))
        .expect("the favorite stays");
    assert_eq!(favorite.path, "/Volumes/complete-test-1/docs");
    assert_eq!(favorite.volume.map(|v| v.id).as_deref(), Some(new_id));
    let announced = volume_broadcast::server_moves_from("/Volumes/complete-test");
    let moved = &announced.last().expect("the panes hear about it").places[0];
    assert_eq!(
        (moved.old_volume_id.as_str(), moved.new_volume_id.as_str()),
        (old_id, new_id)
    );
    assert_eq!(moved.new_root, "/Volumes/complete-test-1");
    assert_eq!(
        moved.connection_state,
        Some(ConnectionState::OsMount),
        "it just mounted"
    );
}

/// An address another saved SMB host holds is refused, naming it, and nothing moves.
#[tokio::test]
async fn an_address_another_host_holds_is_refused() {
    let id = "smb-198-18-0-30-taken";
    let host = saved_host(mounted_row("198.18.0.30", 445, "taken-test", id));
    let others = [(SmbServer::new("198.18.0.31", 445), "Office NAS".to_string())];

    let outcome = move_host(host, SmbServer::new("198.18.0.31", 445), &others, no_manual_entry).await;

    assert_eq!(
        outcome,
        SavedServerOutcome::AddressTaken {
            name: "Office NAS".to_string()
        }
    );
    let row = known_shares::share_by_volume_id(id).expect("still saved");
    assert_eq!(row.address.as_deref(), Some("198.18.0.30"), "untouched");
}

/// ❗ **A share still mounted from the old address refuses the move**: it's an OS mount
/// anyone may be using, and Cmdr doesn't take it down behind the person's back.
#[tokio::test]
async fn a_share_still_mounted_from_the_old_address_refuses_the_move() {
    let id = "smb-198-18-0-40-mounted";
    let host = saved_host(mounted_row("198.18.0.40", 445, "mounted-test", id));
    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.register(id, Arc::new(cmdr_fs::volume::InMemoryVolume::new("mounted-test")));

    let outcome = move_host(host, SmbServer::new("198.18.0.41", 445), &[], no_manual_entry).await;
    manager.unregister(id);

    assert_eq!(
        outcome,
        SavedServerOutcome::ShareMounted {
            name: "mounted-test".to_string()
        }
    );
    assert_eq!(
        known_shares::share_by_volume_id(id)
            .and_then(|row| row.address)
            .as_deref(),
        Some("198.18.0.40")
    );
}

/// A copy on one of its shares refuses the move, like the other protocols'.
#[tokio::test]
async fn an_operation_on_a_share_refuses_the_move() {
    let id = "smb-198-18-0-50-busy";
    let host = saved_host(mounted_row("198.18.0.50", 445, "busy-test", id));
    let _copy =
        QueuedOperationFixture::park_naming_on(app_engine(), "smb-move-running", vec![id.to_string()], Vec::new());

    let outcome = move_host(host, SmbServer::new("198.18.0.51", 445), &[], no_manual_entry).await;

    assert_eq!(outcome, SavedServerOutcome::OperationRunning);
}

/// ❗ **A share-list mount of the OLD address that set out before Save isn't remembered**:
/// it would save the old address again, a second host beside the moved one. That mount
/// names no saved place up front, so the move marks the SERVER it left.
#[tokio::test]
async fn a_share_list_mount_of_the_old_address_that_lands_after_the_move_is_not_remembered() {
    use crate::network::connect_wiring::DialTicket;
    use crate::network::smb_saved_shares::{MountedShare, remember_mount_since};

    let _secrets = crate::test_support::isolate_secrets();
    let host = saved_host(mounted_row("198.18.0.60", 445, "listed-test", "smb-198-18-0-60-listed"));
    let set_out = DialTicket::now();
    let outcome = move_host(host, SmbServer::new("198.18.0.61", 445), &[], no_manual_entry).await;
    assert_eq!(outcome, SavedServerOutcome::Saved);

    let mounted = |share: &'static str| MountedShare {
        host_name: "198.18.0.60",
        address: "198.18.0.60",
        port: 445,
        share,
        username: None,
        mount_path: "/nonexistent/listed-test",
    };
    remember_mount_since(mounted("other-share"), set_out).await;
    let saved_at_old = |share: &str| {
        known_shares::saved_shares()
            .into_iter()
            .any(|row| row.share_name == share && row.address.as_deref() == Some("198.18.0.60"))
    };
    assert!(!saved_at_old("other-share"), "the old address isn't saved again");

    // A mount the person starts AFTER the move is their own request, and saves.
    remember_mount_since(mounted("after-share"), DialTicket::now()).await;
    assert!(saved_at_old("after-share"));
}

/// ❗ Two spellings of one Keychain key (`testnas.local` → `nas`) are one entry: the move
/// must leave it in place rather than copy it onto itself and delete it.
#[tokio::test]
async fn a_move_between_two_spellings_of_one_key_keeps_the_password() {
    let _secrets = crate::test_support::isolate_secrets();
    let host = saved_host(mounted_row(
        "respell-test.local",
        445,
        "respell-test",
        "smb-respell-test",
    ));
    keychain::save_credentials("respell-test.local", None, "test_user_1", "pa55").expect("the test store accepts");

    let outcome = move_host(host, SmbServer::new("respell-test", 445), &[], no_manual_entry).await;

    assert_eq!(outcome, SavedServerOutcome::Saved);
    assert!(
        keychain::get_credentials("respell-test", None).is_ok(),
        "the password stays"
    );
}

// ============================================================================
// Eject and save
// ============================================================================

/// A host with two saved shares mounted from `address`, both registered as volumes.
fn host_with_two_mounted_shares(address: &str, ids: [&str; 2]) -> SmbHost {
    assert!(known_shares::remember_share(mounted_row(address, 445, "first", ids[0])).is_none());
    let host = saved_host(mounted_row(address, 445, "second", ids[1]));
    let manager = crate::file_system::volume::manager::get_volume_manager();
    for id in ids {
        manager.register(id, Arc::new(cmdr_fs::volume::InMemoryVolume::new(id)));
    }
    host
}

fn unregister(ids: &[&str]) {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    for id in ids {
        manager.unregister(id);
    }
}

fn refused() -> EjectError {
    EjectError::UnmountRefused {
        holders: eject::holders::HolderScan::Incomplete { named: Vec::new() },
        detail: "diskutil unmount: dissented by PID 4242 (Preview)".to_string(),
    }
}

/// Runs "Eject and save" with a takedown that answers `answer` for every mount,
/// answering the outcome and the mounts it was asked to take down.
async fn eject_and_move_answering(
    host: SmbHost,
    to: SmbServer,
    others: &[(SmbServer, String)],
    cancel: &CancellationToken,
    answer: impl Fn() -> Result<TakenDown, EjectError>,
) -> (EjectAndSaveOutcome, Vec<OldMount>) {
    let asked = std::sync::Mutex::new(Vec::new());
    let outcome = eject_and_move_with(host, to, others, no_manual_entry, cancel, |mount| {
        asked.lock().expect("test lock").push(mount);
        let answer = answer();
        async move { answer }
    })
    .await;
    (outcome, asked.into_inner().expect("test lock"))
}

/// ❗ **Every refusal that touches no mount answers BEFORE any eject**: ejecting and then
/// failing to save is the worst outcome there is.
#[tokio::test]
async fn eject_and_save_refuses_a_taken_address_before_touching_a_mount() {
    let ids = ["smb-198-18-0-70-first", "smb-198-18-0-70-second"];
    let host = host_with_two_mounted_shares("198.18.0.70", ids);
    let others = [(SmbServer::new("198.18.0.71", 445), "Office NAS".to_string())];

    let (outcome, asked) = eject_and_move_answering(
        host,
        SmbServer::new("198.18.0.71", 445),
        &others,
        &CancellationToken::new(),
        || Ok(TakenDown::Unmounted),
    )
    .await;
    unregister(&ids);

    assert!(
        asked.is_empty(),
        "nothing is ejected for a move that can't land: {asked:?}"
    );
    assert!(
        matches!(outcome, EjectAndSaveOutcome::Settled { answer: SavedServerOutcome::AddressTaken { ref name } } if name == "Office NAS"),
        "got {outcome:?}"
    );
}

#[tokio::test]
async fn eject_and_save_refuses_a_running_operation_before_touching_a_mount() {
    let ids = ["smb-198-18-0-72-first", "smb-198-18-0-72-second"];
    let host = host_with_two_mounted_shares("198.18.0.72", ids);
    let _copy = QueuedOperationFixture::park_naming_on(
        app_engine(),
        "smb-eject-and-save-running",
        vec![ids[0].to_string()],
        Vec::new(),
    );

    let (outcome, asked) = eject_and_move_answering(
        host,
        SmbServer::new("198.18.0.73", 445),
        &[],
        &CancellationToken::new(),
        || Ok(TakenDown::Unmounted),
    )
    .await;
    unregister(&ids);

    assert!(asked.is_empty(), "got {asked:?}");
    assert!(
        matches!(
            outcome,
            EjectAndSaveOutcome::Settled {
                answer: SavedServerOutcome::OperationRunning
            }
        ),
        "got {outcome:?}"
    );
}

/// ❗ **Every mounted share goes, not just the first**, and then the move lands. The
/// registry still holding them pins the unmount notification's lag: an eject that
/// answered `Ok` doesn't refuse the move it was for.
#[tokio::test]
async fn eject_and_save_takes_down_every_old_address_mount_then_moves() {
    let _secrets = crate::test_support::isolate_secrets();
    let ids = ["smb-198-18-0-74-first", "smb-198-18-0-74-second"];
    let host = host_with_two_mounted_shares("198.18.0.74", ids);

    let (outcome, asked) = eject_and_move_answering(
        host,
        SmbServer::new("198.18.0.75", 445),
        &[],
        &CancellationToken::new(),
        || Ok(TakenDown::Forced),
    )
    .await;
    unregister(&ids);

    assert_eq!(
        asked,
        vec![
            OldMount::Volume {
                share: "first".to_string(),
                volume_id: ids[0].to_string()
            },
            OldMount::Volume {
                share: "second".to_string(),
                volume_id: ids[1].to_string()
            },
        ]
    );
    assert!(
        matches!(
            outcome,
            EjectAndSaveOutcome::Settled {
                answer: SavedServerOutcome::Saved
            }
        ),
        "got {outcome:?}"
    );
    let row = known_shares::share_by_volume_id(ids[1]).expect("still saved");
    assert_eq!(row.address.as_deref(), Some("198.18.0.75"), "moved");
}

/// ❗ **A refused eject stops everything and saves nothing**, naming the share and
/// carrying the refusal, so the sheet can say who's holding it.
#[tokio::test]
async fn eject_and_save_answers_a_refused_eject_and_saves_nothing() {
    let ids = ["smb-198-18-0-76-first", "smb-198-18-0-76-second"];
    let host = host_with_two_mounted_shares("198.18.0.76", ids);

    let (outcome, asked) = eject_and_move_answering(
        host,
        SmbServer::new("198.18.0.77", 445),
        &[],
        &CancellationToken::new(),
        || Err(refused()),
    )
    .await;
    unregister(&ids);

    assert_eq!(
        asked.len(),
        1,
        "the second share isn't tried after a refusal: {asked:?}"
    );
    assert!(
        matches!(outcome, EjectAndSaveOutcome::EjectRefused { ref name, error: EjectError::UnmountRefused { .. } } if name == "first"),
        "got {outcome:?}"
    );
    let row = known_shares::share_by_volume_id(ids[0]).expect("still saved");
    assert_eq!(row.address.as_deref(), Some("198.18.0.76"), "untouched");
}

/// A Cancel that lands while a share is ejecting stops before the next one and saves
/// nothing: what already went stays gone, which is all an unmount can do.
#[tokio::test]
async fn eject_and_save_called_off_mid_way_saves_nothing() {
    let ids = ["smb-198-18-0-78-first", "smb-198-18-0-78-second"];
    let host = host_with_two_mounted_shares("198.18.0.78", ids);
    let cancel = CancellationToken::new();

    let (outcome, asked) = eject_and_move_answering(host, SmbServer::new("198.18.0.79", 445), &[], &cancel, || {
        cancel.cancel();
        Ok(TakenDown::Unmounted)
    })
    .await;
    unregister(&ids);

    assert_eq!(asked.len(), 1, "got {asked:?}");
    assert!(matches!(outcome, EjectAndSaveOutcome::Cancelled), "got {outcome:?}");
    let row = known_shares::share_by_volume_id(ids[0]).expect("still saved");
    assert_eq!(row.address.as_deref(), Some("198.18.0.78"), "untouched");
}
