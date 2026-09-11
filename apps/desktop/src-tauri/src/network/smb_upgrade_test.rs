//! Unit tests for `smb_upgrade.rs`.
//!
//! A sibling file rather than an inline `mod tests`: the suite is bigger than
//! the module it covers, and keeping them in one file pushed it past the
//! `file-length` threshold. Declared as a child module of `smb_upgrade`, so
//! `use super::*` still reaches its private items.

use super::*;
use std::path::Path;
use std::time::Duration;

/// What the OS records about an ordinary mount of `share`, at the share root.
fn mount_of(server: &str, share: &str, port: u16) -> SmbMountInfo {
    SmbMountInfo {
        server: server.to_string(),
        share: share.to_string(),
        subpath: None,
        username: None,
        port,
    }
}

/// The auto-upgrade paths read a mount's identity off `statfs`, which on a mount
/// whose server went quiet waits 30-120 s and parks a tokio worker for all of it.
/// The read is bounded, and a mount that doesn't answer is reported as such.
#[tokio::test]
async fn a_mount_that_doesnt_answer_its_identity_read_is_not_waited_on() {
    // Stands in for a `statfs` on a hung mount: it returns only once the test lets it.
    let (release, hung) = std::sync::mpsc::channel::<()>();

    let read = tokio::time::timeout(
        Duration::from_secs(3),
        read_identity_within("/Volumes/hung", Duration::from_millis(200), move |_| {
            let _ = hung.recv();
            None
        }),
    )
    .await;
    // Frees the blocking thread whichever way it went.
    let _ = release.send(());

    match read {
        Ok(IdentityRead::NotResponding) => {}
        Ok(other) => panic!("a mount that doesn't answer must read as NotResponding, got {other:?}"),
        Err(_) => panic!("the identity read isn't bounded: still waiting after 3 s"),
    }
}

/// A mount that answers keeps its answer, `None` included: that's every GVFS mount
/// on Linux, and the caller falls back to the requested share's id for it.
#[tokio::test]
async fn a_mount_that_answers_its_identity_read_keeps_the_answer() {
    let read = read_identity_within("/Volumes/data", Duration::from_secs(2), |_| {
        Some(MountIdentity {
            volume_id: "smb-nas-445-data".to_string(),
            share_root: "sub".to_string(),
        })
    })
    .await;
    let IdentityRead::Answered(Some(identity)) = read else {
        panic!("a mount that answered must keep its identity, got {read:?}");
    };
    assert_eq!(identity.volume_id, "smb-nas-445-data");
    assert_eq!(identity.share_root, "sub");

    let read = read_identity_within("/run/user/1000/gvfs/x", Duration::from_secs(2), |_| None).await;
    assert!(matches!(read, IdentityRead::Answered(None)), "got {read:?}");
}

// ── register_replacing_predecessor ─────────────────────────────────

/// A minimal `Volume` impl that records its lifecycle hooks and, like a
/// real `SmbVolume`, stops serving requests once its session is torn down.
/// Used to verify `register_replacing_predecessor` retires the displaced
/// volume without breaking the holders still using it.
mod tracking {
    use crate::file_system::listing::metadata::FileEntry;
    use crate::file_system::volume::{SpaceInfo, Volume, VolumeError};
    use std::future::Future;
    use std::path::{Path, PathBuf};
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub(super) struct TrackingVolume {
        pub(super) on_unmount_called: Arc<AtomicBool>,
        pub(super) on_superseded_called: Arc<AtomicBool>,
        root: PathBuf,
        smb_state: Option<crate::file_system::volume::ConnectionState>,
    }

    /// The hook flags of one `TrackingVolume`, for assertions.
    pub(super) struct Hooks {
        pub(super) unmounted: Arc<AtomicBool>,
        pub(super) superseded: Arc<AtomicBool>,
    }

    impl TrackingVolume {
        /// A volume that isn't an SMB volume at all (no session, and
        /// `backend_kind()` is `Local`), which is what an OS-mounted share looks
        /// like before its upgrade.
        pub(super) fn create(label: &str) -> (Arc<dyn Volume>, Hooks) {
            Self::create_with_smb_state(label, None)
        }

        /// A volume that reports an SMB connection state, for the
        /// "is this already upgraded?" checks.
        pub(super) fn create_with_smb_state(
            label: &str,
            smb_state: Option<crate::file_system::volume::ConnectionState>,
        ) -> (Arc<dyn Volume>, Hooks) {
            Self::create_at(&format!("/tmp/tracking-{label}"), smb_state)
        }

