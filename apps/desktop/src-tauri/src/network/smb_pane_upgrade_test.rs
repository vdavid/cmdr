//! Unit tests for `smb_pane_upgrade.rs`: which shares a pane landing qualifies, the
//! per-share cooldown, and the per-share switch.

use super::*;
use crate::file_system::volume::smb_volume_id;

const SERVER: &str = "192.168.50.10";
const SHARE: &str = "testshare";
const ROOT: &str = "/Volumes/testshare";

fn mount_of(server: &str, share: &str) -> SmbMountInfo {
    SmbMountInfo {
        server: server.to_string(),
        share: share.to_string(),
        subpath: None,
        username: None,
        port: 445,
    }
}

/// `plan` for a share at [`ROOT`] whose mount the kernel still lists, switch on.
fn plan_for(
    volume_id: &str,
    backend: Option<BackendKind>,
    cooldowns: &mut Cooldowns,
    now: Instant,
) -> Option<PaneUpgrade> {
    plan(
        volume_id,
        backend,
        Some(Path::new(ROOT)),
        |_| Some(mount_of(SERVER, SHARE)),
        |_| true,
        cooldowns,
        now,
    )
}

// ── Which shares qualify ──────────────────────────────────────────────────────

/// The case the issue is about: a pane on a share macOS mounted, still served by
/// the OS mount, gets an attempt, at the mount it rides on.
#[test]
fn a_pane_on_an_os_mounted_share_tries_the_direct_connection() {
    let id = smb_volume_id(SERVER, 445, SHARE);
    let upgrade = plan_for(&id, Some(BackendKind::Local), &mut Cooldowns::default(), Instant::now())
        .expect("an OS-mounted share qualifies");
    assert_eq!(upgrade.mount_path, ROOT);
    assert_eq!(upgrade.info.share, SHARE);
}

/// A share with its own session has nothing to upgrade, whatever its state: a
/// `Disconnected` one recovers through `attempt_reconnect`.
#[test]
fn a_share_with_a_session_is_left_alone() {
    let id = smb_volume_id(SERVER, 445, SHARE);
    assert!(plan_for(&id, Some(BackendKind::Smb), &mut Cooldowns::default(), Instant::now()).is_none());
}

/// A local navigation stops at the id, before the mount table is read.
#[test]
fn a_local_volume_never_reads_the_mount_table() {
    for id in ["root", "path-volumes-backup-1a2b3c", "sftp-nas-22-test_user_1-1a2b3c"] {
        let upgrade = plan(
            id,
            Some(BackendKind::Local),
            Some(Path::new("/")),
            |_| panic!("{id} made the mount table be read"),
            |_| true,
            &mut Cooldowns::default(),
            Instant::now(),
        );
        assert!(upgrade.is_none(), "{id} qualified");
    }
}

/// An id the registry doesn't know has no mount to dial beside.
#[test]
fn an_unregistered_share_is_left_alone() {
    let id = smb_volume_id(SERVER, 445, SHARE);
    let upgrade = plan(
        &id,
        None,
        None,
        |_| Some(mount_of(SERVER, SHARE)),
        |_| true,
        &mut Cooldowns::default(),
        Instant::now(),
    );
    assert!(upgrade.is_none());
}

/// The mount has to still be there, and has to be THIS share: a share unmounted a
/// moment ago, or another share mounted at the same path since, has nothing to
/// dial for this id.
#[test]
fn only_the_mount_that_derives_this_id_qualifies() {
    let id = smb_volume_id(SERVER, 445, SHARE);
    for (label, mount) in [
        ("unmounted", None),
        ("another share", Some(mount_of(SERVER, "Multimedia"))),
    ] {
        let upgrade = plan(
            &id,
            Some(BackendKind::Local),
            Some(Path::new(ROOT)),
            move |_| mount,
            |_| true,
            &mut Cooldowns::default(),
            Instant::now(),
        );
        assert!(upgrade.is_none(), "{label} qualified");
    }
}

