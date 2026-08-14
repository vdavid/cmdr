//! Unit tests for `smb_upgrade.rs`.
//!
//! A sibling file rather than an inline `mod tests`: the suite is bigger than
//! the module it covers, and keeping them in one file pushed it past the
//! `file-length` threshold. Declared as a child module of `smb_upgrade`, so
//! `use super::*` still reaches its private items.

use super::*;
use std::path::Path;
use std::time::Duration;

#[test]
fn system_keychain_aliases_include_the_mdns_service_form_for_an_ip() {
    use crate::network::{HostSource, NetworkHost};
    let hosts = [NetworkHost {
        id: "naspolya".into(),
        name: "Naspolya".into(),
        hostname: Some("Naspolya.local".into()),
        ip_address: Some("192.168.1.111".into()),
        port: 445,
        source: HostSource::Discovered,
    }];
    // We mount by IP; Finder keyed its password by the mDNS service name. The alias
    // set must include that form so the keychain lookup can find it.
    let aliases = system_keychain_aliases_from("192.168.1.111", &hosts);
    assert!(
        aliases.contains(&"Naspolya._smb._tcp.local".to_string()),
        "got {aliases:?}"
    );
    assert!(aliases.contains(&"Naspolya.local".to_string()));
    assert!(aliases.contains(&"Naspolya".to_string()));
}

#[test]
fn system_keychain_aliases_empty_for_an_unknown_server() {
    assert!(system_keychain_aliases_from("10.9.9.9", &[]).is_empty());
}

#[test]
fn is_private_ipv4_recognizes_rfc1918_and_link_local() {
    assert!(is_private_ipv4("10.0.0.1"));
    assert!(is_private_ipv4("192.168.1.111"));
    assert!(is_private_ipv4("172.16.5.7"));
    assert!(is_private_ipv4("169.254.1.2"), "link-local should count");
}

#[test]
fn is_private_ipv4_rejects_public_and_special() {
    assert!(!is_private_ipv4("8.8.8.8"));
    assert!(!is_private_ipv4("100.64.0.1"), "Tailscale/CGNAT not private");
    assert!(!is_private_ipv4("127.0.0.1"), "loopback not private");
    assert!(!is_private_ipv4("naspolya"), "hostnames return false");
    assert!(!is_private_ipv4(""));
    assert!(!is_private_ipv4("::1"), "IPv6 currently returns false");
}

/// `resolve_ip_to_hostname_with_wait` must return immediately (no polling)
/// when the IP isn't a private-range IPv4 — Tailscale/public DNS won't show
/// up in mDNS so there's nothing to wait for.
#[tokio::test]
async fn wait_helper_returns_immediately_for_non_private_ip() {
    let start = std::time::Instant::now();
    let result = resolve_ip_to_hostname_with_wait("8.8.8.8", Duration::from_millis(500)).await;
    let elapsed = start.elapsed();
    assert_eq!(result, None);
    assert!(
        elapsed < Duration::from_millis(50),
        "expected fast path (< 50ms), took {:?}",
        elapsed
    );
}

/// `network.enabled` is a process-global, so the two tests that flip it must
/// not run concurrently: one setting it `false` while the other polls made
/// the other short-circuit and fail (whichever lost the race).
/// Async-aware: both tests `await` while holding it.
static NETWORK_FLAG_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// `resolve_ip_to_hostname_with_wait` must short-circuit when the runtime
/// `network.enabled` flag is off, even for a private IP — mDNS isn't running
/// so polling would just burn the timeout.
#[tokio::test]
async fn wait_helper_short_circuits_when_network_disabled() {
    let _serialized = NETWORK_FLAG_LOCK.lock().await;
    let prev = crate::network::is_network_enabled();
    crate::network::set_network_enabled_flag(false);

    let start = std::time::Instant::now();
    let result = resolve_ip_to_hostname_with_wait("192.168.1.111", Duration::from_millis(500)).await;
    let elapsed = start.elapsed();

    // Restore before assertions so other tests aren't poisoned by panics.
    crate::network::set_network_enabled_flag(prev);

    assert_eq!(result, None);
    assert!(
        elapsed < Duration::from_millis(50),
        "expected fast path (< 50ms), took {:?}",
        elapsed
    );
}