        /// A volume rooted at an explicit mount path. A replace happens at ONE
        /// root (the upgrade swaps the backend serving a mount, never the mount
        /// itself), and the registry keeps the incumbent when two roots claim
        /// one ID, so a pair standing in for a replace has to share this.
        pub(super) fn create_at(
            root: &str,
            smb_state: Option<crate::file_system::volume::ConnectionState>,
        ) -> (Arc<dyn Volume>, Hooks) {
            let unmounted = Arc::new(AtomicBool::new(false));
            let superseded = Arc::new(AtomicBool::new(false));
            let vol = Arc::new(Self {
                on_unmount_called: Arc::clone(&unmounted),
                on_superseded_called: Arc::clone(&superseded),
                root: PathBuf::from(root),
                smb_state,
            }) as Arc<dyn Volume>;
            (vol, Hooks { unmounted, superseded })
        }
    }

    impl Volume for TrackingVolume {
        fn name(&self) -> &str {
            "tracking"
        }
        fn root(&self) -> &Path {
            &self.root
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn list_directory<'a>(
            &'a self,
            _path: &'a Path,
            _on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync + 'a)>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
            Box::pin(async { Err(VolumeError::NotSupported) })
        }
        fn get_metadata<'a>(
            &'a self,
            _path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
            Box::pin(async { Err(VolumeError::NotSupported) })
        }
        fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async { false })
        }
        /// Stands in for any real request on a held volume reference: it
        /// succeeds while the session is up and reports the session gone
        /// once `on_unmount` has torn it down, exactly like `SmbVolume`'s
        /// `check_connection` gate.
        fn is_directory<'a>(
            &'a self,
            _path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
            let torn_down = self.on_unmount_called.load(Ordering::Relaxed);
            Box::pin(async move {
                if torn_down {
                    Err(VolumeError::DeviceDisconnected("session torn down".to_string()))
                } else {
                    Ok(true)
                }
            })
        }
        fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
            Box::pin(async { Err(VolumeError::NotSupported) })
        }
        fn connection_state(&self) -> Option<crate::file_system::volume::ConnectionState> {
            self.smb_state
        }
        /// ❗ Paired with `smb_state`: a state without the kind is a volume the
        /// upgrade's `is_already_direct` sees as some other backend's live session,
        /// which is exactly the confusion the kind exists to prevent.
        fn backend_kind(&self) -> crate::file_system::volume::BackendKind {
            match self.smb_state {
                Some(_) => crate::file_system::volume::BackendKind::Smb,
                None => crate::file_system::volume::BackendKind::Local,
            }
        }
        fn on_unmount(&self) {
            self.on_unmount_called.store(true, Ordering::Relaxed);
        }
        /// Retires without tearing the session down, so holders keep working.
        fn on_superseded(&self) {
            self.on_superseded_called.store(true, Ordering::Relaxed);
        }
    }
}

/// `register_replacing_predecessor` must retire the displaced volume via
/// `on_superseded`, NOT `on_unmount`. The device is still there and the
/// predecessor may still be serving in-flight work; unmounting it tears a
/// live session out from under those holders.
#[tokio::test]
async fn predecessor_is_superseded_not_unmounted() {
    use std::sync::atomic::Ordering;

    let volume_id = "test-register-replacing-predecessor-replace";
    let manager = crate::file_system::volume::manager::get_volume_manager();

    // Both at the SAME mount root: an upgrade replaces the backend serving a
    // mount, so this is not an identity conflict and the swap goes through.
    let (old_volume, old_hooks) = tracking::TrackingVolume::create_at("/Volumes/naspi", None);
    let (new_volume, new_hooks) = tracking::TrackingVolume::create_at("/Volumes/naspi", None);

    manager.register(volume_id, old_volume);
    assert!(!old_hooks.superseded.load(Ordering::Relaxed));

    register_replacing_predecessor(volume_id, std::sync::Arc::clone(&new_volume)).await;

    assert!(
        old_hooks.superseded.load(Ordering::Relaxed),
        "displaced volume's on_superseded must have been called"
    );
    assert!(
        !old_hooks.unmounted.load(Ordering::Relaxed),
        "displaced volume must NOT be unmounted: the device is still there and in-flight work still holds it"
    );
    assert!(
        !new_hooks.superseded.load(Ordering::Relaxed) && !new_hooks.unmounted.load(Ordering::Relaxed),
        "new volume gets no lifecycle hook"
    );

    let current = manager.get(volume_id).expect("new volume should be registered");
    assert!(
        std::sync::Arc::ptr_eq(&current, &new_volume),
        "new volume should be the one registered under volume_id"
    );

    manager.unregister(volume_id);
}

