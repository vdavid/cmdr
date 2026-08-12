//! SMB upgrade helpers: establish direct smb2 connections for OS-mounted SMB volumes.
//!
//! Shared across three upgrade paths:
//! 1. **Startup** (`file_system::upgrade_existing_smb_mounts`): scans existing mounts
//! 2. **Mount-time** (`volumes::watcher::try_upgrade_smb_mount`): FSEvents detects new mount
//! 3. **Manual** (`commands::network::upgrade_to_smb_volume`): user clicks "Connect directly"

use crate::network::get_discovered_hosts;

/// Derives the SMB volume ID from `statfs(mount_path)` (macOS) or
/// `/proc/mounts` (Linux). Returns `None` if the path isn't an SMB mount.
///
/// Used so the mount-time `register_smb_volume` derives the same canonical ID
/// as the OS-event watcher (which only has the mount path to work with). The
/// caller passed `server` may be an mDNS service name or display string that
/// statfs would normalize to an IP, so deriving from statfs is what makes the
/// two sites agree.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn volume_id_from_statfs(mount_path: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    let info = crate::volumes::get_smb_mount_info(mount_path)?;
    #[cfg(target_os = "linux")]
    let info = crate::volumes_linux::get_smb_mount_info(mount_path)?;
    Some(crate::file_system::volume::smb_volume_id(
        &info.server,
        info.port,
        &info.share,
    ))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn volume_id_from_statfs(_mount_path: &str) -> Option<String> {
    None
}

/// Why a direct connection couldn't be established, as a typed reason rather
/// than a sentence.
///
/// Word-free by design: the frontend renders the copy from the message catalog
/// (`$lib/error-messages/` convention — classification in Rust, words on the frontend).
/// A raw `No route to host (os error 65)` has no business reaching a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeFailure {
    /// Nothing answered on the SMB port: the server is off, asleep, or not on
    /// this network right now.
    Unreachable,
    /// It answered, but the handshake ran out of time.
    TooSlow,
    /// It answered and then something we can't act on went wrong.
    Unexpected,
}

impl UpgradeFailure {
    /// Classifies a connect failure by io kind and smb2 error kind, never by
    /// message text.
    pub(crate) fn from_smb_error(err: &smb2::Error) -> Self {
        use std::io::ErrorKind as Io;
        if let smb2::Error::Io(io_err) = err {
            return match io_err.kind() {
                Io::HostUnreachable | Io::NetworkUnreachable | Io::ConnectionRefused | Io::NotConnected => {
                    Self::Unreachable
                }
                Io::TimedOut => Self::TooSlow,
                _ => Self::Unexpected,
            };
        }
        match err.kind() {
            smb2::ErrorKind::TimedOut => Self::TooSlow,
            smb2::ErrorKind::ConnectionLost => Self::Unreachable,
            _ => Self::Unexpected,
        }
    }
}

/// Result of an SMB volume upgrade attempt.
#[derive(serde::Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UpgradeResult {
    /// Upgrade succeeded: volume now uses direct smb2.
    Success,
    /// Credentials needed: frontend should show login form.
    CredentialsNeeded {
        server: String,
        share: String,
        port: u16,
        /// Friendly display name for the server (mDNS hostname or IP).
        display_name: String,
        /// Username hint from stored credentials or the OS mount.
        username_hint: Option<String>,
        /// Optional message explaining why credentials are needed.
        message: Option<String>,
    },
    /// Couldn't reach the server (DNS, network, unreachable, too slow).
    NetworkError {
        reason: UpgradeFailure,
        /// Friendly server name for the frontend to name in its copy.
        display_name: String,
    },
}

