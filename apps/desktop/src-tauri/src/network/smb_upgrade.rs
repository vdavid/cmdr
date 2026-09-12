//! SMB upgrade helpers: establish direct smb2 connections for OS-mounted SMB volumes.
//!
//! Shared across three upgrade paths:
//! 1. **Startup** (`file_system::upgrade_existing_smb_mounts`): scans existing mounts
//! 2. **Mount-time** (`volumes::watcher::try_upgrade_smb_mount`): FSEvents detects new mount
//! 3. **Manual** (`smb_connect_directly`): user clicks "Connect directly"
//!
//! Who a mount's server is (what to dial, what to call it, which saved credentials
//! go with it) is `smb_server_address`'s question.

use crate::ignore_poison::IgnorePoison;
use crate::network::smb_connect_failure::{
    DirectConnectOutcome, UpgradeError, UpgradeFailure, log_direct_connect_failure,
};
use crate::network::smb_server_address::{
    ServerAddress, friendly_server_name, get_keychain_password, resolve_ip_to_hostname_with_wait,
    resolve_server_address,
};
#[cfg(target_os = "macos")]
use crate::volumes::SmbMountInfo;
#[cfg(target_os = "linux")]
use crate::volumes_linux::SmbMountInfo;

/// What a mount is, as the OS records it: which volume it belongs to, and where
/// it sits inside that volume's share.
///
/// The two travel together because they come from the same `statfs` row and are
/// answers to the same question. Deriving them apart is how a mount ends up
/// keyed as one share and addressed as another.
#[derive(Debug)]
struct MountIdentity {
    /// From `smb_volume_id(server, port, share)`: the SHARE's identity, so a mount
    /// anchored inside a share is the same volume as the share itself.
    volume_id: String,
    /// Where the mount sits inside the share (`SmbVolume`'s `share_root`), empty
    /// for the ordinary mount at the share root.
    share_root: String,
}