/// The registry KEEPS the incumbent when a second mount root claims one ID, so
/// retiring the incumbent first leaves that ID pointing at a volume whose watcher
/// has been stopped: the share stays registered but stops seeing its own changes.
///
/// Real incident: `localhost:11480` mounting twice 7 ms apart produced this on every
/// cycle, 320 times across a week on one machine.
#[tokio::test]
async fn a_registration_the_registry_refuses_leaves_the_incumbent_untouched() {
    use std::sync::atomic::Ordering;

    let volume_id = "test-register-replacing-predecessor-conflict";
    let manager = crate::file_system::volume::manager::get_volume_manager();

    // A SECOND live mount of the same filesystem, which macOS suffixes.
    let (incumbent, incumbent_hooks) = tracking::TrackingVolume::create_at("/Volumes/naspi", None);
    let (second_mount, second_hooks) = tracking::TrackingVolume::create_at("/Volumes/naspi-1", None);

    manager.register(volume_id, std::sync::Arc::clone(&incumbent));

    register_replacing_predecessor(volume_id, std::sync::Arc::clone(&second_mount)).await;

    assert!(
        !incumbent_hooks.superseded.load(Ordering::Relaxed),
        "the incumbent stays the registered volume, so it must NOT be retired: a superseded volume has no watcher"
    );
    assert!(
        !incumbent_hooks.unmounted.load(Ordering::Relaxed),
        "the incumbent's mount is still there"
    );
    assert!(
        !second_hooks.superseded.load(Ordering::Relaxed) && !second_hooks.unmounted.load(Ordering::Relaxed),
        "the refused volume gets no lifecycle hook either"
    );

    let current = manager
        .get(volume_id)
        .expect("the incumbent should still be registered");
    assert!(
        std::sync::Arc::ptr_eq(&current, &incumbent),
        "the incumbent stays active; the registry only records the second root"
    );
    assert!(
        manager
            .known_roots(volume_id)
            .iter()
            .any(|r| r.as_path() == Path::new("/Volumes/naspi-1")),
        "the second mount root is recorded as a fallback"
    );

    manager.unregister(volume_id);
}

/// The lifecycle invariant behind the whole swap: an operation that grabbed
/// an `Arc` to the volume before an upgrade keeps working on it afterwards.
///
/// This is the real-world failure it pins. A copy to a NAS held `src_vol` /
/// `dst_vol` clones (`volume/copy.rs`) while a redundant SMB upgrade
/// replaced the volume; the swap called `on_unmount` on the predecessor,
/// which dropped the smb2 session, and the running copy died with
/// `DeviceDisconnected` on a connection that was demonstrably healthy.
#[tokio::test]
async fn a_held_volume_reference_keeps_working_across_a_replace() {
    let volume_id = "test-register-replacing-predecessor-held-reference";
    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.unregister(volume_id);

    let (old_volume, _) = tracking::TrackingVolume::create_at("/Volumes/naspi", None);
    manager.register(volume_id, old_volume);

    // What a running transfer holds: an `Arc` clone taken before the swap.
    let held = manager.get(volume_id).expect("registered above");
    assert!(
        held.is_directory(Path::new("/anything")).await.is_ok(),
        "the held reference works before the swap"
    );

    let (new_volume, _) = tracking::TrackingVolume::create_at("/Volumes/naspi", None);
    register_replacing_predecessor(volume_id, new_volume).await;

    assert!(
        held.is_directory(Path::new("/anything")).await.is_ok(),
        "the held reference must survive the swap: an upgrade is not a disconnect"
    );

    manager.unregister(volume_id);
}

// ── Connect retry ──────────────────────────────────────────────────

fn io_error(kind: std::io::ErrorKind) -> smb2::Error {
    smb2::Error::Io(std::io::Error::new(kind, "test"))
}