/// Internal error type for upgrade attempts, distinguishing auth from network failures.
pub(crate) enum UpgradeError {
    Auth,
    Network {
        reason: UpgradeFailure,
        display_name: String,
    },
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
    use crate::file_system::volume::SmbConnectionState;
    matches!(
        crate::file_system::volume::manager::get_volume_manager()
            .get(volume_id)
            .and_then(|v| v.smb_connection_state()),
        Some(SmbConnectionState::Direct)
    )
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
    if let Some(prev) = manager.get(volume_id) {
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
    use crate::file_system::volume::smb::connect_smb_volume;
    use std::sync::Arc;

    // Resolve mDNS service names (like "Naspolya._smb._tcp.local") to an IP
    let resolved_server = resolve_server_address(server);

    // Derive the volume ID before connect so SmbVolume's internal ID, the
    // ID we pass to `connect_smb_volume`, and the ID the OS-event watcher
    // computes via `volume_id_for_mount` all agree. Statfs is the canonical
    // source — `server` as passed in may be an mDNS service name or display
    // string that wouldn't match what the watcher later sees.
    let volume_id = volume_id_from_statfs(mount_path)
        .unwrap_or_else(|| crate::file_system::volume::smb_volume_id(server, port, share));

    // Another path (a manual "Connect directly", the mount-time upgrade, an
    // earlier pass) may have finished the job while we waited on mDNS. Replacing
    // a healthy direct volume costs a whole session setup and hands every
    // in-flight holder to a superseded instance for no reason.
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

    let params =
        crate::file_system::volume::smb::SmbConnectionParams::new(&resolved_server, share, port, username, password);
    match connect_with_retry(|| connect_smb_volume(share, mount_path, &volume_id, params.clone())).await {
        Ok(volume) => {
            // Overwrite-with-retire so SmbVolume always wins over any
            // LocalPosixVolume the watcher may have registered in the race
            // window, and any prior SmbVolume is retired (not torn down) before
            // we replace it.
            register_replacing_predecessor(&volume_id, Arc::new(volume)).await;
            log::info!("Registered SmbVolume for {} (id={})", mount_path, volume_id);
            // The session is installed and Direct. If the user had indexing
            // enabled for this volume (a persisted index DB with a completed
            // scan), resume it — the backend-autonomous recovery that keeps a NAS
            // index from silently going dark after a disconnect/restart. No-op for
            // a never-enabled share.
            crate::index_host::index().resume_after_reconnect(volume_id.clone());
        }
        Err(e) => {
            // Log-only path: nothing here reaches a person, so the raw error is
            // exactly what we want (it's the diagnostic). The volume stays on the
            // OS mount, which still works, just slower.
            log::warn!(
                "Couldn't establish an smb2 connection for {}/{} ({:?}): {}. Staying on the OS mount.",
                server,
                share,
                UpgradeFailure::from_smb_error(&e),
                e
            );
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

/// Attempts the smb2 connection and registers the volume. Returns `Ok(())` on success.
pub(crate) async fn try_smb_upgrade(
    server: &str,
    share: &str,
    mount_path: &str,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
    volume_id: &str,
) -> Result<(), UpgradeError> {
    use crate::file_system::volume::smb::connect_smb_volume;
    use crate::network::smb_util::is_auth_error;
    use std::sync::Arc;

    // Resolve mDNS service names to connectable addresses
    let resolved_server = resolve_server_address(server);
    let display = friendly_server_name(server);

    // Same re-check as the auto path: the 1.5 s mDNS wait upstream is enough
    // time for another path to have upgraded this volume already.
    if is_already_direct(volume_id) {
        log::debug!("{volume_id} is already a direct smb2 connection; nothing to upgrade");
        return Ok(());
    }

    let params =
        crate::file_system::volume::smb::SmbConnectionParams::new(&resolved_server, share, port, username, password);
    match connect_with_retry(|| connect_smb_volume(share, mount_path, volume_id, params.clone())).await {
        Ok(volume) => {
            register_replacing_predecessor(volume_id, Arc::new(volume)).await;
            log::info!("Registered SmbVolume for {} (id={})", mount_path, volume_id);
            // Manual "Connect directly" also installs a Direct session; resume the
            // drive index the same way the auto-upgrade path does (no-op unless the
            // user had it enabled), so the two install paths stay consistent.
            crate::index_host::index().resume_after_reconnect(volume_id.to_string());
            Ok(())
        }
        Err(e) => {
            if is_auth_error(&e) {
                Err(UpgradeError::Auth)
            } else {
                let reason = UpgradeFailure::from_smb_error(&e);
                // The raw error stays in the log where it's useful; the caller
                // gets the typed reason and the frontend writes the sentence.
                log::warn!(
                    "Couldn't establish an smb2 connection for {}/{} ({:?}): {}",
                    resolved_server,
                    share,
                    reason,
                    e
                );
                Err(UpgradeError::Network {
                    reason,
                    display_name: display,
                })
            }
        }
    }
}

/// Looks up the mDNS hostname for an IP address from discovered hosts.
///
/// Returns the hostname (like "naspolya") without `.local` suffix.
pub(crate) fn resolve_ip_to_hostname(ip: &str) -> Option<String> {
    let hosts = get_discovered_hosts();
    for host in &hosts {
        if host.ip_address.as_deref() == Some(ip) {
            // Return the service name (lowercased), which is what Keychain keys use
            return Some(host.name.to_lowercase());
        }
    }
    None
}

/// Returns true if `ip` is a literal IPv4 address in a private range (RFC 1918 or
/// link-local 169.254/16). mDNS can only help for those: public/VPN/Tailscale IPs
/// won't show up in the local mDNS cache, so there's no point waiting on them.
///
/// Returns `false` for non-IP strings (hostnames), since `resolve_ip_to_hostname`
/// only matches discovered hosts by exact IP.
pub(crate) fn is_private_ipv4(ip: &str) -> bool {
    use std::net::Ipv4Addr;
    let Ok(addr) = ip.parse::<Ipv4Addr>() else {
        return false;
    };
    addr.is_private() || addr.is_link_local()
}

/// Like `resolve_ip_to_hostname`, but waits briefly for mDNS to populate the
/// discovered-host cache when the lookup misses on the first try. Solves the
/// startup race where macOS auto-remounts an SMB share, FSEvents fires before
/// mDNS has had time to find the host, and `statfs`-derived IP-only Keychain
/// lookups miss the credentials we have keyed by hostname.
///
/// Only waits for private-range IPv4 addresses (where mDNS is plausible) and only
/// if `is_network_enabled()`. Otherwise returns whatever the immediate sync
/// lookup gave us. Polls every 100ms up to `timeout`. The caller is responsible
/// for kicking off discovery via `network::ensure_mdns_started` before calling
/// this; the wait alone won't start the daemon.
pub(crate) async fn resolve_ip_to_hostname_with_wait(ip: &str, timeout: std::time::Duration) -> Option<String> {
    // Fast path: already in the cache.
    if let Some(hostname) = resolve_ip_to_hostname(ip) {
        return Some(hostname);
    }
    // No point waiting for non-private IPs (Tailscale, public DNS, etc.) or when
    // networking is disabled by the user.
    if !is_private_ipv4(ip) || !crate::network::is_network_enabled() {
        return None;
    }

    let poll_interval = std::time::Duration::from_millis(100);
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        tokio::time::sleep(poll_interval).await;
        if let Some(hostname) = resolve_ip_to_hostname(ip) {
            log::debug!(
                "Resolved IP {} to hostname {} after waiting {:?}",
                ip,
                hostname,
                start.elapsed()
            );
            return Some(hostname);
        }
    }
    log::debug!(
        "Couldn't resolve IP {} to a hostname via mDNS within {:?}; proceeding without",
        ip,
        timeout
    );
    None
}

/// Resolves a server address from `statfs` to a connectable address.
///
/// `statfs` can return different formats depending on how the mount was created:
/// - An IP address like `192.168.1.111`: usable as-is
/// - A DNS hostname like `fileserver.corp.example.com`: usable as-is
/// - An mDNS service name like `Naspolya._smb._tcp.local`: NOT resolvable by DNS, must be resolved
///   to an IP via the mDNS discovery state
///
/// Returns the resolved IP if possible, otherwise the original string.
pub(crate) fn resolve_server_address(server: &str) -> String {
    // Detect mDNS service names (contain "._tcp" or "._udp")
    if !server.contains("._tcp") && !server.contains("._udp") {
        return server.to_string();
    }

    // Extract the service/display name (everything before the first "._")
    let service_name = server.split("._").next().unwrap_or(server);

    // Look up the discovered host by name (case-insensitive)
    let hosts = get_discovered_hosts();
    for host in &hosts {
        if host.name.eq_ignore_ascii_case(service_name) {
            if let Some(ref ip) = host.ip_address {
                log::debug!("Resolved mDNS service name {} to IP {}", server, ip);
                return ip.clone();
            }
            // Host found but no IP yet; try the hostname
            if let Some(ref hostname) = host.hostname {
                log::debug!("Resolved mDNS service name {} to hostname {}", server, hostname);
                return hostname.clone();
            }
        }
    }

    log::warn!(
        "Could not resolve mDNS service name {} (no matching discovered host)",
        server
    );
    server.to_string()
}

/// Extracts the friendly display name from a server address.
///
/// For mDNS service names like `Naspolya._smb._tcp.local`, returns `Naspolya`.
/// For IPs or hostnames, tries `resolve_ip_to_hostname`, falls back to the raw string.
pub(crate) fn friendly_server_name(server: &str) -> String {
    // mDNS service name: extract the part before "._"
    if server.contains("._tcp") || server.contains("._udp") {
        return server.split("._").next().unwrap_or(server).to_string();
    }
    // IP address: try to resolve to mDNS hostname
    resolve_ip_to_hostname(server).unwrap_or_else(|| server.to_string())
}

/// The server-name forms another app (Finder) might have keyed an SMB password under for
/// this server, gathered from the discovery state. Finder typically uses the full mDNS
/// service name (`Naspolya._smb._tcp.local`), while we mount by IP — so for each
/// discovered host that's the same identity as `server`, we contribute its advertised
/// name, its `.local` hostname, and the synthesized `{name}._smb._tcp.local` service form.
/// Pure over `hosts` for testability; the live wrapper feeds `get_discovered_hosts()`.
/// macOS-only: every caller reads the system keychain, and an ungated definition fails the
/// Linux build via `#![deny(unused)]`.
#[cfg(target_os = "macos")]
pub(crate) fn system_keychain_aliases(server: &str) -> Vec<String> {
    system_keychain_aliases_from(server, &get_discovered_hosts())
}

// `test` keeps the pure helper compiling for the unit tests below, which run on Linux too.
#[cfg(any(target_os = "macos", test))]
fn system_keychain_aliases_from(server: &str, hosts: &[crate::network::NetworkHost]) -> Vec<String> {
    use crate::network::server_identity::same_server;
    let mut out = Vec::new();
    for h in hosts {
        let matches = same_server(&h.name, server, hosts)
            || h.hostname.as_deref().is_some_and(|hn| same_server(hn, server, hosts))
            || h.ip_address.as_deref() == Some(server);
        if matches {
            out.push(h.name.clone());
            out.push(format!("{}._smb._tcp.local", h.name));
            if let Some(hn) = &h.hostname {
                out.push(hn.clone());
            }
        }
    }
    out
}

/// Tries to retrieve SMB credentials from the Keychain.
///
/// Tries multiple keys: by IP (from statfs), by hostname (from mDNS discovery),
/// at both share-level and server-level.
pub(crate) async fn get_keychain_password(
    server_ip: &str,
    hostname: Option<&str>,
    share: &str,
) -> Option<(String, String)> {
    let server_ip = server_ip.to_string();
    let hostname = hostname.map(|s| s.to_string());
    let share = share.to_string();

    tokio::task::spawn_blocking(move || {
        use crate::network::keychain;

        // Build a list of server names to try (hostname first, then IP)
        let mut servers_to_try: Vec<&str> = Vec::new();
        if let Some(ref h) = hostname {
            servers_to_try.push(h);
        }
        servers_to_try.push(&server_ip);

        for server in &servers_to_try {
            // Try share-level credentials first (more specific)
            if let Ok(creds) = keychain::get_credentials(server, Some(&share)) {
                log::debug!("Found Keychain credentials via {}/{}", server, share);
                return Some((creds.username, creds.password));
            }
            // Try server-level credentials
            if let Ok(creds) = keychain::get_credentials(server, None) {
                log::debug!("Found Keychain credentials via {} (server-level)", server);
                return Some((creds.username, creds.password));
            }
        }

        log::debug!("No Keychain credentials for {:?} / {} / {}", hostname, server_ip, share);
        None
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
#[path = "smb_upgrade_test.rs"]
mod tests;