/// Times out gracefully when no host ever shows up in the cache (and falls
/// back to `None` so the caller can use IP-only Keychain lookup).
#[tokio::test]
async fn wait_helper_times_out_gracefully() {
    let _serialized = NETWORK_FLAG_LOCK.lock().await;
    // Ensure network is "enabled" so we exercise the polling path.
    let prev = crate::network::is_network_enabled();
    crate::network::set_network_enabled_flag(true);

    // Use a unique private IP that no test has ever populated, so the cache
    // miss is deterministic.
    let timeout = Duration::from_millis(300);
    let start = std::time::Instant::now();
    let result = resolve_ip_to_hostname_with_wait("10.255.255.254", timeout).await;
    let elapsed = start.elapsed();

    crate::network::set_network_enabled_flag(prev);

    assert_eq!(result, None);
    assert!(
        elapsed >= timeout,
        "should have polled until timeout; elapsed {:?}",
        elapsed
    );
    // Generous upper bound — single poll interval slack.
    assert!(
        elapsed < timeout + Duration::from_millis(250),
        "shouldn't blow past timeout by much; elapsed {:?}",
        elapsed
    );
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
        smb_state: Option<crate::file_system::volume::SmbConnectionState>,
    }

    /// The hook flags of one `TrackingVolume`, for assertions.
    pub(super) struct Hooks {
        pub(super) unmounted: Arc<AtomicBool>,
        pub(super) superseded: Arc<AtomicBool>,
    }

    impl TrackingVolume {
        /// A volume that isn't an SMB volume at all (`smb_connection_state()`
        /// is `None`), which is what an OS-mounted share looks like before
        /// its upgrade.
        pub(super) fn create(label: &str) -> (Arc<dyn Volume>, Hooks) {
            Self::create_with_smb_state(label, None)
        }

        /// A volume that reports an SMB connection state, for the
        /// "is this already upgraded?" checks.
        pub(super) fn create_with_smb_state(
            label: &str,
            smb_state: Option<crate::file_system::volume::SmbConnectionState>,
        ) -> (Arc<dyn Volume>, Hooks) {
            Self::create_at(&format!("/tmp/tracking-{label}"), smb_state)
        }

        /// A volume rooted at an explicit mount path. A replace happens at ONE
        /// root (the upgrade swaps the backend serving a mount, never the mount
        /// itself), and the registry keeps the incumbent when two roots claim
        /// one ID, so a pair standing in for a replace has to share this.
        pub(super) fn create_at(
            root: &str,
            smb_state: Option<crate::file_system::volume::SmbConnectionState>,
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
        fn smb_connection_state(&self) -> Option<crate::file_system::volume::SmbConnectionState> {
            self.smb_state
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
    use crate::file_system::volume::{SmbConnectionState, smb_volume_id};

    // TEST-NET-2 (RFC 5737): reserved, never routed. If the upgrade doesn't
    // short-circuit, it tries to connect here and fails.
    let server = "198.51.100.7";
    let share = "unreachable";
    let volume_id = smb_volume_id(server, 445, share);
    let manager = crate::file_system::volume::manager::get_volume_manager();

    let (direct, _) =
        tracking::TrackingVolume::create_with_smb_state("already-direct", Some(SmbConnectionState::Direct));
    manager.register(&volume_id, std::sync::Arc::clone(&direct));

    let result = try_smb_upgrade(server, share, "/Volumes/unreachable", None, None, 445, &volume_id).await;

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
    use crate::file_system::volume::{SmbConnectionState, smb_volume_id};

    let server = "198.51.100.8";
    let share = "unreachable";
    let volume_id = smb_volume_id(server, 445, share);
    let manager = crate::file_system::volume::manager::get_volume_manager();

    let (direct, _) =
        tracking::TrackingVolume::create_with_smb_state("already-direct-auto", Some(SmbConnectionState::Direct));
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
    use crate::file_system::volume::SmbConnectionState;
    let manager = crate::file_system::volume::manager::get_volume_manager();

    for (label, state, expected) in [
        ("direct", Some(SmbConnectionState::Direct), true),
        ("disconnected", Some(SmbConnectionState::Disconnected), false),
        ("os-mount-state", Some(SmbConnectionState::OsMount), false),
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

/// The reason the fallback log can't just print `UpgradeFailure`.
///
/// `UpgradeFailure` crosses IPC to pick the network-error copy, and the auth case
/// never reaches that surface (it goes to `CredentialsNeeded` instead), so it has
/// no auth variant and folds a rejected password into `Unexpected`. That's what
/// made a stale Keychain password read as a flaky server in the log while the
/// share sat silently on the kernel mount.
///
/// If someone gives `UpgradeFailure` an auth variant, this fails: fold the auth
/// branch of `log_direct_connect_failure` back into the generic one at the same
/// time, so there's one classification rather than two that can disagree.
#[test]
fn an_auth_rejection_is_invisible_to_upgrade_failure_so_the_log_asks_is_auth_error_itself() {
    let rejected = smb2::Error::Auth {
        message: "STATUS_LOGON_FAILURE during SessionSetup".to_string(),
    };

    assert!(
        crate::network::smb_util::is_auth_error(&rejected),
        "a rejected password must be recognizable as auth, or the log can't name it"
    );
    assert_eq!(
        UpgradeFailure::from_smb_error(&rejected),
        UpgradeFailure::Unexpected,
        "UpgradeFailure has no auth variant; the fallback log must not use it to describe an auth failure"
    );
}