/// Reads a mount's identity from `statfs(mount_path)` (macOS) or `/proc/mounts`
/// (Linux). Returns `None` if the path isn't an SMB mount.
///
/// Used so the mount-time `register_smb_volume` derives the same canonical ID
/// as the OS-event watcher (which only has the mount path to work with). The
/// caller passed `server` may be an mDNS service name or display string that
/// statfs would normalize to an IP, so deriving from statfs is what makes the
/// two sites agree. The anchor rides along for the same reason: the caller knows
/// which share it ASKED for, only the mount knows where the OS put it (a DFS
/// referral lands a second mount a directory inside the namespace root).
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn identity_from_statfs(mount_path: &str) -> Option<MountIdentity> {
    #[cfg(target_os = "macos")]
    let info = crate::volumes::get_smb_mount_info(mount_path)?;
    #[cfg(target_os = "linux")]
    let info = crate::volumes_linux::get_smb_mount_info(mount_path)?;
    Some(MountIdentity {
        volume_id: crate::file_system::volume::smb_volume_id(&info.server, info.port, &info.share),
        share_root: info.subpath.unwrap_or_default(),
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn identity_from_statfs(_mount_path: &str) -> Option<MountIdentity> {
    None
}

/// How long reading the mount behind a volume may take before an upgrade stops
/// waiting on it: "Connect directly" answers `MountNotResponding`, and the auto
/// paths leave the share on the kernel mount.
///
/// Above the 2 s read tier (`commands/CLAUDE.md`), because a 2 s bound on a
/// sub-millisecond mount-table read has tripped on a CPU-saturated machine before
/// the blocking task was even scheduled (`commands/volumes.rs::resolve_location_inner`).
/// Still short enough that a hung mount answers while someone watches the toast.
pub(crate) const MOUNT_READ_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

/// What reading a mount's identity came back with.
#[derive(Debug)]
enum IdentityRead {
    /// The mount answered, with its identity or with `None` (see [`identity_from_statfs`]).
    Answered(Option<MountIdentity>),
    /// The mount didn't answer within the limit.
    NotResponding,
}

/// Reads a mount's identity with `read`, giving up after `limit`.
///
/// `read` is [`identity_from_statfs`] outside tests: a `statfs` that waits 30-120 s on
/// a mount whose server went quiet, on an async path that would otherwise hold a
/// tokio worker for all of it.
async fn read_identity_within(
    mount_path: &str,
    limit: std::time::Duration,
    read: impl FnOnce(&str) -> Option<MountIdentity> + Send + 'static,
) -> IdentityRead {
    let path = mount_path.to_string();
    crate::deadline::blocking_with_timeout(limit, IdentityRead::NotResponding, move || {
        IdentityRead::Answered(read(&path))
    })
    .await
}

/// Delays between direct-connect attempts.
///
/// The first connect to a private LAN address shortly after launch routinely
/// comes back `EHOSTUNREACH` while the route and the macOS Local Network
/// permission settle, and the identical attempt moments later succeeds (three
/// times in one session on 2026-08-01, each followed by a clean connect).
/// Deliberately short: someone is watching a "Connecting directly…" toast.
const CONNECT_RETRY_BACKOFF: [std::time::Duration; 2] = [
    std::time::Duration::from_millis(300),
    std::time::Duration::from_millis(1200),
];

/// How long the attempts themselves may have taken before we stop retrying.
///
/// This is what keeps a genuinely-down server failing promptly. An `EHOSTUNREACH`
/// comes back instantly, so a real blip gets its retries; an attempt that ate the
/// 10 s connect timeout already answered the question, and stacking another would
/// triple a user's wait for nothing.
const CONNECT_RETRY_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

/// Runs `connect` and retries a failure that never reached the server.
///
/// Retries only when the attempts have been cheap AND the failure is one a
/// moment's wait can fix. An auth rejection is final (retrying risks locking the
/// account; the "Sign in" flow owns that recovery), and so is anything the
/// server itself answered with.
async fn connect_with_retry<T, F, Fut>(mut connect: F) -> Result<T, smb2::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, smb2::Error>>,
{
    let started = std::time::Instant::now();
    for delay in CONNECT_RETRY_BACKOFF {
        match connect().await {
            Ok(value) => return Ok(value),
            Err(e) => {
                if UpgradeFailure::from_smb_error(&e) != UpgradeFailure::Unreachable
                    || started.elapsed() >= CONNECT_RETRY_BUDGET
                {
                    return Err(e);
                }
                log::debug!("Direct connect didn't reach the server ({e}); retrying in {delay:?}");
                tokio::time::sleep(delay).await;
            }
        }
    }
    connect().await
}

/// Whether `volume_id` already resolves to a HEALTHY direct smb2 volume, in
/// which case an upgrade has nothing to do.
///
/// Every upgrade path checks this immediately before connecting, not just at
/// entry: all three paths wait up to 1.5 s for mDNS first, and the startup pass
/// waits up to 15 s, so another path can finish the job during the wait. Without
/// the re-check we paid a TCP connect, a negotiate, and a session setup to
/// replace a perfectly good volume.
///
/// `Disconnected` deliberately does NOT count. That's the manual "Connect
/// directly" recovery path after a share dropped; short-circuiting it would
/// dead-end the user on a broken volume.
pub(crate) fn is_already_direct(volume_id: &str) -> bool {
    use crate::file_system::volume::{BackendKind, ConnectionState};
    // ❗ Both halves: a live SFTP or WebDAV session also reports `Direct`, and
    // treating one as an upgraded share would skip the mount this function guards.
    crate::file_system::volume::manager::get_volume_manager()
        .get(volume_id)
        .is_some_and(|v| v.backend_kind() == BackendKind::Smb && v.connection_state() == Some(ConnectionState::Direct))
}

/// One lock per volume id, so only one upgrade attempt for a given share is ever
/// in flight.
///
/// The map holds a small entry per SMB volume this run has tried to upgrade (a
/// handful), and the lock inside it is what the attempt actually waits on.
static VOLUME_UPGRADE_LOCKS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>,
> = std::sync::LazyLock::new(Default::default);

/// Takes the upgrade lock for `volume_id`, waiting for any attempt already
/// running on it.
///
/// **Why a lock and not another `is_already_direct` check.** Every path already
/// re-checks `is_already_direct` immediately before connecting, but that is a
/// check-then-act: two paths can both look, both see "not direct", and both
/// connect. They did, 20 ms apart, on the mount in ERR-ABXW4. The loser then
/// announced a kernel-mount fallback that the winner's session had already
/// disproved, and nothing retracts a notice once the frontend has it: the user
/// was told they were on the slow path while a direct session served their files.
///
/// Holding this across the connect turns the check into lock-check-act. The
/// second path waits, then sees the first path's `Direct` volume and skips, so
/// the false notice can't be raised and the redundant session, auth round-trip,
/// and volume replacement never happen either.
///
/// Held across the whole attempt (up to the 10 s connect timeout plus retries),
/// which is the point: a manual "Connect directly" that lands mid-attempt waits
/// for the real answer instead of racing to a second one.
async fn lock_volume_upgrade(volume_id: &str) -> tokio::sync::OwnedMutexGuard<()> {
    let lock = {
        let mut locks = VOLUME_UPGRADE_LOCKS.lock_ignore_poison();
        std::sync::Arc::clone(locks.entry(volume_id.to_string()).or_default())
    };
    lock.lock_owned().await
}

/// Whether an existing-mount upgrade pass is in flight.
static UPGRADE_PASS_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A single in-flight run of `file_system::upgrade_existing_smb_mounts`.
///
/// `ensure_network_discovery_started` fires on EVERY user networking action
/// (opening Network, "Connect to server…", clicking "Connect directly"), and
/// each pass waits up to 15 s for mDNS before it does anything. Two clicks nine
/// seconds apart stacked two passes that both fired blind. Holding this guard
/// for the lifetime of the pass means extra triggers are dropped instead of
/// queued; the running pass re-scans after its wait, so it still picks up
/// anything that mounted in the meantime.
pub(crate) struct UpgradePass;

impl UpgradePass {
    /// `Some` if no pass is in flight, `None` if one already is.
    pub(crate) fn begin() -> Option<Self> {
        use std::sync::atomic::Ordering;
        UPGRADE_PASS_PENDING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self)
    }
}

impl Drop for UpgradePass {
    fn drop(&mut self) {
        UPGRADE_PASS_PENDING.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Register `new_volume` under `volume_id`, retiring any predecessor first.
///
/// **The predecessor is superseded, never unmounted.** A re-register (a manual
/// "Connect directly", an `NSWorkspaceDidMountNotification`, a redundant
/// upgrade pass) means a newer instance owns the id, NOT that the device went
/// away. Anything that grabbed an `Arc` before the swap is still using the old
/// instance: a running transfer holds `src_vol` / `dst_vol` clones for its whole
/// duration (`write_operations::transfer::volume::copy`), a viewer holds a read
/// stream, the indexer holds a scan session. `on_unmount` here dropped the smb2
/// session under all of them and killed a live copy with `DeviceDisconnected` on
/// a healthy connection. `Volume::on_superseded` retires the id-scoped parts
/// (watcher, scan pool, state events) and leaves the session to be released when
/// the last `Arc` drops.
///
/// `SmbVolume::on_superseded` is lock-free, but the trait's DEFAULT delegates to
/// `on_unmount`, which uses `blocking_write()` / `blocking_lock()` (designed for
/// the sync FSEvents thread) and panics inside a tokio runtime. `spawn_blocking`
/// keeps that default legal for any backend. Awaited so the retirement completes
/// before `register` swaps the new volume in.
pub(crate) async fn register_replacing_predecessor(
    volume_id: &str,
    new_volume: std::sync::Arc<dyn crate::file_system::volume::Volume>,
) {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    // Ask BEFORE retiring anyone. When a second mount root claims this ID (macOS
    // suffixes the later mount of one share), the registry keeps the incumbent and
    // only records the new root, so retiring the incumbent here would leave the ID
    // pointing at a volume whose watcher we just stopped: the share stays listed
    // and silently stops seeing its own changes.
    let refused = manager.would_keep_incumbent(volume_id, new_volume.root());
    // Carry the mount roots across before anyone is retired. The registry keeps
    // the roots it has for this ID either way, so whichever instance ends up
    // holding it has to know where each of them sits inside the share, or a later
    // promotion has to refuse a root that is perfectly good.
    carry_mount_roots(manager.get(volume_id).as_deref(), new_volume.as_ref());
    if !refused && let Some(prev) = manager.get(volume_id) {
        log::debug!("Replacing existing volume at id={volume_id}; retiring the predecessor (session stays up)");
        let _ = tokio::task::spawn_blocking(move || prev.on_superseded()).await;
    }
    manager.register(volume_id, new_volume);

    // Tell the frontend the volume's connection state changed (os_mount → direct).
    // The auto-upgrade paths often coincide with an FSEvents mount event that triggers
    // a broadcast anyway, but the after-sign-in and already-mounted paths have no
    // mount event at all: without this, the picker keeps the stale os_mount dot.
    crate::volume_broadcast::emit_volumes_changed();
}

/// Hands the incoming SMB volume every mount root the outgoing one knew about,
/// and tells the outgoing one where the incoming root sits inside the share.
///
/// A no-op unless both sides are `SmbVolume`s. The registry deals in
/// `dyn Volume`, and a mount anchor is a notion only this backend has. Why the
/// exchange goes both ways: `SmbVolume::exchange_mount_roots_with`.
fn carry_mount_roots(
    incumbent: Option<&dyn crate::file_system::volume::Volume>,
    newcomer: &dyn crate::file_system::volume::Volume,
) {
    use cmdr_smb::volume::SmbVolume;

    let (Some(incumbent), Some(newcomer)) = (
        incumbent.and_then(|v| v.as_any().downcast_ref::<SmbVolume>()),
        newcomer.as_any().downcast_ref::<SmbVolume>(),
    ) else {
        return;
    };
    newcomer.exchange_mount_roots_with(incumbent);
}

/// Tries to establish a direct smb2 connection and register as `SmbVolume`.
///
/// Best-effort: logs a warning and returns quietly on failure. The FSEvents
/// watcher will register a `LocalPosixVolume` as fallback.
pub(crate) async fn register_smb_volume(
    server: &str,
    share: &str,
    mount_path: &str,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
) {
    use cmdr_smb::volume::connect_smb_volume;
    use std::sync::Arc;

    // Resolve mDNS service names (like "Naspolya._smb._tcp.local") to an IP.
    // Nothing to dial means nothing to do THIS pass: dialing the service name
    // anyway can only burn the resolver timeout and report a failure that isn't
    // one. The pass that runs once discovery goes active resolves it and connects.
    let ServerAddress::Connectable(resolved_server) = resolve_server_address(server) else {
        log::debug!("Leaving {mount_path} on the kernel mount for now: {server} isn't discovered yet");
        return;
    };

    // Derive the volume ID before connect so SmbVolume's internal ID, the
    // ID we pass to `connect_smb_volume`, and the ID the OS-event watcher
    // computes via `volume_id_for_mount` all agree. Statfs is the canonical
    // source — `server` as passed in may be an mDNS service name or display
    // string that wouldn't match what the watcher later sees.
    //
    // A mount that doesn't answer stays on the kernel mount, and nothing is
    // announced: its identity is what keys the volume, so a guess could file it
    // under another share's id, and the notice's retry would meet the same silent
    // mount. The next mount event or upgrade pass asks again.
    let identity = match read_identity_within(mount_path, MOUNT_READ_LIMIT, identity_from_statfs).await {
        IdentityRead::Answered(identity) => identity,
        IdentityRead::NotResponding => {
            log::warn!(
                "Leaving {mount_path} on the kernel mount: it didn't answer a status read within {MOUNT_READ_LIMIT:?}, so there's no telling which volume it is"
            );
            return;
        }
    };
    let share_root = identity.as_ref().map(|i| i.share_root.clone()).unwrap_or_default();
    let volume_id = identity
        .map(|i| i.volume_id)
        .unwrap_or_else(|| crate::file_system::volume::smb_volume_id(server, port, share));

    // Serialize against any other attempt on this same volume, then re-check under
    // the lock. Another path (a manual "Connect directly", the mount-time upgrade,
    // an earlier pass) may have finished the job while we waited on mDNS or on the
    // lock itself. Replacing a healthy direct volume costs a whole session setup
    // and hands every in-flight holder to a superseded instance for no reason.
    let _upgrade_guard = lock_volume_upgrade(&volume_id).await;
    if is_already_direct(&volume_id) {
        log::debug!("{volume_id} is already a direct smb2 connection; skipping the upgrade");
        return;
    }

    log::debug!(
        "Establishing smb2 connection for SmbVolume: {}:{}/{}",
        resolved_server,
        port,
        share
    );

    let params = cmdr_smb::volume::SmbConnectionParams::new(&resolved_server, share, port, username, password);
    match connect_with_retry(|| {
        connect_smb_volume(
            share,
            cmdr_smb::volume::MountAnchor::new(mount_path, &share_root),
            &volume_id,
            params.clone(),
            crate::volume_host::host(),
        )
    })
    .await
    {
        Ok(volume) => {
            // Overwrite-with-retire so SmbVolume always wins over any
            // LocalPosixVolume the watcher may have registered in the race
            // window, and any prior SmbVolume is retired (not torn down) before
            // we replace it.
            register_replacing_predecessor(&volume_id, Arc::new(volume)).await;
            log::info!("Registered SmbVolume for {} (id={})", mount_path, volume_id);
            // This server is off the slow path, so the next genuine fallback on it is
            // news again rather than a repeat.
            crate::network::os_mount_notice::clear_os_mount_notice(server);
            // The session is installed and Direct. If the user had indexing
            // enabled for this volume (a persisted index DB with a completed
            // scan), resume it — the backend-autonomous recovery that keeps a NAS
            // index from silently going dark after a disconnect/restart. No-op for
            // a never-enabled share.
            crate::index_host::index().resume_after_reconnect(volume_id.clone());
        }
        Err(e) => {
            // The raw error belongs in the log, where it's the diagnostic. The volume
            // stays on the OS mount, which still works, at a fraction of the speed.
            log_direct_connect_failure(server, share, &e, DirectConnectOutcome::StaysOnKernelMount, username);
            // And tell the person, once per server: this is the only path that leaves
            // someone on the slow connection with nothing but a small yellow dot to
            // notice it by. The frontend's notice carries a retry button.
            crate::network::os_mount_notice::announce_os_mount_fallback(server, &volume_id, share);
        }
    }
}

/// Resolve the mount's hostname (with mDNS wait), look up stored Keychain
/// credentials, and register the OS-mounted SMB share as a direct smb2 volume.
///
/// Single entry point for the two fire-and-forget auto-upgrade paths — startup
/// (`file_system::upgrade_existing_smb_mounts`) and mount-time
/// (`volumes::watcher::try_upgrade_smb_mount`). They were byte-for-byte
/// identical except the startup copy used the one-shot `resolve_ip_to_hostname`,
/// so it looked up creds by LAN IP and missed hostname-keyed creds → guest →
/// `STATUS_LOGON_FAILURE`. Keeping both callers here means the resolver choice
/// can't drift between them again. (The manual "Connect directly" path uses
/// `try_smb_upgrade` instead, because it surfaces `CredentialsNeeded` to prompt.)
///
/// Uses `resolve_ip_to_hostname_with_wait` (polls the mDNS host cache up to
/// 1500 ms), not the one-shot resolver: macOS auto-remounts give us the LAN IP
/// via statfs, but stored creds are keyed by the mDNS hostname (e.g.
/// `smb://naspolya/share`). A no-wait lookup races mDNS and misses. Fails open —
/// if mDNS never warms, the IP-keyed lookup still runs, then guest.
pub(crate) async fn resolve_and_register_smb_volume(server: &str, share: &str, mount_path: &str, port: u16) {
    let hostname = resolve_ip_to_hostname_with_wait(server, std::time::Duration::from_millis(1500)).await;
    let creds = get_keychain_password(server, hostname.as_deref(), share).await;
    let (username, password) = match &creds {
        Some((u, p)) => (Some(u.as_str()), Some(p.as_str())),
        None => (None, None),
    };
    register_smb_volume(server, share, mount_path, username, password, port).await;
}

/// Attempts the smb2 connection for the mount `info` describes, and registers the
/// volume. Returns `Ok(())` on success.
///
/// `info` is the caller's read of the mount, anchor included, so this never reads
/// it again: a second `statfs` would be one more unbounded wait on a mount that may
/// have just stopped answering.
pub(crate) async fn try_smb_upgrade(
    info: &SmbMountInfo,
    mount_path: &str,
    username: Option<&str>,
    password: Option<&str>,
    volume_id: &str,
) -> Result<(), UpgradeError> {
    use cmdr_smb::volume::connect_smb_volume;
    use std::sync::Arc;

    let (server, share, port) = (info.server.as_str(), info.share.as_str(), info.port);

    // Resolve mDNS service names to connectable addresses
    let display = friendly_server_name(server);
    // The user asked for this one, so answer rather than go quiet — but answer
    // now. Dialing an undiscovered service name spends the resolver's timeout to
    // reach the same place, and `Unreachable` is what it means: we can't find
    // this server on the network right now.
    let ServerAddress::Connectable(resolved_server) = resolve_server_address(server) else {
        log::info!(
            target: "smb_fallback",
            "{server}/{share} can't be dialed: mDNS hasn't discovered it, so there's no address for it yet."
        );
        return Err(UpgradeError::Network {
            reason: UpgradeFailure::Unreachable,
            display_name: display,
        });
    };

    // Same lock and re-check as the auto path: the 1.5 s mDNS wait upstream is
    // enough time for another path to have upgraded this volume already, and if one
    // is mid-attempt we want its answer rather than a second connection racing it.
    let _upgrade_guard = lock_volume_upgrade(volume_id).await;
    if is_already_direct(volume_id) {
        log::debug!("{volume_id} is already a direct smb2 connection; nothing to upgrade");
        return Ok(());
    }

    let params = cmdr_smb::volume::SmbConnectionParams::new(&resolved_server, share, port, username, password);
    let share_root = info.subpath.clone().unwrap_or_default();
    match connect_with_retry(|| {
        connect_smb_volume(
            share,
            cmdr_smb::volume::MountAnchor::new(mount_path, &share_root),
            volume_id,
            params.clone(),
            crate::volume_host::host(),
        )
    })
    .await
    {
        Ok(volume) => {
            register_replacing_predecessor(volume_id, Arc::new(volume)).await;
            log::info!("Registered SmbVolume for {} (id={})", mount_path, volume_id);
            // Same as the auto path: the server is off the kernel mount, so a later
            // fallback on it is worth a fresh notice.
            crate::network::os_mount_notice::clear_os_mount_notice(server);
            // Manual "Connect directly" also installs a Direct session; resume the
            // drive index the same way the auto-upgrade path does (no-op unless the
            // user had it enabled), so the two install paths stay consistent.
            crate::index_host::index().resume_after_reconnect(volume_id.to_string());
            Ok(())
        }
        Err(e) => {
            // The raw error stays in the log where it's useful; the caller gets the
            // typed reason and the frontend writes the sentence.
            log_direct_connect_failure(
                &resolved_server,
                share,
                &e,
                DirectConnectOutcome::SurfacedToCaller,
                username,
            );
            Err(UpgradeError::from_connect_error(&e, username, display))
        }
    }
}

#[cfg(test)]
#[path = "smb_upgrade_test.rs"]
mod tests;
