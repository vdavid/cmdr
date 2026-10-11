//! How a saved share's mount answers read to the servers family.

use super::*;

fn outcome(error: MountError) -> String {
    format!("{:?}", outcome_of(&error))
}

/// ❗ **A guest turned away is asked for a password, ❌ never told its password
/// was wrong**: nobody offered one. An account the share turns away reads as a
/// rejection, since the sheet's answer is another password for the same place.
#[test]
fn a_refused_mount_asks_the_right_question() {
    let (server, share) = ("nas".to_string(), "photos".to_string());
    assert_eq!(
        outcome(MountError::AuthRequired {
            server: server.clone(),
            share: share.clone()
        }),
        "NeedsCredentials"
    );
    assert_eq!(
        outcome(MountError::AuthFailed { server: server.clone() }),
        "AuthenticationRejected"
    );
    assert_eq!(
        outcome(MountError::PermissionDenied {
            server: server.clone(),
            share: share.clone(),
            username: "sven".to_string()
        }),
        "AuthenticationRejected"
    );
    assert_eq!(outcome(MountError::Timeout { server: server.clone() }), "TimedOut");
    assert_eq!(outcome(MountError::Cancelled { share: share.clone() }), "Cancelled");
    for about_the_share in [
        MountError::HostUnreachable { server: server.clone() },
        MountError::ShareNotFound {
            server: server.clone(),
            share: share.clone(),
        },
        MountError::MountMissing { server, share },
        MountError::GvfsMissing,
    ] {
        assert_eq!(outcome(about_the_share), "Unreachable");
    }
}

/// A row no mount went through has no address to dial, and says so without
/// dialing anything.
#[tokio::test]
async fn a_share_that_never_mounted_has_nothing_to_dial() {
    let row = KnownNetworkShare {
        server_name: "nas".to_string(),
        share_name: "photos".to_string(),
        protocol: "smb".to_string(),
        last_connected_at: String::new(),
        last_connection_mode: ConnectionMode::Guest,
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: None,
        address: None,
        port: None,
        volume_id: None,
        mount_path: None,
        pinned: false,
    };
    let answer = connect_saved_share(row, DialTicket::now(), "never-mounted", None, None).await;
    assert!(matches!(answer, ServerConnectOutcome::Unreachable), "got {answer:?}");
}

/// ❗ **An Add's share row republishes the lists.** The Servers hub re-reads the
/// saved stores on `volumes-changed`, and an Add that only changed a share's
/// account changed no volume: the hub kept "private as otheruser" until the pane
/// left and came back (QA round 7).
#[test]
fn an_added_share_row_tells_the_lists_to_redraw() {
    let _recorder = crate::volume_broadcast::recorder_test_lock();
    let before = crate::volume_broadcast::volumes_changed_requests();
    remember_named_share("192.0.2.77", "redraw-test", Some("testuser"));
    assert!(crate::volume_broadcast::volumes_changed_requests() > before);
}

/// The share a favorite saves is a PLACE (address, port, volume id, mount path), so a pick can dial
/// it once it's unmounted, and ❗ unpinned.
#[test]
fn a_favorited_share_row_is_an_unpinned_place_the_mount_described() {
    let row = favorited_share_row(
        "192.0.2.9",
        10445,
        "naspi",
        Some("david"),
        "/Volumes/naspi",
        "smb-naspi-1",
    );
    assert!(row.is_share());
    assert!(!row.pinned);
    assert_eq!(row.address.as_deref(), Some("192.0.2.9"));
    assert_eq!(row.port, Some(10445));
    assert_eq!(row.username.as_deref(), Some("david"));
    assert_eq!(row.last_connection_mode, ConnectionMode::Credentials);
    assert_eq!(row.volume_id.as_deref(), Some("smb-naspi-1"));
    assert_eq!(row.mount_path.as_deref(), Some("/Volumes/naspi"));
    assert_eq!(known_shares::place_id(&row), "smb-naspi-1");

    let guest = favorited_share_row("nas.local", 445, "public", None, "/Volumes/public", "smb-public-1");
    assert_eq!(guest.port, None, "445 is the default and isn't written");
    assert_eq!(guest.last_connection_mode, ConnectionMode::Guest);
    assert_eq!(
        guest.server_name, "nas.local",
        "on 445 the server goes by its bare address"
    );
}

/// A share Cmdr mounts on a port off 445 is filed under the manual server's discovery name
/// (`localhost:11481`), so a favorited one must be too, or the hub lists a second, port-less
/// "localhost" that doesn't group with it.
#[test]
fn a_favorited_share_off_445_files_under_the_same_server_name_a_cmdr_mount_uses() {
    let row = favorited_share_row("localhost", 11481, "private", None, "/Volumes/private", "smb-private-1");
    assert_eq!(
        row.server_name,
        crate::network::manual_servers::discovery_name("localhost", 11481)
    );
    assert_eq!(row.server_name, "localhost:11481");
    assert_eq!(
        row.address.as_deref(),
        Some("localhost"),
        "the address stays what the mount dialed"
    );
}