/// The first direct connect to a LAN address right after launch routinely
/// comes back `EHOSTUNREACH` while the route and the macOS Local Network
/// permission settle; the identical attempt moments later succeeds. That
/// class of failure is worth one more try.
#[tokio::test]
async fn a_connect_that_never_reached_the_server_is_retried() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let attempts = AtomicUsize::new(0);

    let result = connect_with_retry(|| async {
        if attempts.fetch_add(1, Ordering::Relaxed) == 0 {
            Err(io_error(std::io::ErrorKind::HostUnreachable))
        } else {
            Ok("connected")
        }
    })
    .await;

    assert_eq!(result.ok(), Some("connected"));
    assert_eq!(attempts.load(Ordering::Relaxed), 2, "one retry, then success");
}

/// Retrying a rejected password is pointless and risks locking the account.
/// The "Sign in" flow owns that recovery.
#[tokio::test]
async fn a_rejected_credential_is_never_retried() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let attempts = AtomicUsize::new(0);

    let result: Result<&str, _> = connect_with_retry(|| async {
        attempts.fetch_add(1, Ordering::Relaxed);
        Err(smb2::Error::Auth {
            message: "bad password".to_string(),
        })
    })
    .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::Relaxed), 1, "auth failures are final");
}

/// A server that's genuinely gone must still fail promptly: the retries are
/// capped in count, not just in delay.
#[tokio::test]
async fn retries_are_bounded_so_a_dead_server_fails_fast() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let attempts = AtomicUsize::new(0);

    let started = std::time::Instant::now();
    let result: Result<&str, _> = connect_with_retry(|| async {
        attempts.fetch_add(1, Ordering::Relaxed);
        Err(io_error(std::io::ErrorKind::HostUnreachable))
    })
    .await;
    let elapsed = started.elapsed();

    assert!(result.is_err());
    assert_eq!(
        attempts.load(Ordering::Relaxed),
        CONNECT_RETRY_BACKOFF.len() + 1,
        "one initial attempt plus one per backoff step, then give up"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "someone is watching a 'Connecting directly…' toast; took {:?}",
        elapsed
    );
}

/// An attempt that burned the whole connect timeout has already answered the
/// question. Retrying it would stack another 10 s onto a user's wait, so the
/// budget stops the loop even though the failure kind looks retryable.
#[tokio::test]
async fn a_slow_first_attempt_spends_the_retry_budget() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let attempts = AtomicUsize::new(0);

    let result: Result<&str, _> = connect_with_retry(|| async {
        attempts.fetch_add(1, Ordering::Relaxed);
        // allowed-test-sleep: the slow attempt IS the subject — it's what spends the retry budget.
        tokio::time::sleep(CONNECT_RETRY_BUDGET + Duration::from_millis(50)).await;
        Err(io_error(std::io::ErrorKind::HostUnreachable))
    })
    .await;

    assert!(result.is_err());
    assert_eq!(
        attempts.load(Ordering::Relaxed),
        1,
        "a slow attempt spends the budget; don't stack another"
    );
}

// ── Upgrade idempotence and pass coalescing ────────────────────────

/// An upgrade of a volume that is ALREADY a healthy direct smb2 connection
/// must cost nothing: no TCP connect, no negotiate, no session setup, and
/// above all no swap of a perfectly good volume.
///
/// The startup pass used to snapshot its eligibility list up to 15 s before
/// acting, so it happily re-upgraded volumes another path had already
/// upgraded in the meantime. It replaced one volume three times in 15
/// seconds, and the third replacement landed mid-copy.
#[tokio::test]
async fn upgrading_an_already_direct_volume_costs_nothing() {
    use crate::file_system::volume::{ConnectionState, smb_volume_id};

    // TEST-NET-2 (RFC 5737): reserved, never routed. If the upgrade doesn't
    // short-circuit, it tries to connect here and fails.
    let server = "198.51.100.7";
    let share = "unreachable";
    let volume_id = smb_volume_id(server, 445, share);
    let manager = crate::file_system::volume::manager::get_volume_manager();

    let (direct, _) = tracking::TrackingVolume::create_with_smb_state("already-direct", Some(ConnectionState::Direct));
    manager.register(&volume_id, std::sync::Arc::clone(&direct));

    let result = try_smb_upgrade(
        &mount_of(server, share, 445),
        "/Volumes/unreachable",
        None,
        None,
        &volume_id,
    )
    .await;

    assert!(
        result.is_ok(),
        "an already-direct volume is the desired end state, so the upgrade succeeds trivially"
    );
    let current = manager.get(&volume_id).expect("still registered");
    assert!(
        std::sync::Arc::ptr_eq(&current, &direct),
        "the healthy volume must not be swapped out from under its holders"
    );

    manager.unregister(&volume_id);
}