// ── The per-share switch ──────────────────────────────────────────────────────

/// A share switched to stay on the macOS mount isn't dialed, and doesn't spend its
/// cooldown either: switched back on, the next navigation tries right away.
#[test]
fn a_switched_off_share_is_not_upgraded_and_keeps_its_cooldown() {
    let id = smb_volume_id(SERVER, 445, SHARE);
    let mut cooldowns = Cooldowns::default();
    let now = Instant::now();
    let upgrade = plan(
        &id,
        Some(BackendKind::Local),
        Some(Path::new(ROOT)),
        |_| Some(mount_of(SERVER, SHARE)),
        |_| false,
        &mut cooldowns,
        now,
    );
    assert!(upgrade.is_none(), "a switched-off share was planned for a dial");
    assert!(
        plan_for(&id, Some(BackendKind::Local), &mut cooldowns, now).is_some(),
        "the switched-off look spent the cooldown"
    );
}

/// The switch flipped off AFTER the pane-open check (during the mDNS wait, say) still
/// counts: the dial goes through `register_smb_volume`, which reads it again at act
/// time. Nothing is dialed and nothing is registered.
#[tokio::test]
async fn the_dial_reads_the_switch_again_at_act_time() {
    let _secrets = crate::test_support::isolate_secrets();
    // TEST-NET-2 (RFC 5737): never routed, and not a private range, so no mDNS wait.
    // A dial would burn the connect retries before failing.
    let server = "198.51.100.41";
    let share = "pane-open-switched-off";
    let volume_id = smb_volume_id(server, 445, share);
    crate::network::known_shares::set_direct_connection_enabled(server, share, false);

    let start = Instant::now();
    dial(PaneUpgrade {
        mount_path: "/Volumes/pane-open-switched-off".to_string(),
        info: mount_of(server, share),
    })
    .await;
    let elapsed = start.elapsed();

    crate::network::known_shares::set_direct_connection_enabled(server, share, true);
    assert!(
        elapsed < Duration::from_secs(1),
        "must return before any connect attempt; took {elapsed:?}"
    );
    assert!(
        crate::file_system::volume::manager::get_volume_manager()
            .get(&volume_id)
            .is_none(),
        "nothing may be registered for a share that stays on the OS mount"
    );
}

// ── The cooldown ──────────────────────────────────────────────────────────────

/// Browsing a share lists a directory per step. An unreachable server would be
/// re-dialed on every one of them without the cooldown.
#[test]
fn the_next_navigation_inside_the_cooldown_does_not_dial_again() {
    let id = smb_volume_id(SERVER, 445, SHARE);
    let mut cooldowns = Cooldowns::default();
    let start = Instant::now();
    assert!(plan_for(&id, Some(BackendKind::Local), &mut cooldowns, start).is_some());
    assert!(
        plan_for(
            &id,
            Some(BackendKind::Local),
            &mut cooldowns,
            start + Duration::from_secs(1)
        )
        .is_none()
    );
    assert!(
        plan_for(
            &id,
            Some(BackendKind::Local),
            &mut cooldowns,
            start + RETRY_COOLDOWN - Duration::from_millis(1)
        )
        .is_none()
    );
    assert!(
        plan_for(&id, Some(BackendKind::Local), &mut cooldowns, start + RETRY_COOLDOWN).is_some(),
        "once the cooldown runs out, the share gets another try"
    );
}

/// The cooldown is per share: trying one doesn't hold another back.
#[test]
fn the_cooldown_is_per_share() {
    let mut cooldowns = Cooldowns::default();
    let now = Instant::now();
    assert!(cooldowns.try_claim("smb-testshare-1", now));
    assert!(cooldowns.try_claim("smb-multimedia-2", now));
    assert!(!cooldowns.try_claim("smb-testshare-1", now));
}
