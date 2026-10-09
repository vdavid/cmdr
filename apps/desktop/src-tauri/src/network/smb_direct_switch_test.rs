//! Tests for `smb_direct_switch.rs`: the switch as the volume switcher reaches it,
//! by volume id. The hand-back itself is `smb_upgrade_test.rs`'s.

use super::*;
use crate::file_system::volume::LocalPosixVolume;
use crate::test_support::TestDir;
use std::sync::Arc;

/// A volume registered for the length of one test, under an id no other test uses.
struct Registered(String);

impl Registered {
    fn at(label: &str, root: &std::path::Path) -> Self {
        let id = format!("test-direct-switch-{label}");
        get_volume_manager().register(&id, Arc::new(LocalPosixVolume::local_folder(label, root)));
        Self(id)
    }
}

impl Drop for Registered {
    fn drop(&mut self) {
        get_volume_manager().unregister(&self.0);
    }
}

fn share_on(server: &'static str, share: &'static str) -> impl FnOnce(&str) -> Option<SmbMountInfo> + Send + 'static {
    move |_| {
        Some(SmbMountInfo {
            server: server.to_string(),
            share: share.to_string(),
            subpath: None,
            username: None,
            port: 445,
        })
    }
}

/// Off and back on, read back through the volume each time. A share on the OS mount
/// has no session to hand back, so both flips only save.
#[tokio::test]
async fn the_switch_reads_back_what_was_set() {
    let dir = TestDir::new("direct_switch_roundtrip");
    let volume = Registered::at("roundtrip", &dir);
    let (server, share) = ("198.51.100.52", "roundtrip");
    let limit = Duration::from_secs(3);

    assert_eq!(
        direct_connection_enabled_within(&volume.0, limit, share_on(server, share)).await,
        Some(true),
        "on until someone turns it off"
    );

    let off = set_direct_connection_within(&volume.0, false, limit, share_on(server, share)).await;
    let read_off = direct_connection_enabled_within(&volume.0, limit, share_on(server, share)).await;
    let on = set_direct_connection_within(&volume.0, true, limit, share_on(server, share)).await;
    let read_on = direct_connection_enabled_within(&volume.0, limit, share_on(server, share)).await;

    assert_eq!(off, DirectConnectionSwitch::Saved);
    assert_eq!(read_off, Some(false));
    assert_eq!(on, DirectConnectionSwitch::Saved);
    assert_eq!(read_on, Some(true));
}

/// A volume with no SMB share behind it has no switch: nothing is saved, and the
/// row that would show one gets `None` to hide it by.
#[tokio::test]
async fn a_volume_with_no_share_behind_it_has_no_switch() {
    let dir = TestDir::new("direct_switch_local");
    let volume = Registered::at("local", &dir);
    let limit = Duration::from_secs(3);

    assert_eq!(direct_connection_enabled_within(&volume.0, limit, |_| None).await, None);
    assert_eq!(
        set_direct_connection_within(&volume.0, false, limit, |_| None).await,
        DirectConnectionSwitch::NotAnSmbShare
    );
    assert_eq!(
        set_direct_connection_within("test-direct-switch-never-registered", false, limit, |_| None).await,
        DirectConnectionSwitch::NotAnSmbShare
    );
}

/// A mount whose server went quiet blocks `statfs` for 30-120 s. The read is
/// bounded, and a switch that couldn't see its share saves nothing.
#[tokio::test]
async fn a_mount_that_doesnt_answer_saves_nothing() {
    let dir = TestDir::new("direct_switch_hung");
    let volume = Registered::at("hung", &dir);
    // Stands in for a `statfs` on a hung mount: it returns only once the test lets it.
    let (release, hung) = std::sync::mpsc::channel::<()>();

    let answer = set_direct_connection_within(&volume.0, false, Duration::from_millis(200), move |_| {
        let _ = hung.recv();
        None
    })
    .await;
    // Frees the blocking thread.
    let _ = release.send(());

    assert_eq!(answer, DirectConnectionSwitch::MountNotResponding);
}

/// Switched off, a share's slow-connection notice has nothing left to offer: its
/// button would do exactly what the user just opted out of. The withdrawal happens
/// here, so every route to the switch takes the notice down. Switching ON leaves it:
/// the connect that follows may still fail, and then the notice still holds.
#[tokio::test]
async fn switching_off_withdraws_the_shares_slow_connection_notice() {
    use crate::network::os_mount_notice::{
        FallbackNotice, announce_os_mount_fallback, server_is_told, withdraw_os_mount_notice,
    };
    use crate::network::smb_connect_failure::UpgradeFailure;
    let dir = TestDir::new("direct_switch_withdraws");
    let volume = Registered::at("withdraws", &dir);
    let (server, share) = ("198.51.100.53", "withdraws");
    let limit = Duration::from_secs(3);
    announce_os_mount_fallback(
        server,
        &volume.0,
        share,
        UpgradeFailure::Unreachable,
        FallbackNotice::Announce,
    );

    let on = set_direct_connection_within(&volume.0, true, limit, share_on(server, share)).await;
    let told_after_on = server_is_told(server);
    let off = set_direct_connection_within(&volume.0, false, limit, share_on(server, share)).await;
    let told_after_off = server_is_told(server);
    // Leaves the saved choice and the ledger as every other test expects them.
    set_direct_connection_within(&volume.0, true, limit, share_on(server, share)).await;
    withdraw_os_mount_notice(&volume.0);

    assert_eq!(on, DirectConnectionSwitch::Saved);
    assert!(told_after_on, "switching on keeps the notice");
    assert_eq!(off, DirectConnectionSwitch::Saved);
    assert!(!told_after_off, "switching off withdraws the notice naming this share");
}