/// The same short-circuit on the auto-upgrade path (startup and mount-time),
/// which is the one that fired the redundant upgrades.
#[tokio::test]
async fn the_auto_upgrade_path_skips_an_already_direct_volume() {
    use crate::file_system::volume::{ConnectionState, smb_volume_id};

    let server = "198.51.100.8";
    let share = "unreachable";
    let volume_id = smb_volume_id(server, 445, share);
    let manager = crate::file_system::volume::manager::get_volume_manager();

    let (direct, _) =
        tracking::TrackingVolume::create_with_smb_state("already-direct-auto", Some(ConnectionState::Direct));
    manager.register(&volume_id, std::sync::Arc::clone(&direct));

    let start = std::time::Instant::now();
    register_smb_volume(server, share, "/Volumes/unreachable", None, None, 445).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(200),
        "must short-circuit before any connect attempt; took {:?}",
        elapsed
    );
    let current = manager.get(&volume_id).expect("still registered");
    assert!(
        std::sync::Arc::ptr_eq(&current, &direct),
        "the healthy volume must not be swapped"
    );

    manager.unregister(&volume_id);
}

/// Only a HEALTHY direct volume is "nothing left to do". A DISCONNECTED
/// `SmbVolume` still wants the work: that's the manual "Connect directly"
/// recovery path after a share dropped. Skipping it there would dead-end the
/// user on a broken volume.
#[test]
fn only_a_healthy_direct_volume_short_circuits_the_upgrade() {
    use crate::file_system::volume::ConnectionState;
    let manager = crate::file_system::volume::manager::get_volume_manager();

    for (label, state, expected) in [
        ("direct", Some(ConnectionState::Direct), true),
        ("disconnected", Some(ConnectionState::Disconnected), false),
        ("os-mount-state", Some(ConnectionState::OsMount), false),
        ("plain-local", None, false),
    ] {
        let volume_id = format!("test-already-direct-{label}");
        let (vol, _) = tracking::TrackingVolume::create_with_smb_state(label, state);
        manager.register(&volume_id, vol);
        assert_eq!(is_already_direct(&volume_id), expected, "{label} volume");
        manager.unregister(&volume_id);
    }
}

/// `ensure_network_discovery_started` runs on every user networking action,
/// and each upgrade pass waits up to 15 s for mDNS before acting. Two clicks
/// nine seconds apart stacked two passes, both firing blind. Only one pass
/// may be in flight.
#[test]
fn a_second_upgrade_pass_is_dropped_while_one_is_pending() {
    let first = UpgradePass::begin().expect("the first pass runs");
    assert!(
        UpgradePass::begin().is_none(),
        "a second pass while one is still pending must be dropped, not stacked"
    );
    drop(first);
    assert!(
        UpgradePass::begin().is_some(),
        "once the pass finishes, the next networking action starts a new one"
    );
}

/// When no predecessor exists, `register_replacing_predecessor` just
/// registers — no lifecycle hook (there's nothing to call it on), no panic.
#[tokio::test]
async fn register_with_no_predecessor_just_registers() {
    use std::sync::atomic::Ordering;

    let volume_id = "test-register-replacing-predecessor-fresh";
    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.unregister(volume_id); // belt-and-suspenders in case a prior test leaked.

    let (new_volume, new_hooks) = tracking::TrackingVolume::create("fresh");
    register_replacing_predecessor(volume_id, std::sync::Arc::clone(&new_volume)).await;

    assert!(!new_hooks.superseded.load(Ordering::Relaxed));
    assert!(!new_hooks.unmounted.load(Ordering::Relaxed));
    assert!(manager.get(volume_id).is_some());

    manager.unregister(volume_id);
}

/// Two upgrade attempts on one volume must not overlap.
///
/// The bug this guards (ERR-ABXW4): the mount-time and startup paths both fired on
/// one mount 20 ms apart, both passed the `is_already_direct` check, and both
/// connected. The one that failed announced a kernel-mount fallback that the other
/// one's session had already disproved, and a notice can't be retracted once the
/// frontend has it.
#[tokio::test]
async fn one_volume_upgrades_one_at_a_time() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let in_flight = Arc::new(AtomicUsize::new(0));
    let overlapped = Arc::new(AtomicUsize::new(0));

    let attempt = |id: &'static str, in_flight: Arc<AtomicUsize>, overlapped: Arc<AtomicUsize>| async move {
        let _guard = lock_volume_upgrade(id).await;
        if in_flight.fetch_add(1, Ordering::AcqRel) != 0 {
            overlapped.fetch_add(1, Ordering::AcqRel);
        }
        // Long enough that a second attempt would land inside this window if the
        // lock weren't holding it out.
        // allowed-test-sleep: the window IS the subject — it stands in for an attempt's
        // connect, and overlap can only be observed while one is still in flight.
        tokio::time::sleep(Duration::from_millis(50)).await;
        in_flight.fetch_sub(1, Ordering::AcqRel);
    };

    let a = tokio::spawn(attempt("smb-same-volume", in_flight.clone(), overlapped.clone()));
    let b = tokio::spawn(attempt("smb-same-volume", in_flight.clone(), overlapped.clone()));
    let (a, b) = tokio::join!(a, b);
    a.unwrap();
    b.unwrap();

    assert_eq!(
        overlapped.load(Ordering::Acquire),
        0,
        "two attempts on the same volume id overlapped"
    );
}

/// The lock is per volume, so an upgrade on one share can't stall an unrelated one.
/// A single global lock would serialize every share on a NAS that remounts them all
/// at login, turning a parallel warm-up into a queue.
#[tokio::test]
async fn different_volumes_upgrade_concurrently() {
    let first = lock_volume_upgrade("smb-volume-one").await;
    // Would deadlock on a shared lock; must return promptly on a per-volume one.
    let second = tokio::time::timeout(Duration::from_secs(5), lock_volume_upgrade("smb-volume-two")).await;
    assert!(second.is_ok(), "an unrelated volume's upgrade was blocked");
    drop(first);
}

// ── Refusals from a real server ───────────────────────────────────────────────

/// The `both` fixture's port: what the Rust integration lane sets, else smb2's default.
fn both_fixture_port() -> u16 {
    std::env::var("SMB_CONSUMER_BOTH_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10482)
}

/// A guest the SHARE turns away is refused at the share, not at sign-in.
///
/// ERR-SHUSC's shape: a Samba server that signs guests in, and a share that won't let
/// them open it. Read as a sign-in refusal, the log said guest access was turned off.
/// The `both` fixture reproduces it: `map to guest = Bad User`, and a `private` share
/// with `valid users = testuser`. Nothing is mounted at the path, and a refused
/// attempt registers nothing, so there's nothing to clean up.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_upgrade_reads_a_guest_refused_at_the_share() {
    use crate::network::smb_connect_failure::{Refusal, RefusedAt, SignInIdentity};

    let port = both_fixture_port();
    let volume_id = crate::file_system::volume::smb_volume_id("127.0.0.1", port, "private");
    let result = try_smb_upgrade(
        &mount_of("127.0.0.1", "private", port),
        "/Volumes/never-mounted",
        None,
        None,
        &volume_id,
    )
    .await;

    assert!(
        matches!(
            result,
            Err(UpgradeError::Refused(Refusal {
                identity: SignInIdentity::Guest,
                at: RefusedAt::Share,
            }))
        ),
        "a guest `private` turns away signed in, so the share refused it, got {result:?}"
    );
}

/// The other step: a wrong password never gets as far as the share. No fixture has a
/// second account, so an ACCOUNT refused at the share is unit-tested only
/// (`smb_connect_failure_test.rs`).
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_upgrade_reads_a_wrong_password_as_refused_at_sign_in() {
    use crate::network::smb_connect_failure::{Refusal, RefusedAt, SignInIdentity};

    let port = both_fixture_port();
    let volume_id = crate::file_system::volume::smb_volume_id("127.0.0.1", port, "private");
    let result = try_smb_upgrade(
        &mount_of("127.0.0.1", "private", port),
        "/Volumes/never-mounted",
        Some("testuser"),
        Some("not-the-password"),
        &volume_id,
    )
    .await;

    assert!(
        matches!(
            result,
            Err(UpgradeError::Refused(Refusal {
                identity: SignInIdentity::Account,
                at: RefusedAt::SignIn,
            }))
        ),
        "got {result:?}"
    );
}
